//! Klattara as WebAssembly, for KlattPad on the web (`www/`).
//!
//! No bindings library: the page passes text in through memory it gets from
//! [`klattara_alloc`], and the engine hands each piece of audio back as it is made
//! through the one function the page supplies, `env.klattara_audio`, which gets
//! 32-bit float samples and returns 0 to stop. Speech runs in a Web Worker, so the
//! page stays responsive; stopping ends that worker.
//!
//! The controls mean what KlattPad's mean on Windows (src/plat/klattpad/src/speech.rs):
//! the Rate slider is the NVDA add-on's, with 15 as normal, and a selection plays
//! after the commands before it.

use klattara::{Reading, VOICES, Voice, follow_commands, speak_with};
use std::cell::RefCell;

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn klattara_audio(samples: *const f32, count: usize) -> u32;
}

#[cfg(not(target_arch = "wasm32"))]
unsafe fn klattara_audio(_samples: *const f32, _count: usize) -> u32 {
    1
}

const MIN_RATE: f64 = 29.0;
const MAX_RATE: f64 = 500.0;

thread_local! {
    static MESSAGE: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// The speaking rate at a Rate slider position, as in KlattPad and the NVDA add-on.
pub fn rate_at(position: u32) -> f64 {
    (f64::from(position.min(100)) / 100.0 * (MAX_RATE - MIN_RATE) + MIN_RATE).round() / 100.0
}

/// The voice the controls have chosen.
pub fn voice_from_controls(voice_index: usize, rate_position: u32, volume_position: u32) -> Voice {
    let mut voice = Voice::from(VOICES.get(voice_index).unwrap_or(&VOICES[0]));
    voice.rate = rate_at(rate_position);
    voice.volume = f64::from(volume_position.min(100)) / 100.0;
    voice
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

/// Speaks UTF-8 text (or the selection from `start` to `end`, in UTF-16 units),
/// handing the audio to `klattara_audio`. Returns 0 when it was all spoken, 1 when
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
    rate_position: u32,
    volume_position: u32,
    sample_rate: u32,
) -> i32 {
    let bytes = if length == 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(text, length) } };
    let text = String::from_utf8_lossy(bytes);
    let mut voice = voice_from_controls(voice_index, rate_position, volume_position);
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
    fn the_rate_slider_matches_klattpad() {
        assert_eq!(rate_at(15), 1.0);
        assert_eq!(rate_at(0), 0.29);
        assert_eq!(rate_at(100), 5.0);
    }

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
    fn speaks() {
        let text = "Hello.";
        let r = unsafe { klattara_speak(text.as_ptr(), text.len(), 0, 0, 0, 15, 100, 22050) };
        // Off the web, klattara_audio takes every piece.
        assert_eq!(r, 0);
    }
}
