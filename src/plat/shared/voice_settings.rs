//! Each voice's own value of every [:cv] setting, in the command's units:
//! shared by the macOS and Android libraries, included with #[path].

use engine::{Gender, Voice};

// The engine's own constants for rc and sm (commands.rs): rc runs the open
// quotient from the least rich to the most rich, and sm the tilt from none to
// the smoothest. The tests below check them against [:cv] itself.
const LEAST_RICH_OPEN_QUOTIENT: f64 = 0.9;
const MOST_RICH_OPEN_QUOTIENT: f64 = 0.2;
const SMOOTHEST_TILT: f64 = 25.0;

pub fn setting_of(voice: &Voice, code: &str) -> f64 {
    let optional = |value: Option<f64>| value.unwrap_or(f64::NAN);
    match code {
        "pi" => voice.pitch,
        "in" => voice.inflection * 100.0,
        "hs" => voice.head_size,
        "rg" => voice.roughness,
        "br" => voice.breathiness,
        "rc" => {
            (voice.open_quotient - LEAST_RICH_OPEN_QUOTIENT) / (MOST_RICH_OPEN_QUOTIENT - LEAST_RICH_OPEN_QUOTIENT)
                * 100.0
        }
        "sm" => voice.tilt / SMOOTHEST_TILT * 100.0,
        "ge" => match voice.gender {
            Gender::Female => 0.0,
            Gender::Male => 1.0,
        },
        "f4" => optional(voice.upper_formants[0]),
        "f5" => optional(voice.upper_formants[1]),
        "f6" => optional(voice.upper_formants[2]),
        "b4" => optional(voice.upper_bandwidths[0]),
        "b5" => optional(voice.upper_bandwidths[1]),
        "b6" => optional(voice.upper_bandwidths[2]),
        "fl" => voice.flutter,
        "vvl" => voice.gains.voicing * 100.0,
        "fvl" => voice.gains.frication * 100.0,
        "avl" => voice.gains.aspiration * 100.0,
        "nvl" => voice.gains.nasal * 100.0,
        "fa" => voice.pitch_shape.baseline_fall,
        "ha" => voice.pitch_shape.hat_rise,
        "st" => voice.pitch_shape.stress_rise,
        "qk" => voice.pitch_shape.quickness,
        "lx" => voice.lax_breathiness,
        "oq" => voice.open_quotient * 100.0,
        "vibspeed" => voice.vibrato_speed,
        "vibdepth" => voice.vibrato_depth,
        "vibdelay" => voice.vibrato_delay,
        _ => f64::NAN,
    }
}

