//! Speech from text or phonetic input: the pipeline to audio.

use crate::commands::{self, Part};
use crate::frame::Frame;
use crate::intonation::pitch_keyframes;
use crate::phoneme::Phoneme;
use crate::singing::{VIBRATO_DELAY, VIBRATO_DEPTH, VIBRATO_SPEED};
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
    /// Head size, as a percentage of normal. A larger head has a longer vocal tract,
    /// so every resonance is lower by the same share: at 125, all of them are at 80%
    /// of where they were. Their bandwidths move with them, which keeps the shape of
    /// the spectrum the same.
    pub head_size: f64,
    /// Roughness, from 0 for none to 100 for the most: uneven pitch and strength from
    /// one pulse of the voice to the next.
    pub roughness: f64,
    /// F4 to F6, Hz, where set. Those not set come from the gender's model. A value set
    /// here is that many Hz on either model, and head size still moves it. F6 is the
    /// value on most sounds: the sharp sounds that raise it rise by the same share.
    pub upper_formants: [Option<f64>; 3],
    /// The bandwidths of F4 to F6, Hz, where set, in the same way.
    pub upper_bandwidths: [Option<f64>; 3],
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
    /// How fast notes' vibrato swings, Hz.
    pub vibrato_speed: f64,
    /// How far notes' vibrato swings the pitch either way, in cents.
    pub vibrato_depth: f64,
    /// How long a note holds still before its vibrato starts, ms.
    pub vibrato_delay: f64,
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
            head_size: first.head_size,
            roughness: first.roughness,
            upper_formants: first.upper_formants,
            upper_bandwidths: first.upper_bandwidths,
            flutter: first.flutter,
            level: first.level,
            volume: 1.0,
            expand_abbreviations: true,
            phrase_prediction: false,
            vibrato_speed: VIBRATO_SPEED,
            vibrato_depth: VIBRATO_DEPTH,
            vibrato_delay: VIBRATO_DELAY,
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
        self.head_size = def.head_size;
        self.roughness = def.roughness;
        self.upper_formants = def.upper_formants;
        self.upper_bandwidths = def.upper_bandwidths;
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
    VoiceSettings {
        flutter: voice.flutter * voice.inflection.clamp(0.0, 1.0),
        volume: voice.volume * voice.level,
        roughness: voice.roughness / 100.0,
        vibrato_speed: voice.vibrato_speed,
    }
}

/// Words per minute on ordinary text at each rate, measured on a paragraph of sentences
/// of mixed lengths. Speech isn't quite in proportion to the rate, since pauses shrink
/// faster than words do: rate 3 is 634 words per minute, not 3 times 190. Short sentences
/// run slower than this, and long ones with few pauses faster.
const WORDS_PER_MINUTE: [(f64, f64); 11] = [
    (38.0, 0.25),
    (87.0, 0.5),
    (138.0, 0.75),
    (190.0, 1.0),
    (298.0, 1.5),
    (409.0, 2.0),
    (634.0, 3.0),
    (863.0, 4.0),
    (1094.0, 5.0),
    (1326.0, 6.0),
    (1796.0, 8.0),
];

/// The slowest and fastest speeds, in words per minute: the engine's slowest and fastest
/// rates.
pub const SLOWEST_WPM: f64 = WORDS_PER_MINUTE[0].0;
pub const FASTEST_WPM: f64 = WORDS_PER_MINUTE[WORDS_PER_MINUTE.len() - 1].0;

/// The rate that speaks at about this many words per minute.
pub fn rate_for_words_per_minute(wpm: f64) -> f64 {
    let wpm = wpm.clamp(SLOWEST_WPM, FASTEST_WPM);
    let after = WORDS_PER_MINUTE.partition_point(|point| point.0 < wpm).clamp(1, WORDS_PER_MINUTE.len() - 1);
    let ((slow, slow_rate), (fast, fast_rate)) = (WORDS_PER_MINUTE[after - 1], WORDS_PER_MINUTE[after]);
    slow_rate + (fast_rate - slow_rate) * (wpm - slow) / (fast - slow)
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
    let mut frames: Vec<(f64, Frame)> = Vec::new();
    for part in commands::parts(input, &mut voice.clone()) {
        let offset = frames.last().map_or(0.0, |last| last.0);
        frames.extend(plan(&part.phrases, &part.voice).into_iter().map(|(t, frame)| (t + offset, frame)));
    }
    frames
}

/// Speaks ordinary text, returning samples between −1 and 1.
pub fn speak_text(input: &str, voice: &Voice, sample_rate: u32) -> Vec<f32> {
    let parts = commands::parts(input, &mut voice.clone());
    parts.iter().flat_map(|part| render(&plan(&part.phrases, &part.voice), &part.voice, sample_rate)).collect()
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
    // Phonetic input can't be cut anywhere: its words are in brackets.
    let pieces = if reading == Reading::Phonemes { vec![input] } else { pieces(input) };
    for piece in pieces {
        let parts = match reading {
            Reading::Text => commands::parts(piece, voice),
            Reading::Spelled => vec![Part { voice: *voice, phrases: text::spelled_phrases(piece) }],
            Reading::Phonemes => vec![Part { voice: *voice, phrases: parse(piece)? }],
        };
        for part in parts {
            synth.set_settings(synth_settings(&part.voice));
            // Text with no words in it, such as a command alone, makes no sound.
            for batch in batches(part.phrases).into_iter().filter(|batch| !batch.is_empty()) {
                if !tracks::render_with(&mut synth, &plan(&batch, &part.voice), on_audio) {
                    return Ok(false);
                }
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
    fn speed_commands_speak_at_about_the_speed_asked_for() {
        let paragraph = "The old mill stood at the edge of the river. Nobody had worked there for years, and \
            the roof had begun to fall in. We walked around it twice. Was anyone still living nearby? We could not \
            tell. In the end we went home, tired and hungry. The next day it rained; we stayed inside and read.";
        let words = paragraph.split_whitespace().count() as f64;
        for wpm in [50.0, 100.0, 190.0, 250.0, 350.0, 450.0, 600.0, 800.0, 1000.0, 1500.0] {
            let frames = text_frames(&format!("[:speed {wpm}]{paragraph}"), &Voice::default());
            let measured = words / frames.last().unwrap().0 * 60_000.0;
            assert!((measured / wpm - 1.0).abs() < 0.05, "asked for {wpm}, got {measured:.0}");
        }
    }

    #[test]
    fn the_normal_speed_is_the_normal_rate() {
        assert_eq!(rate_for_words_per_minute(190.0), 1.0);
        assert_eq!(rate_for_words_per_minute(SLOWEST_WPM), 0.25);
        assert_eq!(rate_for_words_per_minute(FASTEST_WPM), 8.0);
        assert_eq!(rate_for_words_per_minute(1.0), 0.25);
        assert_eq!(rate_for_words_per_minute(9999.0), 8.0);
        let rates: Vec<f64> = (40..1800).step_by(10).map(|wpm| rate_for_words_per_minute(f64::from(wpm))).collect();
        assert!(rates.windows(2).all(|pair| pair[1] > pair[0]));
    }

    #[test]
    fn volume_commands_scale_the_sound() {
        let loudest = |audio: Vec<f32>| audio.iter().fold(0.0f32, |high, x| high.max(x.abs()));
        let full = loudest(speak_text("Hello.", &Voice::default(), 22050));
        let half = loudest(speak_text("[:volume 50]Hello.", &Voice::default(), 22050));
        assert!((half / full - 0.5).abs() < 0.01, "{half} {full}");
        assert_eq!(loudest(speak_text("[:volume 0]Hello.", &Voice::default(), 22050)), 0.0);
        assert_eq!(
            speak_text("[:volume 100]Hello.", &Voice::default(), 22050),
            speak_text("Hello.", &Voice::default(), 22050)
        );
    }

    #[test]
    fn reset_returns_everything_to_the_start() {
        let plain = speak_text("Hello.", &Voice::default(), 22050);
        let changed = "[:vb][:cv pi 300 in 200 hs 70 rg 80 br 60][:speed 500][:volume 40][:vibspeed 9]";
        let text = format!("{changed}[:phone klattara enable][:reset]Hello.");
        assert_eq!(speak_text(&text, &Voice::default(), 22050), plain);
        // And it lasts, as other commands do.
        let mut voice = Voice { rate: 2.0, ..Voice::default() };
        samples(&format!("{changed}[:reset]"), &mut voice);
        assert_eq!(voice, Voice::default());
    }

    #[test]
    fn a_new_voice_keeps_the_speed_volume_and_vibrato() {
        let mut voice = Voice::default();
        samples("[:speed 400][:volume 50][:vibdepth 40][:vibspeed 5][:vibdelay 200][:vb]", &mut voice);
        assert_eq!(voice.pitch, 210.0);
        assert_eq!(
            (voice.volume, voice.vibrato_depth, voice.vibrato_speed, voice.vibrato_delay),
            (0.5, 40.0, 5.0, 200.0)
        );
        assert_eq!(voice.rate, rate_for_words_per_minute(400.0));
    }

    #[test]
    fn vibrato_commands_change_the_singing_only() {
        let speech = "Hello there.";
        assert_eq!(
            speak_text(&format!("[:vibdepth 90][:vibspeed 9][:vibdelay 50]{speech}"), &Voice::default(), 22050),
            speak_text(speech, &Voice::default(), 22050)
        );
        let on = "[:phone klattara enable]";
        let song = "[AA1<800,20>].";
        let plain = speak_text(&format!("{on}{song}"), &Voice::default(), 22050);
        for command in ["[:vibdepth 90]", "[:vibspeed 9]", "[:vibdelay 300]"] {
            assert_ne!(speak_text(&format!("{on}{command}{song}"), &Voice::default(), 22050), plain, "{command}");
        }
    }

    #[test]
    fn richness_smoothness_and_gender_reach_the_sound() {
        let voice = Voice::default();
        let plain = speak_text("Hello there.", &voice, 22050);
        for command in ["[:cv rc 90]", "[:cv sm 60]", "[:cv ge f]"] {
            let changed = speak_text(&format!("{command}Hello there."), &voice, 22050);
            assert_eq!(changed.len(), plain.len(), "{command}");
            assert_ne!(changed, plain, "{command}");
        }
        // Each value of Steve's own changes nothing.
        for command in ["[:cv sm 0]", "[:cv ge m]", "[:cv ge 1]"] {
            assert_eq!(speak_text(&format!("{command}Hello there."), &voice, 22050), plain, "{command}");
        }
        // Gender is the same as the setting that it names.
        let female = Voice { gender: crate::Gender::Female, ..Voice::default() };
        assert_eq!(speak_text("[:cv ge f]Hello there.", &voice, 22050), speak_text("Hello there.", &female, 22050));
    }

    #[test]
    fn upper_formants_reach_the_sound_and_leave_the_timing_alone() {
        let voice = Voice::default();
        let plain = speak_text("Hello there.", &voice, 22050);
        for command in
            ["[:cv f4 3800]", "[:cv f5 4500]", "[:cv f6 6000]", "[:cv b4 600]", "[:cv b5 600]", "[:cv b6 2000]"]
        {
            let changed = speak_text(&format!("{command}Hello there."), &voice, 22050);
            assert_eq!(changed.len(), plain.len(), "{command}");
            assert_ne!(changed, plain, "{command}");
        }
        // Setting Steve's own values changes nothing.
        let own = "[:cv f4 3300 f5 3750 f6 4900 b4 250 b5 200 b6 1000]Hello there.";
        assert_eq!(speak_text(own, &voice, 22050), plain);
    }

    #[test]
    fn mistakes_are_spoken_and_the_text_goes_on() {
        let voice = Voice::default();
        let alert = speak_text("Incorrect command.", &voice, 22050);
        assert_eq!(speak_text("[:foo]", &voice, 22050), alert);
        // Streaming speaks it too, and it isn't an error to the program.
        let mut streamed = Voice::default();
        assert_eq!(samples("[:foo]", &mut streamed), alert);
        assert!(samples("Before [:foo] after.", &mut streamed).len() > alert.len());
    }

    #[test]
    fn phonetic_reading_still_reports_a_bad_phoneme_as_an_error() {
        let mut voice = Voice::default();
        assert!(speak_with("[QQ]", Reading::Phonemes, &mut voice, 22050, &mut |_| true).is_err());
    }

    #[test]
    fn voice_commands_last_between_calls() {
        let brittany = Voice::from(crate::find_voice("Brittany").unwrap());
        let as_brittany = speak_text("Hello.", &brittany, 22050);
        let mut voice = Voice::default();
        assert!(samples("[:vb]", &mut voice).is_empty());
        assert_eq!(samples("Hello.", &mut voice), as_brittany);
        assert_eq!(samples("Hello.", &mut voice), as_brittany);
        samples("[:vs]", &mut voice);
        assert_eq!(samples("Hello.", &mut voice), speak_text("Hello.", &Voice::default(), 22050));
    }

    #[test]
    fn a_command_in_a_single_utterance_changes_only_that_utterance() {
        let voice = Voice::default();
        let brittany = Voice::from(crate::find_voice("Brittany").unwrap());
        assert_eq!(speak_text("[:vb]Hello.", &voice, 22050), speak_text("Hello.", &brittany, 22050));
        assert_eq!(voice, Voice::default());
    }

    #[test]
    fn each_part_is_spoken_by_its_own_voice() {
        let voice = Voice::default();
        let brittany = Voice::from(crate::find_voice("Brittany").unwrap());
        let one = speak_text("One.", &voice, 22050);
        let two = speak_text("Two.", &brittany, 22050);
        let both = speak_text("One. [:vb] Two.", &voice, 22050);
        assert_eq!(both, [one.clone(), two.clone()].concat());
        // Streaming speaks the same, and Brittany's quieter level is hers alone.
        let mut streamed = Voice::default();
        assert_eq!(samples("One. [:vb] Two.", &mut streamed), both);
    }

    #[test]
    fn frames_follow_each_part_in_order() {
        let frames = text_frames("One. [:cv pi 300] Two.", &Voice::default());
        assert!(frames.windows(2).all(|w| w[1].0 >= w[0].0));
        let highest = |range: &[(f64, Frame)]| range.iter().fold(0.0f64, |high, (_, f)| high.max(f.f0));
        let middle = frames.len() / 2;
        assert!(highest(&frames[..middle]) < 200.0 && highest(&frames[middle..]) > 250.0);
    }

    #[test]
    fn cv_pitch_and_inflection_reach_the_contour() {
        let f0_range = |input: &str| {
            let frames = text_frames(input, &Voice::default());
            let voiced: Vec<f64> = frames.iter().filter(|(_, f)| f.f0 > 0.0).map(|(_, f)| f.f0).collect();
            let (low, high) = voiced.iter().fold((f64::MAX, 0.0f64), |(l, h), x| (l.min(*x), h.max(*x)));
            (low, high)
        };
        let text = "We walked along the river at night.";
        let (normal_low, normal_high) = f0_range(text);
        let (low, high) = f0_range(&format!("[:cv pi 220]{text}"));
        assert!((low / normal_low - 2.0).abs() < 0.01 && (high / normal_high - 2.0).abs() < 0.01, "{low} {high}");
        let (flat_low, flat_high) = f0_range(&format!("[:cv in 0]{text}"));
        assert_eq!((flat_low, flat_high), (110.0, 110.0));
        let (wide_low, wide_high) = f0_range(&format!("[:cv in 200]{text}"));
        assert!(wide_high / wide_low > normal_high / normal_low * 1.5, "{wide_low} {wide_high}");
    }

    #[test]
    fn cv_breathiness_adds_breath_to_voicing() {
        let breath = |input: &str| {
            let frames = text_frames(input, &Voice::default());
            frames.iter().filter(|(_, f)| f.av >= 59.0).fold(0.0f64, |high, (_, f)| high.max(f.ah))
        };
        assert_eq!(breath("Hello there."), 0.0);
        assert!((breath("[:cv br 40]Hello there.") - 40.0).abs() < 1.0);
    }

    #[test]
    fn roughness_changes_the_sound_and_not_the_timing() {
        let plain = speak_text("Hello there.", &Voice::default(), 22050);
        let rough = speak_text("[:cv rg 80]Hello there.", &Voice::default(), 22050);
        assert_eq!(plain.len(), rough.len());
        assert_ne!(plain, rough);
        assert_eq!(speak_text("[:cv rg 0]Hello there.", &Voice::default(), 22050), plain);
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
