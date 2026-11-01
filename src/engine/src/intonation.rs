//! Pitch contours: the intonation model of SharpVox, by ByteSizedFox (MIT License).
//!
//! Each phrase gets a slow rise and fall over the whole phrase, and a quicker step up
//! and down for each accented word, as in the command-response model of Fujisaki and
//! Hirose (1984). They published it for Japanese, and Fujisaki and Ohno (1995) fitted it
//! to English. Anthony C. Bartman's version runs both as two-pole filters, and TGSpeechBox,
//! by Tamas Geczy, is where SharpVox's filters are ported from. Under those, SharpVox has
//! its own rules for English, which are what this file follows:
//!
//! 1. The last stressed word of a phrase carries its main accent, the nucleus. Each
//!    stressed word before it carries a smaller head accent, smaller again if it is one
//!    syllable long or a light word like "get" or "very". Other words carry none.
//! 2. Accents near the start of a phrase are smaller, so that they don't stack on the
//!    phrase's rise. Every accent is as long as its vowel, within limits.
//! 3. The baseline declines through each phrase, and phrases after a comma sit lower.
//! 4. A statement falls from its nucleus to its end. A comma and a yes-no question rise
//!    on their last vowel, and the question's nucleus is small, so that the rise starts
//!    low.
//! 5. Each phrase is centered on its average, so the voice's pitch is the average pitch.
//! 6. Each sound nudges the pitch a little: high vowels and voiceless consonants lift it,
//!    and nasals and voiced consonants lower it.
//!
//! Everything runs in steps of 5 ms on SharpVox's scale of 256 units to the octave, and
//! its values are kept as they are, so the two can be compared.
//!
//! Klattara differs from it in these ways:
//!
//! - The contour is worked out at the normal rate and compressed with the speech. SharpVox
//!   keeps the filters at their own speed, so at fast rates neighbouring accents blend.
//!   The user chose this by ear for an earlier version of the same filters.
//! - A question that begins with a question word falls like a statement, and a break
//!   without a pause falls a little and no further, both chosen by ear.
//! - A semicolon or colon falls part of the way, where SharpVox treats it as a comma.
//! - An exclamation falls like a statement, with a taller nucleus, and sits higher. In
//!   SharpVox it doesn't fall, and centering cancels its higher level.
//! - A statement falls from its nucleus even if that has no primary stress, so that "It
//!   is." still falls.
//! - A word at the end of a phrase keeps its accent: "I said no."
//! - A word that has lost its accent for being repeated stays without one.
//! - The nudges follow the inflection up to normal, so inflection 0 is a true monotone.
//! - The sizes of the baseline fall, the rise over the phrase and the accents, and how
//!   quickly the pitch responds, are settings of the voice: [`PitchShape`], which works
//!   like DECtalk's bf, hr, sr and qu, as far as this model allows. Its baseline falls for
//!   each sentence at a fixed rate and its hat holds a plateau to the last stress, whereas
//!   here the baseline falls through each phrase, and the rise is the phrase filter's.
//! - Left out: SharpVox's small dips at word boundaries, about a quarter of a semitone,
//!   and its speech vibrato, about a tenth. Its flutter is left out because the voice
//!   source here has its own.

use crate::phoneme::{Manner, Phoneme, Stress};
use crate::singing::{NOTE_GLIDE_MS, is_note, pitch_hz};
use crate::utterance::{Boundary, Segment, WordAccent};
use crate::voicedefs::PitchShape;

/// One step of the model, ms.
const FRAME_MS: f64 = 5.0;
const UNITS_PER_OCTAVE: f64 = 256.0;
const UNITS_PER_NEPER: f64 = 369.3246;

/// The size of the commands, in nepers (natural-log units of F0).
const PHRASE_COMMAND: f64 = 0.057;
const NUCLEAR_ACCENT: f64 = 0.18;
const HEAD_ACCENT: f64 = 0.17;
/// How prominent a head accent is, beside a nucleus at 1.
const HEAD_PROMINENCE: f64 = 0.85;
/// The nucleus of an exclamation.
const EMPHATIC_ACCENT: f64 = 0.30;
/// Head accents of light words and of one-syllable words.
const LIGHT_WORD: f64 = 0.7;
const ONE_SYLLABLE_WORD: f64 = 0.8;
/// The nucleus of a yes-no question, which is low ahead of the rise.
const QUESTION_NUCLEUS: f64 = 0.4;

/// How long the phrase filter takes to peak, in steps: 325 ms.
const PHRASE_RISE_FRAMES: f64 = 65.0;
/// How much an accent shrinks while the phrase's rise is under way, and for how long.
const PHRASE_START_SHRINK: f64 = 0.7;
const PHRASE_HUMP_FRAMES: f64 = 78.0;
/// An accent lasts as long as its vowel, within these limits, in steps: 45 to 260 ms.
const SHORTEST_PULSE: i32 = 9;
const LONGEST_PULSE: i32 = 52;
/// A nucleus runs past its vowel, so that the fall lands on what follows.
const NUCLEAR_PULSE: f64 = 1.15;
/// How long an accent takes to arrive, in steps: a quarter of its length, within these.
const QUICKEST_ATTACK: i32 = 4;
const SLOWEST_ATTACK: i32 = 9;
/// How early accents start, in steps before their vowel.
const HEAD_ONSET: i64 = 3;
const NUCLEAR_ONSET: i64 = 4;

/// The middle of the baseline, in units. It falls from the voice's baseline fall above
/// this to the same amount below.
const BASELINE_MIDDLE: f64 = 21.0;
/// How far the end of a phrase rises, in units: on starting its last vowel, then later on.
const COMMA_RISE: (f64, f64) = (20.0, 57.0);
const QUESTION_RISE: (f64, f64) = (39.0, 85.0);
/// A statement ends at this share of the pitch it would have had.
const FINAL_SHARE: f64 = 0.67;
/// How much lower each phrase after a comma sits, in units, for at most two of them.
const CONTINUATION_LEVEL: f64 = -24.0;
const MOST_CONTINUATIONS: u32 = 2;
/// How much higher an exclamation sits, in units.
const EXCLAMATION_REGISTER: f64 = 57.0;
/// SharpVox's baseline voice, whose range is 120%. At inflection 1, that is the range here.
const PITCH_RANGE: f64 = 1.2;

/// How long the pitch takes to respond, as a share of normal, for a quickness in percent.
/// In DECtalk the speed of its filter on the pitch grows in a straight line with quickness:
/// 1500 + 15 for each percent.
fn slowness(quickness: f64) -> f64 {
    let speed = |percent: f64| 1500.0 + 15.0 * percent;
    speed(PitchShape::NORMAL.quickness) / speed(quickness)
}

/// How far each kind of phrase falls from its nucleus, as a share of a statement's fall.
/// The other two are the old ends of these phrases as shares of a statement's.
fn final_fall(boundary: Boundary) -> f64 {
    match boundary {
        Boundary::Period | Boundary::WhQuestion | Boundary::Exclamation => 1.0,
        Boundary::Clause => 0.625,
        // The user chose this over a comma-like rise by ear (samples 44_breaks_fall and
        // _rise).
        Boundary::Minor => 0.375,
        Boundary::Comma | Boundary::Question => 0.0,
    }
}

/// The two filters, stepped once per frame. Ported from SharpVox's FujisakiPitchModel,
/// which is itself ported from TGSpeechBox's pitch model.
#[derive(Default)]
struct CommandFilters {
    phrase_coefficients: [f64; 3],
    accent_coefficients: [f64; 3],
    phrase_input: f64,
    accent_input: f64,
    accent_frames_left: i32,
    phrase_output: [f64; 2],
    accent_output: [f64; 2],
}

impl CommandFilters {
    /// An impulse into the phrase filter, whose response peaks at about `amount`, after
    /// `rise` frames.
    fn phrase(&mut self, amount: f64, rise: f64) {
        let decay = -1.0 / rise;
        let r = decay.exp();
        let (b, c) = (2.0 * r, -(r * r));
        // Brings the height of the peak to the size of the command.
        let p = (std::f64::consts::E * decay).exp();
        self.phrase_coefficients = [1.0 - b * p - c * p, b, c];
        self.phrase_input = amount;
    }

    /// A pulse into the accent filter, held for `frames` frames, which it follows with
    /// an attack of `attack` frames.
    fn accent(&mut self, amount: f64, frames: i32, attack: f64) {
        let r = (-1.0 / attack).exp();
        let (b, c) = (2.0 * r, -(r * r));
        self.accent_coefficients = [1.0 - b - c, b, c];
        self.accent_input = amount;
        self.accent_frames_left = frames;
    }

    /// Where the phrase filter has got to.
    fn phrase_response(&self) -> f64 {
        self.phrase_output[0]
    }

    /// Moves on one frame, and returns the sum of both responses, in nepers.
    fn step(&mut self) -> f64 {
        let [a, b, c] = self.phrase_coefficients;
        let phrase = a * self.phrase_input + b * self.phrase_output[0] + c * self.phrase_output[1];
        self.phrase_output = [phrase, self.phrase_output[0]];
        self.phrase_input = 0.0;
        let pulse = if self.accent_frames_left > 0 {
            self.accent_frames_left -= 1;
            self.accent_input
        } else {
            0.0
        };
        let [a, b, c] = self.accent_coefficients;
        let accent = a * pulse + b * self.accent_output[0] + c * self.accent_output[1];
        self.accent_output = [accent, self.accent_output[0]];
        phrase + accent
    }
}

/// How much each sound lifts the pitch at its start, in units: SharpVox's table.
fn sound_nudge(phoneme: Phoneme) -> i64 {
    use Phoneme::*;
    match phoneme {
        IY | UW => 29,
        IH | UH | W | Y => 18,
        ER | OY => 15,
        EY => 12,
        AX | OW => 9,
        EH | AH => 6,
        AE | AA | AO | AY | AW | R | L | LX | Pause => 0,
        F | TH | S | SH | P | T | K | CH => 82,
        HH => 57,
        DX => -3,
        M | N | NG | V | DH | Z | ZH | B | D | G | JH => -15,
    }
}

/// The nudges of the sounds of a phrase, a frame at a time.
struct SoundNudges<'a> {
    phrase: &'a [Segment],
    /// The frame on which each sound ends.
    ends: Vec<i64>,
    /// The sound it has got to, and how far past its end the next must wait.
    at: Option<usize>,
    wait: i64,
    voiced: i64,
    voiceless: i64,
}

impl<'a> SoundNudges<'a> {
    fn new(phrase: &'a [Segment], ends: Vec<i64>) -> Self {
        SoundNudges { phrase, ends, at: None, wait: 0, voiced: 0, voiceless: 0 }
    }

    fn step(&mut self, frame: i64) -> f64 {
        let end = self.at.map_or(0, |i| self.ends[i]);
        let next = self.at.map_or(0, |i| i + 1);
        if frame > end + self.wait && next < self.phrase.len() {
            self.at = Some(next);
            let phoneme = self.phrase[next].phoneme;
            let half = sound_nudge(phoneme) >> 1;
            let before_voiceless = self.phrase.get(next + 1).is_some_and(|s| !s.phoneme.is_voiced());
            if phoneme.is_voiced() {
                self.voiced = half << 1;
                self.voiceless = 0;
                // In steps: 25 ms.
                self.wait = if before_voiceless { 5 } else { 0 };
            } else {
                self.voiceless = half;
                self.voiced = 0;
                // In steps: 30 ms.
                self.wait = if phoneme.manner() == Manner::Stop { 6 } else { 0 };
            }
        }
        let nudge = self.voiced + self.voiceless;
        // As SharpVox has it, in whole units: these decay by a unit or so a step, and
        // those below zero hold until the next sound.
        let decay = |x: i64| (x * 98 * 655) >> 16;
        self.voiced = decay(self.voiced);
        self.voiceless = decay(self.voiceless);
        nudge as f64
    }
}

/// One accent, before the start of the phrase scales it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Accent {
    /// The vowel it belongs to, by its place in the phrase.
    vowel: usize,
    /// Nepers.
    amount: f64,
    /// Frames.
    pulse: i32,
    attack: i32,
    /// The frame it fires on, which may be early.
    frame: i64,
}

/// What happens during a phrase.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Event {
    Accent(Accent),
    /// The end of the phrase moves to this level, in units.
    Boundary(f64),
}

/// The vowel that carries the main accent of a phrase: the last with primary stress in a
/// word that can have an accent, or else the last with any stress, or else the last.
fn nucleus(phrase: &[Segment]) -> Option<usize> {
    let vowel = |s: &Segment| s.phoneme.is_vowel();
    phrase
        .iter()
        .rposition(|s| vowel(s) && s.stress == Stress::Primary && s.accent != WordAccent::Unaccented)
        .or_else(|| phrase.iter().rposition(|s| vowel(s) && s.stress != Stress::Unstressed))
        .or_else(|| phrase.iter().rposition(vowel))
}

/// How long a pulse is, and how long it takes to arrive, for a vowel of so many frames.
fn pulse_and_attack(frames: i32) -> (i32, i32) {
    let pulse = frames.clamp(SHORTEST_PULSE, LONGEST_PULSE);
    (pulse, (pulse / 4).clamp(QUICKEST_ATTACK, SLOWEST_ATTACK))
}

/// The number of vowels in the word of the segment at `i`.
fn vowels_in_word(phrase: &[Segment], i: usize) -> usize {
    let start = (0..=i).rev().find(|&j| phrase[j].word_initial).unwrap_or(0);
    let end = (i..phrase.len()).find(|&j| phrase[j].word_final).unwrap_or(phrase.len() - 1);
    phrase[start..=end].iter().filter(|s| s.phoneme.is_vowel()).count()
}

/// The events of a phrase, in order.
fn phrase_events(phrase: &[Segment], boundary: Boundary, shape: &PitchShape) -> Vec<(i64, Event)> {
    let stress = shape.stress_rise / PitchShape::NORMAL.stress_rise;
    let begin = phrase[0].start;
    let frame_of = |t: f64| ((t - begin) / FRAME_MS).round() as i64;
    let frames_of = |seg: &Segment| ((seg.duration / FRAME_MS).round() as i32).max(1);
    let main = nucleus(phrase);
    // A head accent goes on an earlier word, not an earlier syllable of the same one.
    let main_word = main.map_or(0, |n| (0..=n).rev().find(|&j| phrase[j].word_initial).unwrap_or(0));
    let last_vowel = phrase.iter().rposition(|s| s.phoneme.is_vowel());
    let mut events = Vec::new();
    for (i, seg) in phrase.iter().enumerate() {
        if !seg.phoneme.is_vowel() {
            continue;
        }
        if i < main_word && seg.stress == Stress::Primary && seg.accent != WordAccent::Unaccented {
            let light = if seg.accent == WordAccent::Light { LIGHT_WORD } else { 1.0 };
            let short = if vowels_in_word(phrase, i) == 1 { ONE_SYLLABLE_WORD } else { 1.0 };
            let (pulse, attack) = pulse_and_attack(frames_of(seg));
            let frame = frame_of(seg.start) - HEAD_ONSET;
            let amount = HEAD_ACCENT * HEAD_PROMINENCE * light * short * stress;
            events.push((frame, Event::Accent(Accent { vowel: i, amount, pulse, attack, frame })));
        }
        if main == Some(i) {
            let amount = match boundary {
                Boundary::Exclamation => EMPHATIC_ACCENT,
                Boundary::Question => NUCLEAR_ACCENT * QUESTION_NUCLEUS,
                _ => NUCLEAR_ACCENT,
            } * stress;
            let (pulse, attack) = pulse_and_attack((f64::from(frames_of(seg)) * NUCLEAR_PULSE) as i32);
            let frame = frame_of(seg.start) - NUCLEAR_ONSET;
            events.push((frame, Event::Accent(Accent { vowel: i, amount, pulse, attack, frame })));
        }
        if last_vowel == Some(i) {
            let at_start = frame_of(seg.start);
            match boundary {
                Boundary::Comma => {
                    events.push((at_start, Event::Boundary(COMMA_RISE.0)));
                    events.push((frame_of(seg.end()), Event::Boundary(COMMA_RISE.1)));
                }
                // On the voicing, not on a voiceless consonant after it.
                Boundary::Question => {
                    events.push((at_start, Event::Boundary(QUESTION_RISE.0)));
                    events.push((frame_of(seg.start + 0.75 * seg.duration), Event::Boundary(QUESTION_RISE.1)));
                }
                _ => {}
            }
        }
    }
    // An early event never moves the clock back, so the later ones keep their time.
    let mut latest = 0;
    for (frame, _) in &mut events {
        latest = latest.max(*frame);
        *frame = latest;
    }
    events
}

/// The pitch of one phrase: times in ms, and heights in units from the voice's pitch.
fn phrase_contour(
    phrase: &[Segment],
    boundary: Boundary,
    continuing: u32,
    inflection: f64,
    shape: &PitchShape,
) -> Vec<(f64, f64)> {
    let begin = phrase[0].start;
    let end = phrase[phrase.len() - 1].end();
    let frame_of = |t: f64| ((t - begin) / FRAME_MS).round() as i64;
    let frames = frame_of(end).max(1);
    let last_vowel = phrase.iter().rposition(|s| s.phoneme.is_vowel());

    // The baseline reaches its end at the last vowel, or halfway if the phrase rises.
    let ramp_frames = last_vowel.map_or(frames, |v| frame_of(phrase[v].start)) + 1;
    let rises = matches!(boundary, Boundary::Comma | Boundary::Question);
    let (baseline_start, baseline_end) = (BASELINE_MIDDLE + shape.baseline_fall, BASELINE_MIDDLE - shape.baseline_fall);
    let ramp_step = (baseline_start - baseline_end) / ramp_frames as f64 * if rises { 0.5 } else { 1.0 };

    // The fall runs from the nucleus to the end of the last voiced sound, and is done
    // four fifths of the way there, on the voicing.
    let main = nucleus(phrase);
    let fall_start = main.map(|n| frame_of(phrase[n].start));
    let fall_span = main.map_or(1.0, |n| {
        let voiced_end = phrase[n..].iter().rposition(|s| s.phoneme.is_voiced()).map_or(n, |i| n + i);
        (0.8 * (frame_of(phrase[voiced_end].end()) - frame_of(phrase[n].start)) as f64).max(1.0)
    });
    let fall = FINAL_SHARE.ln() * UNITS_PER_NEPER * final_fall(boundary);

    let mut filters = CommandFilters::default();
    let slow = slowness(shape.quickness);
    let command = PHRASE_COMMAND * (shape.hat_rise / PitchShape::NORMAL.hat_rise);
    filters.phrase(command, PHRASE_RISE_FRAMES * slow);
    let smoothing = 8.0 * slow;
    let mut events = phrase_events(phrase, boundary, shape).into_iter().peekable();
    let (mut ramp, mut boundary_level, mut smooth_end) = (0.0, 0.0, 0.0);
    let mut raw = Vec::with_capacity(frames as usize + 1);
    for frame in 0..=frames {
        while let Some((_, event)) = events.next_if(|(due, _)| *due <= frame) {
            match event {
                Event::Boundary(level) => boundary_level = level,
                Event::Accent(accent) => {
                    // While the phrase's rise is under way, an accent is smaller, so
                    // that the first word doesn't stack the two into a spike.
                    let hump = if command > 0.0 {
                        (filters.phrase_response() / command)
                            .min(1.0)
                            .max((1.0 - frame as f64 / PHRASE_HUMP_FRAMES).clamp(0.0, 1.0))
                    } else {
                        0.0
                    };
                    let amount = accent.amount * (1.0 - PHRASE_START_SHRINK * hump);
                    if amount > 0.0 {
                        filters.accent(amount, accent.pulse, f64::from(accent.attack) * slow);
                    }
                }
            }
        }
        let baseline = baseline_start - ramp;
        if baseline > baseline_end {
            ramp += ramp_step;
        }
        let response = filters.step() * UNITS_PER_NEPER;
        let shaping = match fall_start {
            Some(start) if frame >= start => fall * ((frame - start) as f64 / fall_span).min(1.0),
            _ => 0.0,
        };
        smooth_end = (smooth_end * (smoothing - 1.0) + boundary_level + shaping) / smoothing;
        raw.push(baseline + response + smooth_end);
    }

    // The average of the voiced frames before the fall, which the pitch of the voice is.
    let ends: Vec<i64> = phrase.iter().map(|s| frame_of(s.end())).collect();
    let (mut sum, mut count, mut at) = (0.0, 0, 0);
    for (frame, height) in raw.iter().enumerate() {
        let frame = frame as i64;
        while at + 1 < phrase.len() && ends[at] <= frame {
            at += 1;
        }
        if phrase[at].phoneme.is_voiced() && fall_start.is_none_or(|start| frame < start) {
            sum += height;
            count += 1;
        }
    }
    let mean = if count > 0 { sum / f64::from(count) } else { 0.0 };

    let register = if boundary == Boundary::Exclamation { EXCLAMATION_REGISTER } else { 0.0 };
    let level = CONTINUATION_LEVEL * f64::from(continuing.min(MOST_CONTINUATIONS)) + register;
    let mut nudges = SoundNudges::new(phrase, ends);
    let mut smoothed: Option<f64> = None;
    let mut points = Vec::with_capacity(raw.len() + 1);
    for (frame, height) in raw.iter().enumerate() {
        let units =
            (height - mean + level) * PITCH_RANGE * inflection + nudges.step(frame as i64) * inflection.clamp(0.0, 1.0);
        let value = smoothed.map_or(units, |s| (s * 3.0 + units) / 4.0);
        smoothed = Some(value);
        points.push((begin + frame as f64 * FRAME_MS, value));
    }
    // Keep to the phrase, and end exactly where it does.
    points.retain(|&(t, _)| t < end);
    points.push((end, smoothed.unwrap_or(0.0)));
    points
}

/// Pitch keyframes (time in ms, F0 in Hz) for timed segments.
pub fn shaped_keyframes(segments: &[Segment], base_hz: f64, inflection: f64, shape: &PitchShape) -> Vec<(f64, f64)> {
    let mut keyframes = Vec::new();
    // The last pitch sung, which the next note moves from, across a rest too.
    let mut sung = None;
    // How many phrases in a row have followed a comma, or something like one.
    let mut continuing = 0;
    let mut previous = Boundary::Period;
    let mut i = 0;
    while i < segments.len() {
        if segments[i].phoneme == Phoneme::Pause {
            i += 1;
            continue;
        }
        let start = i;
        while i < segments.len()
            && segments[i].phoneme != Phoneme::Pause
            && segments[i].phrase == segments[start].phrase
        {
            i += 1;
        }
        if segments[start].singing {
            keyframes.extend(sung_contour(&segments[start..i], &mut sung));
            previous = Boundary::Period;
            continue;
        }
        sung = None;
        // A phrase that ends without a pause ends at a break (`|`).
        let boundary = match segments.get(i) {
            Some(next) if next.phoneme != Phoneme::Pause => Boundary::Minor,
            next => next.and_then(|s| s.boundary).unwrap_or(Boundary::Period),
        };
        continuing =
            if matches!(previous, Boundary::Comma | Boundary::Clause | Boundary::Minor) { continuing + 1 } else { 0 };
        previous = boundary;
        for (t, units) in phrase_contour(&segments[start..i], boundary, continuing, inflection, shape) {
            keyframes.push((t, base_hz * 2f64.powf(units / UNITS_PER_OCTAVE)));
        }
    }
    keyframes
}

/// Pitch keyframes with the normal shape.
#[cfg(test)]
pub fn pitch_keyframes(segments: &[Segment], base_hz: f64, inflection: f64) -> Vec<(f64, f64)> {
    shaped_keyframes(segments, base_hz, inflection, &PitchShape::NORMAL)
}

/// The contour of a sung stretch, in Hz: only its written pitches. A sound with no
/// pitch of its own holds the one before it, or at the start the first to come.
fn sung_contour(stretch: &[Segment], sung: &mut Option<f64>) -> Vec<(f64, f64)> {
    let first = stretch.iter().find_map(|s| s.written.pitch).map_or(0.0, pitch_hz);
    let mut hz = sung.unwrap_or(first);
    let mut points = vec![(stretch[0].start, hz)];
    for seg in stretch {
        let Some(pitch) = seg.written.pitch else { continue };
        let glide = if is_note(pitch) { NOTE_GLIDE_MS.min(seg.duration) } else { seg.duration };
        points.push((seg.start, hz));
        hz = pitch_hz(pitch);
        points.push((seg.start + glide, hz));
    }
    points.push((stretch[stretch.len() - 1].end(), hz));
    *sung = Some(hz);
    let mut last = f64::NEG_INFINITY;
    points.retain(|&(t, _)| {
        let keep = t > last;
        if keep {
            last = t;
        }
        keep
    });
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timing::assign_durations;
    use crate::utterance::{parse, segments};

    /// The pitch keyframes of phonetic input, and its segments.
    fn sung(input: &str) -> (Vec<Segment>, Vec<(f64, f64)>) {
        let mut segs = segments(&parse(input).unwrap());
        assign_durations(&mut segs, 1.0);
        let keyframes = pitch_keyframes(&segs, 110.0, 1.0);
        (segs, keyframes)
    }

    /// F0 at time `t`, between keyframes.
    fn f0_at(keyframes: &[(f64, f64)], t: f64) -> f64 {
        let i = keyframes.partition_point(|k| k.0 <= t);
        if i == 0 || i == keyframes.len() {
            return keyframes[i.saturating_sub(1)].1;
        }
        let ((t0, a), (t1, b)) = (keyframes[i - 1], keyframes[i]);
        a + (b - a) * (t - t0) / (t1 - t0)
    }

    #[test]
    fn a_note_holds_flat_with_no_accent_or_falling_end() {
        let (segs, keys) = sung("[AA1<1000,34>].");
        let vowel = &segs[1];
        assert_eq!(vowel.duration, 1000.0);
        for share in [0.0, 0.25, 0.5, 0.99, 1.0] {
            assert_eq!(f0_at(&keys, vowel.start + share * vowel.duration), 440.0);
        }
    }

    #[test]
    fn notes_take_100_ms_to_arrive_and_hz_take_their_whole_sound() {
        let (segs, keys) = sung("[AA1<500,13> AA1<500,25> AA1<500,300>].");
        let (second, third) = (&segs[2], &segs[3]);
        assert_eq!(f0_at(&keys, second.start), 130.81);
        assert!((f0_at(&keys, second.start + 50.0) - (130.81 + 261.63) / 2.0).abs() < 0.01);
        assert_eq!(f0_at(&keys, second.start + 100.0), 261.63);
        assert_eq!(f0_at(&keys, second.start + 400.0), 261.63);
        assert_eq!(f0_at(&keys, third.start), 261.63);
        assert!((f0_at(&keys, third.start + 250.0) - (261.63 + 300.0) / 2.0).abs() < 0.01);
        assert_eq!(f0_at(&keys, third.end()), 300.0);
    }

    #[test]
    fn a_sound_with_no_pitch_holds_the_one_nearby() {
        // The D before the first note takes that note; the T after holds it.
        let (segs, keys) = sung("[D AA1<500,25> T].");
        assert_eq!(f0_at(&keys, segs[1].start), 261.63);
        assert_eq!(f0_at(&keys, segs[3].end()), 261.63);
    }

    #[test]
    fn the_next_note_moves_from_the_last_across_a_rest() {
        let (segs, keys) = sung("[AA1<500,13> _<200> AA1<500,25>].");
        let after = &segs[3];
        assert_eq!(f0_at(&keys, after.start), 130.81);
        assert_eq!(f0_at(&keys, after.start + 100.0), 261.63);
    }

    #[test]
    fn notes_do_not_depend_on_the_voices_pitch() {
        let mut segs = segments(&parse("[AA1<500,25>].").unwrap());
        assign_durations(&mut segs, 1.0);
        assert_eq!(pitch_keyframes(&segs, 110.0, 1.0), pitch_keyframes(&segs, 210.0, 0.0));
    }

    /// The timed segments of phonetic input.
    fn timed(input: &str) -> Vec<Segment> {
        let mut segs = segments(&parse(input).unwrap());
        assign_durations(&mut segs, 1.0);
        segs
    }

    fn contour(input: &str) -> Vec<(f64, f64)> {
        pitch_keyframes(&timed(input), 110.0, 1.0)
    }

    fn peak(c: &[(f64, f64)]) -> f64 {
        c.iter().map(|p| p.1).fold(0.0, f64::max)
    }

    /// The accents of a phrase of phonetic input, leaving out its starting pause.
    fn accents(segs: &[Segment], boundary: Boundary) -> Vec<Accent> {
        let phrase: Vec<Segment> = segs.iter().filter(|s| s.phoneme != Phoneme::Pause).cloned().collect();
        phrase_events(&phrase, boundary, &PitchShape::NORMAL)
            .into_iter()
            .filter_map(|(_, event)| match event {
                Event::Accent(accent) => Some(accent),
                Event::Boundary(_) => None,
            })
            .collect()
    }

    #[test]
    fn the_phrase_filter_peaks_at_the_size_of_its_command_after_325_ms() {
        let mut filters = CommandFilters::default();
        filters.phrase(0.057, PHRASE_RISE_FRAMES);
        let response: Vec<f64> = (0..400).map(|_| filters.step()).collect();
        let (at, height) = response.iter().enumerate().fold((0, 0.0), |b, (i, &r)| if r > b.1 { (i, r) } else { b });
        assert!((64..=66).contains(&at), "{at}");
        assert!((height - 0.057).abs() < 0.001, "{height}");
        assert!(response[399] < 0.1 * height, "it falls away afterward");
    }

    #[test]
    fn the_accent_filter_follows_its_pulse_and_returns() {
        let mut filters = CommandFilters::default();
        filters.accent(0.18, 52, 9.0);
        let response: Vec<f64> = (0..300).map(|_| filters.step()).collect();
        assert!((response[51] - 0.18).abs() < 0.01, "{}", response[51]);
        assert!(response[20] > 0.5 * 0.18 && response[20] < response[51], "it rises quickly");
        assert!(response[299].abs() < 0.001, "and comes back");
    }

    #[test]
    fn the_last_stressed_word_gets_the_main_accent_and_earlier_ones_smaller() {
        let segs = timed("[DH AH0] [K AE1 T] [AH0 N D] [D AA1 G IY0] [R AE1 N].");
        let accents = accents(&segs, Boundary::Period);
        let head = HEAD_ACCENT * HEAD_PROMINENCE;
        assert_eq!(accents.len(), 3);
        // One syllable, two, and then the nucleus.
        assert!((accents[0].amount - head * ONE_SYLLABLE_WORD).abs() < 1e-12);
        assert!((accents[1].amount - head).abs() < 1e-12);
        assert_eq!(accents[2].amount, NUCLEAR_ACCENT);
    }

    #[test]
    fn a_phrase_with_no_primary_stress_still_has_a_nucleus() {
        let last_vowel = |input: &str, want: Phoneme| {
            let segs = timed(input);
            let phrase: Vec<Segment> = segs.iter().filter(|s| s.phoneme != Phoneme::Pause).cloned().collect();
            assert_eq!(phrase[nucleus(&phrase).unwrap()].phoneme, want, "{input}");
        };
        last_vowel("[IH0 T] [IH0 Z].", Phoneme::IH);
        last_vowel("[K AE2 T] [IH0 Z].", Phoneme::AE);
    }

    #[test]
    fn light_and_unaccented_words_are_weaker_or_have_no_accent() {
        let mut segs = timed("[V EH1 R IY0] [D AA1 G IY0] [R AE1 N].");
        let word = |segs: &[Segment], vowel: Phoneme| segs.iter().position(|s| s.phoneme == vowel).unwrap();
        let first = word(&segs, Phoneme::EH);
        let normal = accents(&segs, Boundary::Period);
        segs[first].accent = WordAccent::Light;
        let light = accents(&segs, Boundary::Period);
        assert_eq!(normal.len(), 3);
        assert!((light[0].amount - normal[0].amount * LIGHT_WORD).abs() < 1e-12);
        segs[first].accent = WordAccent::Unaccented;
        let none = accents(&segs, Boundary::Period);
        assert_eq!(none.len(), 2);
        assert_eq!(none[0].amount, normal[1].amount);
    }

    #[test]
    fn a_word_that_lost_its_accent_for_being_repeated_stays_without_one() {
        let segs = timed("[D AA2 G IY0] [R AE1 N].");
        assert_eq!(accents(&segs, Boundary::Period).len(), 1);
    }

    #[test]
    fn accents_last_as_long_as_their_vowel_within_limits() {
        assert_eq!(pulse_and_attack(1), (9, 4));
        assert_eq!(pulse_and_attack(20), (20, 5));
        assert_eq!(pulse_and_attack(36), (36, 9));
        assert_eq!(pulse_and_attack(500), (52, 9));
    }

    #[test]
    fn accents_at_the_start_of_a_phrase_are_smaller() {
        // How far the accent lifts the pitch, with the phrase's own rise taken away.
        let rise = |input: &str| {
            let segs = timed(input);
            let phrase: Vec<Segment> = segs.iter().filter(|s| s.phoneme != Phoneme::Pause).cloned().collect();
            let with = phrase_contour(&phrase, Boundary::Comma, 0, 1.0, &PitchShape::NORMAL);
            let vowel = phrase.iter().position(|s| s.accented).unwrap();
            let at = |t: f64| f0_at(&with, t);
            at(phrase[vowel].start + phrase[vowel].duration) - at(phrase[vowel].start)
        };
        let first = rise("[M AE1 N AH0 N] [DH AH0 M].");
        let later = rise("[AH0 N D] [DH AH0 N] [AH0 N D] [DH AH0 N] [AH0 N D] [M AE1 N AH0 N] [DH AH0 M].");
        assert!(first < later, "{first} {later}");
    }

    #[test]
    fn statements_fall_and_questions_rise() {
        let statement = contour("[IH1 T] [R EY1 N Z].");
        let question = contour("[IH1 T] [R EY1 N Z]?");
        let last = |c: &[(f64, f64)]| c[c.len() - 1].1;
        assert!(last(&statement) < 85.0, "{statement:?}");
        assert!(last(&question) > 125.0, "{question:?}");
    }

    #[test]
    fn a_comma_rises_less_than_a_question_and_a_semicolon_falls_less_than_a_period() {
        let words = "[DH AH0] [D AA1 G] [R AE1 N] [IH0 N] [DH AH0] [P AA1 R K]";
        let phrase_end = |mark: &str| {
            let segs = timed(&format!("{words}{mark}"));
            let phrase_end = segs.iter().rfind(|s| s.phoneme != Phoneme::Pause).unwrap().end();
            f0_at(&pitch_keyframes(&segs, 110.0, 1.0), phrase_end)
        };
        assert!(phrase_end(",") < phrase_end("?") && phrase_end(",") > 110.0);
        assert!(phrase_end(".") < phrase_end(";") && phrase_end(";") < 110.0);
    }

    #[test]
    fn wh_questions_fall_like_statements() {
        let contour_as = |boundary: Boundary| {
            let mut phrases = parse("[W EH1 R] [ER0] [Y UW0] [G OW1 IH0 NG].").unwrap();
            phrases[0].boundary = boundary;
            let mut segs = segments(&phrases);
            assign_durations(&mut segs, 1.0);
            pitch_keyframes(&segs, 110.0, 1.0)
        };
        assert_eq!(contour_as(Boundary::WhQuestion), contour_as(Boundary::Period));
        assert!(contour_as(Boundary::Question).last().unwrap().1 > 125.0);
    }

    #[test]
    fn a_question_has_a_small_nucleus_and_its_rise_lands_on_the_vowel() {
        let segs = timed("[IH1 T] [R EY1 N Z]?");
        let accents = accents(&segs, Boundary::Question);
        assert!((accents.last().unwrap().amount - NUCLEAR_ACCENT * QUESTION_NUCLEUS).abs() < 1e-12);
        // The rise is mostly there before the voiceless end would hide it.
        let keys = pitch_keyframes(&segs, 110.0, 1.0);
        let vowel = segs.iter().rfind(|s| s.phoneme.is_vowel()).unwrap();
        assert!(f0_at(&keys, vowel.end()) > f0_at(&keys, vowel.start) * 1.08);
    }

    #[test]
    fn exclamations_sit_higher_with_a_taller_nucleus_and_still_fall() {
        let input = "[IH0 T] [R EY1 N Z] [AH0 G EH1 N]";
        let statement = contour(&format!("{input}."));
        let exclamation = contour(&format!("{input}!"));
        assert!(peak(&exclamation) > 1.15 * peak(&statement), "{statement:?} {exclamation:?}");
        assert!(exclamation.last().unwrap().1 < peak(&exclamation) / 1.5, "{exclamation:?}");
        let segs = timed(&format!("{input}!"));
        assert_eq!(accents(&segs, Boundary::Exclamation).last().unwrap().amount, EMPHATIC_ACCENT);
    }

    #[test]
    fn the_voices_pitch_is_the_average_before_the_fall() {
        let segs = timed("[DH AH0] [OW1 L D] [M IH1 L] [S T UH1 D] [AA1 N] [DH AH0] [EH1 JH] [AH0 V] [W AO1 T ER0].");
        let keys = pitch_keyframes(&segs, 110.0, 1.0);
        let last_stress = segs.iter().rfind(|s| s.accented).unwrap().start;
        let voiced: Vec<f64> = (0..)
            .map(|n| segs[1].start + f64::from(n) * 5.0)
            .take_while(|&t| t < last_stress)
            .filter(|&t| segs.iter().any(|s| s.phoneme.is_voiced() && s.start <= t && t < s.end()))
            .map(|t| f0_at(&keys, t).log2())
            .collect();
        let average = 2f64.powf(voiced.iter().sum::<f64>() / voiced.len() as f64);
        assert!((average - 110.0).abs() < 3.0, "{average}");
    }

    #[test]
    fn phrases_after_a_comma_sit_lower_for_two_of_them() {
        let phrase = "[M IH1 L] [S T UH1 D] [AA1 N]";
        let start_of = |n: usize, count: usize| {
            let input = vec![phrase; count].join(", ") + ".";
            let segs = timed(&input);
            let keys = pitch_keyframes(&segs, 110.0, 1.0);
            let starts: Vec<f64> =
                segs.iter().filter(|s| s.word_initial && s.phoneme == Phoneme::M).map(|s| s.start).collect();
            f0_at(&keys, starts[n] + 5.0)
        };
        let semitones = |low: f64, high: f64| 12.0 * (high / low).log2();
        let step = -CONTINUATION_LEVEL * PITCH_RANGE * 12.0 / UNITS_PER_OCTAVE;
        assert!((semitones(start_of(1, 4), start_of(0, 4)) - step).abs() < 0.6);
        assert!((semitones(start_of(2, 4), start_of(1, 4)) - step).abs() < 0.6);
        assert!(semitones(start_of(3, 4), start_of(2, 4)).abs() < 0.6, "no lower than two");
    }

    #[test]
    fn consonants_nudge_the_pitch_of_the_vowel_after_them() {
        let at_vowel = |input: &str| {
            let segs = timed(input);
            let vowel = segs.iter().find(|s| s.phoneme == Phoneme::AE).unwrap();
            let keys = pitch_keyframes(&segs, 110.0, 1.0);
            f0_at(&keys, vowel.start + 15.0)
        };
        assert!(at_vowel("[AH0 T AE1 N AH0 N AH0].") > at_vowel("[AH0 D AE1 N AH0 N AH0]."));
        assert!(sound_nudge(Phoneme::IY) > sound_nudge(Phoneme::AA));
    }

    /// Pitch keyframes of phonetic input, with a shape.
    fn shaped(input: &str, shape: PitchShape) -> (Vec<Segment>, Vec<(f64, f64)>) {
        let segs = timed(input);
        let keys = shaped_keyframes(&segs, 110.0, 1.0, &shape);
        (segs, keys)
    }

    fn semitones(low: f64, high: f64) -> f64 {
        12.0 * (high / low).log2()
    }

    const NOTHING: PitchShape = PitchShape { baseline_fall: 0.0, hat_rise: 0.0, stress_rise: 0.0, quickness: 40.0 };

    #[test]
    fn the_baseline_falls_as_far_as_the_voice_says() {
        let input = "[M AA1 AA0 AA0 AA0 AA0 AA0 AA0 AA0 AA0 AA0 AA0 AA0 AA0 AA0 AA0 AA1 M].";
        let drop = |baseline_fall: f64| {
            let (segs, keys) = shaped(input, PitchShape { baseline_fall, ..NOTHING });
            let vowels: Vec<&Segment> = segs.iter().filter(|s| s.phoneme == Phoneme::AA).collect();
            let middle = |seg: &Segment| f0_at(&keys, seg.start + seg.duration / 2.0);
            semitones(middle(vowels[14]), middle(vowels[2]))
        };
        assert!(drop(0.0).abs() < 0.05, "{}", drop(0.0));
        assert!(drop(18.0) > 0.8, "{}", drop(18.0));
        assert!((drop(36.0) / drop(18.0) - 2.0).abs() < 0.1, "{} {}", drop(36.0), drop(18.0));
    }

    #[test]
    fn the_hat_rise_sets_the_height_of_the_rise_over_a_phrase() {
        let input = "[M AA0 AA0 AA0 AA0 AA0 AA0 AA0 AA0 AA0 AA1 M].";
        let height = |hat_rise: f64| {
            let (segs, keys) = shaped(input, PitchShape { hat_rise, ..NOTHING });
            let start = segs[2].start + 40.0;
            let end = segs.iter().rfind(|s| s.phoneme == Phoneme::AA).unwrap().start;
            let inside: Vec<(f64, f64)> = keys.iter().filter(|p| p.0 >= start && p.0 < end).copied().collect();
            semitones(f0_at(&keys, start), peak(&inside))
        };
        assert!(height(0.0).abs() < 0.1, "{}", height(0.0));
        assert!(height(18.0) > 0.6, "{}", height(18.0));
        assert!((height(36.0) / height(18.0) - 2.0).abs() < 0.4, "{} {}", height(36.0), height(18.0));
    }

    #[test]
    fn the_stress_rise_sets_the_size_of_the_accents() {
        let input = "[M AE1 N AH0 N] [D AA1 G IY0] [R AE1 N].";
        let segs = timed(input);
        let phrase: Vec<Segment> = segs.iter().filter(|s| s.phoneme != Phoneme::Pause).cloned().collect();
        let amounts = |stress_rise: f64| -> Vec<f64> {
            let shape = PitchShape { stress_rise, ..PitchShape::NORMAL };
            phrase_events(&phrase, Boundary::Period, &shape)
                .into_iter()
                .filter_map(|(_, event)| match event {
                    Event::Accent(accent) => Some(accent.amount),
                    Event::Boundary(_) => None,
                })
                .collect()
        };
        let (normal, half) = (amounts(32.0), amounts(16.0));
        assert_eq!(normal.len(), 3);
        for (a, b) in normal.iter().zip(&half) {
            assert!((a / b - 2.0).abs() < 1e-9);
        }
        // With none, the contour is the same as with no accents to give.
        let (_, none) = shaped(input, PitchShape { stress_rise: 0.0, ..PitchShape::NORMAL });
        let (_, normal) = shaped(input, PitchShape::NORMAL);
        assert_ne!(none, normal);
    }

    #[test]
    fn a_quicker_larynx_gets_there_sooner() {
        assert_eq!(slowness(40.0), 1.0);
        assert!((slowness(0.0) - 1.4).abs() < 1e-9);
        assert!((slowness(100.0) - 0.7).abs() < 1e-9);
        // How far an accent in mid-phrase has risen, soon after it starts.
        let input = "[AH0 AH0 AH0 AH0 AH0 AH0 AH0 AH0 AH0 AH0 AH0 AH0] [M AE1 N AH0 N] [D AA1 G].";
        let risen = |quickness: f64| {
            let (segs, keys) = shaped(input, PitchShape { quickness, ..PitchShape::NORMAL });
            let vowel = segs.iter().find(|s| s.phoneme == Phoneme::AE).unwrap();
            f0_at(&keys, vowel.start + 15.0) / f0_at(&keys, vowel.start - 15.0)
        };
        assert!(
            risen(100.0) > risen(40.0) && risen(40.0) > risen(0.0),
            "{} {} {}",
            risen(100.0),
            risen(40.0),
            risen(0.0)
        );
    }

    #[test]
    fn the_normal_shape_is_the_model_as_it_stands() {
        assert_eq!(BASELINE_MIDDLE + PitchShape::NORMAL.baseline_fall, 39.0);
        assert_eq!(BASELINE_MIDDLE - PitchShape::NORMAL.baseline_fall, 3.0);
        assert_eq!(PHRASE_COMMAND * (PitchShape::NORMAL.hat_rise / PitchShape::NORMAL.hat_rise), PHRASE_COMMAND);
    }

    #[test]
    fn keyframes_move_forward_in_time() {
        let c = contour("[DH AH0] [K W IH1 K] [B R AW1 N] [F AA1 K S], [JH AH1 M P S]!");
        assert!(c.windows(2).all(|w| w[1].0 > w[0].0));
    }

    #[test]
    fn breaks_without_a_pause_fall_a_little_and_start_a_new_contour() {
        let input = "[M AE1 N] [M AE1 N] | [M AE1 N] [M AE1 N].";
        let segs = timed(input);
        let keys = pitch_keyframes(&segs, 110.0, 1.0);
        let break_at = segs.iter().filter(|s| s.phoneme == Phoneme::N).nth(1).unwrap().end();
        let at_break = f0_at(&keys, break_at - 1.0);
        let after = peak(&keys.iter().filter(|p| p.0 > break_at).copied().collect::<Vec<_>>());
        assert!(at_break < 110.0 && after > at_break * 1.1, "{at_break} {after}");
        // Less of a fall than a statement's.
        let end = keys.last().unwrap().1;
        assert!(end < at_break, "{end} {at_break}");
    }

    #[test]
    fn inflection_zero_is_monotone() {
        for input in ["[M AE1 N]?", "[S AE1 T IY1], [K AE1 T IY0]!", "[T IY1 S P UW1 N]."] {
            let segs = timed(input);
            assert!(pitch_keyframes(&segs, 110.0, 0.0).iter().all(|p| (p.1 - 110.0).abs() < 1e-9), "{input}");
        }
    }

    #[test]
    fn the_shape_is_the_same_at_any_pitch() {
        let segs = timed("[M AE1 N] [R AE1 N]?");
        let (low, high) = (pitch_keyframes(&segs, 100.0, 1.0), pitch_keyframes(&segs, 300.0, 1.0));
        for (a, b) in low.iter().zip(&high) {
            assert!((b.1 / a.1 - 3.0).abs() < 1e-9);
        }
    }
}
