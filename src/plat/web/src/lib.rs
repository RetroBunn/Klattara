//! Klattara as WebAssembly, for KlattPad on the web (`www/`).
//!
//! No bindings library: the page passes text in through memory it gets from
//! [`klattara_alloc`], and the engine hands each piece of audio back as it is made
//! through the one function the page supplies, `env.klattara_audio`, which gets
//! 32-bit float samples and returns 0 to stop. Speech runs in a Web Worker, so the
//! page stays responsive; stopping ends that worker.
//!
//! The page's settings arrive as Klattara's own commands ("[:speed 190][:cv fl 0
//! hs 90]"), applied after the voice is chosen, the way the Mac app and the NVDA
//! add-on send them; a selection then plays after the commands before it.

// voice_settings.rs names the engine crate `engine`, as the Mac and Android libraries do.
extern crate klattara as engine;

use klattara::{Reading, VOICES, Voice, follow_commands, speak_with};
use std::cell::RefCell;

#[path = "../../shared/voice_settings.rs"]
mod voice_settings;

/// The [:cv] settings the page offers, each voice's own value of which
/// `klattara_voice_settings` gives.
const CV_CODES: [&str; 25] = [
    "pi", "in", "fl", "ge", "hs", "rg", "br", "lx", "rc", "oq", "sm", "vvl", "fvl", "avl", "nvl", "fa", "ha", "st", "qk",
    "f4", "b4", "f5", "b5", "f6", "b6",
];

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn klattara_audio(samples: *const f32, count: usize) -> u32;
}

#[cfg(not(target_arch = "wasm32"))]
unsafe fn klattara_audio(_samples: *const f32, _count: usize) -> u32 {
    1
}

thread_local! {
    static MESSAGE: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// What Play reads: the selection if there is one, after the commands before it,
/// or else everything. Positions are in UTF-16 units, as a text box counts them.
pub fn part_to_speak(text: &str, start: usize, end: usize, voice: &mut Voice) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    let (start, end) = (start.min(units.len()), end.min(units.len()));
    if start >= end {
        return text.to_string();
    }
    follow_commands(&String::from_utf16_lossy(&units[..start]), voice);
    String::from_utf16_lossy(&units[start..end])
}

fn set_message(text: &str) {
    MESSAGE.with(|m| {
        let mut m = m.borrow_mut();
        m.clear();
        m.extend_from_slice(text.as_bytes());
    });
}

/// Memory for the page to write text into.
#[unsafe(no_mangle)]
pub extern "C" fn klattara_alloc(size: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(size.max(1));
    let pointer = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    pointer
}

/// # Safety
/// `pointer` and `size` must come from one call to [`klattara_alloc`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_free(pointer: *mut u8, size: usize) {
    if !pointer.is_null() {
        drop(unsafe { Vec::from_raw_parts(pointer, 0, size.max(1)) });
    }
}

/// The last message (the voice list, or what went wrong), as UTF-8.
#[unsafe(no_mangle)]
pub extern "C" fn klattara_message() -> *const u8 {
    MESSAGE.with(|m| m.borrow().as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn klattara_message_length() -> usize {
    MESSAGE.with(|m| m.borrow().len())
}

/// Puts the voices' full names in the message, one per line.
#[unsafe(no_mangle)]
pub extern "C" fn klattara_voices() -> usize {
    let names: Vec<&str> = VOICES.iter().map(|v| v.full_name).collect();
    set_message(&names.join("\n"));
    VOICES.len()
}

/// Puts a voice's own value of every [:cv] setting in the message, as JSON
/// ({"pi":110,...}, null where the engine works it out). Returns 0, or -1 for no
/// such voice.
#[unsafe(no_mangle)]
pub extern "C" fn klattara_voice_settings(voice_index: usize) -> i32 {
    let Some(def) = VOICES.get(voice_index) else { return -1 };
    let voice = Voice::from(def);
    let items: Vec<String> = CV_CODES
        .iter()
        .map(|code| {
            let v = voice_settings::setting_of(&voice, code);
            format!("\"{code}\":{}", if v.is_finite() { format!("{v}") } else { "null".into() })
        })
        .collect();
    set_message(&format!("{{{}}}", items.join(",")));
    0
}

/// Speaks UTF-8 text (or the selection from `start` to `end`, in UTF-16 units),
/// handing the audio to `klattara_audio`. `commands` are the page's settings as
/// Klattara commands, applied after the voice is chosen. Returns 0 when it was all spoken, 1 when
/// the page stopped it, and -1 for a mistake in the text, described in the message.
///
/// # Safety
/// `text` must point to `length` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_speak(
    text: *const u8,
    length: usize,
    start: usize,
    end: usize,
    voice_index: usize,
    commands: *const u8,
    commands_length: usize,
    abbreviations: u32,
    phrase_prediction: u32,
    sample_rate: u32,
) -> i32 {
    let bytes = if length == 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(text, length) } };
    let text = String::from_utf8_lossy(bytes);
    let command_bytes =
        if commands_length == 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(commands, commands_length) } };
    let mut voice = Voice::from(VOICES.get(voice_index).unwrap_or(&VOICES[0]));
    follow_commands(&String::from_utf8_lossy(command_bytes), &mut voice);
    voice.expand_abbreviations = abbreviations != 0;
    voice.phrase_prediction = phrase_prediction != 0;
    let part = part_to_speak(&text, start, end, &mut voice);
    set_message("");
    let result = speak_with(&part, Reading::Text, &mut voice, sample_rate.clamp(8000, 96000), &mut |chunk| {
        unsafe { klattara_audio(chunk.as_ptr(), chunk.len()) != 0 }
    });
    match result {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(error) => {
            set_message(&error.to_string());
            -1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selection_follows_the_commands_before_it() {
        let mut voice = Voice::default();
        let text = "[:phone klattara enable] [AA1]";
        assert_eq!(part_to_speak(text, 25, 30, &mut voice), "[AA1]");
        assert!(voice.phoneme_input);
        assert_eq!(part_to_speak("One two", 3, 3, &mut Voice::default()), "One two");
    }

    #[test]
    fn selections_count_utf16_units() {
        // The emoji is two UTF-16 units, as a browser text box counts it.
        assert_eq!(part_to_speak("😀 hi", 3, 5, &mut Voice::default()), "hi");
    }

    #[test]
    fn lists_the_voices() {
        assert_eq!(klattara_voices(), 5);
        let names = MESSAGE.with(|m| String::from_utf8(m.borrow().clone()).unwrap());
        assert!(names.starts_with("Standard Steve\n"));
    }

    #[test]
    fn gives_each_voices_own_settings() {
        assert_eq!(klattara_voice_settings(2), 0);
        let json = MESSAGE.with(|m| String::from_utf8(m.borrow().clone()).unwrap());
        assert!(json.starts_with("{\"pi\":") && json.contains("\"f4\":"), "{json}");
        assert_eq!(klattara_voice_settings(99), -1);
    }

    #[test]
    fn settings_arrive_as_commands() {
        let mut voice = Voice::from(&VOICES[0]);
        follow_commands("[:speed 400][:cv fl 0 hs 90]", &mut voice);
        assert_eq!((voice.flutter, voice.head_size), (0.0, 90.0));
    }

    #[test]
    fn speaks() {
        let text = "Hello.";
        let cmd = "[:speed 300][:cv fl 0]";
        let r = unsafe { klattara_speak(text.as_ptr(), text.len(), 0, 0, 0, cmd.as_ptr(), cmd.len(), 1, 0, 22050) };
        // Off the web, klattara_audio takes every piece.
        assert_eq!(r, 0);
    }
}
