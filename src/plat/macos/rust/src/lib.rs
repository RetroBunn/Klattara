//! Klattara's C interface for macOS: the same functions as klattara.dll,
//! declared in src/plat/windows/klattara.h, built as a static library that
//! the speech synthesis extension links. The interface is compiled from the
//! Windows layer's own source, unchanged, so the two cannot drift apart.
//!
//! One function is added for the Mac app, declared in klattara_mac.h:
//! [`klattara_mac_voice_setting`], which tells it each voice's own value of
//! every [:cv] setting, so it can show where a voice starts.

#[path = "../../../windows/src/lib.rs"]
mod ffi;

pub use ffi::*;

use engine::{VOICES, Voice};
use std::ffi::{CStr, c_char, c_int};

#[path = "../../../shared/voice_settings.rs"]
mod voice_settings;
use voice_settings::setting_of;

/// A voice's own value of a [:cv] setting, in the units the command takes
/// ("in" in percent, "oq" in percent, "ge" 0 female or 1 male, and so on).
/// NaN when the voice leaves it to the engine (F4 to F6 and their bandwidths
/// follow from head size and gender unless set), or for an unknown voice or
/// code.
///
/// # Safety
/// `code` must be null or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_mac_voice_setting(index: c_int, code: *const c_char) -> f64 {
    if code.is_null() || index < 0 {
        return f64::NAN;
    }
    // SAFETY: the caller passes a NUL-terminated string.
    let code = unsafe { CStr::from_ptr(code) }.to_string_lossy();
    let Some(def) = VOICES.get(index as usize) else { return f64::NAN };
    setting_of(&Voice::from(def), &code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::follow_commands;

    const CODES: [&str; 25] = [
        "pi", "in", "hs", "rg", "br", "rc", "sm", "ge", "f4", "f5", "f6", "b4", "b5", "b6", "fl", "vvl", "fvl",
        "avl", "nvl", "fa", "ha", "st", "qk", "lx", "oq",
    ];

    /// Setting a value with [:cv] and reading it back gives the value: the
    /// readings are in the command's own units, for every setting.
    #[test]
    fn readings_are_in_the_commands_units() {
        let samples = [
            ("pi", 123.0), ("in", 150.0), ("hs", 90.0), ("rg", 30.0), ("br", 40.0), ("rc", 70.0), ("sm", 60.0),
            ("ge", 0.0), ("f4", 3300.0), ("f5", 4000.0), ("f6", 5000.0), ("b4", 250.0), ("b5", 300.0),
            ("b6", 1000.0), ("fl", 40.0), ("vvl", 80.0), ("fvl", 120.0), ("avl", 60.0), ("nvl", 150.0),
            ("fa", 30.0), ("ha", 40.0), ("st", 50.0), ("qk", 70.0), ("lx", 20.0), ("oq", 45.0),
        ];
        assert_eq!(samples.len(), CODES.len());
        for (code, value) in samples {
            let mut voice = Voice::from(&VOICES[0]);
            follow_commands(&format!("[:cv {code} {value}]"), &mut voice);
            let read = setting_of(&voice, code);
            assert!((read - value).abs() < 1e-6, "{code}: set {value}, read {read}");
        }
    }

    /// Every voice has a reading for every setting, except the upper formants
    /// and bandwidths a voice may leave to the engine.
    #[test]
    fn every_voice_has_a_reading_for_every_setting() {
        for index in 0..VOICES.len() as c_int {
            for code in CODES {
                let c = std::ffi::CString::new(code).unwrap();
                // SAFETY: a NUL-terminated string.
                let value = unsafe { klattara_mac_voice_setting(index, c.as_ptr()) };
                let may_be_automatic = matches!(code, "f4" | "f5" | "f6" | "b4" | "b5" | "b6");
                assert!(value.is_finite() || may_be_automatic, "voice {index}, {code}: {value}");
            }
        }
    }
}
