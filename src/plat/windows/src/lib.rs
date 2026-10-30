//! Klattara as a standalone library with a C interface (klattara.dll), for
//! any program that wants to speak with it directly. See klattara.h.
//!
//! Every entry point catches internal errors, so a bug in the engine
//! returns an error code instead of bringing down the host program, which
//! for a screen reader would mean silence.
//!
//! An engine may be used from any thread, but from one thread at a time.

use engine::{Reading, Voice, speak_with, transcribe};
use std::ffi::{CStr, c_char, c_int, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Success.
pub const KLATTARA_OK: c_int = 0;
/// Speaking stopped early because the audio callback asked it to.
pub const KLATTARA_STOPPED: c_int = 1;
/// A null pointer, a value out of range, or invalid phonetic input.
pub const KLATTARA_INVALID: c_int = -1;
/// An internal error. Please report it.
pub const KLATTARA_INTERNAL_ERROR: c_int = -2;

/// Read the input as ordinary text.
pub const KLATTARA_READ_TEXT: c_int = 0;
/// Spell the input character by character.
pub const KLATTARA_READ_SPELLED: c_int = 1;
/// Read the input as bracketed ARPAbet, such as "[HH AH0 L OW1]".
pub const KLATTARA_READ_PHONEMES: c_int = 2;

/// Receives audio: `count` signed 16-bit mono samples. Returns 0 to go on,
/// or anything else to stop speaking at once.
pub type AudioCallback = Option<unsafe extern "C" fn(user: *mut c_void, samples: *const i16, count: c_int) -> c_int>;

/// A speech engine: a sample rate and the voice settings.
pub struct KlattaraEngine {
    sample_rate: u32,
    voice: Voice,
}

/// Runs `body`, turning a panic into `fallback`.
fn guard<T>(fallback: T, body: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or(fallback)
}

/// The library version from Cargo.toml, such as "1.0.0", ending in a null.
const VERSION: &CStr = match CStr::from_bytes_with_nul(concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes()) {
    Ok(version) => version,
    Err(_) => panic!("the version contains a null"),
};

/// The library version, as a constant string such as "1.0.0".
#[unsafe(no_mangle)]
pub extern "C" fn klattara_version() -> *const c_char {
    VERSION.as_ptr()
}

/// Creates an engine producing audio at `sample_rate` Hz (8000 to 96000;
/// 22050 is recommended). Returns null if the rate is out of range.
#[unsafe(no_mangle)]
pub extern "C" fn klattara_create(sample_rate: c_int) -> *mut KlattaraEngine {
    guard(std::ptr::null_mut(), || {
        if !(8000..=96000).contains(&sample_rate) {
            return std::ptr::null_mut();
        }
        Box::into_raw(Box::new(KlattaraEngine { sample_rate: sample_rate as u32, voice: Voice::default() }))
    })
}

/// Frees an engine. Null is ignored.
///
/// # Safety
/// `engine` must come from [`klattara_create`] and not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_destroy(engine: *mut KlattaraEngine) {
    if !engine.is_null() {
        // SAFETY: the caller passes a pointer from klattara_create, once.
        guard((), || drop(unsafe { Box::from_raw(engine) }));
    }
}

/// Applies `change` to the engine's voice if `value` is in `range`.
///
/// # Safety
/// `engine` must be null or a live engine from [`klattara_create`].
unsafe fn set(
    engine: *mut KlattaraEngine,
    value: f64,
    range: std::ops::RangeInclusive<f64>,
    change: fn(&mut Voice, f64),
) -> c_int {
    // SAFETY: the caller passes null or a live engine.
    let Some(engine) = (unsafe { engine.as_mut() }) else { return KLATTARA_INVALID };
    if !range.contains(&value) {
        return KLATTARA_INVALID;
    }
    guard(KLATTARA_INTERNAL_ERROR, || {
        change(&mut engine.voice, value);
        KLATTARA_OK
    })
}

/// Sets the speaking rate: 1.0 is about 200 words per minute, 2.0 twice as
/// fast. Valid from 0.25 to 8.
///
/// # Safety
/// `engine` must be null or a live engine from [`klattara_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_set_rate(engine: *mut KlattaraEngine, rate: f64) -> c_int {
    unsafe { set(engine, rate, 0.25..=8.0, |voice, value| voice.rate = value) }
}

/// Sets the base pitch in Hz, from 40 to 600. The default is 110.
///
/// # Safety
/// `engine` must be null or a live engine from [`klattara_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_set_pitch(engine: *mut KlattaraEngine, hz: f64) -> c_int {
    unsafe { set(engine, hz, 40.0..=600.0, |voice, value| voice.pitch = value) }
}

/// Sets how much the pitch moves: 0 is monotone, 1 normal, up to 2.5.
///
/// # Safety
/// `engine` must be null or a live engine from [`klattara_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_set_inflection(engine: *mut KlattaraEngine, inflection: f64) -> c_int {
    unsafe { set(engine, inflection, 0.0..=2.5, |voice, value| voice.inflection = value) }
}

/// Sets the volume: 0 is silent, 1 normal, up to 2.
///
/// # Safety
/// `engine` must be null or a live engine from [`klattara_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_set_volume(engine: *mut KlattaraEngine, volume: f64) -> c_int {
    unsafe { set(engine, volume, 0.0..=2.0, |voice, value| voice.volume = value) }
}

/// Sets whether abbreviations in text are read in full: nonzero (the
/// default) reads "Dr." as "doctor" and "Calif." as "California"; zero reads
/// them as written, "D R".
///
/// # Safety
/// `engine` must be null or a live engine from [`klattara_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_set_abbreviations(engine: *mut KlattaraEngine, expand: c_int) -> c_int {
    // SAFETY: the caller passes null or a live engine.
    let Some(engine) = (unsafe { engine.as_mut() }) else { return KLATTARA_INVALID };
    engine.voice.expand_abbreviations = expand != 0;
    KLATTARA_OK
}

/// Sets whether long stretches of text without punctuation are broken into
/// phrases, each with its own pitch contour and no pause, like Eloquence's
/// phrase prediction: nonzero turns it on; zero, the default, turns it off.
///
/// # Safety
/// `engine` must be null or a live engine from [`klattara_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_set_phrase_prediction(engine: *mut KlattaraEngine, predict: c_int) -> c_int {
    // SAFETY: the caller passes null or a live engine.
    let Some(engine) = (unsafe { engine.as_mut() }) else { return KLATTARA_INVALID };
    engine.voice.phrase_prediction = predict != 0;
    KLATTARA_OK
}

/// Speaks UTF-8 `input`, passing the audio to `callback` in chunks as it is
/// made, with `user` passed back unchanged. `reading` is one of the
/// KLATTARA_READ_ constants. Returns KLATTARA_OK when finished,
/// KLATTARA_STOPPED if the callback stopped it, or an error.
///
/// # Safety
/// `engine` must be null or a live engine from [`klattara_create`], `input`
/// null or a NUL-terminated string, and `callback` must be safe to call
/// with `user`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_speak(
    engine: *mut KlattaraEngine,
    input: *const c_char,
    reading: c_int,
    callback: AudioCallback,
    user: *mut c_void,
) -> c_int {
    // SAFETY: the caller passes null or a live engine.
    let Some(engine) = (unsafe { engine.as_ref() }) else { return KLATTARA_INVALID };
    let Some(callback) = callback else { return KLATTARA_INVALID };
    if input.is_null() {
        return KLATTARA_INVALID;
    }
    let reading = match reading {
        KLATTARA_READ_TEXT => Reading::Text,
        KLATTARA_READ_SPELLED => Reading::Spelled,
        KLATTARA_READ_PHONEMES => Reading::Phonemes,
        _ => return KLATTARA_INVALID,
    };
    // SAFETY: the caller passes a NUL-terminated string.
    let input = unsafe { CStr::from_ptr(input) }.to_string_lossy();
    guard(KLATTARA_INTERNAL_ERROR, || {
        let mut samples: Vec<i16> = Vec::new();
        let mut on_audio = |chunk: &[f32]| {
            samples.clear();
            samples.extend(chunk.iter().map(|&x| (x.clamp(-1.0, 1.0) * 32767.0).round() as i16));
            // SAFETY: the caller guarantees the callback is safe with `user`.
            unsafe { callback(user, samples.as_ptr(), samples.len() as c_int) == 0 }
        };
        match speak_with(&input, reading, &engine.voice, engine.sample_rate, &mut on_audio) {
            Ok(true) => KLATTARA_OK,
            Ok(false) => KLATTARA_STOPPED,
            Err(_) => KLATTARA_INVALID,
        }
    })
}

/// Writes how UTF-8 `text` will be pronounced, in bracketed ARPAbet, into
/// `buffer` as a NUL-terminated string if it fits in `size` bytes. Returns
/// the size needed including the NUL, or KLATTARA_INVALID.
///
/// # Safety
/// `text` must be null or NUL-terminated, and `buffer` null or writable for
/// `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_transcribe(text: *const c_char, buffer: *mut c_char, size: c_int) -> c_int {
    if text.is_null() {
        return KLATTARA_INVALID;
    }
    // SAFETY: the caller passes a NUL-terminated string.
    let text = unsafe { CStr::from_ptr(text) }.to_string_lossy();
    guard(KLATTARA_INTERNAL_ERROR, || {
        let result = transcribe(&text);
        let needed = result.len() + 1;
        if !buffer.is_null() && size >= 0 && needed <= size as usize {
            // SAFETY: the caller guarantees `size` writable bytes, and the
            // result plus its NUL fits.
            unsafe {
                std::ptr::copy_nonoverlapping(result.as_ptr(), buffer.cast::<u8>(), result.len());
                *buffer.add(result.len()) = 0;
            }
        }
        c_int::try_from(needed).unwrap_or(KLATTARA_INVALID)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn collect(user: *mut c_void, samples: *const i16, count: c_int) -> c_int {
        let out = unsafe { &mut *user.cast::<Vec<i16>>() };
        out.extend_from_slice(unsafe { std::slice::from_raw_parts(samples, count as usize) });
        0
    }

    unsafe extern "C" fn stop_at_once(_: *mut c_void, _: *const i16, _: c_int) -> c_int {
        1
    }

    #[test]
    fn reports_its_version() {
        let version = unsafe { CStr::from_ptr(klattara_version()) };
        assert_eq!(version.to_str(), Ok("1.0.0"));
    }

    #[test]
    fn speaks_through_the_c_interface() {
        let engine = klattara_create(22050);
        assert!(!engine.is_null());
        let mut audio: Vec<i16> = Vec::new();
        let user = (&mut audio as *mut Vec<i16>).cast::<c_void>();
        unsafe {
            assert_eq!(klattara_set_rate(engine, 2.0), KLATTARA_OK);
            assert_eq!(klattara_set_rate(engine, 100.0), KLATTARA_INVALID);
            assert_eq!(
                klattara_speak(engine, c"Hello, world.".as_ptr(), KLATTARA_READ_TEXT, Some(collect), user),
                KLATTARA_OK
            );
            assert!(audio.len() > 5000 && audio.iter().any(|&s| s.abs() > 3000));
            let result = klattara_speak(engine, c"Hello.".as_ptr(), KLATTARA_READ_TEXT, Some(stop_at_once), user);
            assert_eq!(result, KLATTARA_STOPPED);
            assert_eq!(
                klattara_speak(engine, c"[XX]".as_ptr(), KLATTARA_READ_PHONEMES, Some(collect), user),
                KLATTARA_INVALID
            );
            klattara_destroy(engine);
        }
    }

    #[test]
    fn abbreviations_can_be_read_as_written() {
        // "Inc." read in full ("incorporated") is longer than as written ("ink").
        let length = |expand: c_int| {
            let engine = klattara_create(22050);
            let mut audio: Vec<i16> = Vec::new();
            let user = (&mut audio as *mut Vec<i16>).cast::<c_void>();
            unsafe {
                assert_eq!(klattara_set_abbreviations(engine, expand), KLATTARA_OK);
                klattara_speak(engine, c"Apple Inc. is here.".as_ptr(), KLATTARA_READ_TEXT, Some(collect), user);
                klattara_destroy(engine);
            }
            audio.len()
        };
        assert!(length(1) > length(0) + 5000, "{} {}", length(1), length(0));
        assert_eq!(unsafe { klattara_set_abbreviations(std::ptr::null_mut(), 1) }, KLATTARA_INVALID);
    }

    #[test]
    fn phrase_prediction_can_be_turned_on() {
        // A break lengthens the syllable before it, so the sentence gets a
        // little longer.
        let length = |predict: c_int| {
            let engine = klattara_create(22050);
            let mut audio: Vec<i16> = Vec::new();
            let user = (&mut audio as *mut Vec<i16>).cast::<c_void>();
            let text = c"The old man walked slowly down the long dusty road toward the small town where he lived.";
            unsafe {
                assert_eq!(klattara_set_phrase_prediction(engine, predict), KLATTARA_OK);
                klattara_speak(engine, text.as_ptr(), KLATTARA_READ_TEXT, Some(collect), user);
                klattara_destroy(engine);
            }
            audio.len()
        };
        assert!(length(1) > length(0) + 500, "{} {}", length(1), length(0));
        assert_eq!(unsafe { klattara_set_phrase_prediction(std::ptr::null_mut(), 1) }, KLATTARA_INVALID);
    }

    #[test]
    fn rejects_bad_arguments() {
        assert!(klattara_create(100).is_null());
        unsafe {
            let null = std::ptr::null_mut();
            assert_eq!(klattara_set_pitch(null, 110.0), KLATTARA_INVALID);
            assert_eq!(klattara_speak(null, c"x".as_ptr(), 0, Some(collect), null.cast()), KLATTARA_INVALID);
            klattara_destroy(null);
        }
    }

    #[test]
    fn pitch_goes_from_40_to_600_hz() {
        let engine = klattara_create(22050);
        unsafe {
            assert_eq!(klattara_set_pitch(engine, 40.0), KLATTARA_OK);
            assert_eq!(klattara_set_pitch(engine, 600.0), KLATTARA_OK);
            assert_eq!(klattara_set_pitch(engine, 39.0), KLATTARA_INVALID);
            assert_eq!(klattara_set_pitch(engine, 601.0), KLATTARA_INVALID);
            klattara_destroy(engine);
        }
    }

    #[test]
    fn transcribes_into_a_buffer() {
        let mut buffer = [0 as c_char; 64];
        let needed = unsafe { klattara_transcribe(c"hello".as_ptr(), buffer.as_mut_ptr(), 64) };
        let text = unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_str().unwrap();
        assert_eq!(text, "[HH AH0 L OW1].");
        assert_eq!(needed as usize, text.len() + 1);
        assert_eq!(unsafe { klattara_transcribe(c"hello".as_ptr(), std::ptr::null_mut(), 0) }, needed);
    }
}
