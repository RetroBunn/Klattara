//! Letter-to-sound rules for words that are not in the dictionary.
//!
//! A joint-sequence model (Bisani & Ney 2002), trained on CMUdict by
//! src/tools/train_lts.py: each word is a chain of units, each a letter with
//! the phonemes (and stress) it stands for, and an M-gram model with
//! Kneser-Ney smoothing scores each unit after the four before it. A word
//! is pronounced by a beam search for the most likely chain that spells it.
//! A table of word endings then fixes the main stress where the ending decides
//! it, since stress can't be found from a few letters alone (Church 1985).
//!
//! On dictionary words held out of training, 65.3% of words match a listed
//! pronunciation exactly, stress included; 71.2% ignoring stress; and 89.7%
//! of phonemes are right. Bisani & Ney's own system reached about 75% of
//! words ignoring stress on this dictionary, with a far larger model.

use crate::lexicon::{Pronunciation, decode};
use crate::phoneme::{Phoneme, Stress};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::LazyLock;

static MODEL: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../data/lts/joint.bin"));
static ENDINGS: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../data/lts/stress_endings.tsv"));
static DEFAULT: LazyLock<JointModel<'static>> = LazyLock::new(|| JointModel::parse(MODEL).with_stress_endings(ENDINGS));

const END: u16 = 0;
const START: u16 = 1;
/// Entry sizes: with backoff weight and children, and at the last order.
const ENTRY_SIZE: usize = 8;
const LAST_ENTRY_SIZE: usize = 3;
/// Stored -log10 values: 255 means -7.
const LOG_RANGE: f32 = 7.0;
/// Partial pronunciations kept at each letter.
const BEAM: usize = 16;

/// A unit's phoneme codes, as a range of the model data. (Its letter is
/// recorded in `by_letter`.)
struct Unit {
    phonemes: Range<usize>,
}

/// One order of the M-gram trie: where its entries start, and their size.
struct Level {
    start: usize,
    entry_size: usize,
}

/// A joint-sequence model read in place from its binary form (see
/// train_lts.py for the layout).
#[doc(hidden)]
pub struct JointModel<'a> {
    data: &'a [u8],
    units: Vec<Unit>,
    /// The units each letter can begin, by letter code.
    by_letter: Vec<Vec<u16>>,
    levels: Vec<Level>,
    /// Word endings that fix the main stress, in syllables from the end of
    /// the word. A model that looks at the last few units can't see how far
    /// a stress is from the end, so "relegation" came out as "RELegation".
    stress_endings: HashMap<&'a str, usize>,
}

/// A partial pronunciation during the search.
#[derive(Clone)]
struct Hypothesis {
    /// The last M-1 units, oldest first.
    history: Vec<u16>,
    score: f32,
    units: Vec<u16>,
}

impl<'a> JointModel<'a> {
    pub fn parse(data: &'a [u8]) -> Self {
        let u32_at = |at: usize| u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]) as usize;
        assert_eq!(&data[..4], b"KJSM", "letter-to-sound model header");
        assert_eq!(u32_at(4), 2, "letter-to-sound model version");
        let order = u32_at(8);
        let unit_count = u32_at(12);
        let mut at = 16;
        let mut units = Vec::with_capacity(unit_count);
        let mut by_letter = vec![Vec::new(); 27];
        for id in 0..unit_count {
            let (letter, len) = (data[at], data[at + 1] as usize);
            units.push(Unit { phonemes: at + 2..at + 2 + len });
            if letter > 0 {
                by_letter[letter as usize].push(id as u16);
            }
            at += 2 + len;
        }
        let mut levels = Vec::with_capacity(order);
        for n in 1..=order {
            let count = u32_at(at);
            let (entry_size, sentinel) = if n < order { (ENTRY_SIZE, 4) } else { (LAST_ENTRY_SIZE, 0) };
            levels.push(Level { start: at + 4, entry_size });
            at += 4 + count * entry_size + sentinel;
        }
        JointModel { data, units, by_letter, levels, stress_endings: HashMap::new() }
    }

    fn u32_at(&self, at: usize) -> usize {
        let d = self.data;
        u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]) as usize
    }

    fn entry(&self, level: usize, index: usize) -> usize {
        self.levels[level].start + index * self.levels[level].entry_size
    }

    fn unit_at(&self, level: usize, index: usize) -> u16 {
        let at = self.entry(level, index);
        u16::from_le_bytes([self.data[at], self.data[at + 1]])
    }

    fn log_prob_at(&self, level: usize, index: usize) -> f32 {
        -(self.data[self.entry(level, index) + 2] as f32) / 255.0 * LOG_RANGE
    }

    fn log_backoff_at(&self, level: usize, index: usize) -> f32 {
        -(self.data[self.entry(level, index) + 3] as f32) / 255.0 * LOG_RANGE
    }

    /// The extensions of entry `index` of `level`, as indices into the next level.
    fn children(&self, level: usize, index: usize) -> Range<usize> {
        let first = |i: usize| self.u32_at(self.entry(level, i) + 4);
        first(index)..first(index + 1)
    }

    /// The extension of an entry by `unit`, if the model has it.
    fn child(&self, level: usize, index: usize, unit: u16) -> Option<usize> {
        if level + 1 >= self.levels.len() {
            return None;
        }
        let range = self.children(level, index);
        let (mut lo, mut hi) = (range.start, range.end);
        while lo < hi {
            let mid = (lo + hi) / 2;
            match self.unit_at(level + 1, mid).cmp(&unit) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return Some(mid),
            }
        }
        None
    }

    /// For each suffix of `history` (length 1, 2, ...), its entry in the
    /// trie, if the model has that context.
    fn contexts(&self, history: &[u16]) -> Vec<Option<usize>> {
        (1..=history.len())
            .map(|len| {
                let suffix = &history[history.len() - len..];
                let mut node = suffix[0] as usize;
                for (level, &unit) in suffix[1..].iter().enumerate() {
                    node = self.child(level, node, unit)?;
                }
                Some(node)
            })
            .collect()
    }

    /// Log10 probability of `unit` after the history whose contexts are given,
    /// backing off to shorter contexts as needed.
    fn log_prob(&self, contexts: &[Option<usize>], unit: u16) -> f32 {
        let mut backoff = 0.0;
        for (level, node) in contexts.iter().enumerate().rev() {
            if let Some(node) = *node {
                if let Some(child) = self.child(level, node, unit) {
                    return backoff + self.log_prob_at(level + 1, child);
                }
                backoff += self.log_backoff_at(level, node);
            }
        }
        backoff + self.log_prob_at(0, unit as usize)
    }

    /// Adds the table of stress endings, as written by train_lts.py: one
    /// ending and its place in the stress, separated by a tab, on each line.
    pub fn with_stress_endings(mut self, table: &'a str) -> Self {
        self.stress_endings = table
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .filter_map(|(ending, place)| Some((ending, place.parse().ok()?)))
            .collect();
        self
    }

    /// Moves the main stress to where the word's ending says it falls.
    fn stress_by_ending(&self, word: &str, mut sounds: Pronunciation) -> Pronunciation {
        let ending =
            (3..=6).rev().filter(|&k| word.len() > k).find_map(|k| self.stress_endings.get(&word[word.len() - k..]));
        let vowels: Vec<usize> = (0..sounds.len()).filter(|&i| sounds[i].0.is_vowel()).collect();
        if let Some(&main) = ending.and_then(|&place| vowels.len().checked_sub(place)).and_then(|at| vowels.get(at)) {
            for &i in &vowels {
                if i == main {
                    sounds[i].1 = Stress::Primary;
                } else if sounds[i].1 == Stress::Primary {
                    sounds[i].1 = Stress::Secondary;
                }
            }
        }
        sounds
    }

    /// Predicts the pronunciation of a lowercase word of the letters a to z.
    /// Returns None for anything else.
    pub fn predict(&self, word: &str) -> Option<Vec<(Phoneme, Stress)>> {
        self.predict_scored(word).map(|(sounds, _)| sounds)
    }

    /// Predicts a pronunciation as [`Self::predict`] does, with how sure the
    /// model is: the average log10 probability per letter, about -1 for most
    /// words and lower the less sure it is. On held-out words, the least sure
    /// fifth were 47% right and the surest fifth 84%.
    pub fn predict_scored(&self, word: &str) -> Option<(Vec<(Phoneme, Stress)>, f32)> {
        if word.is_empty() || !word.bytes().all(|b| b.is_ascii_lowercase()) {
            return None;
        }
        let order = self.levels.len();
        let mut beam = vec![Hypothesis { history: vec![START; order - 1], score: 0.0, units: Vec::new() }];
        for letter in word.bytes() {
            let candidates = &self.by_letter[(letter - b'a' + 1) as usize];
            let mut next = Vec::with_capacity(beam.len() * candidates.len());
            for (h, hypothesis) in beam.iter().enumerate() {
                let contexts = self.contexts(&hypothesis.history);
                for &unit in candidates {
                    next.push((hypothesis.score + self.log_prob(&contexts, unit), h, unit));
                }
            }
            next.sort_by(|a, b| b.0.total_cmp(&a.0));
            // Hypotheses with the same recent history can only diverge by
            // their scores so far, so only the best one is kept.
            let mut seen = HashSet::new();
            let mut kept = Vec::with_capacity(BEAM);
            for (score, h, unit) in next {
                let old = &beam[h];
                let mut history = old.history[1..].to_vec();
                history.push(unit);
                if !seen.insert(history.clone()) {
                    continue;
                }
                let mut units = old.units.clone();
                units.push(unit);
                kept.push(Hypothesis { history, score, units });
                if kept.len() == BEAM {
                    break;
                }
            }
            beam = kept;
        }
        let end = |h: &Hypothesis| h.score + self.log_prob(&self.contexts(&h.history), END);
        let best = beam.iter().max_by(|a, b| end(a).total_cmp(&end(b)))?;
        let codes: Vec<u8> = best
            .units
            .iter()
            .flat_map(|&u| self.data[self.units[u as usize].phonemes.clone()].iter().copied())
            .collect();
        let sounds = self.stress_by_ending(word, with_one_primary_stress(decode(&codes)));
        Some((sounds, end(best) / word.len() as f32))
    }
}

/// Predicts the pronunciation of a lowercase word of the letters a to z
/// with the built-in model. Returns None for anything else.
pub fn predict(word: &str) -> Option<Pronunciation> {
    DEFAULT.predict(word)
}

/// [`predict`], with how sure the model is (see [`JointModel::predict_scored`]).
pub fn predict_scored(word: &str) -> Option<(Pronunciation, f32)> {
    DEFAULT.predict_scored(word)
}

/// Every word has exactly one primary stress: extra ones become secondary,
/// and if there is none, the first secondary (or else the first vowel) is
/// promoted.
fn with_one_primary_stress(mut word: Pronunciation) -> Pronunciation {
    let vowels: Vec<usize> = (0..word.len()).filter(|&i| word[i].0.is_vowel()).collect();
    let mut primaries = vowels.iter().filter(|&&i| word[i].1 == Stress::Primary);
    if primaries.next().is_some() {
        let extra: Vec<usize> = primaries.copied().collect();
        for i in extra {
            word[i].1 = Stress::Secondary;
        }
    } else if let Some(&i) = vowels.iter().find(|&&i| word[i].1 == Stress::Secondary).or(vowels.first()) {
        word[i].1 = Stress::Primary;
    }
    word
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phoneme::Phoneme::*;

    fn primaries(word: &Pronunciation) -> usize {
        word.iter().filter(|(_, s)| *s == Stress::Primary).count()
    }

    #[test]
    fn predicts_regular_words() {
        assert_eq!(predict("cat").unwrap(), [(K, Stress::Unstressed), (AE, Stress::Primary), (T, Stress::Unstressed)]);
        let stop = predict("stop").unwrap();
        assert_eq!(stop.iter().map(|p| p.0).collect::<Vec<_>>(), [S, T, AA, P]);
    }

    #[test]
    fn invented_words_get_one_primary_stress() {
        for word in ["klattara", "zyxquv", "blorptastic", "a", "nnnn"] {
            let p = predict(word).unwrap();
            if p.iter().any(|(ph, _)| ph.is_vowel()) {
                assert_eq!(primaries(&p), 1, "{word}: {p:?}");
            }
        }
        let klattara = predict("klattara").unwrap();
        assert_eq!(&klattara.iter().map(|p| p.0).collect::<Vec<_>>()[..2], [K, L]);
    }

    #[test]
    fn word_endings_fix_the_main_stress() {
        let main = |word: &str| {
            let sounds = predict(word).unwrap();
            sounds.iter().find(|(_, s)| *s == Stress::Primary).unwrap().0
        };
        // "relegation", "blorpification": the stress is on the -a- of -ation.
        assert_eq!(main("relegation"), EY);
        assert_eq!(main("blorpification"), EY);
        // Every word still has one main stress, and short words are left alone.
        let word = predict("relegation").unwrap();
        assert_eq!(primaries(&word), 1);
        assert_eq!(predict("cat"), JointModel::parse(MODEL).predict("cat"));
    }

    #[test]
    fn rejects_non_letters() {
        assert!(predict("").is_none());
        assert!(predict("don't").is_none());
        assert!(predict("Cat").is_none());
    }
}
