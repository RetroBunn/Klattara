//! The cascade/parallel formant synthesizer (Klatt 1980).
//!
//! Voicing and aspiration excite a cascade of resonators (nasal zero, nasal
//! pole, then F1 to F6), which gives correct relative formant amplitudes for
//! vowels without any per-formant level controls. Frication noise excites a
//! bank of parallel resonators (F1 to F6, each with its own amplitude) plus a
//! flat bypass path, which lets fricatives and bursts shape their spectra
//! freely. The two branches are summed at the output.

use crate::frame::{Frame, NUM_FORMANTS};
use crate::resonator::{AntiResonator, Resonator};
use crate::source::{GlottalSource, Noise, TiltFilter};

/// Settings that belong to the voice as a whole rather than to frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceSettings {
    /// Slow, quasi-random wobble of F0 (KLSYN88 flutter). 0 is perfectly
    /// steady; 25 is the natural-sounding value Klatt & Klatt suggest.
    pub flutter: f64,
    /// Output gain; 1.0 is normal.
    pub volume: f64,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        VoiceSettings { flutter: 25.0, volume: 1.0 }
    }
}

/// How often, in samples, filter coefficients are recomputed while
/// parameters move between frames (under 1 ms at common sample rates).
const UPDATE_INTERVAL: usize = 16;

/// Brings the summed branches to the level of other screen-reader voices:
/// speech averages about −19 dBFS RMS, as Microsoft's voices do, between
/// Eloquence (−20.6) and DECtalk's Paul (−16.8), measured on the same
/// sentences. Peaks stay near −3 dBFS, leaving headroom for emphasis.
const OUTPUT_SCALE: f64 = 0.28;

/// At equal dB settings, aspiration noise through the cascade is far stronger
/// than the glottal pulse. This brings AH = 60 dB to about 12 dB below a
/// vowel at AV = 60 dB, the level of a typical /h/.
const ASPIRATION_SCALE: f64 = 0.07;

/// Converts a Klatt-style dB amplitude to a linear gain: 60 dB is unity and
/// 0 dB or below is silence.
fn db_to_gain(db: f64) -> f64 {
    if db <= 0.0 { 0.0 } else { 10f64.powf((db - 60.0) / 20.0) }
}

/// The linear amplitude controls of a frame. These are interpolated every
/// sample, in the linear domain, so onsets and offsets are smooth.
#[derive(Clone, Copy, Debug)]
struct Gains {
    av: f64,
    ah: f64,
    af: f64,
    ab: f64,
    par: [f64; NUM_FORMANTS],
}

impl Gains {
    fn from_frame(frame: &Frame) -> Self {
        Gains {
            av: db_to_gain(frame.av),
            ah: db_to_gain(frame.ah),
            af: db_to_gain(frame.af),
            ab: db_to_gain(frame.ab),
            par: frame.par_amp.map(db_to_gain),
        }
    }

    fn lerp(&self, other: &Gains, t: f64) -> Gains {
        let mix = |a: f64, b: f64| a + (b - a) * t;
        Gains {
            av: mix(self.av, other.av),
            ah: mix(self.ah, other.ah),
            af: mix(self.af, other.af),
            ab: mix(self.ab, other.ab),
            par: std::array::from_fn(|k| mix(self.par[k], other.par[k])),
        }
    }
}

/// A formant synthesizer. Feed it frames with [`Synth::render`]; it glides
/// smoothly from each frame to the next.
#[derive(Clone, Debug)]
pub struct Synth {
    sample_rate: f64,
    settings: VoiceSettings,
    /// Where the parameters currently stand: the end of the last render.
    current: Frame,
    source: GlottalSource,
    tilt: TiltFilter,
    noise: Noise,
    nasal_zero: AntiResonator,
    nasal_pole: Resonator,
    cascade: [Resonator; NUM_FORMANTS],
    parallel: [Resonator; NUM_FORMANTS],
}

impl Synth {
    /// Creates a silent synthesizer. `sample_rate` must be at least 8000 Hz;
    /// 22050 Hz or more keeps sibilants like /s/ crisp.
    pub fn new(sample_rate: u32) -> Self {
        assert!(sample_rate >= 8000, "sample rate {sample_rate} Hz is too low");
        let sample_rate = sample_rate as f64;
        let mut synth = Synth {
            sample_rate,
            settings: VoiceSettings::default(),
            current: Frame::default(),
            source: GlottalSource::new(sample_rate),
            tilt: TiltFilter::default(),
            noise: Noise::default(),
            nasal_zero: AntiResonator::default(),
            nasal_pole: Resonator::default(),
            cascade: [Resonator::default(); NUM_FORMANTS],
            parallel: [Resonator::default(); NUM_FORMANTS],
        };
        synth.set_settings(VoiceSettings::default());
        synth.update_filters(&Frame::default());
        synth
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate as u32
    }

    pub fn settings(&self) -> VoiceSettings {
        self.settings
    }

    pub fn set_settings(&mut self, settings: VoiceSettings) {
        self.settings = settings;
        self.source.set_flutter(settings.flutter);
    }

    /// Returns to silence and clears all internal state, so that rendering
    /// the same frames again produces exactly the same samples.
    pub fn reset(&mut self) {
        self.source.reset();
        self.tilt.reset();
        self.noise.reset();
        self.nasal_zero.reset();
        self.nasal_pole.reset();
        self.cascade.iter_mut().for_each(Resonator::reset);
        self.parallel.iter_mut().for_each(Resonator::reset);
        self.jump_to(&Frame::default());
    }

    /// Moves the parameters to `frame` at once, without a glide.
    pub fn jump_to(&mut self, frame: &Frame) {
        self.current = *frame;
        self.update_filters(frame);
    }

    /// Fills `out` with samples (−1 to 1) while gliding linearly from the
    /// current parameters to `target`, which is reached at the last sample.
    pub fn render(&mut self, target: &Frame, out: &mut [f32]) {
        let start = self.current;
        let len = out.len() as f64;
        let start_gains = Gains::from_frame(&start);
        let end_gains = Gains::from_frame(target);
        for (i, sample) in out.iter_mut().enumerate() {
            if i % UPDATE_INTERVAL == 0 {
                self.update_filters(&start.lerp(target, i as f64 / len));
            }
            let gains = start_gains.lerp(&end_gains, (i + 1) as f64 / len);
            *sample = self.next_sample(&gains) as f32;
        }
        self.current = *target;
    }

    fn update_filters(&mut self, p: &Frame) {
        let sr = self.sample_rate;
        self.source.set_targets(p.f0, p.oq);
        self.source.set_creak(p.creak);
        self.source.set_singing(p.vibrato, p.steady);
        self.tilt.set(p.tl, sr);
        self.nasal_zero.set(p.fnz, p.bnz, sr);
        self.nasal_pole.set(p.fnp, p.bnp, sr);
        for k in 0..NUM_FORMANTS {
            self.cascade[k].set(p.freq[k], p.bw[k], sr);
            self.parallel[k].set_peak(p.freq[k], p.par_bw[k], sr);
        }
    }

    fn next_sample(&mut self, gains: &Gains) -> f64 {
        let voice = self.tilt.process(self.source.next_sample()) * gains.av;

        // While the folds vibrate, airflow and therefore turbulence noise are
        // weaker in the closed phase of each period (Klatt 1980).
        let mut noise = self.noise.next_sample();
        if gains.av > 0.0 && self.source.is_closed() {
            noise *= 0.5;
        }

        let mut cascade = voice + noise * gains.ah * ASPIRATION_SCALE;
        cascade = self.nasal_zero.process(cascade);
        cascade = self.nasal_pole.process(cascade);
        for resonator in &mut self.cascade {
            cascade = resonator.process(cascade);
        }

        let frication = noise * gains.af;
        let mut parallel = frication * gains.ab;
        for (k, resonator) in self.parallel.iter_mut().enumerate() {
            let y = resonator.process(frication * gains.par[k]);
            // Neighbouring resonators are out of phase between their peaks;
            // alternating signs keeps them from cancelling there.
            if k % 2 == 0 {
                parallel += y;
            } else {
                parallel -= y;
            }
        }

        soft_clip((cascade + parallel) * OUTPUT_SCALE * self.settings.volume)
    }
}

/// Leaves normal levels untouched and rounds peaks above 0.8 smoothly toward
/// 1, instead of clipping them harshly.
fn soft_clip(x: f64) -> f64 {
    const KNEE: f64 = 0.8;
    let magnitude = x.abs();
    if magnitude <= KNEE { x } else { x.signum() * (KNEE + (1.0 - KNEE) * ((magnitude - KNEE) / (1.0 - KNEE)).tanh()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::magnitude_at;

    const SR: u32 = 22050;

    /// A steady vowel at 100 Hz, without flutter, so harmonics fall exactly
    /// on multiples of 100 Hz.
    fn steady_vowel(f1: f64, f2: f64, f3: f64) -> Vec<f32> {
        let mut synth = Synth::new(SR);
        synth.set_settings(VoiceSettings { flutter: 0.0, volume: 1.0 });
        let mut frame = Frame { f0: 100.0, av: 60.0, oq: 0.5, ..Frame::default() };
        frame.freq[..3].copy_from_slice(&[f1, f2, f3]);
        frame.bw[..3].copy_from_slice(&[70.0, 100.0, 150.0]);
        synth.jump_to(&frame);
        let mut warmup = vec![0.0; 2205];
        synth.render(&frame, &mut warmup);
        let mut out = vec![0.0; SR as usize];
        synth.render(&frame, &mut out);
        out
    }

    fn render_frames(synth: &mut Synth, frames: &[Frame]) -> Vec<f32> {
        let mut out = Vec::new();
        for frame in frames {
            let mut chunk = vec![0.0; 110];
            synth.render(frame, &mut chunk);
            out.extend_from_slice(&chunk);
        }
        out
    }

    #[test]
    fn same_frames_give_identical_audio() {
        let frames = [
            Frame { f0: 120.0, av: 60.0, ah: 40.0, af: 50.0, ab: 40.0, ..Frame::default() },
            Frame { f0: 100.0, av: 55.0, ..Frame::default() },
            Frame::default(),
        ];
        let mut synth = Synth::new(SR);
        let first = render_frames(&mut synth, &frames);
        synth.reset();
        let second = render_frames(&mut synth, &frames);
        assert_eq!(first, second);
        assert!(first.iter().any(|&x| x != 0.0));
    }

    #[test]
    fn silent_frames_give_silence() {
        let mut synth = Synth::new(SR);
        let out = render_frames(&mut synth, &[Frame::default(); 20]);
        assert!(out.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn vowel_spectrum_peaks_at_its_formants() {
        let sr = SR as f64;
        // /ɑ/ as in "hod": strong near F1 = 730 Hz, weak in the valley near 1800 Hz.
        let ah = steady_vowel(730.0, 1090.0, 2440.0);
        let ratio = magnitude_at(&ah, 700.0, sr) / magnitude_at(&ah, 1800.0, sr);
        assert!(ratio > 10.0, "/ɑ/ F1-to-valley ratio {ratio}");
        // /i/ as in "heed": strong near F2 = 2290 Hz, weak in the valley near 1100 Hz.
        let ee = steady_vowel(270.0, 2290.0, 3010.0);
        let ratio = magnitude_at(&ee, 2300.0, sr) / magnitude_at(&ee, 1100.0, sr);
        assert!(ratio > 3.0, "/i/ F2-to-valley ratio {ratio}");
    }

    #[test]
    fn frication_follows_parallel_formant() {
        let mut frame = Frame { af: 60.0, ..Frame::default() };
        frame.freq[5] = 6000.0;
        frame.par_amp[5] = 60.0;
        let mut synth = Synth::new(SR);
        synth.jump_to(&frame);
        let mut out = vec![0.0; SR as usize];
        synth.render(&frame, &mut out);
        let sr = SR as f64;
        // Average over a few nearby frequencies, since noise spectra are ragged.
        let band = |center: f64| (0..5).map(|k| magnitude_at(&out, center + 20.0 * k as f64, sr)).sum::<f64>();
        // One pole pair at 6 kHz with an 800 Hz bandwidth stands about 17 dB
        // (a factor of 7) above its level at 1 kHz.
        let ratio = band(6000.0) / band(1000.0);
        assert!(ratio > 5.0, "6 kHz to 1 kHz ratio {ratio}");
    }

    #[test]
    fn output_stays_finite_and_in_range() {
        let mut loud = Frame { f0: 300.0, av: 80.0, ah: 80.0, af: 80.0, ab: 80.0, ..Frame::default() };
        loud.par_amp = [80.0; NUM_FORMANTS];
        loud.bw = [20.0; NUM_FORMANTS];
        let mut synth = Synth::new(SR);
        let out = render_frames(&mut synth, &[loud; 50]);
        assert!(out.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
    }
}
