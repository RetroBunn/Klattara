//! Speech from text or phonetic input: the pipeline to audio.

use crate::commands;
use crate::frame::Frame;
use crate::intonation::pitch_keyframes;
use crate::phoneme::Phoneme;
use crate::synth::{Synth, VoiceSettings};
use crate::text;
use crate::timing::assign_durations;
use crate::tracks;
use crate::utterance::{Boundary, ParseError, Phrase, Segment, notation, parse, segments};
use crate::voicedefs::{Gender, VOICES, VoiceDef};

/// The adjustable character of a voice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Voice {
    /// Which model of the vocal tract the voice uses.
    pub gender: Gender,
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
    /// Breath noise heard through voicing, as in [`VoiceDef::breathiness`].
    pub breathiness: f64,
    /// Slow pitch wobble, as in [`VoiceSettings::flutter`].
    pub flutter: f64,
    /// Output gain that brings the voice to the level of the others, as in
    /// [`VoiceDef::level`]. The volume adds to this.
    pub level: f64,
    pub volume: f64,
    /// Whether abbreviations in text are read in full ("Dr." as "doctor",
    /// "Calif." as "California"), or as written ("D R").
    pub expand_abbreviations: bool,
    /// Whether long stretches of text without punctuation are broken into
    /// phrases, each with its own pitch contour and no pause, like
    /// Eloquence's phrase prediction. Off by default, as screen readers
    /// usually have it.
    pub phrase_prediction: bool,
    /// Whether a bracket of valid ARPAbet in text is spoken as a word. Off by
    /// default, and set by commands in the text: see [`crate::commands`].
    pub phoneme_input: bool,
}

impl Default for Voice {
    /// The first voice in [`VOICES`], with everything else at normal.
    fn default() -> Self {
        let first = &VOICES[0];
        Voice {
            gender: first.gender,
            pitch: first.pitch,
            inflection: first.inflection,
            rate: 1.0,
            open_quotient: first.open_quotient,
            tilt: first.tilt,
            breathiness: first.breathiness,
            flutter: first.flutter,
            level: first.level,
            volume: 1.0,
            expand_abbreviations: true,
            phrase_prediction: false,
            phoneme_input: false,
        }
    }
}

impl From<&VoiceDef> for Voice {
    /// The voice as it is defined, with everything else at normal.
    fn from(def: &VoiceDef) -> Self {
        let mut voice = Voice::default();
        voice.take_on(def);
        voice
    }
}

impl Voice {
    /// Becomes the voice `def`: its gender, pitch, inflection and quality. Rate,
    /// volume and how text is read stay as they are.
    pub fn take_on(&mut self, def: &VoiceDef) {
        self.gender = def.gender;
        self.pitch = def.pitch;
        self.inflection = def.inflection;
        self.open_quotient = def.open_quotient;
        self.tilt = def.tilt;
        self.breathiness = def.breathiness;
        self.flutter = def.flutter;
        self.level = def.level;
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
        // Written lengths stay as written, so they play no part in the compression.
        if seg.written.duration.is_some() {
            continue;
        }
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
    VoiceSettings { flutter: voice.flutter * voice.inflection.clamp(0.0, 1.0), volume: voice.volume * voice.level }
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
    let frames = tracks::frames(&segments, &pitch, voice);
    if segments.iter().all(|s| s.written.duration.is_none()) {
        return frames.into_iter().map(|(t, frame)| (t / compression, frame)).collect();
    }
    // A sound with a written length takes exactly that long, and the rest is
    // compressed around it.
    let scale = |seg: &Segment| if seg.written.duration.is_some() { 1.0 } else { compression };
    let (mut at, mut real_start) = (0, 0.0);
    frames
        .into_iter()
        .map(|(t, frame)| {
            while at + 1 < segments.len() && t >= segments[at].end() {
                real_start += segments[at].duration / scale(&segments[at]);
                at += 1;
            }
            (real_start + (t - segments[at].start) / scale(&segments[at]), frame)
        })
        .collect()
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

/// Plans ordinary text: synthesizer frames with their times in ms. A command in
/// the text applies to the rest of it, and not beyond.
pub fn text_frames(input: &str, voice: &Voice) -> Vec<(f64, Frame)> {
    plan(&commands::phrases(input, &mut voice.clone()), voice)
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
/// by returning false, and an error only for invalid phonetic input. A command
/// in text changes `voice`, so it lasts until another changes it back.
pub fn speak_with(
    input: &str,
    reading: Reading,
    voice: &mut Voice,
    sample_rate: u32,
    on_audio: &mut dyn FnMut(&[f32]) -> bool,
) -> Result<bool, ParseError> {
    let mut synth = Synth::new(sample_rate);
    synth.set_settings(synth_settings(voice));
    // Phonetic input can't be cut anywhere: its words are in brackets.
    let parts = if reading == Reading::Phonemes { vec![input] } else { pieces(input) };
    for part in parts {
        let phrases = match reading {
            Reading::Text => commands::phrases(part, voice),
            Reading::Spelled => text::spelled_phrases(part),
            Reading::Phonemes => parse(part)?,
        };
        // Text with no words in it, such as a command alone, makes no sound.
        for batch in batches(phrases).into_iter().filter(|batch| !batch.is_empty()) {
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
    notation(&commands::phrases(input, &mut voice.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The samples of `input`, read as text, by `voice`.
    fn samples(input: &str, voice: &mut Voice) -> Vec<f32> {
        let mut audio = Vec::new();
        let done = speak_with(input, Reading::Text, voice, 22050, &mut |chunk| {
            audio.extend_from_slice(chunk);
            true
        });
        assert_eq!(done, Ok(true));
        audio
    }

    /// How long the sung part of `input` lasts at `rate`, ms.
    fn sung_length(input: &str, rate: f64) -> f64 {
        let frames = text_frames(input, &Voice { rate, ..Voice::default() });
        let sung: Vec<f64> = frames.iter().filter(|(_, f)| f.steady == 1.0).map(|(t, _)| *t).collect();
        sung[sung.len() - 1] - sung[0]
    }

    #[test]
    fn written_lengths_stay_exact_at_any_rate() {
        let on = "[:phone klattara enable]";
        for rate in [0.5, 1.0, 2.24, 5.0] {
            let note = sung_length(&format!("{on}[AA1<800,20>]."), rate);
            assert!((note - 800.0).abs() < 0.001, "rate {rate}: {note}");
            let rest = sung_length(&format!("{on}[AA1<400,20> _<400> AA1<400,20>]."), rate);
            assert!((rest - 1200.0).abs() < 0.001, "rate {rate}: {rest}");
        }
    }

    #[test]
    fn speech_around_a_song_still_follows_the_rate() {
        let input = "[:phone klattara enable]Now listen to this. [AA1<800,20>].";
        let total = |rate: f64| text_frames(input, &Voice { rate, ..Voice::default() }).last().unwrap().0;
        assert!((sung_length(input, 1.0) - sung_length(input, 3.0)).abs() < 0.001);
        // The speech shrinks, and the note doesn't, so the whole shrinks by less than 3.
        assert!(total(3.0) < total(1.0) * 0.7, "{} {}", total(3.0), total(1.0));
        assert!(total(3.0) > total(1.0) / 3.0, "{} {}", total(3.0), total(1.0));
    }

    #[test]
    fn at_the_normal_rate_singing_follows_the_written_times_exactly() {
        let frames = phoneme_frames("[AA1<500,13> AA1<500,25>].", &Voice::default()).unwrap();
        let f0_at = |t: f64| frames.iter().rev().find(|f| f.0 <= t).unwrap().1.f0;
        let begin = frames.iter().find(|(_, f)| f.steady == 1.0).unwrap().0;
        assert_eq!(f0_at(begin + 499.0), 130.81);
        assert_eq!(f0_at(begin + 600.0), 261.63);
    }

    #[test]
    fn phoneme_input_lasts_between_calls_until_disabled() {
        let word = "[HH AH0 L OW1].";
        let mut voice = Voice::default();
        let as_text = samples(word, &mut voice);
        let as_word = speak_phonemes(word, &Voice::default(), 22050).unwrap();
        assert_ne!(as_text, as_word);
        // The command alone says nothing, and the next call reads phonemes.
        assert!(samples("[:phone klattara enable]", &mut voice).is_empty());
        assert_eq!(samples(word, &mut voice), as_word);
        assert_eq!(samples(word, &mut voice), as_word);
        assert!(samples("[:phone Klattara disable]", &mut voice).is_empty());
        assert_eq!(samples(word, &mut voice), as_text);
    }

    #[test]
    fn a_command_in_a_single_utterance_goes_no_further() {
        let voice = Voice::default();
        let text = "[:phone klattara enable] [HH AH0 L OW1].";
        assert_eq!(transcribe_with(text, &voice), "[HH AH0 L OW1].");
        assert_eq!(speak_text(text, &voice, 22050), speak_phonemes("[HH AH0 L OW1].", &voice, 22050).unwrap());
        assert!(!voice.phoneme_input);
    }

    #[test]
    fn spelled_and_phonetic_readings_have_no_commands() {
        let mut voice = Voice::default();
        speak_with("[:phone klattara enable]", Reading::Spelled, &mut voice, 22050, &mut |_| true).unwrap();
        assert!(!voice.phoneme_input);
        assert!(speak_with("[:phone klattara enable]", Reading::Phonemes, &mut voice, 22050, &mut |_| true).is_err());
        assert!(!voice.phoneme_input);
    }

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
        let done = speak_with(&text, Reading::Text, &mut Voice::default(), 22050, &mut |_| {
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
        let mut voice = Voice::default();
        let mut streamed = Vec::new();
        let done = speak_with(text, Reading::Text, &mut voice, 22050, &mut |chunk| {
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
        let done = speak_with(&text, Reading::Text, &mut Voice::default(), 22050, &mut |_| {
            chunks += 1;
            false
        });
        assert_eq!((done, chunks), (Ok(false), 1));
        // Left to run, it says every batch.
        let mut samples = 0;
        let done = speak_with(&"word ".repeat(1200), Reading::Text, &mut Voice::default(), 22050, &mut |chunk| {
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
