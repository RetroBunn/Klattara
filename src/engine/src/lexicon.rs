//! The pronunciation dictionary: CMUdict and the words added to it by hand in
//! src/data/extra_words.dict, compiled into the binary by build.rs and
//! searched in place.
//!
//! CMUdict is Copyright (C) 1993-2015 Carnegie Mellon University, used under
//! its BSD-style license (src/data/cmudict/LICENSE), which requires the
//! notice to be reproduced with binary distributions.

use crate::arpabet::STRESS_SHIFT;
use crate::phoneme::{Phoneme, Stress};

static DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/cmudict.bin"));
static ADDED: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../data/extra_words.dict"));

/// The engine's phoneme for each ARPAbet code, in [`crate::arpabet::SYMBOLS`] order.
const PHONEMES: [Phoneme; 39] = {
    use Phoneme::*;
    [
        AA, AE, AH, AO, AW, AY, B, CH, D, DH, EH, ER, EY, F, G, HH, IH, IY, JH, K, L, M, N, NG, OW, OY, P, R, S, SH, T,
        TH, UH, UW, V, W, Y, Z, ZH,
    ]
};

/// A pronunciation: phonemes with their stress. Consonants are unstressed.
pub type Pronunciation = Vec<(Phoneme, Stress)>;

fn read_u32(at: usize) -> usize {
    u32::from_le_bytes([DATA[at], DATA[at + 1], DATA[at + 2], DATA[at + 3]]) as usize
}

fn count() -> usize {
    read_u32(0)
}

/// Entry `i`: its word, variant number, and encoded phonemes.
fn entry(i: usize) -> (&'static [u8], u8, &'static [u8]) {
    let body = 4 + 4 * count();
    let at = body + read_u32(4 + 4 * i);
    let word_len = DATA[at] as usize;
    let word = &DATA[at + 1..at + 1 + word_len];
    let variant = DATA[at + 1 + word_len];
    let phone_count = DATA[at + 2 + word_len] as usize;
    let phones = &DATA[at + 3 + word_len..at + 3 + word_len + phone_count];
    (word, variant, phones)
}

/// Decodes compiled phoneme codes (see src/arpabet.rs).
pub fn decode(phones: &[u8]) -> Pronunciation {
    phones
        .iter()
        .map(|&code| {
            let phoneme = PHONEMES[(code & ((1 << STRESS_SHIFT) - 1)) as usize];
            let stress = match code >> STRESS_SHIFT {
                2 => Stress::Primary,
                3 => Stress::Secondary,
                _ => Stress::Unstressed,
            };
            (phoneme, stress)
        })
        .collect()
}

/// All pronunciations of a word, in dictionary order. The word must be
/// lowercase, as CMUdict's entries are.
pub fn variants(word: &str) -> Vec<Pronunciation> {
    let key = word.as_bytes();
    let first = partition_point(count(), |i| entry(i).0 < key);
    (first..count()).map(entry).take_while(|(w, _, _)| *w == key).map(|(_, _, phones)| decode(phones)).collect()
}

/// The main pronunciation of a word, if the dictionary has it.
///
/// A few CMUdict entries mark only secondary stress ("instagram",
/// "accredit"), which would leave the word with no pitch accent. Their first
/// secondary stress is taken as primary. Words with no stress at all, such
/// as "a" and "and", are left alone.
pub fn lookup(word: &str) -> Option<Pronunciation> {
    let mut found = variants(word).into_iter().next()?;
    if !found.iter().any(|&(_, s)| s == Stress::Primary)
        && let Some(first) = found.iter_mut().find(|(_, s)| *s == Stress::Secondary)
    {
        first.1 = Stress::Primary;
    }
    Some(found)
}

/// Whether a word was added to the dictionary by hand.
pub fn is_added(word: &str) -> bool {
    ADDED.lines().filter(|line| !line.starts_with('#')).any(|line| line.split_whitespace().next() == Some(word))
}

/// The first index in 0..n for which `before` is false.
fn partition_point(n: usize, before: impl Fn(usize) -> bool) -> usize {
    let (mut lo, mut hi) = (0, n);
    while lo < hi {
        let mid = (lo + hi) / 2;
        if before(mid) { lo = mid + 1 } else { hi = mid }
    }
    lo
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arpabet::SYMBOLS;
    use Phoneme::*;

    #[test]
    fn codes_match_symbols() {
        for (symbol, phoneme) in SYMBOLS.iter().zip(PHONEMES) {
            assert_eq!(Phoneme::parse(symbol).map(|p| p.0), Some(phoneme));
        }
    }

    #[test]
    fn words_added_by_hand_replace_the_dictionary() {
        // Not in CMUdict at all.
        assert_eq!(lookup("meme").unwrap(), [(M, Stress::Unstressed), (IY, Stress::Primary), (M, Stress::Unstressed)]);
        // In CMUdict, wrongly, as "you-ber"; its one entry is gone.
        assert_eq!(variants("uber").len(), 1);
        assert_eq!(lookup("uber").unwrap()[0], (UW, Stress::Primary));
        assert!(is_added("uber") && !is_added("cat") && !is_added("#"));
    }

    #[test]
    fn finds_words_and_variants() {
        assert!(count() > 130_000);
        assert_eq!(
            lookup("hello").unwrap(),
            [(HH, Stress::Unstressed), (AH, Stress::Unstressed), (L, Stress::Unstressed), (OW, Stress::Primary)]
        );
        let read = variants("read");
        assert_eq!(read.len(), 2);
        assert_eq!(read[1][1], (IY, Stress::Primary));
        assert!(lookup("klattaraqz").is_none());
        // Entries with only secondary stress get a primary; unstressed ones don't.
        assert_eq!(lookup("instagram").unwrap()[0], (IH, Stress::Primary));
        assert!(lookup("and").unwrap().iter().all(|&(_, s)| s == Stress::Unstressed));
        assert!(lookup("").is_none());
    }

    #[test]
    fn finds_the_first_and_last_entries() {
        let (first, _, _) = entry(0);
        let (last, _, _) = entry(count() - 1);
        let first = std::str::from_utf8(first).unwrap();
        let last = std::str::from_utf8(last).unwrap();
        assert!(lookup(first).is_some() && lookup(last).is_some());
    }
}
