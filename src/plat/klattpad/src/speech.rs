//! What the controls mean to the engine, and speaking to a WAV file.

use klattara::{Reading, VOICES, Voice, follow_commands, speak_with, wav};
use std::path::Path;

pub const SAMPLE_RATE: u32 = 22050;

/// The Rate slider is the same as the NVDA add-on's, so its numbers mean the same
/// here: hundredths of normal speed, spread evenly over positions 0 to 100.
const MIN_RATE: f64 = 29.0;
const MAX_RATE: f64 = 500.0;
/// The Rate slider's normal position.
pub const NORMAL_RATE_POSITION: u32 = 15;
/// The Volume slider's starting position, full volume.
pub const FULL_VOLUME_POSITION: u32 = 100;

/// The speaking rate at a Rate slider position.
fn rate_at(position: u32) -> f64 {
    (f64::from(position) / 100.0 * (MAX_RATE - MIN_RATE) + MIN_RATE).round() / 100.0
}

/// The voice the controls have chosen.
pub fn voice_from_controls(voice_index: usize, rate_position: u32, volume_position: u32) -> Voice {
    let mut voice = Voice::from(VOICES.get(voice_index).unwrap_or(&VOICES[0]));
    voice.rate = rate_at(rate_position);
    voice.volume = f64::from(volume_position) / 100.0;
    voice
}

/// What Play reads: the selection if there is one, or else all the text. A selection
/// follows the commands before it, so a line of a song plays as the song does.
pub fn part_to_speak(text: &[u16], selection: (usize, usize), voice: &mut Voice) -> String {
    let (start, end) = (selection.0.min(text.len()), selection.1.min(text.len()));
    if start >= end {
        return String::from_utf16_lossy(text);
    }
    follow_commands(&String::from_utf16_lossy(&text[..start]), voice);
    String::from_utf16_lossy(&text[start..end])
}

/// Speaks text to a WAV file.
pub fn export_wav(text: &str, voice: &mut Voice, path: &Path) -> std::io::Result<()> {
    let mut audio = Vec::new();
    speak_with(text, Reading::Text, voice, SAMPLE_RATE, &mut |chunk| {
        audio.extend_from_slice(chunk);
        true
    })
    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error.to_string()))?;
    wav::write_wav(path, &audio, SAMPLE_RATE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    #[test]
    fn the_rate_slider_matches_the_nvda_add_on() {
        assert_eq!(rate_at(NORMAL_RATE_POSITION), 1.0);
        assert_eq!(rate_at(0), 0.29);
        assert_eq!(rate_at(100), 5.0);
    }

    #[test]
    fn the_controls_choose_the_voice() {
        let voice = voice_from_controls(1, NORMAL_RATE_POSITION, 50);
        assert_eq!((voice.pitch, voice.rate, voice.volume), (210.0, 1.0, 0.5));
    }

    #[test]
    fn at_the_starting_positions_the_voice_is_as_defined() {
        assert_eq!(voice_from_controls(0, NORMAL_RATE_POSITION, FULL_VOLUME_POSITION), Voice::default());
    }

    #[test]
    fn an_unknown_voice_is_the_default() {
        assert_eq!(voice_from_controls(99, NORMAL_RATE_POSITION, FULL_VOLUME_POSITION), Voice::default());
    }

    #[test]
    fn play_reads_the_selection_or_else_everything() {
        let text = utf16("One two three.");
        let mut voice = Voice::default();
        assert_eq!(part_to_speak(&text, (4, 7), &mut voice), "two");
        assert_eq!(part_to_speak(&text, (5, 5), &mut voice), "One two three.");
        assert_eq!(part_to_speak(&text, (0, 0), &mut voice), "One two three.");
        // A selection reaching past the end stops at the end.
        assert_eq!(part_to_speak(&text, (8, 500), &mut voice), "three.");
    }

    #[test]
    fn a_selection_follows_the_commands_before_it() {
        let text = utf16("[:phone klattara enable] [AA1<400,20>] [AA1<400,22>]");
        let start = text.len() - "[AA1<400,22>]".len();
        let mut voice = Voice::default();
        assert_eq!(part_to_speak(&text, (start, text.len()), &mut voice), "[AA1<400,22>]");
        assert!(voice.phoneme_input);
        // Nothing before the selection turned it on.
        let mut voice = Voice::default();
        part_to_speak(&utf16("Hello [AA1]"), (6, 11), &mut voice);
        assert!(!voice.phoneme_input);
    }

    #[test]
    fn exports_the_same_audio_as_speaking() {
        let path = std::env::temp_dir().join(format!("klattpad-test-{}.wav", std::process::id()));
        let voice = Voice::default();
        export_wav("Hello, world.", &mut voice.clone(), &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        let spoken = klattara::speak_text("Hello, world.", &voice, SAMPLE_RATE);
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(bytes.len(), 44 + spoken.len() * 2);
    }
}
