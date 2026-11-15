//! Second-order digital resonators and antiresonators (Klatt 1980).

use std::f64::consts::PI;

/// A digital resonator (spectral pole pair):
/// y\[n\] = a·x\[n\] + b·y\[n−1\] + c·y\[n−2\].
#[derive(Clone, Copy, Debug)]
pub struct Resonator {
    a: f64,
    b: f64,
    c: f64,
    y1: f64,
    y2: f64,
}

impl Default for Resonator {
    /// A pass-through resonator.
    fn default() -> Self {
        Resonator { a: 1.0, b: 0.0, c: 0.0, y1: 0.0, y2: 0.0 }
    }
}

impl Resonator {
    /// Tunes the resonator for unity gain at 0 Hz, as in Klatt's cascade
    /// branch. Chained this way, resonators reproduce natural relative formant
    /// amplitudes on their own. A frequency outside the usable range turns the
    /// resonator into a pass-through.
    ///
    /// A cascade resonator's peak gain grows as its pole angle approaches
    /// Nyquist, so at low sample rates formants near the top of the range blow
    /// up: a steady /iy/ peaks at 3.4 at 8000 Hz against 0.24 at 22050 Hz, and
    /// the soft limiter in the output stage is driven to the rail. Widen the
    /// bandwidth until the peak gain is back to what the same formant has at
    /// 22050 Hz, the rate the engine was tuned at; at and above that rate the
    /// peak is already at or below its reference, so the sound there is
    /// unchanged. Widening keeps the unity DC gain, so the resonator still
    /// passes lower frequencies through untouched.
    pub fn set(&mut self, freq: f64, bw: f64, sample_rate: f64) {
        if !in_range(freq, sample_rate) {
            (self.a, self.b, self.c) = (1.0, 0.0, 0.0);
            return;
        }
        let mut bw = bw;
        if in_range(freq, REFERENCE_SAMPLE_RATE) {
            let peak_ref = peak_gain_for(freq, bw, REFERENCE_SAMPLE_RATE);
            if peak_gain_for(freq, bw, sample_rate) > peak_ref {
                let mut lo = bw;
                let mut hi = 20000.0;
                for _ in 0..40 {
                    let mid = (lo + hi) / 2.0;
                    if peak_gain_for(freq, mid, sample_rate) > peak_ref {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                bw = hi;
            }
        }
        let (b, c) = pole_coefficients(freq, bw, sample_rate);
        (self.a, self.b, self.c) = (1.0 - b - c, b, c);
    }

    /// Tunes the resonator for unity gain at its center frequency, for the
    /// parallel branch, so that its amplitude control sets the height of the
    /// spectral peak directly. A frequency outside the usable range silences it.
    pub fn set_peak(&mut self, freq: f64, bw: f64, sample_rate: f64) {
        if !in_range(freq, sample_rate) {
            (self.a, self.b, self.c) = (0.0, 0.0, 0.0);
            return;
        }
        let (b, c) = pole_coefficients(freq, bw, sample_rate);
        // |1 − b·e^(−jw) − c·e^(−2jw)| at the center frequency.
        let w = 2.0 * PI * freq / sample_rate;
        let re = 1.0 - b * w.cos() - c * (2.0 * w).cos();
        let im = b * w.sin() + c * (2.0 * w).sin();
        (self.a, self.b, self.c) = (re.hypot(im), b, c);
    }

    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.a * x + self.b * self.y1 + self.c * self.y2;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    pub fn reset(&mut self) {
        self.y1 = 0.0;
        self.y2 = 0.0;
    }

    /// The squared magnitude response |H(e^jw)|^2 at a frequency, for the
    /// resonator's current tuning. Used to compare how much noise the
    /// cascade passes at different sample rates (see `Synth`).
    pub fn response_sq(&self, freq: f64, sample_rate: f64) -> f64 {
        let w = 2.0 * PI * freq / sample_rate;
        let (sin_w, cos_w) = w.sin_cos();
        let (sin_2w, cos_2w) = (2.0 * w).sin_cos();
        let re = 1.0 - self.b * cos_w - self.c * cos_2w;
        let im = self.b * sin_w + self.c * sin_2w;
        self.a * self.a / (re * re + im * im)
    }
}

/// A digital antiresonator (spectral zero pair), the exact inverse of a
/// resonator tuned with [`Resonator::set`]:
/// y\[n\] = a·x\[n\] + b·x\[n−1\] + c·x\[n−2\].
/// Used for the nasal zero.
#[derive(Clone, Copy, Debug)]
pub struct AntiResonator {
    a: f64,
    b: f64,
    c: f64,
    x1: f64,
    x2: f64,
}

impl Default for AntiResonator {
    /// A pass-through antiresonator.
    fn default() -> Self {
        AntiResonator { a: 1.0, b: 0.0, c: 0.0, x1: 0.0, x2: 0.0 }
    }
}

impl AntiResonator {
    /// Tunes the antiresonator for unity gain at 0 Hz. A frequency outside the
    /// usable range turns it into a pass-through.
    pub fn set(&mut self, freq: f64, bw: f64, sample_rate: f64) {
        if !in_range(freq, sample_rate) {
            (self.a, self.b, self.c) = (1.0, 0.0, 0.0);
            return;
        }
        let (b, c) = pole_coefficients(freq, bw, sample_rate);
        let a = 1.0 - b - c;
        (self.a, self.b, self.c) = (1.0 / a, -b / a, -c / a);
    }

    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.a * x + self.b * self.x1 + self.c * self.x2;
        self.x2 = self.x1;
        self.x1 = x;
        y
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
    }
}

/// The feedback coefficients (b, c) of a pole pair at `freq` with bandwidth `bw`.
fn pole_coefficients(freq: f64, bw: f64, sample_rate: f64) -> (f64, f64) {
    let r = (-PI * bw.max(1.0) / sample_rate).exp();
    (2.0 * r * (2.0 * PI * freq / sample_rate).cos(), -r * r)
}

/// The sample rate the engine was tuned at: no resonator's peak gain may
/// exceed what the same formant has there.
const REFERENCE_SAMPLE_RATE: f64 = 22050.0;

/// The peak gain of a cascade resonator at its own center frequency:
/// |H(e^{jw})| with w = 2*pi*freq/sample_rate and r from the bandwidth.
/// At the pole angle the denominator is (1 - r) * sqrt((1 + r)^2 - 4r cos^2 w).
fn peak_gain_for(freq: f64, bw: f64, sample_rate: f64) -> f64 {
    let (b, c) = pole_coefficients(freq, bw, sample_rate);
    let a = 1.0 - b - c;
    let r = (-PI * bw.max(1.0) / sample_rate).exp();
    let w = 2.0 * PI * freq / sample_rate;
    let denom = (1.0 - r) * ((1.0 + r) * (1.0 + r) - 4.0 * r * w.cos() * w.cos()).sqrt();
    a / denom
}

/// The squared magnitude response |H(e^jw)|^2 at `eval` of a resonator
/// tuned for `freq` with bandwidth `bw` at `sample_rate`, without widening.
/// The reference the cascade's noise scale is measured against.
pub fn response_sq_for(freq: f64, bw: f64, sample_rate: f64, eval: f64) -> f64 {
    if !in_range(freq, sample_rate) {
        return 1.0;
    }
    let (b, c) = pole_coefficients(freq, bw, sample_rate);
    let a = 1.0 - b - c;
    let w = 2.0 * PI * eval / sample_rate;
    let (sin_w, cos_w) = w.sin_cos();
    let (sin_2w, cos_2w) = (2.0 * w).sin_cos();
    let re = 1.0 - b * cos_w - c * cos_2w;
    let im = b * sin_w + c * sin_2w;
    a * a / (re * re + im * im)
}

/// Whether a resonance at `freq` can be realized: above 0 Hz and safely
/// below the Nyquist frequency.
fn in_range(freq: f64, sample_rate: f64) -> bool {
    freq > 0.0 && freq < 0.475 * sample_rate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::steady_gain;

    const SR: f64 = 22050.0;

    #[test]
    fn cascade_resonator_has_unity_dc_gain() {
        let mut r = Resonator::default();
        r.set(700.0, 80.0, SR);
        let mut y = 0.0;
        for _ in 0..20000 {
            y = r.process(1.0);
        }
        assert!((y - 1.0).abs() < 1e-9, "DC gain {y}");
    }

    #[test]
    fn cascade_resonator_peaks_at_its_frequency() {
        let mut r = Resonator::default();
        r.set(1000.0, 100.0, SR);
        let at_peak = steady_gain(|x| r.process(x), 1000.0, SR);
        let below = steady_gain(|x| r.process(x), 500.0, SR);
        let above = steady_gain(|x| r.process(x), 2000.0, SR);
        assert!(at_peak > 4.0 * below && at_peak > 4.0 * above);
    }

    #[test]
    fn parallel_resonator_has_unity_peak_gain() {
        let mut r = Resonator::default();
        r.set_peak(2500.0, 300.0, SR);
        let gain = steady_gain(|x| r.process(x), 2500.0, SR);
        assert!((gain - 1.0).abs() < 0.02, "peak gain {gain}");
    }

    #[test]
    fn antiresonator_notches_its_frequency() {
        let mut z = AntiResonator::default();
        z.set(1200.0, 60.0, SR);
        let at_zero = steady_gain(|x| z.process(x), 1200.0, SR);
        let far = steady_gain(|x| z.process(x), 300.0, SR);
        assert!(at_zero < 0.05 && far > 0.5, "zero {at_zero}, far {far}");
    }

    /// A formant near Nyquist used to blow up at low sample rates: at 8000 Hz
    /// a 3750 Hz resonator's peak gain was six times its 22050 Hz value, and
    /// /iy/ peaked at 3.4 before the soft limiter. Widening the bandwidth caps
    /// the peak gain at its tuned-rate value.
    #[test]
    fn peak_gain_near_nyquist_stays_at_its_tuned_rate_value() {
        let freq = 3750.0;
        let bw = 200.0;
        let mut low = Resonator::default();
        low.set(freq, bw, 8000.0);
        let mut high = Resonator::default();
        high.set(freq, bw, SR);
        let gain_low = steady_gain(|x| low.process(x), freq, 8000.0);
        let gain_high = steady_gain(|x| high.process(x), freq, SR);
        assert!(
            gain_low <= gain_high * 1.05,
            "peak gain at 8000 Hz is {gain_low}, at 22050 Hz {gain_high}"
        );
    }

    #[test]
    fn matching_pole_and_zero_cancel() {
        let mut p = Resonator::default();
        let mut z = AntiResonator::default();
        p.set(250.0, 100.0, SR);
        z.set(250.0, 100.0, SR);
        for n in 0..5000 {
            let x = ((n * 7919) % 101) as f64 / 50.0 - 1.0;
            let y = p.process(z.process(x));
            assert!((y - x).abs() < 1e-9);
        }
    }

    #[test]
    fn out_of_range_frequencies_are_safe() {
        let mut cascade = Resonator::default();
        cascade.set(15000.0, 100.0, SR);
        assert_eq!(cascade.process(0.5), 0.5);
        let mut parallel = Resonator::default();
        parallel.set_peak(15000.0, 100.0, SR);
        assert_eq!(parallel.process(0.5), 0.0);
    }
}
