//! Singing: sounds with a written length and pitch, as in DECtalk.
//!
//! Any sound in phonetic input may be followed by `<milliseconds,pitch>`, by
//! `<milliseconds>` or by `<,pitch>`: `[D AA1<400,20>]`. A pitch of 1 to 39 is a
//! note from [`NOTES`], sung with vibrato. A pitch of 40 to 550 is in Hz, with no
//! vibrato. `_<milliseconds>` is a rest.
//!
//! A phrase with a written pitch follows only its written pitches. It has no
//! accents and no falling end, and a sound with no pitch of its own holds the one
//! before it. Moving to a note takes [`NOTE_GLIDE_MS`], and moving to a pitch in Hz
//! takes the whole of its sound. `[:vibspeed]`, `[:vibdepth]` and `[:vibdelay]` change
//! the vibrato. Written lengths stay exact at any speaking rate,
//! so a song keeps its tempo. DECtalk's do too: from rate 90 to rate 600, Paul's
//! 800 ms note stayed within 30 ms while a sentence went from 4168 ms to 679 ms.

/// The highest pitch that is a note rather than Hz.
const LAST_NOTE: u32 = 39;
/// The highest pitch that can be written: the engine's highest, in Hz.
const HIGHEST_PITCH: u32 = 550;
/// The longest sound that can be written, ms.
const LONGEST_SOUND: u32 = 60_000;

/// How long moving to a note takes, ms.
pub const NOTE_GLIDE_MS: f64 = 100.0;
/// Vibrato on notes, as it starts out: its speed in Hz, DECtalk's, and how far the pitch
/// swings either way, in cents. DECtalk's swing is a fixed number of Hz, which is strong
/// on low notes and faint on high ones: 54 cents at C2 and 7 at C5. This is the same
/// interval on every note, about what DECtalk's comes to around C3. Its wave is a sine
/// followed on every sample, where DECtalk's moves in 25 steps to the cycle.
pub const VIBRATO_SPEED: f64 = 6.2;
pub const VIBRATO_DEPTH: f64 = 25.0;
/// How long a note holds still before its vibrato starts, as it starts out, ms.
pub const VIBRATO_DELAY: f64 = 0.0;
/// How long vibrato takes to fade in, ms, so that it never starts with a jolt.
pub const VIBRATO_FADE: f64 = 120.0;
/// The written symbol for a rest.
pub const REST: &str = "_";

/// Notes 1 to 39 in Hz, tuned to A = 440. Notes 1 to 37 climb by semitones from C2
/// to C5, and 38 and 39 stay on C5.
const NOTES: [f64; LAST_NOTE as usize] = [
    65.41, 69.30, 73.42, 77.78, 82.41, 87.31, 92.50, 98.00, 103.83, 110.00, 116.54, 123.47, // C2 to B2
    130.81, 138.59, 146.83, 155.56, 164.81, 174.61, 185.00, 196.00, 207.65, 220.00, 233.08, 246.94, // C3 to B3
    261.63, 277.18, 293.66, 311.13, 329.63, 349.23, 369.99, 392.00, 415.30, 440.00, 466.16, 493.88, // C4 to B4
    523.25, 523.25, 523.25, // C5
];

/// The length and pitch written after a sound, as written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Written {
    /// Milliseconds.
    pub duration: Option<u32>,
    /// A note from 1 to 39, or Hz from 40 to 550.
    pub pitch: Option<u32>,
}

/// Whether a written pitch is a note, which has vibrato, rather than Hz.
pub fn is_note(pitch: u32) -> bool {
    pitch <= LAST_NOTE
}

/// A written pitch in Hz.
pub fn pitch_hz(pitch: u32) -> f64 {
    if is_note(pitch) { NOTES[pitch as usize - 1] } else { f64::from(pitch) }
}

/// Splits the inside of a phonetic word into its sounds, at spaces outside `<>`.
pub fn split_sounds(body: &str) -> Vec<&str> {
    let (mut sounds, mut start, mut inside) = (Vec::new(), None, false);
    for (at, c) in body.char_indices() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ => {}
        }
        if c.is_whitespace() && !inside {
            if let Some(from) = start.take() {
                sounds.push(&body[from..at]);
            }
        } else if start.is_none() {
            start = Some(at);
        }
    }
    if let Some(from) = start {
        sounds.push(&body[from..]);
    }
    sounds
}

/// Splits a sound as written into its symbol and its length and pitch, or gives
/// `None` if they are not valid.
pub fn split_written(sound: &str) -> Option<(&str, Written)> {
    let Some((symbol, rest)) = sound.split_once('<') else { return Some((sound, Written::default())) };
    let (duration, pitch) = rest.strip_suffix('>')?.split_once(',').unwrap_or((rest.strip_suffix('>')?, ""));
    // As in DECtalk, a missing number and a zero both mean none.
    let number = |text: &str| -> Option<u32> {
        let text = text.trim();
        if text.is_empty() { Some(0) } else { text.parse().ok() }
    };
    let (duration, pitch) = (number(duration)?, number(pitch)?);
    if duration > LONGEST_SOUND || pitch > HIGHEST_PITCH {
        return None;
    }
    Some((symbol, Written { duration: (duration > 0).then_some(duration), pitch: (pitch > 0).then_some(pitch) }))
}

/// A length and pitch as they are written after a sound, or nothing if there are none.
pub fn write(written: Written) -> String {
    match (written.duration, written.pitch) {
        (None, None) => String::new(),
        (Some(duration), None) => format!("<{duration}>"),
        (None, Some(pitch)) => format!("<,{pitch}>"),
        (Some(duration), Some(pitch)) => format!("<{duration},{pitch}>"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_run_from_c2_to_c5_by_semitones() {
        assert_eq!(pitch_hz(1), 65.41);
        assert_eq!(pitch_hz(13), 130.81);
        assert_eq!(pitch_hz(25), 261.63);
        assert_eq!(pitch_hz(34), 440.00);
        assert_eq!(pitch_hz(37), 523.25);
        for note in 1..=37u32 {
            let tuned = 440.0 * 2f64.powf((f64::from(note) - 34.0) / 12.0);
            assert!((pitch_hz(note) - tuned).abs() < 0.01, "note {note}: {}", pitch_hz(note));
        }
    }

    #[test]
    fn the_last_notes_stay_on_c5_and_40_starts_hz() {
        assert_eq!((pitch_hz(38), pitch_hz(39)), (523.25, 523.25));
        assert!(is_note(1) && is_note(39) && !is_note(40));
        assert_eq!((pitch_hz(40), pitch_hz(550)), (40.0, 550.0));
    }

    #[test]
    fn sounds_split_at_spaces_outside_the_marks() {
        assert_eq!(split_sounds(" D  AA1<400, 20> _<100>\tT "), ["D", "AA1<400, 20>", "_<100>", "T"]);
        assert!(split_sounds("  ").is_empty());
    }

    #[test]
    fn lengths_and_pitches_are_read_as_written() {
        let read = |sound| split_written(sound).map(|(symbol, w)| (symbol.to_string(), w.duration, w.pitch));
        assert_eq!(read("AA1"), Some(("AA1".to_string(), None, None)));
        assert_eq!(read("AA1<400>"), Some(("AA1".to_string(), Some(400), None)));
        assert_eq!(read("AA1<,20>"), Some(("AA1".to_string(), None, Some(20))));
        assert_eq!(read("AA1<400,20>"), Some(("AA1".to_string(), Some(400), Some(20))));
        assert_eq!(read("AA1<400, 20>"), Some(("AA1".to_string(), Some(400), Some(20))));
        assert_eq!(read("AA1<0,0>"), Some(("AA1".to_string(), None, None)));
        for bad in ["AA1<400", "AA1<x>", "AA1<400,x>", "AA1<400,551>", "AA1<60001>", "AA1<-1>", "AA1<1,2,3>"] {
            assert_eq!(read(bad), None, "{bad}");
        }
    }

    #[test]
    fn written_marks_read_back_the_same() {
        for written in [(None, None), (Some(400), None), (None, Some(20)), (Some(400), Some(20))] {
            let written = Written { duration: written.0, pitch: written.1 };
            assert_eq!(split_written(&format!("AA1{}", write(written))), Some(("AA1", written)));
        }
    }
}
