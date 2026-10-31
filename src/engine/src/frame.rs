//! Acoustic parameter frames.

/// Number of formants in the cascade and parallel branches.
pub const NUM_FORMANTS: usize = 6;

/// A snapshot of the synthesizer's control parameters.
///
/// Frequencies and bandwidths are in Hz. Amplitudes are in dB, where 0 means
/// off and about 60 is normal speaking level. The synthesizer moves linearly
/// from one frame to the next, so a sequence of frames describes continuous
/// parameter tracks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// Fundamental frequency (pitch), Hz. 0 stops the glottal source.
    pub f0: f64,
    /// Amplitude of voicing, dB.
    pub av: f64,
    /// Open quotient: the fraction of each glottal period during which the
    /// vocal folds are open, 0.2 to 0.9. Higher values sound softer.
    pub oq: f64,
    /// Spectral tilt of the voicing source: extra attenuation at 3 kHz, dB.
    pub tl: f64,
    /// Creaky voice, 0 (normal) to 1: lower, irregular pitch with uneven
    /// pulses, as before a glottalized /t/.
    pub creak: f64,
    /// Vibrato: how far the pitch swings either way, in cents. 0 is none. How fast it
    /// swings is in [`crate::VoiceSettings::vibrato_speed`].
    pub vibrato: f64,
    /// 1 holds the pitch free of flutter, as a sung note is; 0 is ordinary speech.
    pub steady: f64,
    /// Amplitude of aspiration noise (breath, as in /h/), dB. Excites the
    /// cascade branch, so it takes on the formants of the current vowel.
    pub ah: f64,
    /// Amplitude of frication noise, dB. Excites the parallel branch.
    pub af: f64,
    /// Formant frequencies F1 to F6, Hz. Shared by both branches.
    pub freq: [f64; NUM_FORMANTS],
    /// Cascade formant bandwidths B1 to B6, Hz.
    pub bw: [f64; NUM_FORMANTS],
    /// Nasal pole frequency, Hz.
    pub fnp: f64,
    /// Nasal pole bandwidth, Hz.
    pub bnp: f64,
    /// Nasal zero frequency, Hz. Set equal to the nasal pole to switch
    /// nasality off; the two then cancel exactly.
    pub fnz: f64,
    /// Nasal zero bandwidth, Hz.
    pub bnz: f64,
    /// Parallel formant amplitudes A1 to A6, dB, applied to frication noise.
    /// Each sets the height of its spectral peak directly.
    pub par_amp: [f64; NUM_FORMANTS],
    /// Parallel formant bandwidths, Hz.
    pub par_bw: [f64; NUM_FORMANTS],
    /// Amplitude of the parallel bypass path, which passes frication noise
    /// with a flat spectrum (as in /f/ and /th/), dB.
    pub ab: f64,
}

impl Default for Frame {
    /// Silence, with a neutral vowel configuration (the KLSYN defaults).
    fn default() -> Self {
        Frame {
            f0: 0.0,
            av: 0.0,
            oq: 0.5,
            tl: 0.0,
            creak: 0.0,
            vibrato: 0.0,
            steady: 0.0,
            ah: 0.0,
            af: 0.0,
            freq: [500.0, 1500.0, 2500.0, 3300.0, 3750.0, 4900.0],
            bw: [60.0, 90.0, 150.0, 250.0, 200.0, 1000.0],
            fnp: 250.0,
            bnp: 100.0,
            fnz: 250.0,
            bnz: 100.0,
            par_amp: [0.0; NUM_FORMANTS],
            par_bw: [80.0, 200.0, 350.0, 500.0, 600.0, 800.0],
            ab: 0.0,
        }
    }
}

impl Frame {
    /// The frame a fraction `t` (0 to 1) of the way from `self` to `other`.
    ///
    /// Pitch never glides from or to 0: when either end is unvoiced, the
    /// other end's F0 is used, so voicing starts and stops at a steady pitch.
    pub fn lerp(&self, other: &Frame, t: f64) -> Frame {
        let mix = |a: f64, b: f64| a + (b - a) * t;
        let mix_all = |a: &[f64; NUM_FORMANTS], b: &[f64; NUM_FORMANTS]| std::array::from_fn(|k| mix(a[k], b[k]));
        let f0 = if self.f0 <= 0.0 {
            other.f0
        } else if other.f0 <= 0.0 {
            self.f0
        } else {
            mix(self.f0, other.f0)
        };
        Frame {
            f0,
            av: mix(self.av, other.av),
            oq: mix(self.oq, other.oq),
            tl: mix(self.tl, other.tl),
            creak: mix(self.creak, other.creak),
            vibrato: mix(self.vibrato, other.vibrato),
            steady: mix(self.steady, other.steady),
            ah: mix(self.ah, other.ah),
            af: mix(self.af, other.af),
            freq: mix_all(&self.freq, &other.freq),
            bw: mix_all(&self.bw, &other.bw),
            fnp: mix(self.fnp, other.fnp),
            bnp: mix(self.bnp, other.bnp),
            fnz: mix(self.fnz, other.fnz),
            bnz: mix(self.bnz, other.bnz),
            par_amp: mix_all(&self.par_amp, &other.par_amp),
            par_bw: mix_all(&self.par_bw, &other.par_bw),
            ab: mix(self.ab, other.ab),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_reaches_both_ends() {
        let a = Frame { f0: 100.0, av: 0.0, ..Frame::default() };
        let b = Frame { f0: 200.0, av: 60.0, ..Frame::default() };
        assert_eq!(a.lerp(&b, 0.0), a);
        assert_eq!(a.lerp(&b, 1.0), b);
        let mid = a.lerp(&b, 0.5);
        assert_eq!(mid.f0, 150.0);
        assert_eq!(mid.av, 30.0);
    }

    #[test]
    fn pitch_does_not_glide_from_zero() {
        let silent = Frame::default();
        let voiced = Frame { f0: 120.0, ..Frame::default() };
        assert_eq!(silent.lerp(&voiced, 0.1).f0, 120.0);
        assert_eq!(voiced.lerp(&silent, 0.9).f0, 120.0);
    }
}
