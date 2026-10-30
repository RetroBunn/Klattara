//! Speech from text or phonetic input: the pipeline to audio.

use crate::frame::Frame;
use crate::intonation::pitch_keyframes;
use crate::phoneme::Phoneme;
use crate::synth::{Synth, VoiceSettings};
use crate::text;
use crate::timing::assign_durations;
use crate::tracks;
use crate::utterance::{Boundary, ParseError, Phrase, Segment, notation, parse, segments};

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

/// Text longer than this many bytes is read piece by piece.
const LONGEST_TEXT: usize = 16 * 1024;

/// Cuts very long text into pieces to read one at a time, so that memory and
/// the wait for speech to start, during which it cannot be cancelled, stay
/// small however much text there is. A piece ends after a sentence if it can,
/// or else at a space.
fn pieces(text: &str) -> Vec<&str> {
    let (mut pieces, mut rest) = (Vec::new(), text);
    while rest.len() > LONGEST_TEXT {
        let mut end = LONGEST_TEXT;
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        let head = &rest[..end];
        let cut = head.rfind(". ").map(|at| at + 1).or_else(|| head.rfind(char::is_whitespace)).filter(|&at| at > 0);
        let (piece, tail) = rest.split_at(cut.unwrap_or(end));
        pieces.push(piece);
        rest = tail;
    }
    pieces.push(rest);
    pieces
}

/// No phrase runs longer than this many words: text with no punctuation gets
/// a comma's pause here, as a reader would need a breath.
const LONGEST_PHRASE: usize = 200;
/// Long input is planned and rendered in batches of about this many words.
const BATCH_WORDS: usize = 500;

/// Groups phrases into batches to plan and render one at a time, so that
/// speech starts at once and memory stays small however long the input is.
/// As one plan, 200,000 words without punctuation needed a single block of
/// 888 MB, and failing to get one in a 32-bit process ends the whole program.
/// Each batch ends in a pause, so the next can start afresh, and input shorter
/// than a batch stays as it is.
fn batches(phrases: Vec<Phrase>) -> Vec<Vec<Phrase>> {
    let (mut batches, mut batch, mut words) = (Vec::new(), Vec::new(), 0);
    for phrase in phrases {
        let boundary = phrase.boundary;
        let mut left = phrase.words.len();
        let mut rest = phrase.words.into_iter();
        loop {
            let piece: Vec<_> = rest.by_ref().take(LONGEST_PHRASE).collect();
            left -= piece.len();
            words += piece.len();
            let full = words >= BATCH_WORDS;
            let boundary = if left > 0 || (full && !boundary.has_pause()) { Boundary::Comma } else { boundary };
            batch.push(Phrase { words: piece, boundary });
            if full {
                batches.push(std::mem::take(&mut batch));
                words = 0;
            }
            if left == 0 {
                break;
            }
        }
    }
    if !batch.is_empty() || batches.is_empty() {
        batches.push(batch);
    }
    batches
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
    let mut synth = Synth::new(sample_rate);
    synth.set_settings(synth_settings(voice));
    // Phonetic input can't be cut anywhere: its words are in brackets.
    let parts = if reading == Reading::Phonemes { vec![input] } else { pieces(input) };
    for part in parts {
        let phrases = match reading {
            Reading::Text => text::phrases_with(part, voice.expand_abbreviations, voice.phrase_prediction),
            Reading::Spelled => text::spelled_phrases(part),
            Reading::Phonemes => parse(part)?,
        };
        for batch in batches(phrases) {
            if !tracks::render_with(&mut synth, &plan(&batch, voice), on_audio) {
                return Ok(false);
            }
        }
    }
    Ok(true)
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
    fn very_long_text_is_read_in_pieces() {
        // Ordinary text, and no text at all, stay as they are.
        assert_eq!(pieces(""), [""]);
        assert_eq!(pieces("One. Two."), ["One. Two."]);
        // After a sentence if it can; nothing is lost.
        let text = "One more sentence here. ".repeat(3000);
        let parts = pieces(&text);
        assert!(parts.len() > 1 && parts.iter().all(|p| p.len() <= LONGEST_TEXT));
        assert!(parts[..parts.len() - 1].iter().all(|p| p.ends_with('.')));
        assert_eq!(parts.concat(), text);
        // Else at a space, or anywhere between characters if there is none.
        let text = "word ".repeat(10000);
        assert!(pieces(&text)[1].starts_with(" word"));
        let text = "\u{e9}".repeat(40000);
        let parts = pieces(&text);
        assert!(parts.len() > 1);
        assert_eq!(parts.concat(), text);
    }

    #[test]
    fn long_unbroken_text_starts_quickly() {
        // Pronouncing one word this long used to take minutes.
        let text = "TWFuIGlzIGRpc3Rpbmd1aXNoZWQ".repeat(10000);
        let mut chunks = 0;
        let done = speak_with(&text, Reading::Text, &Voice::default(), 22050, &mut |_| {
            chunks += 1;
            false
        });
        assert_eq!((done, chunks), (Ok(false), 1));
    }

    #[test]
    fn ordinary_speech_is_one_batch() {
        let text = "One, two, three. Four, five, six.";
        let phrases = text::phrases_with(text, true, false);
        assert_eq!(batches(phrases.clone()), [phrases]);
        // Streaming makes exactly the audio of rendering it whole.
        let voice = Voice::default();
        let mut streamed = Vec::new();
        let done = speak_with(text, Reading::Text, &voice, 22050, &mut |chunk| {
            streamed.extend_from_slice(chunk);
            true
        });
        assert_eq!(done, Ok(true));
        assert_eq!(streamed, speak_text(text, &voice, 22050));
    }

    #[test]
    fn long_input_is_planned_in_batches_that_end_in_pauses() {
        let check = |text: &str, prediction: bool, total: usize| {
            let parts = batches(text::phrases_with(text, true, prediction));
            assert!(parts.len() >= total / (BATCH_WORDS + LONGEST_PHRASE), "{}", parts.len());
            let mut seen = 0;
            for batch in &parts {
                let words: usize = batch.iter().map(|p| p.words.len()).sum();
                seen += words;
                assert!(words < BATCH_WORDS + LONGEST_PHRASE, "{words}");
                assert!(batch.iter().all(|p| p.words.len() <= LONGEST_PHRASE));
                assert!(batch.last().unwrap().boundary.has_pause());
            }
            assert_eq!(seen, total);
        };
        // No punctuation at all, with and without phrase prediction, whose
        // phrases have no pauses of their own.
        check(&"word ".repeat(5000), false, 5000);
        check(&"the old man walked down the road toward the town ".repeat(500), true, 5000);
        // The last piece keeps the sentence's own ending.
        let parts = batches(text::phrases_with(&format!("{}word?", "word ".repeat(999)), true, false));
        assert_eq!(parts.last().unwrap().last().unwrap().boundary, Boundary::Question);
    }

    #[test]
    fn long_input_starts_at_once_and_can_stop() {
        let (mut chunks, text) = (0, "word ".repeat(5000));
        let done = speak_with(&text, Reading::Text, &Voice::default(), 22050, &mut |_| {
            chunks += 1;
            false
        });
        assert_eq!((done, chunks), (Ok(false), 1));
        // Left to run, it says every batch.
        let mut samples = 0;
        let done = speak_with(&"word ".repeat(1200), Reading::Text, &Voice::default(), 22050, &mut |chunk| {
            samples += chunk.len();
            true
        });
        assert_eq!(done, Ok(true));
        assert!(samples > 1200 * 22050 / 5, "{samples}");
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
