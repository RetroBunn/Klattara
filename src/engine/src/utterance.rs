//! Phonetic input, syllable structure, and the allophone rules that turn
//! phonemes into segments in context.
//!
//! Input is written as words in square brackets, each a list of ARPAbet
//! symbols with CMUdict stress digits, separated by punctuation:
//!
//! `[HH AH0 L OW1] [W ER1 L D].`
//!
//! The punctuation marks `,` `;` `:` `.` `?` `!` end a phrase with a pause.
//! `|` ends a phrase without a pause, such as after a long subject noun phrase.

use crate::phoneme::{Manner, Phoneme, Stress};
use crate::phonetics::spec;
use crate::singing::{self, Written};
use std::fmt;

/// What ends a phrase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boundary {
    /// A phrase break with no pause (`|`).
    Minor,
    Comma,
    /// Semicolon or colon.
    Clause,
    Period,
    /// A question that rises at the end: a yes-no question ("Are you
    /// coming?") or one worded as a statement ("You're leaving?").
    Question,
    /// A question that begins with a question word ("Where are you going?"),
    /// which falls at the end like a statement. Written "?" in notation, and
    /// read back as [`Boundary::Question`].
    WhQuestion,
    Exclamation,
}

impl Boundary {
    pub fn has_pause(self) -> bool {
        self != Boundary::Minor
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub phonemes: Vec<(Phoneme, Stress)>,
    /// Whether the last consonant is an ending, the -s of "songs" or the -ed
    /// of "turned", rather than part of the word itself as in "bronze" or
    /// "bend". Endings do not lengthen the vowel (Klatt's rule 9).
    pub ending: bool,
    /// The length and pitch written after each sound, or empty if there are none.
    /// A word that is only a [`Phoneme::Pause`] is a rest.
    pub written: Vec<Written>,
}

impl Word {
    /// A word whose ending is guessed from its sounds alone: a final Z after
    /// a nasal or L is nearly always the -s ending ("songs", "calls").
    pub fn from_sounds(phonemes: Vec<(Phoneme, Stress)>) -> Word {
        let ending = matches!(
            phonemes.as_slice(),
            [.., (before, _), (Phoneme::Z, _)] if matches!(before.manner(), Manner::Nasal | Manner::Liquid)
        );
        Word { phonemes, ending, written: Vec::new() }
    }

    /// A word whose sounds may have lengths and pitches written after them.
    fn with_written(phonemes: Vec<(Phoneme, Stress)>, written: Vec<Written>) -> Word {
        let any = written.iter().any(|w| *w != Written::default());
        Word { written: if any { written } else { Vec::new() }, ..Word::from_sounds(phonemes) }
    }

    /// A rest of this many milliseconds.
    fn rest(duration: u32) -> Word {
        let written = vec![Written { duration: Some(duration), pitch: None }];
        Word { phonemes: vec![(Phoneme::Pause, Stress::Unstressed)], ending: false, written }
    }

    fn is_rest(&self) -> bool {
        matches!(self.phonemes[..], [(Phoneme::Pause, _)])
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Phrase {
    pub words: Vec<Word>,
    pub boundary: Boundary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

/// Parses bracketed phonetic input into phrases. Words after the last
/// punctuation mark form a phrase ending in a period.
pub fn parse(input: &str) -> Result<Vec<Phrase>, ParseError> {
    let mut phrases: Vec<Phrase> = Vec::new();
    let mut words = Vec::new();
    let mut chars = input.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        let boundary = match c {
            '[' => {
                let mut body = String::new();
                loop {
                    match chars.next() {
                        Some((_, ']')) => break,
                        Some((_, c)) => body.push(c),
                        None => {
                            return Err(ParseError(format!(
                                "the word starting at character {} has no closing bracket",
                                at + 1
                            )));
                        }
                    }
                }
                let sounds = singing::split_sounds(&body);
                if sounds.is_empty() {
                    return Err(ParseError(format!("empty word at character {}", at + 1)));
                }
                let (mut phonemes, mut written) = (Vec::new(), Vec::new());
                for sound in sounds {
                    let unknown = || ParseError(format!("unknown phoneme \"{sound}\""));
                    let (symbol, marks) = singing::split_written(sound).ok_or_else(unknown)?;
                    if symbol == singing::REST {
                        let duration = marks.duration.ok_or_else(unknown)?;
                        if !phonemes.is_empty() {
                            words.push(Word::with_written(std::mem::take(&mut phonemes), std::mem::take(&mut written)));
                        }
                        words.push(Word::rest(duration));
                    } else {
                        phonemes.push(Phoneme::parse(symbol).ok_or_else(unknown)?);
                        written.push(marks);
                    }
                }
                if !phonemes.is_empty() {
                    words.push(Word::with_written(phonemes, written));
                }
                continue;
            }
            '|' => Boundary::Minor,
            ',' => Boundary::Comma,
            ';' | ':' => Boundary::Clause,
            '.' => Boundary::Period,
            '?' => Boundary::Question,
            '!' => Boundary::Exclamation,
            c if c.is_whitespace() => continue,
            c => return Err(ParseError(format!("unexpected \"{c}\" at character {}", at + 1))),
        };
        close_phrase(&mut phrases, &mut words, boundary);
    }
    close_phrase(&mut phrases, &mut words, Boundary::Period);
    Ok(phrases)
}

/// Ends the phrase being built at a punctuation mark. Punctuation with no
/// words before it ("?!" or "...") strengthens the previous phrase's ending
/// instead.
pub fn close_phrase(phrases: &mut Vec<Phrase>, words: &mut Vec<Word>, boundary: Boundary) {
    if !words.is_empty() {
        phrases.push(Phrase { words: std::mem::take(words), boundary });
    } else if let Some(last) = phrases.last_mut()
        && ((boundary.has_pause() && !last.boundary.has_pause())
            || matches!(boundary, Boundary::Question | Boundary::WhQuestion))
    {
        last.boundary = boundary;
    }
}

/// Writes phrases in the bracketed ARPAbet notation that [`parse`] reads.
pub fn notation(phrases: &[Phrase]) -> String {
    let mut out = String::new();
    for phrase in phrases {
        for word in &phrase.words {
            if !out.is_empty() {
                out.push(' ');
            }
            let symbols: Vec<String> = word
                .phonemes
                .iter()
                .enumerate()
                .map(|(i, &(p, s))| {
                    let symbol = match (p, p.is_vowel(), s) {
                        (Phoneme::Pause, _, _) => singing::REST.to_string(),
                        (_, false, _) => format!("{p:?}"),
                        (_, true, Stress::Unstressed) => format!("{p:?}0"),
                        (_, true, Stress::Primary) => format!("{p:?}1"),
                        (_, true, Stress::Secondary) => format!("{p:?}2"),
                    };
                    symbol + &singing::write(word.written.get(i).copied().unwrap_or_default())
                })
                .collect();
            out.push_str(&format!("[{}]", symbols.join(" ")));
        }
        out.push_str(match phrase.boundary {
            Boundary::Minor => " |",
            Boundary::Comma => ",",
            Boundary::Clause => ";",
            Boundary::Period => ".",
            Boundary::Question | Boundary::WhQuestion => "?",
            Boundary::Exclamation => "!",
        });
    }
    out
}

/// A phonetic segment in context, ready for timing and synthesis.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub phoneme: Phoneme,
    /// The phoneme before allophone rules: T or D for a flap, which keeps
    /// the vowel before it as long as T or D would ("writer", "rider").
    pub underlying: Phoneme,
    /// Lexical stress of the syllable the segment belongs to.
    pub stress: Stress,
    /// Klatt's segmental stress: vowels of stressed syllables and the
    /// consonants of their onsets.
    pub stressed: bool,
    /// A consonant before the vowel of its syllable.
    pub onset: bool,
    pub word_initial: bool,
    pub word_final: bool,
    /// The last consonant of a word that is an ending (see [`Word::ending`]).
    pub ending: bool,
    pub in_word_final_syllable: bool,
    /// Neither the first nor the last syllable of its word.
    pub in_word_medial_syllable: bool,
    pub polysyllabic_word: bool,
    /// In the last syllable before any phrase boundary.
    pub in_phrase_final_syllable: bool,
    /// In the last syllable before a pause.
    pub in_clause_final_syllable: bool,
    /// The vowel of a primary-stressed syllable, which carries a pitch accent.
    pub accented: bool,
    /// Part of an exclamation, spoken louder and with a wider pitch range.
    pub exclaimed: bool,
    /// The last accented vowel of an exclamation, which gets the most
    /// emphasis: longer, louder and highest in pitch.
    pub emphasized: bool,
    /// Phrases are counted so rules can avoid applying across boundaries.
    pub phrase: usize,
    /// For pauses, the boundary they mark.
    pub boundary: Option<Boundary>,
    /// For stops: whether the closure is released with a burst.
    pub released: bool,
    /// For /t/: made with a glottal closure, so the voice before it creaks.
    pub glottalized: bool,
    /// The length and pitch written after this sound.
    pub written: Written,
    /// In a stretch of speech with a written pitch, which is sung.
    pub singing: bool,
    /// For released stops: time from release to the start of the next
    /// segment's own sound (burst plus any aspiration), ms.
    pub vot: f64,
    /// Filled in by the timing rules, ms.
    pub start: f64,
    pub duration: f64,
}

impl Segment {
    fn pause(boundary: Option<Boundary>, phrase: usize) -> Segment {
        Segment {
            phoneme: Phoneme::Pause,
            underlying: Phoneme::Pause,
            stress: Stress::Unstressed,
            stressed: false,
            onset: false,
            word_initial: false,
            word_final: false,
            ending: false,
            in_word_final_syllable: false,
            in_word_medial_syllable: false,
            polysyllabic_word: false,
            in_phrase_final_syllable: false,
            in_clause_final_syllable: false,
            accented: false,
            exclaimed: false,
            emphasized: false,
            phrase,
            boundary,
            written: Written::default(),
            singing: false,
            released: false,
            glottalized: false,
            vot: 0.0,
            start: 0.0,
            duration: 0.0,
        }
    }

    pub fn end(&self) -> f64 {
        self.start + self.duration
    }
}

/// The position of each phoneme in its word's syllables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SyllablePlace {
    syllable: usize,
    onset: bool,
}

/// Splits a word into syllables by the maximal onset principle: consonants
/// between two vowels go with the second vowel, as many as English allows to
/// begin a word.
fn syllabify(phonemes: &[Phoneme]) -> (Vec<SyllablePlace>, usize) {
    let vowels: Vec<usize> = (0..phonemes.len()).filter(|&i| phonemes[i].is_vowel()).collect();
    if vowels.is_empty() {
        return (vec![SyllablePlace { syllable: 0, onset: true }; phonemes.len()], 1);
    }
    let mut places = vec![SyllablePlace { syllable: 0, onset: true }; phonemes.len()];
    for (n, &v) in vowels.iter().enumerate() {
        places[v] = SyllablePlace { syllable: n, onset: false };
    }
    // Consonants after the last vowel close the last syllable.
    for place in &mut places[vowels[vowels.len() - 1] + 1..] {
        *place = SyllablePlace { syllable: vowels.len() - 1, onset: false };
    }
    for n in 1..vowels.len() {
        let cluster = vowels[n - 1] + 1..vowels[n];
        let split = cluster.clone().find(|&j| is_legal_onset(&phonemes[j..vowels[n]])).unwrap_or(cluster.end);
        for i in cluster {
            places[i] = if i < split {
                SyllablePlace { syllable: n - 1, onset: false }
            } else {
                SyllablePlace { syllable: n, onset: true }
            };
        }
    }
    (places, vowels.len())
}

/// Whether a consonant cluster can begin an English syllable.
fn is_legal_onset(cluster: &[Phoneme]) -> bool {
    use Phoneme::*;
    let approximant = |p: Phoneme| matches!(p, L | R | W | Y);
    match cluster {
        [] => true,
        [c] => *c != NG && c.is_consonant(),
        [S, second] => matches!(second, P | T | K | M | N | L | W | F),
        [first, second] if approximant(*second) => matches!(
            (first, second),
            (P | B | K | G | F, L | R | Y) | (T | D | TH, R | W) | (K | G | S, W) | (SH, R) | (V | M | HH, Y)
        ),
        [S, P | K, L | R | Y] => true,
        [S, T, R] => true,
        [S, K, W] => true,
        _ => false,
    }
}

/// The CMU dictionary writes the first vowel of "parade", "terrain" and
/// "around" as an unstressed ER, but before a stressed vowel speakers say a
/// schwa and start the next syllable with the R: "puh-RADE". As one ER, the
/// schwa blurred into the R, and "parade" was heard as "prayed".
fn schwa_before_r(phonemes: &[(Phoneme, Stress)]) -> Vec<(Phoneme, Stress)> {
    let mut out = Vec::with_capacity(phonemes.len() + 1);
    for (i, &(phoneme, stress)) in phonemes.iter().enumerate() {
        let before_stress = phonemes.get(i + 1).is_some_and(|&(p, s)| p.is_vowel() && s != Stress::Unstressed);
        if phoneme == Phoneme::ER && stress == Stress::Unstressed && before_stress {
            out.extend([(Phoneme::AH, Stress::Unstressed), (Phoneme::R, Stress::Unstressed)]);
        } else {
            out.push((phoneme, stress));
        }
    }
    out
}

/// Builds the segment sequence for an utterance: syllable structure, stress,
/// allophones, and pauses between phrases. The sequence starts with a short
/// pause so voicing can fade in.
pub fn segments(phrases: &[Phrase]) -> Vec<Segment> {
    let mut out = vec![Segment::pause(None, 0)];
    for (phrase_index, phrase) in phrases.iter().enumerate() {
        for (word_index, word) in phrase.words.iter().enumerate() {
            if word.is_rest() {
                out.push(Segment { written: word.written[0], ..Segment::pause(None, phrase_index) });
                continue;
            }
            // Sounds with lengths and pitches written after them stay as written.
            let spoken = if word.written.is_empty() { schwa_before_r(&word.phonemes) } else { word.phonemes.clone() };
            let phonemes: Vec<Phoneme> = spoken.iter().map(|&(p, _)| p).collect();
            let (places, syllables) = syllabify(&phonemes);
            let last_word = word_index == phrase.words.len() - 1;
            let syllable_stress: Vec<Stress> = (0..syllables)
                .map(|n| {
                    spoken
                        .iter()
                        .zip(&places)
                        .find(|((p, _), place)| p.is_vowel() && place.syllable == n)
                        .map_or(Stress::Unstressed, |((_, s), _)| *s)
                })
                .collect();
            for (i, (&(phoneme, _), place)) in spoken.iter().zip(&places).enumerate() {
                let stress = syllable_stress[place.syllable];
                let final_syllable = place.syllable == syllables - 1;
                let is_stressed = stress != Stress::Unstressed;
                let phoneme =
                    if phoneme == Phoneme::AH && stress == Stress::Unstressed { Phoneme::AX } else { phoneme };
                out.push(Segment {
                    phoneme,
                    underlying: phoneme,
                    stress,
                    stressed: is_stressed && (phoneme.is_vowel() || place.onset),
                    onset: place.onset && phoneme.is_consonant(),
                    word_initial: i == 0,
                    word_final: i == spoken.len() - 1,
                    ending: word.ending && i == spoken.len() - 1,
                    in_word_final_syllable: final_syllable,
                    in_word_medial_syllable: place.syllable > 0 && !final_syllable,
                    polysyllabic_word: syllables > 1,
                    in_phrase_final_syllable: last_word && final_syllable,
                    in_clause_final_syllable: last_word && final_syllable && phrase.boundary.has_pause(),
                    accented: phoneme.is_vowel() && stress == Stress::Primary,
                    written: word.written.get(i).copied().unwrap_or_default(),
                    ..Segment::pause(None, phrase_index)
                });
            }
        }
        if phrase.boundary.has_pause() {
            out.push(Segment::pause(Some(phrase.boundary), phrase_index));
        }
    }
    // Speech always ends in a pause, so final stops are released.
    if out.last().is_some_and(|s| s.phoneme != Phoneme::Pause) {
        out.push(Segment::pause(Some(Boundary::Period), phrases.len() - 1));
    }
    apply_allophone_rules(&mut out);
    mark_exclamations(&mut out);
    mark_singing(&mut out);
    out
}

/// Marks each stretch of speech with a written pitch as sung, all of it.
fn mark_singing(segments: &mut [Segment]) {
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
        if segments[start..i].iter().any(|s| s.written.pitch.is_some()) {
            segments[start..i].iter_mut().for_each(|s| s.singing = true);
        }
    }
}

/// Marks the segments of each stretch of speech that ends in an
/// exclamation mark, and its last accented vowel.
fn mark_exclamations(segments: &mut [Segment]) {
    let mut start = 0;
    for end in 0..segments.len() {
        if segments[end].phoneme != Phoneme::Pause {
            continue;
        }
        if segments[end].boundary == Some(Boundary::Exclamation) {
            let stretch = &mut segments[start..end];
            stretch.iter_mut().for_each(|s| s.exclaimed = true);
            if let Some(nucleus) = stretch.iter_mut().rev().find(|s| s.accented) {
                nucleus.emphasized = true;
            }
        }
        start = end + 1;
    }
}

/// The neighbour of segment `i` at offset `delta`, if it is in the same phrase.
fn neighbour(segments: &[Segment], i: usize, delta: isize) -> Option<&Segment> {
    let j = i.checked_add_signed(delta)?;
    let other = segments.get(j)?;
    (other.phoneme != Phoneme::Pause && other.phrase == segments[i].phrase).then_some(other)
}

/// The consonant that segment `i` really is when it is the schwa of a
/// syllabic consonant. Dictionaries write "bottle" and "button" with a
/// schwa, but in American English the L alone makes the syllable after a
/// consonant, and the N alone after T, D, S or Z ("hidden", "lesson").
/// Syllabic L is found once dark L has been chosen.
pub fn syllabic_consonant(segments: &[Segment], i: usize) -> Option<Phoneme> {
    use Phoneme::*;
    let seg = segments.get(i)?;
    let prev = &segments[i.checked_sub(1)?];
    let next = segments.get(i + 1).filter(|n| !n.word_initial)?;
    // Within a word only: "salt and pepper" keeps the vowel of "and".
    if seg.phoneme != AX || seg.word_initial || !prev.phoneme.is_consonant() {
        return None;
    }
    let vowel_after = segments.get(i + 2).is_some_and(|a| a.phoneme.is_vowel() && !a.word_initial);
    match next.phoneme {
        LX => Some(LX),
        N if matches!(prev.phoneme, T | D | S | Z) && !vowel_after => Some(N),
        _ => None,
    }
}

/// Whether the word containing segment `i` has no stressed syllable, as
/// for function words such as "it", "of" and "at".
fn in_unstressed_word(segments: &[Segment], i: usize) -> bool {
    let start = (0..=i).rev().find(|&j| segments[j].word_initial).unwrap_or(0);
    let end = (i..segments.len()).find(|&j| segments[j].word_final).unwrap_or(segments.len() - 1);
    segments[start..=end].iter().all(|s| !s.phoneme.is_vowel() || s.stress == Stress::Unstressed)
}

fn apply_allophone_rules(segments: &mut [Segment]) {
    use Phoneme::*;
    for i in 0..segments.len() {
        let prev = neighbour(segments, i, -1).cloned();
        let next = neighbour(segments, i, 1).cloned();
        let seg = &segments[i];
        // Across words, a flap needs a small unstressed word: at the end of
        // a word, on either side ("get it", "lot of", "at all"); at the start
        // of one, the word itself ("go to bed"). Between two full words the
        // T or D stays, as in careful speech: "sit again" is not "si-dagain",
        // and "say terrain" is not "say derrain".
        let flap_across = || {
            if seg.word_final {
                in_unstressed_word(segments, i) || in_unstressed_word(segments, i + 1)
            } else {
                !seg.word_initial || in_unstressed_word(segments, i)
            }
        };
        let replacement = match seg.phoneme {
            // Klatt (1979) rule 2: T and D become a flap after a non-nasal
            // sonorant and before a vowel without primary stress ("butter",
            // "get it"). Not before a syllabic N: "button" is never "budden".
            T | D
                if prev.as_ref().is_some_and(|p| p.phoneme.is_sonorant() && p.phoneme.manner() != Manner::Nasal)
                    && next.as_ref().is_some_and(|n| n.phoneme.is_vowel() && n.stress != Stress::Primary)
                    && flap_across()
                    && syllabic_consonant(segments, i + 1) != Some(N) =>
            {
                Some(DX)
            }
            // Klatt's rule 1 velarizes L after a vowel unless a stressed
            // vowel follows. Between vowels, though, a dark L blurs into
            // them ("follow" sounded like "foul"), so here only an L that
            // ends a word or comes before a consonant is dark: "bill", "milk",
            // but not "follow", "color" or "believe".
            L if !seg.word_initial
                && prev.as_ref().is_some_and(|p| p.phoneme.is_vowel())
                && (seg.word_final || next.as_ref().is_none_or(|n| !n.phoneme.is_vowel())) =>
            {
                Some(LX)
            }
            // Before R in the same word, General American raises /ɔ/ toward
            // /o/: "four", "corn" (Klatt's OR nucleus).
            AO if !seg.word_final && next.as_ref().is_some_and(|n| n.phoneme == R) => Some(OW),
            _ => None,
        };
        if let Some(phoneme) = replacement {
            segments[i].phoneme = phoneme;
        }
    }
    for i in 0..segments.len() {
        let Some(release) = spec(segments[i].phoneme).release else { continue };
        let prev = neighbour(segments, i, -1).cloned();
        let next = neighbour(segments, i, 1).cloned();
        let seg = &segments[i];
        let into_sonorant = next.as_ref().is_some_and(|n| n.phoneme.is_sonorant() || n.phoneme == HH);
        // Most American speakers glottalize a /t/ that ends a syllable after
        // a vowel or other sonorant: the voice creaks into the closure
        // (Garellek & Seyfarth 2016 found it in about 60% of such /t/s).
        // That holds before a consonant ("bat now", "that car", "button"),
        // before a pause, and, after Klatt's (1979) rule 3, at the end of a
        // word before a stressed vowel ("that egg"). Klatt's rule also drops
        // the release; that took away the cue that tells /t/ from /d/, so the
        // release stays and the creak is added. At normal speed this took
        // the words misheard in the Harvard sentences from 137 to 118; at 485
        // words per minute, voicing pairs before a consonant were told apart
        // more clearly, and "bat" from "bath".
        let glottalized = seg.phoneme == T
            && prev.as_ref().is_some_and(|p| p.phoneme.is_sonorant())
            && (syllabic_consonant(segments, i + 1) == Some(N)
                || !seg.onset
                    && match &next {
                        None => true,
                        Some(n) if n.phoneme.is_consonant() => true,
                        Some(n) => seg.word_final && n.word_initial && n.stressed,
                    });
        let (released, vot) = match &next {
            // Klatt's rule 4 says a voiceless plosive before another is not
            // released. In American English this holds for stop clusters
            // generally ("act", "big dog", "fed chopped"); a release there
            // would put a misleading burst inside the next closure.
            Some(n) if matches!(n.phoneme.manner(), Manner::Stop | Manner::Affricate) => (false, 0.0),
            // Before a syllabic N the closure opens through the nose, with
            // no burst: "button", "hidden".
            _ if syllabic_consonant(segments, i + 1) == Some(N) => (false, 0.0),
            _ if seg.phoneme.is_voiced() => (true, release.burst_ms + 5.0),
            _ if into_sonorant => {
                let after_s = seg.onset && prev.as_ref().is_some_and(|p| p.phoneme == S && p.onset);
                let strength = if after_s {
                    0.2
                } else if seg.onset && seg.stress != Stress::Unstressed {
                    1.0
                } else {
                    // Before an unstressed vowel, aspiration is brief, so the
                    // short vowel keeps its voicing ("canoe", "chicken").
                    0.4
                };
                (true, (release.vot_ms * strength).max(release.burst_ms + 5.0))
            }
            // Before a pause: released, lightly aspirated.
            None => (true, release.burst_ms + 0.4 * release.vot_ms),
            // Before another obstruent or a nasal: a burst only.
            Some(_) => (true, release.burst_ms),
        };
        segments[i].released = released;
        segments[i].glottalized = glottalized;
        segments[i].vot = vot;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Phoneme::*;

    fn phonemes_of(input: &str) -> Vec<Phoneme> {
        segments(&parse(input).unwrap()).iter().map(|s| s.phoneme).filter(|&p| p != Pause).collect()
    }

    #[test]
    fn sounds_can_have_lengths_and_pitches_written_after_them() {
        let phrases = parse("[D AA1<400,20> T<,41>].").unwrap();
        let word = &phrases[0].words[0];
        assert_eq!(word.phonemes.len(), 3);
        assert_eq!(word.written[0], Written::default());
        assert_eq!(word.written[1], Written { duration: Some(400), pitch: Some(20) });
        assert_eq!(word.written[2], Written { duration: None, pitch: Some(41) });
        // Without any, a word is as it always was.
        assert!(parse("[D AA1 T].").unwrap()[0].words[0].written.is_empty());
        assert_eq!(parse("[D AA1<0,0> T].").unwrap()[0].words[0], parse("[D AA1 T].").unwrap()[0].words[0]);
    }

    #[test]
    fn a_rest_divides_a_word_and_needs_a_length() {
        let phrases = parse("[AA1<400,20> _<300> AA1<400,22>].").unwrap();
        assert_eq!(phrases[0].words.len(), 3);
        assert!(phrases[0].words[1].is_rest());
        assert!(parse("[_].").is_err() && parse("[_<,20>].").is_err());
        let timed = segments(&phrases);
        assert_eq!(timed.iter().filter(|s| s.phoneme == Phoneme::Pause && s.written.duration == Some(300)).count(), 1);
    }

    #[test]
    fn bad_lengths_and_pitches_are_errors() {
        for bad in ["[AA1<400,551>].", "[AA1<400>", "[AA1<x>].", "[AA1<400,20].", "[QQ<400,20>]."] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn written_lengths_and_pitches_read_back_the_same() {
        let text = "[D AA1<400,20> T] [_<300>] [AA1<500,220>], [AA1<,20>].";
        assert_eq!(notation(&parse(text).unwrap()), text);
    }

    #[test]
    fn a_written_pitch_makes_its_whole_stretch_sung() {
        let sung = segments(&parse("[D AA1<400,20>] [T IY1], [S IY1].").unwrap());
        let comma = sung.iter().position(|s| s.boundary == Some(Boundary::Comma)).unwrap();
        assert!(sung[1..comma].iter().all(|s| s.singing));
        assert!(sung[comma + 1..].iter().all(|s| !s.singing));
        // A length alone is not singing.
        assert!(segments(&parse("[AA1<400>].").unwrap()).iter().all(|s| !s.singing));
    }

    #[test]
    fn sung_sounds_keep_their_place_through_the_rules() {
        // "er" before a stressed vowel would become two sounds; sung, it stays one.
        let sung = segments(&parse("[ER0<300,20> AY1<300,22>].").unwrap());
        let sounds: Vec<Phoneme> = sung.iter().filter(|s| s.phoneme != Phoneme::Pause).map(|s| s.phoneme).collect();
        assert_eq!(sounds, [Phoneme::ER, Phoneme::AY]);
        assert_eq!(sung[1].written.duration, Some(300));
        assert_eq!(sung[2].written.pitch, Some(22));
    }

    #[test]
    fn parses_words_and_boundaries() {
        let phrases = parse("[HH AH0 L OW1], [W ER1 L D]?").unwrap();
        assert_eq!(phrases.len(), 2);
        assert_eq!(phrases[0].boundary, Boundary::Comma);
        assert_eq!(phrases[1].boundary, Boundary::Question);
        assert_eq!(phrases[1].words[0].phonemes[1], (ER, Stress::Primary));
        assert_eq!(parse("[AA1]").unwrap()[0].boundary, Boundary::Period);
        assert_eq!(parse("[AA1]?!").unwrap()[0].boundary, Boundary::Question);
    }

    #[test]
    fn reports_input_errors() {
        assert!(parse("[HH AH0 L OW1").is_err());
        assert!(parse("[XX]").is_err());
        assert!(parse("hello").is_err());
        assert!(parse("[]").is_err());
    }

    #[test]
    fn syllabifies_by_maximal_onset() {
        // "extra": EH K | S T R AH
        let (places, n) = syllabify(&[EH, K, S, T, R, AH]);
        assert_eq!(n, 2);
        let syllables: Vec<usize> = places.iter().map(|p| p.syllable).collect();
        assert_eq!(syllables, [0, 0, 1, 1, 1, 1]);
        // "panda": P AE N | D AH
        let (places, _) = syllabify(&[P, AE, N, D, AH]);
        assert_eq!(places.iter().map(|p| p.syllable).collect::<Vec<_>>(), [0, 0, 0, 1, 1]);
    }

    #[test]
    fn unstressed_ah_becomes_schwa() {
        assert_eq!(phonemes_of("[AH0 B AW1 T]")[0], AX);
        assert_eq!(phonemes_of("[B AH1 T]")[1], AH);
    }

    #[test]
    fn flaps_t_and_d_between_vowels() {
        assert_eq!(phonemes_of("[B AH1 T ER0]"), [B, AH, DX, ER]);
        // Across words only next to a small unstressed word.
        assert_eq!(phonemes_of("[G EH1 T] [IH0 T]")[2], DX);
        assert_eq!(phonemes_of("[L AA1 T] [AH0 V]")[2], DX);
        assert_eq!(phonemes_of("[S IH1 T] [AH0 G EH1 N]")[2], T);
        // At the start of a word, only in a small unstressed word: "go to
        // bed" flaps, "say terrain" and "they decide" do not.
        assert_eq!(phonemes_of("[G OW1] [T UW0] [B EH1 D]")[2], DX);
        assert_eq!(phonemes_of("[S EY1] [T ER0 EY1 N]")[2], T);
        assert_eq!(phonemes_of("[DH EY1] [D IH0 S AY1 D]")[2], D);
        // Not before a stressed vowel, after a nasal, or across a pause.
        assert_eq!(phonemes_of("[AH0 T AE1 K]")[1], T);
        assert_eq!(phonemes_of("[W IH1 N T ER0]")[3], T);
        assert_eq!(phonemes_of("[G EH1 T], [IH0 T]")[2], T);
    }

    #[test]
    fn unstressed_er_before_a_stressed_vowel_is_schwa_and_r() {
        // "parade", "around": a schwa, then R starts the stressed syllable.
        assert_eq!(phonemes_of("[P ER0 EY1 D]"), [P, AX, R, EY, D]);
        assert_eq!(phonemes_of("[ER0 AW1 N D]"), [AX, R, AW, N, D]);
        // "bird" and "every" keep their ER.
        assert_eq!(phonemes_of("[B ER1 D]"), [B, ER, D]);
        assert_eq!(phonemes_of("[EH1 V ER0 IY0]"), [EH, V, ER, IY]);
    }

    #[test]
    fn velarizes_l_after_vowels() {
        assert_eq!(phonemes_of("[B IH1 L]")[2], LX);
        assert_eq!(phonemes_of("[L IY1 F]")[0], L);
        assert_eq!(phonemes_of("[B IH0 L IY1 V]")[2], L);
        assert_eq!(phonemes_of("[M IH1 L K]")[2], LX);
        // Between vowels it stays clear: "follow", "color".
        assert_eq!(phonemes_of("[F AA1 L OW0]")[2], L);
        assert_eq!(phonemes_of("[K AH1 L ER0]")[2], L);
        assert_eq!(phonemes_of("[W EH1 L] [AH0 N D]")[2], LX);
    }

    #[test]
    fn stops_before_stops_are_unreleased() {
        // "act big dog": K before T, T before B and G before D are held;
        // only the final G is released.
        let segs = segments(&parse("[AE1 K T] [B IH1 G] [D AO1 G]").unwrap());
        let stops: Vec<bool> = segs.iter().filter(|s| s.phoneme.manner() == Manner::Stop).map(|s| s.released).collect();
        assert_eq!(stops, [false, false, true, false, true, true]);
    }

    #[test]
    fn t_after_a_sonorant_is_glottalized_before_consonants_and_pauses() {
        let glottal = |input: &str| -> Vec<bool> {
            segments(&parse(input).unwrap()).iter().filter(|s| s.phoneme == T).map(|s| s.glottalized).collect()
        };
        // "bat now", "that car", "bat.": glottalized, and still released.
        assert_eq!(glottal("[B AE1 T] [N AW1]"), [true]);
        assert_eq!(glottal("[DH AE1 T] [K AA1 R]"), [true]);
        assert_eq!(glottal("[B AE1 T]."), [true]);
        assert!(segments(&parse("[B AE1 T] [N AW1]").unwrap()).iter().any(|s| s.phoneme == T && s.released));
        // "button", and "that egg" before a stressed vowel.
        assert_eq!(glottal("[B AH1 T AH0 N]"), [true]);
        assert_eq!(glottal("[DH AE1 T] [EH1 G]"), [true]);
        // Not at the start of a syllable ("top", "stop"), nor after an
        // obstruent ("last"), nor before an unstressed vowel ("want it").
        assert_eq!(glottal("[T AA1 P] [S T AA1 P]"), [false, false]);
        assert_eq!(glottal("[L AE1 S T]."), [false]);
        assert_eq!(glottal("[W AA1 N T] [IH0 T]."), [false, true]);
    }

    #[test]
    fn aspirates_stressed_voiceless_stops() {
        let segs = segments(&parse("[P IH1 T] [S P IH1 T] [AH0 P AA1 R T]").unwrap());
        let find = |n: usize| segs.iter().filter(|s| s.phoneme == P).nth(n).unwrap().vot;
        assert_eq!(find(0), 45.0); // "pit": fully aspirated
        assert!(find(1) < 15.0); // "spit": unaspirated after S
        assert_eq!(find(2), 45.0); // "apart": stressed syllable onset
    }

    #[test]
    fn marks_exclamations() {
        let segs = segments(&parse("[IH0 T] [IH1 Z]. [IH0 T] [R EY1 N Z] [AH0 G EH1 N]!").unwrap());
        let statement: Vec<&Segment> = segs.iter().filter(|s| s.phrase == 0).collect();
        assert!(statement.iter().all(|s| !s.exclaimed && !s.emphasized));
        let emphasized: Vec<Phoneme> = segs.iter().filter(|s| s.emphasized).map(|s| s.phoneme).collect();
        assert_eq!(emphasized, [EH]);
        assert!(segs.iter().filter(|s| s.phrase == 1 && s.phoneme != Pause).all(|s| s.exclaimed));
    }

    #[test]
    fn marks_stress_and_phrase_position() {
        let segs = segments(&parse("[R AA1 K ER0].").unwrap());
        let er = segs.iter().find(|s| s.phoneme == ER).unwrap();
        assert!(er.in_clause_final_syllable && er.polysyllabic_word && !er.stressed);
        let k = segs.iter().find(|s| s.phoneme == K).unwrap();
        assert!(k.onset && !k.stressed);
        let aa = segs.iter().find(|s| s.phoneme == AA).unwrap();
        assert!(aa.accented && !aa.in_phrase_final_syllable);
    }
}
