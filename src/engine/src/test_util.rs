//! Measurement helpers shared by the unit tests.

use std::f64::consts::PI;

/// The steady-state gain of a filter at `freq`: feeds it a unit sine wave
/// until the start-up transient has died away, then measures the output peak.
pub fn steady_gain(mut filter: impl FnMut(f64) -> f64, freq: f64, sample_rate: f64) -> f64 {
    let w = 2.0 * PI * freq / sample_rate;
    let mut peak: f64 = 0.0;
    for n in 0..20000 {
        let y = filter((w * n as f64).sin());
        if n >= 10000 {
            peak = peak.max(y.abs());
        }
    }
    peak
}

/// Spectral magnitude of `signal` at `freq` (Goertzel algorithm), normalized
/// so that a sine of amplitude 1 at exactly `freq` measures about 1.
pub fn magnitude_at(signal: &[f32], freq: f64, sample_rate: f64) -> f64 {
    let coeff = 2.0 * (2.0 * PI * freq / sample_rate).cos();
    let (mut s1, mut s2) = (0.0, 0.0);
    for &x in signal {
        let s0 = x as f64 + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    let power = s1 * s1 + s2 * s2 - coeff * s1 * s2;
    2.0 * power.max(0.0).sqrt() / signal.len() as f64
}
