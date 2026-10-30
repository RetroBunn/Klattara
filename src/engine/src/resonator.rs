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
    pub fn set(&mut self, freq: f64, bw: f64, sample_rate: f64) {
        if !in_range(freq, sample_rate) {
            (self.a, self.b, self.c) = (1.0, 0.0, 0.0);
            return;
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
