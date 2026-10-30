//! Excitation sources: the glottal pulse, its spectral tilt, and noise.

use crate::singing::VIBRATO_RATE;
use std::f64::consts::PI;

/// Flutter is a share of F0 in KLSYN88, so it grows with pitch: at 400 Hz it
/// sounded like vibrato. Above this pitch it stays the size it is here, in Hz.
const FLUTTER_CEILING_HZ: f64 = 150.0;

/// Creaky voice at full strength, as at a glottalized /t/: lower and
/// irregular pitch, and more constricted folds (Garellek & Seyfarth 2016).
/// The pitch falls by this share.
const CREAK_PITCH_DROP: f64 = 0.6;
/// Each period's length varies at random by up to this share.
const CREAK_JITTER: f64 = 0.3;
/// Every second pulse is weaker by this share, as in KLSYN88's diplophonia,
/// and each pulse's strength varies at random by up to [`CREAK_SHIMMER`].
const CREAK_ALTERNATION: f64 = 0.7;
const CREAK_SHIMMER: f64 = 0.3;
/// The open quotient shrinks by this share: the folds stay closed longer.
const CREAK_OQ_DROP: f64 = 0.4;

/// The voicing source: the time derivative of glottal airflow, after the
/// KLGLOTT88 model of Klatt & Klatt (1990).
///
/// During the open part of each period the airflow follows a cubic,
/// a·t² − b·t³, which rises smoothly and then shuts off abruptly. Working with
/// its derivative builds in the +6 dB/octave lip-radiation characteristic, so
/// no differentiator is needed at the output. The abrupt closure is the main
/// excitation of the vocal tract. It is rendered with a polyBLEP correction
/// (Välimäki & Huovilainen 2007), so pitch periods need not be whole numbers
/// of samples and high harmonics do not alias into harsh buzz.
///
/// The output has a step of exactly 1 at each closure, so the voicing
/// amplitude sets the strength of excitation regardless of F0 or open quotient.
#[derive(Clone, Debug)]
pub struct GlottalSource {
    sample_rate: f64,
    /// Requested F0 and open quotient, updated as frame parameters move.
    f0: f64,
    oq: f64,
    flutter: f64,
    /// Creaky voice, 0 to 1.
    creak: f64,
    /// How far vibrato swings the pitch either way, in cents.
    vibrato: f64,
    /// 1 holds the pitch free of flutter.
    steady: f64,
    /// Whether a glottal period is in progress.
    running: bool,
    /// Position within the current period, 0 to 1. Opening is at 0.
    phase: f64,
    /// Phase advance per sample, and the open quotient, latched at the start
    /// of each period so the pulse shape never changes partway through.
    phase_step: f64,
    period_oq: f64,
    /// The strength of the current pulse, below 1 only in creaky voice.
    period_gain: f64,
    /// Whether the current pulse is the weaker of an alternating pair.
    weak_pulse: bool,
    /// Draws the random wander of creaky periods.
    wander: Noise,
    samples_elapsed: u64,
}

impl GlottalSource {
    pub fn new(sample_rate: f64) -> Self {
        GlottalSource {
            sample_rate,
            f0: 0.0,
            oq: 0.5,
            flutter: 0.0,
            creak: 0.0,
            vibrato: 0.0,
            steady: 0.0,
            running: false,
            phase: 0.0,
            phase_step: 0.0,
            period_oq: 0.5,
            period_gain: 1.0,
            weak_pulse: false,
            wander: Noise::default(),
            samples_elapsed: 0,
        }
    }

    /// Sets the pitch and open quotient that the next period will use.
    pub fn set_targets(&mut self, f0: f64, oq: f64) {
        self.f0 = f0;
        self.oq = oq;
    }

    /// Sets the KLSYN88 flutter amount: 0 is perfectly steady, 25 is the
    /// natural-sounding value Klatt & Klatt suggest.
    pub fn set_flutter(&mut self, flutter: f64) {
        self.flutter = flutter;
    }

    /// Sets how creaky the voice is, from 0 (normal) to 1, for the next period.
    pub fn set_creak(&mut self, creak: f64) {
        self.creak = creak.clamp(0.0, 1.0);
    }

    /// Sets the vibrato, in cents either way, and how far flutter is held off, from
    /// 0 to 1, for the next period.
    pub fn set_singing(&mut self, vibrato: f64, steady: f64) {
        self.vibrato = vibrato;
        self.steady = steady;
    }

    /// Whether the folds are vibrating and currently closed.
    pub fn is_closed(&self) -> bool {
        self.running && self.phase >= self.period_oq
    }

    pub fn reset(&mut self) {
        self.running = false;
        self.phase = 0.0;
        self.weak_pulse = false;
        self.wander.reset();
        self.samples_elapsed = 0;
    }

    pub fn next_sample(&mut self) -> f64 {
        self.samples_elapsed += 1;
        if !self.running {
            if self.f0 <= 0.0 {
                return 0.0;
            }
            self.start_period();
        }

        let (phase, step, oq) = (self.phase, self.phase_step, self.period_oq);
        let mut y = if phase < oq {
            let tau = phase / oq;
            tau * (2.0 - 3.0 * tau)
        } else {
            0.0
        };
        // At closure the derivative jumps from −1 up to 0.
        let mut since_closure = phase - oq;
        if since_closure < 0.0 {
            since_closure += 1.0;
        }
        y += 0.5 * poly_blep(since_closure, step);
        y *= self.period_gain;

        self.phase += step;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            if self.f0 > 0.0 {
                self.latch_period();
            } else {
                self.running = false;
                self.phase = 0.0;
            }
        }
        y
    }

    fn start_period(&mut self) {
        self.running = true;
        self.phase = 0.0;
        self.latch_period();
    }

    /// Latches F0 and the open quotient for the period that is starting,
    /// applying the KLSYN88 flutter, ΔF0 = (FL/50)·(F0/100)·S Hz, where S is
    /// sin(2π·12.7t) + sin(2π·7.1t) + sin(2π·4.7t), with F0 there no higher
    /// than [`FLUTTER_CEILING_HZ`]. Creaky voice then lowers the pitch, makes
    /// each period's length and strength wander, and weakens every second pulse.
    fn latch_period(&mut self) {
        let t = self.samples_elapsed as f64 / self.sample_rate;
        let wobble = (2.0 * PI * 12.7 * t).sin() + (2.0 * PI * 7.1 * t).sin() + (2.0 * PI * 4.7 * t).sin();
        let size = (FLUTTER_CEILING_HZ / self.f0).min(1.0);
        let flutter = self.flutter * (1.0 - self.steady);
        let mut f0 = self.f0 * (1.0 + flutter / 5000.0 * size * wobble);
        if self.vibrato > 0.0 {
            f0 *= 2f64.powf(self.vibrato * (2.0 * PI * VIBRATO_RATE * t).cos() / 1200.0);
        }
        let c = self.creak;
        self.period_gain = 1.0;
        if c > 0.0 {
            let (jitter, shimmer) = (self.wander.next_uniform(), self.wander.next_uniform());
            f0 *= (1.0 - CREAK_PITCH_DROP * c) * (1.0 + CREAK_JITTER * c * jitter);
            self.weak_pulse = !self.weak_pulse;
            if self.weak_pulse {
                self.period_gain = 1.0 - CREAK_ALTERNATION * c;
            }
            self.period_gain *= 1.0 + CREAK_SHIMMER * c * shimmer;
        } else {
            self.weak_pulse = false;
        }
        self.phase_step = (f0 / self.sample_rate).clamp(1e-6, 0.05);
        self.period_oq = (self.oq * (1.0 - CREAK_OQ_DROP * c)).clamp(0.2, 0.9);
    }
}

/// Polynomial band-limited step residual. `t` is the phase elapsed since the
/// discontinuity (0 to 1), `dt` the phase advance per sample. Adding
/// (h/2)·poly_blep smooths a step of height h across the two nearest samples.
fn poly_blep(t: f64, dt: f64) -> f64 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// Spectral tilt of the voicing source (TL in KLSYN88): a low-pass of two
/// identical one-pole sections that together attenuate 3 kHz by `tl` dB.
#[derive(Clone, Copy, Debug, Default)]
pub struct TiltFilter {
    pole: f64,
    y1: f64,
    y2: f64,
}

impl TiltFilter {
    pub fn set(&mut self, tl: f64, sample_rate: f64) {
        self.pole = if tl <= 0.0 {
            0.0
        } else {
            // Solve (1−p)² = g²·(1 − 2p·cos w + p²) for p, where g is each
            // section's gain at 3 kHz (half the total dB).
            let g2 = 10f64.powf(-tl / 20.0);
            let w = (2.0 * PI * 3000.0 / sample_rate).min(0.9 * PI);
            let a = 1.0 - g2;
            let b = 1.0 - g2 * w.cos();
            (b - (b * b - a * a).sqrt()) / a
        };
    }

    pub fn process(&mut self, x: f64) -> f64 {
        let p = self.pole;
        self.y1 = (1.0 - p) * x + p * self.y1;
        self.y2 = (1.0 - p) * self.y1 + p * self.y2;
        self.y2
    }

    pub fn reset(&mut self) {
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

/// Deterministic noise of unit variance, approximately Gaussian (a sum of
/// uniform values). The seed is fixed, so the same input always produces
/// exactly the same audio.
#[derive(Clone, Copy, Debug)]
pub struct Noise {
    state: u32,
}

const NOISE_SEED: u32 = 0x2545_F491;

impl Default for Noise {
    fn default() -> Self {
        Noise { state: NOISE_SEED }
    }
}

impl Noise {
    pub fn reset(&mut self) {
        self.state = NOISE_SEED;
    }

    pub fn next_sample(&mut self) -> f64 {
        let mut sum = 0.0;
        for _ in 0..4 {
            sum += self.next_u32() as f64 / u32::MAX as f64;
        }
        // Four uniforms on 0..1 have mean 2 and variance 1/3.
        (sum - 2.0) * 3f64.sqrt()
    }

    /// A uniform value from −1 to 1.
    pub fn next_uniform(&mut self) -> f64 {
        2.0 * (self.next_u32() as f64 / u32::MAX as f64) - 1.0
    }

    /// Marsaglia's xorshift32 generator.
    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::steady_gain;

    const SR: f64 = 22050.0;

    fn run_source(f0: f64, oq: f64, samples: usize) -> Vec<f64> {
        let mut source = GlottalSource::new(SR);
        source.set_targets(f0, oq);
        (0..samples).map(|_| source.next_sample()).collect()
    }

    /// The largest swing of F0 from flutter, in Hz, over two seconds.
    fn flutter_swing(f0: f64) -> f64 {
        let mut source = GlottalSource::new(SR);
        source.set_targets(f0, 0.5);
        source.set_flutter(25.0);
        let mut swing: f64 = 0.0;
        for step in 0..2000 {
            source.samples_elapsed = step * 22;
            source.latch_period();
            swing = swing.max((source.phase_step * SR - f0).abs());
        }
        swing
    }

    #[test]
    fn flutter_stays_small_at_high_pitch() {
        // A share of F0 up to the ceiling, then the same size in Hz.
        let (low, ceiling, high) = (flutter_swing(100.0), flutter_swing(150.0), flutter_swing(400.0));
        assert!(low > 0.5 && low < ceiling, "{low} {ceiling}");
        assert!((high - ceiling).abs() < 0.05, "{high} {ceiling}");
    }

    /// The pitch of each period over two seconds, Hz.
    fn period_pitches(f0: f64, vibrato: f64, steady: f64) -> Vec<f64> {
        let mut source = GlottalSource::new(SR);
        source.set_targets(f0, 0.5);
        source.set_flutter(25.0);
        source.set_singing(vibrato, steady);
        (0..2 * SR as usize)
            .filter_map(|_| {
                source.next_sample();
                (source.phase < source.phase_step).then_some(source.phase_step * SR)
            })
            .collect()
    }

    /// The lowest and highest of a list.
    fn range(pitches: &[f64]) -> (f64, f64) {
        pitches.iter().fold((f64::MAX, f64::MIN), |(low, high), x| (low.min(*x), high.max(*x)))
    }

    #[test]
    fn steady_holds_the_pitch_and_speech_has_flutter() {
        let (low, high) = range(&period_pitches(110.0, 0.0, 1.0));
        assert!(high - low < 1e-9, "{low} {high}");
        let (low, high) = range(&period_pitches(110.0, 0.0, 0.0));
        assert!(high - low > 0.5, "{low} {high}");
    }

    #[test]
    fn vibrato_is_the_same_interval_on_every_note() {
        for f0 in [65.41, 130.81, 261.63, 523.25] {
            let (low, high) = range(&period_pitches(f0, 25.0, 1.0));
            // Up and down from the note by 25 cents, whatever the note.
            let (down, up) = (1200.0 * (f0 / low).log2(), 1200.0 * (high / f0).log2());
            assert!((down - 25.0).abs() < 0.5 && (up - 25.0).abs() < 0.5, "{f0}: {down} {up}");
        }
    }

    #[test]
    fn pulse_rate_matches_f0() {
        for f0 in [87.3, 100.0, 211.7] {
            let wave = run_source(f0, 0.5, SR as usize);
            // Each closure is a single upward jump through −0.5.
            let closures = wave.windows(2).filter(|w| w[0] < -0.5 && w[1] >= -0.5).count();
            assert!((closures as f64 - f0).abs() <= 1.0, "f0 {f0}: {closures} closures");
        }
    }

    /// The size of each closure's upward jump over a second of the source.
    fn closure_steps(creak: f64) -> Vec<f64> {
        let mut source = GlottalSource::new(SR);
        source.set_targets(100.0, 0.5);
        source.set_creak(creak);
        let wave: Vec<f64> = (0..SR as usize).map(|_| source.next_sample()).collect();
        // A closure spreads over neighbouring samples; take the largest jump
        // across two samples in each run of jumps.
        let mut steps = Vec::new();
        let mut run: f64 = 0.0;
        for i in 1..wave.len() - 1 {
            let jump = wave[i + 1] - wave[i - 1];
            if jump > 0.05 {
                run = run.max(jump);
            } else if run > 0.0 {
                steps.push(run);
                run = 0.0;
            }
        }
        steps
    }

    #[test]
    fn creak_lowers_the_pitch_and_makes_pulses_uneven() {
        let (modal, creaky) = (closure_steps(0.0), closure_steps(1.0));
        let spread = |s: &[f64]| s.iter().fold(0.0, |m: f64, &x| m.max(x)) / s.iter().fold(f64::MAX, |m, &x| m.min(x));
        assert!((modal.len() as f64 - 100.0).abs() <= 1.0 && spread(&modal) < 1.2, "{}", modal.len());
        // At full creak the pitch falls to about 40 Hz, and every second
        // pulse is much weaker.
        assert!((creaky.len() as f64 - 40.0).abs() <= 5.0, "{}", creaky.len());
        assert!(spread(&creaky) > 2.0);
    }

    #[test]
    fn source_has_no_dc() {
        // 100 Hz at 22050 Hz is exactly 220.5 samples per period: average over 100 periods.
        let wave = run_source(100.0, 0.6, 22050);
        let mean = wave.iter().sum::<f64>() / wave.len() as f64;
        assert!(mean.abs() < 1e-3, "mean {mean}");
    }

    #[test]
    fn source_is_silent_without_f0() {
        assert!(run_source(0.0, 0.5, 1000).iter().all(|&x| x == 0.0));
    }

    #[test]
    fn source_stops_and_restarts() {
        let mut source = GlottalSource::new(SR);
        source.set_targets(100.0, 0.5);
        for _ in 0..1000 {
            source.next_sample();
        }
        source.set_targets(0.0, 0.5);
        for _ in 0..300 {
            source.next_sample();
        }
        assert!((0..100).all(|_| source.next_sample() == 0.0));
        source.set_targets(100.0, 0.5);
        assert!((0..300).any(|_| source.next_sample() != 0.0));
    }

    #[test]
    fn tilt_attenuates_3khz_by_tl() {
        for tl in [6.0, 12.0, 24.0] {
            let mut tilt = TiltFilter::default();
            tilt.set(tl, SR);
            let gain_db = 20.0 * steady_gain(|x| tilt.process(x), 3000.0, SR).log10();
            assert!((gain_db + tl).abs() < 0.1, "tl {tl}: {gain_db} dB");
        }
    }

    #[test]
    fn noise_has_unit_variance_and_zero_mean() {
        let mut noise = Noise::default();
        let samples: Vec<f64> = (0..200_000).map(|_| noise.next_sample()).collect();
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        let var = samples.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / samples.len() as f64;
        assert!(mean.abs() < 0.01 && (var - 1.0).abs() < 0.02, "mean {mean}, var {var}");
    }
}
