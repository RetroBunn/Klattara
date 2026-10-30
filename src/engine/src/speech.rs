//! Speech from text or phonetic input: the pipeline to audio.

use crate::frame::Frame;
use crate::intonation::pitch_keyframes;
use crate::phoneme::Phoneme;
use crate::synth::{Synth, VoiceSettings};
use crate::text;
use crate::timing::assign_durations;
use crate::tracks;
use crate::utterance::{ParseError, Phrase, Segment, notation, parse, segments};

/// The adjustable character of a voice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Voice {
    /// Base pitch, Hz.
    pub pitch: f64,
    /// Scales all pitch movement: 0 is monotone, 1 normal, 2.5 the most, as
    /// DECtalk's pitch range goes from 0 to 250%.
    pub inflection: f64,
    /// Speaking rate: 1.0 is about 200 words per minute including pauses;
    /// 2.0 is twice as fast. Valid from 0.25 to 8.
    pub rate: f64,
    /// Glottal open quotient: lower sounds brighter and buzzier.
    pub open_quotient: f64,
    /// Spectral tilt of the voice source, dB at 3 kHz: higher sounds softer.
    pub tilt: f64,
    /// Slow pitch wobble, as in [`VoiceSettings::flutter`].
    pub flutter: f64,
    pub volume: f64,
    /// Whether abbreviations in text are read in full ("Dr." as "doctor",
    /// "Calif." as "California"), or as written ("D R").
    pub expand_abbreviations: bool,
    /// Whether long stretches of text without punctuation are broken into
    /// phrases, each with its own pitch contour and no pause, like
    /// Eloquence's phrase prediction. Off by default, as screen readers
    /// usually have it.
    pub phrase_prediction: bool,
}

impl Default for Voice {
    /// The default voice: an adult male pitch, with the medium open
    /// quotient, between the DECtalk-like buzz and a soft, breathy voice, and
    /// no extra spectral tilt. With a tilt of 6 dB the voice had 3 to 5 dB
    /// less energy from 1 to 4 kHz than Eloquence or DECtalk, and sounded
    /// breathy at fast rates; the user chose this brighter tone by ear.
    fn default() -> Self {
        Voice {
            pitch: 110.0,
            inflection: 1.0,
            rate: 1.0,
            open_quotient: 0.55,
            tilt: 0.0,
            flutter: 25.0,
            volume: 1.0,
            expand_abbreviations: true,
            phrase_prediction: false,
        }
    }
}

/// Times speech for the voice's rate: segments timed at the normal rate, and
/// the factor that compresses that timeline to the rate.
///
/// The rate sets the overall pace: the total length is the normal-rate
/// speech divided by the rate, plus pauses divided by the rate and its
/// square root, so pauses shrink a little faster than speech. Within that
/// length, speech is compressed evenly. Even compression keeps every timing
/// relation within and between sounds, which makes fast synthetic speech
/// more intelligible than imitating how people speed up, where vowels shrink
/// far more than consonants (Valentini-Botinhao et al. 2014; Janse 2004).
/// Our own Whisper tests agree: at about 350 words per minute, 68% of words
/// were recognized this way against 59% with Klatt's rate rules, and 36%
/// against 30% at about 470.
///
/// Above the normal rate, pauses shrink by the rate once more, and the
/// words get the time saved. At the same overall pace (about 480 words per
/// minute), that raised recognition from 39% to 43%, and the user preferred
/// it by ear. Pitch still marks every boundary.
fn timeline(phrases: &[Phrase], rate: f64) -> (Vec<Segment>, f64) {
    let mut segments = segments(phrases);
    let pause_scale = if rate > 1.0 { rate.powf(-1.5) } else { 1.0 / rate.sqrt() };
    assign_durations(&mut segments, pause_scale);
    let (mut speech, mut pauses) = (0.0, 0.0);
    for seg in &segments {
        if seg.phoneme == Phoneme::Pause { pauses += seg.duration } else { speech += seg.duration }
    }
    let length = (speech + pauses / pause_scale / rate.sqrt()) / rate;
    let compression = if length > 0.0 { (speech + pauses) / length } else { rate };
    (segments, compression)
}

/// The synthesizer settings for a voice. Flutter, the slow wobble that keeps
/// the pitch from sounding mechanical, follows inflection up to normal: with
/// inflection 0 the voice is a true monotone.
fn synth_settings(voice: &Voice) -> VoiceSettings {
    VoiceSettings { flutter: voice.flutter * voice.inflection.clamp(0.0, 1.0), volume: voice.volume }
}

/// Plans speech at the voice's rate.
fn plan(phrases: &[Phrase], voice: &Voice) -> Vec<(f64, Frame)> {
    let (segments, compression) = timeline(phrases, voice.rate.clamp(0.25, 8.0));
    let pitch = pitch_keyframes(&segments, voice.pitch, voice.inflection);
    let frames = tracks::frames(&segments, &pitch, voice.open_quotient, voice.tilt);
    frames.into_iter().map(|(t, frame)| (t / compression, frame)).collect()
}

fn render(frames: &[(f64, Frame)], voice: &Voice, sample_rate: u32) -> Vec<f32> {
    let mut synth = Synth::new(sample_rate);
    synth.set_settings(synth_settings(voice));
    tracks::render(&mut synth, frames)
}

/// Plans phonetic input: synthesizer frames with their times in ms.
pub fn phoneme_frames(input: &str, voice: &Voice) -> Result<Vec<(f64, Frame)>, ParseError> {
    Ok(plan(&parse(input)?, voice))
}

/// Speaks bracketed ARPAbet input, such as `[HH AH0 L OW1] [W ER1 L D].`,
/// returning samples between −1 and 1.
pub fn speak_phonemes(input: &str, voice: &Voice, sample_rate: u32) -> Result<Vec<f32>, ParseError> {
    Ok(render(&phoneme_frames(input, voice)?, voice, sample_rate))
}

/// Plans ordinary text: synthesizer frames with their times in ms.
pub fn text_frames(input: &str, voice: &Voice) -> Vec<(f64, Frame)> {
    plan(&text::phrases_with(input, voice.expand_abbreviations, voice.phrase_prediction), voice)
}

/// Speaks ordinary text, returning samples between −1 and 1.
pub fn speak_text(input: &str, voice: &Voice, sample_rate: u32) -> Vec<f32> {
    render(&text_frames(input, voice), voice, sample_rate)
}

/// How input is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    /// Ordinary text.
    Text,
    /// Character by character: letters by name, digits as numbers.
    Spelled,
    /// Bracketed ARPAbet notation.
    Phonemes,
}

/// Speaks input, handing audio (samples between −1 and 1) to `on_audio`
/// in chunks as it is made. Returns `Ok(false)` if `on_audio` asked to stop
/// by returning false, and an error only for invalid phonetic input.
pub fn speak_with(
    input: &str,
    reading: Reading,
    voice: &Voice,
    sample_rate: u32,
    on_audio: &mut dyn FnMut(&[f32]) -> bool,
) -> Result<bool, ParseError> {
    let phrases = match reading {
        Reading::Text => text::phrases_with(input, voice.expand_abbreviations, voice.phrase_prediction),
        Reading::Spelled => text::spelled_phrases(input),
        Reading::Phonemes => parse(input)?,
    };
    let frames = plan(&phrases, voice);
    let mut synth = Synth::new(sample_rate);
    synth.set_settings(synth_settings(voice));
    Ok(tracks::render_with(&mut synth, &frames, on_audio))
}

/// How text will be pronounced, in the bracketed ARPAbet notation that
/// [`speak_phonemes`] accepts.
pub fn transcribe(input: &str) -> String {
    transcribe_with(input, &Voice::default())
}

/// How text will be pronounced by `voice`, which decides whether
/// abbreviations are read in full and whether long stretches are broken
/// into phrases, shown as `|`.
pub fn transcribe_with(input: &str, voice: &Voice) -> String {
    notation(&text::phrases_with(input, voice.expand_abbreviations, voice.phrase_prediction))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotone_has_no_flutter() {
        let flutter = |inflection| synth_settings(&Voice { inflection, ..Voice::default() }).flutter;
        assert_eq!(flutter(0.0), 0.0);
        assert_eq!(flutter(1.0), Voice::default().flutter);
        assert_eq!(flutter(2.5), Voice::default().flutter);
    }

    #[test]
    fn rate_compresses_the_timeline() {
        let text = "The quick brown fox jumps over the lazy dog, again and again.";
        let length = |rate: f64| text_frames(text, &Voice { rate, ..Voice::default() }).last().unwrap().0;
        let (normal, double) = (length(1.0), length(2.0));
        // Speech halves; pauses shrink a little more.
        assert!(double < 0.5 * normal && double > 0.4 * normal, "{normal} {double}");
    }

    #[test]
    fn fast_speech_gives_pause_time_to_the_words() {
        let phrases = text::phrases_with("One, two, three. Four, five, six.", true, false);
        let parts = |rate: f64| {
            let (segments, compression) = timeline(&phrases, rate);
            let (pauses, speech): (Vec<&Segment>, Vec<&Segment>) =
                segments.iter().partition(|s| s.phoneme == Phoneme::Pause);
            let total = |list: Vec<&Segment>| list.iter().map(|s| s.duration).sum::<f64>() / compression;
            (total(speech), total(pauses))
        };
        let ((speech, pauses), (fast_speech, fast_pauses)) = (parts(1.0), parts(2.0));
        // The overall pace is unchanged: speech plus pauses over the rate
        // and its square root.
        let expected = speech / 2.0 + pauses / 2.0 / 2f64.sqrt();
        assert!((fast_speech + fast_pauses - expected).abs() < 1e-6);
        // Pauses shrink much more than that, and the words are slower.
        assert!(fast_pauses < 0.25 * pauses && fast_speech > 0.5 * speech, "{fast_pauses} {fast_speech}");
    }
}
