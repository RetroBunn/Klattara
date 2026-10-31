//! Text normalization: from ordinary text to phrases of pronounced words.
//!
//! Words come from the pronunciation dictionary, or from letter-to-sound
//! rules when they are not in it. Numbers are expanded into words,
//! abbreviations are read by context (see abbreviations.rs), unknown
//! all-capital abbreviations are spelled out, and function words ("the",
//! "of", "is") lose their stress, so they carry no pitch accent and take
//! their reduced forms. Punctuation marks phrase boundaries, exactly as in
//! phonetic input.

use crate::abbreviations;
use crate::homographs;
use crate::lexicon::{self, Pronunciation};
use crate::lts;
use crate::numbers;
use crate::phoneme::{Phoneme, Stress};
use crate::utterance::{Boundary, Phrase, Word, close_phrase};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    Word(String),
    Number {
        digits: String,
        ordinal: bool,
    },
    /// Single letters written with periods, such as "e.g." or "U.S.A.". The
    /// final period follows as its own token.
    Dotted(String),
    /// The words an abbreviation stands for.
    Expanded(Vec<&'static str>),
    /// Letters to be spelled, such as an initial.
    Spelled(String),
    /// An abbreviation read as written, when expansion is off: by
    /// letter-to-sound rules, not the dictionary, which expands some of them
    /// ("etc" as "et cetera").
    Literal(String),
    Boundary(Boundary),
}

/// The longest word read as one, in characters; the rest of a longer run is
/// the next word. The dictionary's longest word has 28 letters. Pronouncing a
/// word takes time and memory that grow with the square of its length, so a
/// long line of base64 took minutes to start speaking, and could not be
/// cancelled until it did.
const LONGEST_WORD: usize = 64;

pub fn tokenize(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().map(|c| if matches!(c, '\u{2019}' | '\u{2018}') { '\'' } else { c }).collect();
    let at = |i: usize| chars.get(i).copied().unwrap_or(' ');
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        if c.is_alphabetic() && at(i + 1) == '.' && at(i + 2).is_alphabetic() && at(i + 3) == '.' {
            // Single letters with periods: "e.g.", "U.S.A.".
            let mut letters = String::new();
            while at(i).is_alphabetic() && at(i + 1) == '.' && letters.len() < LONGEST_WORD {
                letters.push(at(i));
                i += 2;
            }
            tokens.push(Token::Dotted(letters));
            tokens.push(Token::Boundary(Boundary::Period));
        } else if c.is_alphabetic() {
            // Letters, with apostrophes inside words: "don't", "o'clock".
            while (at(i).is_alphabetic() || (at(i) == '\'' && at(i + 1).is_alphabetic())) && i - start < LONGEST_WORD {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            // Dollars of a country: "US$5" is "five US dollars", "A$5" "five
            // Australian dollars".
            if at(i) == '$'
                && at(i + 1).is_ascii_digit()
                && let Some(country) = dollar_country(&word)
            {
                i = numbers::scan(&chars, i, &mut tokens);
                if let Some(Token::Expanded(units)) = tokens.last_mut()
                    && units.first().is_some_and(|u| u.starts_with("dollar"))
                {
                    units.splice(0..0, country.iter().copied());
                }
                continue;
            }
            if let Some(names) = greek_names(&word) {
                tokens.push(Token::Expanded(names));
                continue;
            }
            // A dot inside a name, as in a web address, is read: "Games.com".
            let next_part = (i + 1..).take_while(|&k| at(k).is_alphabetic()).count();
            if at(i) == '.' && word.chars().count() > 1 && next_part > 1 && at(i + 1).is_lowercase() {
                tokens.push(Token::Word(word));
                tokens.push(Token::Word("dot".to_string()));
                i += 1;
                continue;
            }
            // After a number, a unit keeps its power and what it is per:
            // "km²", "m3", "km/h".
            let after_number = matches!(tokens.last(), Some(Token::Number { .. }));
            if after_number && abbreviations::unit_name(&chars[start..i].iter().collect::<String>()).is_some() {
                i = unit_end(&chars, i);
            }
            tokens.push(Token::Word(chars[start..i].iter().collect()));
        } else if c == '/' && matches!(tokens.last(), Some(Token::Number { .. })) && unit_end(&chars, i) > i {
            // A rate after a number: "459.0/km²" is "per square kilometer".
            i = unit_end(&chars, i);
            tokens.push(Token::Word(chars[start..i].iter().collect()));
        } else if c == '\u{b0}' && matches!(tokens.last(), Some(Token::Number { .. })) {
            // Degrees: "20°C" is "twenty degrees Celsius".
            let one = matches!(tokens.last(), Some(Token::Number { digits, .. }) if digits == "1");
            let mut words = vec![if one { "degree" } else { "degrees" }];
            match at(i + 1) {
                'C' if !at(i + 2).is_alphabetic() => words.push("Celsius"),
                'F' if !at(i + 2).is_alphabetic() => words.push("Fahrenheit"),
                _ => {}
            }
            i += words.len();
            tokens.push(Token::Expanded(words));
        } else if numbers::starts(&chars, i) {
            // Numbers, times, ranges, dates, fractions and money.
            i = numbers::scan(&chars, i, &mut tokens);
        } else if c == '&' {
            tokens.push(Token::Word("and".to_string()));
            i += 1;
        } else if c == '#' && (at(i + 1).is_ascii_digit() || (at(i + 1) == ' ' && at(i + 2).is_ascii_digit())) {
            tokens.push(Token::Expanded(vec!["number"]));
            i += 1;
        } else if c == '@' && at(i + 1).is_alphanumeric() && i > 0 && chars[i - 1].is_alphanumeric() {
            // An email address: "name@example.com", with "at" said in full.
            tokens.push(Token::Expanded(vec!["at"]));
            i += 1;
        } else if c == '\''
            && at(i + 1) == 's'
            && !at(i + 2).is_alphabetic()
            && matches!(tokens.last(), Some(Token::Number { .. }))
        {
            // A number's possessive: "1984's".
            tokens.push(Token::Word("'s".to_string()));
            i += 2;
        } else if let Some((_, words)) = VULGAR_FRACTIONS.iter().find(|(f, _)| *f == c) {
            let mut words = words.to_vec();
            if matches!(tokens.last(), Some(Token::Number { .. })) {
                words.insert(0, "and");
            } else if words[0] == "a" {
                words[0] = "one";
            }
            tokens.push(Token::Expanded(words));
            i += 1;
        } else if matches!(c, '-' | '\u{2212}')
            && at(i + 1).is_ascii_digit()
            && !matches!(tokens.last(), Some(Token::Number { .. }))
            && !(i > 0 && chars[i - 1].is_alphanumeric())
        {
            // A negative number: "-2".
            tokens.push(Token::Expanded(vec!["minus"]));
            i += 1;
        } else if matches!(c, '-' | '\u{2013}')
            && i > 0
            && chars[i - 1] == ' '
            && at(i + 1) == ' '
            && at(i + 2).is_ascii_digit()
            && matches!(tokens.last(), Some(Token::Number { .. } | Token::Expanded(_) | Token::Spelled(_)))
        {
            // A range with spaces, as of times: "1pm - 4pm", "1990 \u{2013} 1995".
            tokens.push(Token::Word("to".to_string()));
            i += 1;
        } else if c == '-'
            && (at(i + 1) == '-' || (i > 0 && chars[i - 1] == ' ' && at(i + 1) == ' '))
            && chars[i + 1..].iter().any(|&after| !after.is_whitespace() && after != '-')
        {
            // A dash typed as hyphens: "yes -- no", "yes - no". A hyphen in a word
            // has no space before it, and at the end of the text nothing follows.
            tokens.push(Token::Boundary(Boundary::Clause));
            while at(i) == '-' {
                i += 1;
            }
        } else if c == '%' && matches!(tokens.last(), Some(Token::Number { .. })) {
            tokens.push(Token::Expanded(vec!["percent"]));
            i += 1;
        } else {
            let boundary = match c {
                ',' | '(' | ')' => Some(Boundary::Comma),
                ';' | ':' | '\u{2013}' | '\u{2014}' => Some(Boundary::Clause),
                '.' | '\u{2026}' => Some(Boundary::Period),
                '?' => Some(Boundary::Question),
                '!' => Some(Boundary::Exclamation),
                _ => None,
            };
            if let Some(boundary) = boundary {
                tokens.push(Token::Boundary(boundary));
            }
            i += 1;
        }
    }
    tokens
}

/// The end of a unit's power and "per" part, from just after its letters:
/// past "²" in "km²", "2" in "km2", and "/h" in "km/h". At a "/" itself,
/// only a known unit follows, as in "/km²".
fn unit_end(chars: &[char], i: usize) -> usize {
    let at = |k: usize| chars.get(k).copied().unwrap_or(' ');
    let power = |k: usize| {
        let mark = matches!(at(k), '\u{b2}' | '\u{b3}') || (matches!(at(k), '2' | '3') && !at(k + 1).is_alphanumeric());
        if mark { k + 1 } else { k }
    };
    let mut end = if at(i) == '/' { i } else { power(i) };
    if at(end) == '/' && at(end + 1).is_alphabetic() {
        let mut k = end + 1;
        while at(k).is_alphabetic() {
            k += 1;
        }
        if abbreviations::unit_name(&chars[end + 1..k].iter().collect::<String>()).is_some() {
            end = power(k);
        }
    }
    end
}

pub const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
pub const TENS: [&str; 10] = ["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];
const SCALES: [&str; 5] = ["", "thousand", "million", "billion", "trillion"];

fn below_thousand(n: u64, out: &mut Vec<String>) {
    let (hundreds, rest) = ((n / 100) as usize, (n % 100) as usize);
    if hundreds > 0 {
        out.push(ONES[hundreds].into());
        out.push("hundred".into());
    }
    if rest >= 20 {
        out.push(TENS[rest / 10].into());
        if rest % 10 > 0 {
            out.push(ONES[rest % 10].into());
        }
    } else if rest > 0 {
        out.push(ONES[rest].into());
    }
}

/// American cardinal numbers, without "and": 1,205 is "one thousand two
/// hundred five".
fn cardinal(n: u64) -> Vec<String> {
    if n == 0 {
        return vec!["zero".into()];
    }
    let mut groups = Vec::new();
    let mut rest = n;
    while rest > 0 {
        groups.push(rest % 1000);
        rest /= 1000;
    }
    let mut out = Vec::new();
    for (scale, &group) in groups.iter().enumerate().rev() {
        if group > 0 {
            below_thousand(group, &mut out);
            if scale > 0 {
                out.push(SCALES[scale].into());
            }
        }
    }
    out
}

/// Years read in pairs: 1999 is "nineteen ninety nine", 1905 "nineteen oh five".
fn year(n: u64) -> Vec<String> {
    let mut out = cardinal(n / 100);
    match n % 100 {
        0 => out.push("hundred".into()),
        low @ 1..=9 => {
            out.push("oh".into());
            out.extend(cardinal(low));
        }
        low => out.extend(cardinal(low)),
    }
    out
}

fn ordinalize(words: &mut [String]) {
    let Some(last) = words.last_mut() else { return };
    *last = match last.as_str() {
        "one" => "first".into(),
        "two" => "second".into(),
        "three" => "third".into(),
        "five" => "fifth".into(),
        "eight" => "eighth".into(),
        "nine" => "ninth".into(),
        "twelve" => "twelfth".into(),
        word if word.ends_with('y') => format!("{}ieth", &word[..word.len() - 1]),
        word => format!("{word}th"),
    };
}

fn digit_by_digit(digits: &str) -> Vec<String> {
    digits.chars().filter_map(|c| c.to_digit(10)).map(|d| ONES[d as usize].to_string()).collect()
}

/// The words for a number as written: grouping commas are ignored, points
/// read as "point" followed by single digits, and long numbers or ones with
/// leading zeros are read digit by digit.
fn number_words(digits: &str, ordinal: bool) -> Vec<String> {
    let mut parts = digits.split('.');
    let whole: String = parts.next().unwrap_or("").chars().filter(char::is_ascii_digit).collect();
    let mut out = if whole.is_empty() {
        // ".267" is "point two six seven".
        Vec::new()
    } else if whole.len() > 15
        || (whole.len() > 9 && !digits.contains(','))
        || (whole.len() > 1 && whole.starts_with('0'))
    {
        digit_by_digit(&whole)
    } else {
        let n: u64 = whole.parse().unwrap_or(0);
        // A four-digit number reads as a year unless written as a quantity:
        // "nineteen ninety nine", "twenty fourteen", but "two thousand five".
        let plain = !digits.contains(['.', ',']) && !ordinal;
        let year_like = (1100..=1999).contains(&n) || (2010..=2099).contains(&n);
        if plain && whole.len() == 4 && year_like { year(n) } else { cardinal(n) }
    };
    if ordinal {
        ordinalize(&mut out);
    }
    for part in parts {
        out.push("point".into());
        out.extend(digit_by_digit(part));
    }
    out
}

/// Words that normally carry no stress in running speech: articles,
/// prepositions, conjunctions, personal pronouns and possessives, and
/// auxiliary verbs. Sorted for binary search.
pub const FUNCTION_WORDS: &[&str] = &[
    "a", "about", "above", "across", "after", "against", "along", "am", "among", "an", "and", "are", "around", "as",
    "at", "be", "been", "before", "behind", "being", "below", "beneath", "beside", "between", "beyond", "but", "by",
    "can", "could", "did", "do", "does", "during", "for", "from", "had", "has", "have", "he", "her", "him", "his", "i",
    "if", "in", "into", "is", "it", "its", "may", "me", "might", "must", "my", "nor", "of", "on", "onto", "or", "our",
    "shall", "she", "should", "than", "the", "their", "them", "they", "through", "till", "to", "toward", "towards",
    "under", "until", "upon", "us", "was", "we", "were", "will", "with", "within", "without", "would", "you", "your",
];

/// Reduced forms that function words take in running speech.
const WEAK_FORMS: &[(&str, &str)] = &[
    ("a", "AH0"),
    ("an", "AH0 N"),
    ("and", "AH0 N D"),
    ("are", "ER0"),
    ("as", "AH0 Z"),
    ("at", "AH0 T"),
    ("can", "K AH0 N"),
    ("for", "F ER0"),
    ("from", "F R AH0 M"),
    ("of", "AH0 V"),
    ("than", "DH AH0 N"),
    ("the", "DH AH0"),
    ("them", "DH AH0 M"),
    ("to", "T AH0"),
    ("was", "W AH0 Z"),
    ("were", "W ER0"),
    ("your", "Y ER0"),
];

/// Words after which a function word is a noun: "a can", "his might".
const NOUN_BEFORE: &[&str] = &["a", "an", "every", "her", "his", "its", "my", "our", "the", "their", "your"];

/// Before a vowel these keep a full vowel: "the apple" [DH IY0], "to eat".
const BEFORE_VOWEL: &[(&str, &str)] = &[("the", "DH IY0"), ("to", "T UW0")];

const LETTER_NAMES: [&str; 26] = [
    "EY1",
    "B IY1",
    "S IY1",
    "D IY1",
    "IY1",
    "EH1 F",
    "JH IY1",
    "EY1 CH",
    "AY1",
    "JH EY1",
    "K EY1",
    "EH1 L",
    "EH1 M",
    "EH1 N",
    "OW1",
    "P IY1",
    "K Y UW1",
    "AA1 R",
    "EH1 S",
    "T IY1",
    "Y UW1",
    "V IY1",
    "D AH1 B AH0 L Y UW0",
    "EH1 K S",
    "W AY1",
    "Z IY1",
];

/// Parses space-separated ARPAbet symbols from the tables above.
fn arpabet(symbols: &str) -> Pronunciation {
    symbols
        .split_whitespace()
        .filter_map(Phoneme::parse)
        .map(|(p, s)| (p, if p.is_vowel() { s } else { Stress::Unstressed }))
        .collect()
}

fn table_lookup(table: &[(&str, &str)], word: &str) -> Option<Pronunciation> {
    table.iter().find(|(w, _)| *w == word).map(|(_, p)| arpabet(p))
}

/// Spells a word letter by letter, stressing the last letter as in "I B M".
fn spell(word: &str) -> Pronunciation {
    let letters: Vec<usize> = word
        .chars()
        .filter(char::is_ascii_alphabetic)
        .map(|c| (c.to_ascii_lowercase() as u8 - b'a') as usize)
        .collect();
    let mut out = Pronunciation::new();
    for (n, &letter) in letters.iter().enumerate() {
        let last = n == letters.len() - 1;
        out.extend(
            arpabet(LETTER_NAMES[letter])
                .into_iter()
                .map(|(p, s)| (p, if s == Stress::Primary && !last { Stress::Secondary } else { s })),
        );
    }
    out
}

/// Appends an ending whose sound depends on the stem's last phoneme: plural
/// and possessive -s, past tense -ed.
fn attach(mut stem: Pronunciation, ending: &str) -> Pronunciation {
    use Phoneme::*;
    let last = stem.last().map(|&(p, _)| p);
    let sibilant = last.is_some_and(|p| matches!(p, S | Z | SH | ZH | CH | JH));
    let voiceless = last.is_some_and(|p| !p.is_voiced());
    let suffix = match ending {
        "s" if sibilant => "IH0 Z",
        "s" if voiceless => "S",
        "s" => "Z",
        "ed" if last.is_some_and(|p| matches!(p, T | D)) => "IH0 D",
        "ed" if voiceless => "T",
        "ed" => "D",
        other => other,
    };
    stem.extend(arpabet(suffix));
    stem
}

/// Suffixes to take off a word that is not in the dictionary, longest first:
/// "klatts", "rebooted", "fairytalelike". These are the ones that beat the
/// letter-to-sound model on held-out dictionary words. The model did better
/// on -ing, -er, -ly, -ment, -able, -ism and -ize, which are left to it.
const SUFFIXES: &[(&str, &str)] = &[
    ("hood", "HH UH2 D"),
    ("like", "L AY2 K"),
    ("ness", "N AH0 S"),
    ("less", "L AH0 S"),
    ("est", "AH0 S T"),
    ("'s", "s"),
    ("s'", "s"),
    ("ed", "ed"),
    ("es", "s"),
    ("s", "s"),
];

fn with_suffix(word: &str) -> Option<Pronunciation> {
    for &(suffix, ending) in SUFFIXES {
        let Some(stem) = word.strip_suffix(suffix) else { continue };
        if stem.len() < 2 {
            continue;
        }
        // Before a suffix that starts with a vowel, one consonant after one
        // vowel means a dropped e: without one the consonant would double,
        // so "coded" is "code", not "cod" ("codded").
        let bytes = stem.as_bytes();
        let n = bytes.len();
        let dropped_e = suffix.starts_with(['e', 'i'])
            && !b"aeiouwxy".contains(&bytes[n - 1])
            && b"aeiouy".contains(&bytes[n - 2])
            && (n < 3 || !b"aeiou".contains(&bytes[n - 3]));
        let mut candidates = vec![stem.to_string(), format!("{stem}e")];
        if dropped_e {
            candidates.swap(0, 1);
        }
        // A doubled consonant undone ("zipped"), or y restored ("tried").
        if n >= 3 && bytes[n - 1] == bytes[n - 2] {
            candidates.push(stem[..n - 1].to_string());
        }
        if let Some(base) = stem.strip_suffix('i') {
            candidates.push(format!("{base}y"));
        }
        // -es follows only a hissing sound, an o or a y made i, or else it
        // is the -s of a stem with an e: "memes" isn't "mem" and "-es".
        if suffix == "es" {
            candidates
                .retain(|c| c.ends_with(['s', 'x', 'z', 'o', 'y', 'e']) || c.ends_with("ch") || c.ends_with("sh"));
        }
        if let Some(base) = candidates.iter().find_map(|c| lexicon::lookup(c)) {
            return Some(attach(base, ending));
        }
    }
    None
}

/// How many inflected or derived forms of a word the dictionary lists. Real
/// words have them ("land", "lands", "landed"), and names rarely do ("caro").
fn relatives(word: &str) -> usize {
    let endings = ["s", "es", "ed", "d", "ing", "er", "ers", "ly", "y", "ful", "ness", "less"];
    let mut forms = endings.map(|ending| format!("{word}{ending}")).to_vec();
    if let Some(stem) = word.strip_suffix('e') {
        forms.extend(["ing", "able", "ation"].map(|ending| format!("{stem}{ending}")));
    }
    if let [.., vowel, last] = word.as_bytes()
        && b"aeiou".contains(vowel)
        && !b"aeiouwxy".contains(last)
    {
        forms.extend(["ing", "ed", "er", "y"].map(|ending| format!("{word}{}{ending}", *last as char)));
    }
    forms.iter().filter(|form| lexicon::lookup(form).is_some()).count()
}

/// A word made of two real words, each of at least `shortest` letters and
/// with at least `related` relatives in the dictionary, as the two words would
/// be said, with the main stress on the first. If it can be split more than
/// one way, the most even split is taken.
fn compound(word: &str, shortest: usize, related: usize) -> Option<Pronunciation> {
    let part = |text: &str| if relatives(text) >= related { lexicon::lookup(text) } else { None };
    let mut best: Option<(usize, Pronunciation)> = None;
    for k in shortest..=word.len().saturating_sub(shortest) {
        let (first, second) = word.split_at(k);
        let evenness = k.min(word.len() - k);
        if best.as_ref().is_none_or(|(even, _)| evenness > *even)
            && let (Some(mut sounds), Some(rest)) = (part(first), part(second))
        {
            sounds.extend(rest.into_iter().map(|(p, s)| (p, if s == Stress::Primary { Stress::Secondary } else { s })));
            best = Some((evenness, sounds));
        }
    }
    best.map(|(_, sounds)| sounds)
}

/// How unsure the letter-to-sound model must be, as its average log10
/// probability per letter, before a word is tried as a compound: of parts of
/// at least four letters with two relatives, or, when it is very unsure, of
/// at least three letters with one.
const UNSURE: f32 = -1.1;
const VERY_UNSURE: f32 = -1.6;

/// Sounds out a word the dictionary does not have. The model handles most
/// compounds well, and splitting every word that could be one is much worse:
/// "rather" isn't "rat" and "her", nor "legend" "leg" and "end". But where the
/// model is unsure, the parts were right 60% of the time against its 44% on
/// held-out dictionary words: "webcam", not "WEBcum".
fn sound_out(word: &str) -> Option<Pronunciation> {
    let (sounds, sureness) = lts::predict_scored(word)?;
    let parts = if sureness < VERY_UNSURE {
        compound(word, 3, 1)
    } else if sureness < UNSURE {
        compound(word, 4, 2)
    } else {
        None
    };
    Some(parts.unwrap_or(sounds))
}

/// Replaces accented Latin letters with plain ones, so that "café" and
/// "naïve" are found in the dictionary, and names such as "Władysław" or
/// "Dvořák" are sounded out rather than spelled.
fn fold_accents(word: &str) -> String {
    word.chars()
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ą' | 'ă' => 'a',
            'ç' | 'ć' | 'č' => 'c',
            'ď' | 'đ' | 'ð' => 'd',
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' | 'ė' => 'e',
            'ğ' => 'g',
            'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ı' | 'į' => 'i',
            'ł' | 'ľ' | 'ĺ' => 'l',
            'ñ' | 'ń' | 'ň' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => 'o',
            'ř' | 'ŕ' => 'r',
            'ś' | 'š' | 'ş' | 'ș' => 's',
            'ť' | 'ţ' | 'ț' => 't',
            'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ű' | 'ů' | 'ų' => 'u',
            'ý' | 'ÿ' => 'y',
            'ź' | 'ż' | 'ž' => 'z',
            other => other,
        })
        .collect()
}

/// Days of the week end in "day", as Eloquence and DECtalk say them. The
/// dictionary lists "dee" first for most ("Monday", "Saturday") but "day" for
/// others ("Thursday", "Sunday"), so the order is no guide.
fn end_in_day(word: &str, pronunciation: &mut Pronunciation) {
    if !(word.ends_with("day") || word.ends_with("days")) {
        return;
    }
    if let Some(v) = pronunciation.iter().rposition(|(p, _)| p.is_vowel())
        && v > 0
        && pronunciation[v].0 == Phoneme::IY
        && pronunciation[v - 1].0 == Phoneme::D
    {
        pronunciation[v] = (Phoneme::EY, Stress::Secondary);
    }
}

/// Short words that stay words when written in capitals inside ordinary
/// text. Any other two-letter word in capitals there is an abbreviation and
/// is spelled: "the US", "Stanford, CA", "photo ID".
const CAPITAL_WORDS: &[&str] = &[
    "AN", "AS", "AT", "BE", "BY", "DO", "GO", "HE", "IF", "IN", "IS", "ME", "MY", "NO", "OF", "OK", "ON", "OR", "SO",
    "TO", "UP", "WE",
];

/// Whether a word is a two-letter abbreviation in capitals, read letter by
/// letter. In text written mostly in capitals, as headlines are, short
/// words in capitals are ordinary words: "LET US KNOW".
fn two_letter_abbreviation(word: &str, shouting: bool) -> bool {
    !shouting && word.len() == 2 && word.chars().all(|c| c.is_ascii_uppercase()) && !CAPITAL_WORDS.contains(&word)
}

/// Two consonants that can begin an English word: an all-capital word that
/// starts with any other pair, such as "NGO" or "NHL", is spelled.
const ONSETS: &[&str] = &[
    "bl", "br", "ch", "cl", "cr", "dr", "dw", "fl", "fr", "gl", "gn", "gr", "kl", "kn", "kr", "ph", "pl", "pr", "ps",
    "qu", "sc", "sh", "sk", "sl", "sm", "sn", "sp", "sq", "st", "sw", "th", "tr", "tw", "wh", "wr",
];

/// The pronunciation of one written word.
fn pronounce(word: &str) -> Pronunciation {
    let lower = fold_accents(&word.to_lowercase());
    let capitals = |w: &str| w.len() >= 2 && w.chars().all(|c| c.is_ascii_uppercase());
    // Plurals of abbreviations: "CDs", "URLs".
    if let Some(stem) = word.strip_suffix('s')
        && capitals(stem)
    {
        return attach(spell(&stem.to_lowercase()), "s");
    }
    // Short abbreviations that cannot be words, though the dictionary may
    // list a name spelled the same ("NGO", "Ngo"). A word added by hand is
    // there to be said as a word ("JSON").
    let vowel = |c: char| "aeiou".contains(c);
    let start: String = lower.chars().take(2).collect();
    if capitals(word)
        && word.len() <= 4
        && !start.chars().any(vowel)
        && !ONSETS.contains(&start.as_str())
        && !lexicon::is_added(&lower)
    {
        return spell(&lower);
    }
    if let Some(mut found) = lexicon::lookup(&lower) {
        end_in_day(&lower, &mut found);
        return found;
    }
    // Possessives of any word, including spelled ones: "NVDA's", "dogs'".
    if lower.ends_with("'s") && word.len() > 2 {
        return attach(pronounce(&word[..word.len() - 2]), "s");
    }
    if let Some(stem) = word.strip_suffix('\'') {
        return pronounce(stem);
    }
    let all_capitals = word.chars().all(|c| c.is_uppercase()) && word.chars().count() > 1;
    if all_capitals {
        return spell(&lower);
    }
    american(&lower).or_else(|| with_suffix(&lower)).or_else(|| sound_out(&lower)).unwrap_or_else(|| spell(&lower))
}

/// First parts that share the stress with the rest of the word, as in
/// "outside", "downtown" and "overseas", or are prefixes, as in
/// "dislocation", so that the word is not stressed like a compound.
const LEVEL_FIRST_PARTS: &[&str] = &[
    "along", "anti", "arch", "back", "dis", "down", "forth", "half", "here", "inter", "mid", "mis", "multi", "near",
    "non", "off", "out", "over", "pre", "self", "there", "trans", "under", "well", "where", "with",
];
/// Endings that keep a word from being stressed like a compound:
/// "nineteen", "worldwide", "Kafkaesque", "procreation".
const LEVEL_ENDINGS: &[&str] = &["teen", "teens", "teenth", "teenths", "wide", "esque", "ation", "ations"];
/// Words made of two words that dictionaries give level stress, as
/// Merriam-Webster marks them: "a WIDEspread problem", "it's wideSPREAD".
const LEVEL_WORDS: &[&str] = &[
    "easygoing",
    "farfetched",
    "goodwill",
    "grassroots",
    "headfirst",
    "henceforth",
    "homegrown",
    "lukewarm",
    "mankind",
    "newfound",
    "shortsighted",
    "softspoken",
    "straightforward",
    "upstate",
    "widespread",
    "worthwhile",
    "yearlong",
];

/// Whether a word is two dictionary words run together, such as "Microsoft"
/// or "baseball": some split of its spelling gives two listed words, of at
/// least three and four letters, whose sounds make up the word's own, apart
/// from an -s ending.
fn is_compound(text: &str, phonemes: &Pronunciation) -> bool {
    let stem = text.strip_suffix("'s").or_else(|| text.strip_suffix("s'")).unwrap_or(text);
    if LEVEL_WORDS.contains(&stem) || LEVEL_ENDINGS.iter().any(|e| stem.ends_with(e)) {
        return false;
    }
    let sounds: Vec<Phoneme> = phonemes.iter().map(|&(p, _)| p).collect();
    let sounds_of = |part: &str| -> Vec<Vec<Phoneme>> {
        lexicon::variants(part).iter().map(|v| v.iter().map(|&(p, _)| p).collect()).collect()
    };
    (3..=stem.len().saturating_sub(4)).filter(|&k| stem.is_char_boundary(k)).any(|k| {
        let (first, last) = stem.split_at(k);
        if LEVEL_FIRST_PARTS.contains(&first) {
            return false;
        }
        let lasts = sounds_of(last);
        sounds_of(first).iter().any(|a| {
            lasts.iter().any(|b| {
                let joined = [a.as_slice(), b.as_slice()].concat();
                let rest = sounds.len().checked_sub(joined.len());
                sounds.starts_with(&joined) && (rest == Some(0) || (text.ends_with('s') && rest <= Some(2)))
            })
        })
    })
}

/// Leaves a word one primary stress, and so one pitch accent: its first
/// primary or its last. The others become secondary.
fn keep_one_primary(phonemes: &mut Pronunciation, first: bool) {
    let primaries: Vec<usize> = (0..phonemes.len()).filter(|&i| phonemes[i].1 == Stress::Primary).collect();
    let keep = if first { primaries.first() } else { primaries.last() };
    if let Some(&keep) = keep {
        for i in primaries.into_iter().filter(|&i| i != keep) {
            phonemes[i].1 = Stress::Secondary;
        }
    }
}

/// British endings and their American forms, which the dictionary lists:
/// "organisation", "colour", "centre", "travelled", "catalogue".
const BRITISH_ENDINGS: &[(&str, &str)] = &[
    ("isation", "ization"),
    ("isations", "izations"),
    ("ise", "ize"),
    ("ised", "ized"),
    ("ises", "izes"),
    ("ising", "izing"),
    ("iser", "izer"),
    ("isers", "izers"),
    ("yse", "yze"),
    ("ysed", "yzed"),
    ("ysing", "yzing"),
    ("our", "or"),
    ("ours", "ors"),
    ("oured", "ored"),
    ("ouring", "oring"),
    ("ourite", "orite"),
    ("ourites", "orites"),
    ("ourable", "orable"),
    ("tre", "ter"),
    ("tres", "ters"),
    ("lled", "led"),
    ("lling", "ling"),
    ("ller", "ler"),
    ("ogue", "og"),
    ("ogues", "ogs"),
    ("ence", "ense"),
];

/// The pronunciation of a British spelling the dictionary lacks, from its
/// American form.
fn american(word: &str) -> Option<Pronunciation> {
    BRITISH_ENDINGS.iter().find_map(|(british, american)| {
        let stem = word.strip_suffix(british)?;
        let us = format!("{stem}{american}");
        lexicon::lookup(&us).or_else(|| with_suffix(&us))
    })
}

/// The words for a country written before "$": "US$", "A$", "C$".
fn dollar_country(prefix: &str) -> Option<&'static [&'static str]> {
    Some(match prefix {
        "US" => &["U.S."],
        "A" | "AU" => &["Australian"],
        "C" | "CA" => &["Canadian"],
        "NZ" => &["New", "Zealand"],
        "HK" => &["Hong", "Kong"],
        "S" => &["Singapore"],
        _ => return None,
    })
}

/// Greek letters, read by name: "α" is "alpha".
const GREEK: &[(char, &str)] = &[
    ('α', "alpha"),
    ('β', "beta"),
    ('γ', "gamma"),
    ('δ', "delta"),
    ('ε', "epsilon"),
    ('ζ', "zeta"),
    ('η', "eta"),
    ('θ', "theta"),
    ('ι', "iota"),
    ('κ', "kappa"),
    ('λ', "lambda"),
    ('μ', "mu"),
    ('ν', "nu"),
    ('ξ', "xi"),
    ('ο', "omicron"),
    ('π', "pi"),
    ('ρ', "rho"),
    ('σ', "sigma"),
    ('ς', "sigma"),
    ('τ', "tau"),
    ('υ', "upsilon"),
    ('φ', "phi"),
    ('χ', "chi"),
    ('ψ', "psi"),
    ('ω', "omega"),
];

/// The names of the letters of a word written in Greek, if it is one.
fn greek_names(word: &str) -> Option<Vec<&'static str>> {
    word.chars()
        .map(|c| {
            // Accented letters, as in "ά", read as plain ones.
            let base = match c.to_lowercase().next()? {
                'ά' => 'α',
                'έ' => 'ε',
                'ή' => 'η',
                'ί' | 'ϊ' | 'ΐ' => 'ι',
                'ό' => 'ο',
                'ύ' | 'ϋ' | 'ΰ' => 'υ',
                'ώ' => 'ω',
                other => other,
            };
            GREEK.iter().find(|(letter, _)| *letter == base).map(|(_, name)| *name)
        })
        .collect()
}

/// Fractions written as one character, with the words for each: "½" is
/// "a half", and "one half" alone.
const VULGAR_FRACTIONS: &[(char, &[&str])] = &[
    ('½', &["a", "half"]),
    ('⅓', &["a", "third"]),
    ('⅔', &["two", "thirds"]),
    ('¼', &["a", "quarter"]),
    ('¾', &["three", "quarters"]),
    ('⅕', &["a", "fifth"]),
    ('⅛', &["an", "eighth"]),
    ('⅜', &["three", "eighths"]),
    ('⅝', &["five", "eighths"]),
    ('⅞', &["seven", "eighths"]),
];

/// A word as read from the text, before phrase-level adjustments.
struct Written {
    /// Lowercase spelling.
    text: String,
    pronunciation: Pronunciation,
    /// Whether it is a function word here, and so unstressed.
    function: bool,
    /// Written with a capital inside a sentence, as a name is: "Polish".
    capitalized: bool,
    /// An ordinary word of the text, not one made from a number, an
    /// abbreviation or a spelling, so it can lose its accent when repeated.
    plain: bool,
}

/// Forms of "be" and the like that follow "there" when it says something
/// exists: "there is", "there might be", "there seems".
const EXISTENTIAL_AFTER: &[&str] = &[
    "is", "are", "was", "were", "be", "been", "being", "has", "have", "had", "will", "would", "may", "might", "must",
    "can", "could", "should", "seems", "seem", "seemed", "appears", "appear", "exists", "exist", "remains", "remain",
];

/// Whether word `i` links clauses here, and so carries no pitch accent,
/// though the dictionary stresses it. Bell Labs' accent rules (Hirschberg
/// 1993, as summarized by Riley and Sproat 1994) leave such closed-class
/// words unaccented. These are a relative or question word inside a phrase
/// ("the town where he lived", but not "Where are you going?"), a
/// subordinating conjunction ("because", "although"), "that" before a
/// pronoun, article or helping verb ("said that the", but not "that book"),
/// and "there" saying something exists ("there is", "is there"). A word
/// that ends its phrase keeps its accent: "You went where?"
fn links_clauses(written: &[Written], i: usize) -> bool {
    let word = &written[i];
    if !word.plain || word.capitalized || i + 1 == written.len() {
        return false;
    }
    let next = &written[i + 1];
    let before = i.checked_sub(1).map(|j| written[j].text.as_str());
    match word.text.as_str() {
        "which" | "who" | "whom" | "whose" | "what" | "where" | "when" | "why" | "how" | "whatever" | "whichever"
        | "wherever" | "whenever" => i > 0,
        "because" | "although" | "though" | "whereas" | "unless" | "whether" | "since" => true,
        // After "a" or "the", "while" is a noun: "for a while".
        "while" => !before.is_some_and(|b| NOUN_BEFORE.contains(&b)),
        "that" => i > 0 && next.function,
        "there" | "there's" => {
            word.text == "there's"
                || EXISTENTIAL_AFTER.contains(&next.text.as_str())
                || before.is_some_and(|b| EXISTENTIAL_AFTER.contains(&b))
        }
        _ => false,
    }
}

/// Words that keep their accent when repeated: numbers, as in "two plus
/// two", and negatives and "yes", which always carry one.
const ALWAYS_NEW: &[&str] = &[
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
    "twenty",
    "thirty",
    "forty",
    "fifty",
    "sixty",
    "seventy",
    "eighty",
    "ninety",
    "hundred",
    "thousand",
    "million",
    "billion",
    "trillion",
    "not",
    "no",
    "never",
    "nothing",
    "none",
    "nobody",
    "nowhere",
    "neither",
    "nor",
    "yes",
];

/// The form under which a word counts as already mentioned: without a
/// possessive or plural ending, so "dogs" and "dog's" count as "dog".
fn lemma(text: &str) -> String {
    let word = text.trim_end_matches("'s").trim_end_matches('\'');
    let sibilant = ["s", "x", "z", "ch", "sh"];
    if let Some(stem) = word.strip_suffix("ies").filter(|s| s.len() > 1) {
        format!("{stem}y")
    } else if let Some(stem) = word.strip_suffix("es").filter(|s| sibilant.iter().any(|e| s.ends_with(e))) {
        stem.to_string()
    } else if let Some(stem) =
        word.strip_suffix('s').filter(|s| s.len() > 2 && !s.ends_with('s') && !s.ends_with('u') && !s.ends_with('i'))
    {
        stem.to_string()
    } else {
        word.to_string()
    }
}

/// Builds a phrase's words, choosing homographs by context and applying
/// function-word reduction.
///
/// The dictionary gives about 1,500 words two or more primary stresses, and
/// every primary gets a pitch accent, so "Microsoft" sounded like "Micro
/// soft". Each word keeps one. A compound keeps its first ("MIcrosoft",
/// "BASEball"). Other words have level stress, and the rhythm rule decides:
/// the last stress at the end of a phrase or before an unstressed syllable
/// ("she's nineTEEN", "go outSIDE"), the first just before another stressed
/// syllable ("NINEteen EIGHty", "the OUTside world").
///
/// Words that link clauses lose their accent (see `links_clauses`), and so
/// does a word already in `mentioned`, as in "my son wants me to buy a DOG,
/// but I'm allergic to dogs" (Hirschberg 1993). Names keep theirs, as do
/// the words in `ALWAYS_NEW`, and a phrase keeps at least one accent.
fn finish(written: &[Written], past_sentence: bool, mentioned: &mut HashSet<String>) -> Vec<Word> {
    let texts: Vec<&str> = written.iter().map(|w| w.text.as_str()).collect();
    let capitalized: Vec<bool> = written.iter().map(|w| w.capitalized).collect();
    let pronunciations: Vec<Pronunciation> = homographs::choose(&texts, &capitalized, past_sentence)
        .into_iter()
        .zip(written)
        .map(|(choice, word)| match choice {
            Some(choice) if choice.add_s => attach(arpabet(choice.arpabet), "s"),
            Some(choice) => arpabet(choice.arpabet),
            None => word.pronunciation.clone(),
        })
        .collect();
    let mut words = Vec::new();
    // Words that lost their accent for being repeated, with their own stress.
    let mut repeated: Vec<(usize, Pronunciation)> = Vec::new();
    for (i, word) in written.iter().enumerate() {
        let next_starts_with_vowel =
            pronunciations.get(i + 1).and_then(|p| p.first()).is_some_and(|(p, _)| p.is_vowel());
        // A function word that ends a phrase keeps its full vowel: "What is
        // it for?", "I want to." An object pronoun stays weak: "I saw them."
        let stranded = i + 1 == written.len() && word.text != "them";
        // After "a", "the" or "my", a helping verb is a noun, with its full
        // sound and stress: "a can of paint", "the will of the people".
        let function = (word.function && !(i > 0 && NOUN_BEFORE.contains(&written[i - 1].text.as_str())))
            || links_clauses(written, i);
        let mut phonemes = if !function || stranded {
            pronunciations[i].clone()
        } else if let Some(strong) = table_lookup(BEFORE_VOWEL, &word.text).filter(|_| next_starts_with_vowel) {
            strong
        } else if let Some(weak) = table_lookup(WEAK_FORMS, &word.text) {
            weak
        } else {
            pronunciations[i].clone()
        };
        if function {
            // One-syllable function words lose their stress; longer ones
            // keep a secondary stress on their strongest syllable.
            let syllables = phonemes.iter().filter(|(p, _)| p.is_vowel()).count();
            for (_, stress) in phonemes.iter_mut() {
                *stress = match *stress {
                    Stress::Primary if syllables > 1 => Stress::Secondary,
                    Stress::Primary | Stress::Secondary if syllables == 1 => Stress::Unstressed,
                    other => other,
                };
            }
        }
        if !function && phonemes.iter().filter(|(_, s)| *s == Stress::Primary).count() > 1 {
            // A clash: the word ends in a stressed syllable, and the next one
            // starts with one.
            let ends_stressed =
                phonemes.iter().rfind(|(p, _)| p.is_vowel()).is_some_and(|&(_, s)| s == Stress::Primary);
            let clash = ends_stressed
                && written.get(i + 1).is_some_and(|next| !next.function)
                && pronunciations[i + 1].iter().find(|(p, _)| p.is_vowel()).is_some_and(|&(_, s)| s == Stress::Primary);
            let first = clash || is_compound(&word.text, &phonemes);
            keep_one_primary(&mut phonemes, first);
        }
        let accented = phonemes.iter().any(|&(_, s)| s == Stress::Primary);
        if !function && accented && word.plain && !word.capitalized {
            let lemma = lemma(&word.text);
            if !ALWAYS_NEW.contains(&lemma.as_str()) && !mentioned.insert(lemma) {
                repeated.push((words.len(), phonemes.clone()));
                for (_, stress) in phonemes.iter_mut().filter(|(_, s)| *s == Stress::Primary) {
                    *stress = Stress::Secondary;
                }
            }
        }
        if !phonemes.is_empty() {
            words.push(Word { written: Vec::new(), ending: has_ending(&word.text, &phonemes), phonemes });
        }
    }
    // A phrase needs an accent: give the last repeated word its own back.
    let unaccented = words.iter().all(|w| w.phonemes.iter().all(|&(_, s)| s != Stress::Primary));
    if let Some((index, own)) = repeated.pop().filter(|_| unaccented) {
        words[index].phonemes = own;
    }
    words
}

/// Whether a word's last consonant is the -s or -ed ending, judged by its
/// spelling: "songs", "Klatt's", "turned", but not "bronze" or "bend".
fn has_ending(text: &str, phonemes: &Pronunciation) -> bool {
    use Phoneme::*;
    match phonemes.last().map(|&(p, _)| p) {
        Some(S | Z) => text.ends_with('s') || text.ends_with("s'"),
        Some(T | D) => text.ends_with("ed"),
        _ => false,
    }
}

/// Whether a word written this way is a function word. A capitalized one in
/// mid-sentence is a name or title instead: "the 3rd of May", "Will Smith".
fn is_function_word(word: &str, sentence_start: bool) -> bool {
    let lower = word.to_lowercase();
    let mut chars = word.chars();
    let capitalized = chars.next().is_some_and(char::is_uppercase) && chars.all(char::is_lowercase) && word.len() > 1;
    FUNCTION_WORDS.binary_search(&lower.as_str()).is_ok() && (sentence_start || !capitalized)
}

/// Spells text character by character, as screen readers do when moving by
/// character or echoing typed keys: letters by name, digits as numbers.
/// Other characters are skipped, since screen readers send symbol names as
/// words.
pub fn spelled_phrases(text: &str) -> Vec<Phrase> {
    let mut words = Vec::new();
    for c in fold_accents(&text.to_lowercase()).chars() {
        let phonemes = if c.is_ascii_lowercase() {
            arpabet(LETTER_NAMES[(c as u8 - b'a') as usize])
        } else if let Some(digit) = c.to_digit(10) {
            pronounce(ONES[digit as usize])
        } else {
            continue;
        };
        words.push(Word { phonemes, ending: false, written: Vec::new() });
    }
    let mut phrases = Vec::new();
    close_phrase(&mut phrases, &mut words, Boundary::Period);
    phrases
}

/// Words that ask the question in a wh-question.
const QUESTION_WORDS: &[&str] = &["how", "what", "whatever", "when", "where", "which", "who", "whom", "whose", "why"];

/// Helping verbs that open a yes-no question: "Are you coming?"
const HELPING_VERBS: &[&str] = &[
    "ain't",
    "am",
    "are",
    "aren't",
    "can",
    "can't",
    "cannot",
    "could",
    "couldn't",
    "did",
    "didn't",
    "do",
    "does",
    "doesn't",
    "don't",
    "had",
    "hadn't",
    "has",
    "hasn't",
    "have",
    "haven't",
    "is",
    "isn't",
    "may",
    "might",
    "mightn't",
    "must",
    "mustn't",
    "shall",
    "shan't",
    "should",
    "shouldn't",
    "was",
    "wasn't",
    "were",
    "weren't",
    "will",
    "won't",
    "would",
    "wouldn't",
];

/// Words that can come before the words that ask: "So what?", "And where
/// is it?"
const QUESTION_OPENERS: &[&str] = &["and", "but", "now", "oh", "okay", "or", "so", "then", "well"];

/// Marks questions that begin with a question word ("Where are you going?")
/// so they end with a fall, like statements; other questions keep the rise.
/// In American telephone conversations, Hedberg, Sosa and Fadden found that
/// 82% of wh-questions fell, while most yes-no questions and questions
/// worded as statements ("You're leaving?") rose.
fn mark_falling_questions(tokens: &mut [Token]) {
    let (mut start, mut last_start) = (0, 0);
    for i in 0..tokens.len() {
        let Token::Boundary(boundary) = tokens[i] else { continue };
        if !matches!(boundary, Boundary::Period | Boundary::Question | Boundary::Exclamation) {
            continue;
        }
        // Punctuation right after a sentence's end, as in "go!?", belongs
        // to that sentence.
        let has_words = tokens[start..i].iter().any(|t| !matches!(t, Token::Boundary(_)));
        let sentence = if has_words { start } else { last_start };
        if boundary == Boundary::Question && asks_with_question_word(&tokens[sentence..i]) {
            tokens[i] = Token::Boundary(Boundary::WhQuestion);
        }
        if has_words {
            last_start = start;
        }
        start = i + 1;
    }
}

/// Whether a question's own words begin with a question word. Its last
/// phrase decides, or the one before if it says nothing either way, as in
/// "Where are you going, John?" A helping verb first makes it a yes-no
/// question: "When you're done, can you call me?"
fn asks_with_question_word(sentence: &[Token]) -> bool {
    for phrase in sentence.split(|t| matches!(t, Token::Boundary(_))).rev() {
        for token in phrase {
            let Token::Word(word) = token else { break };
            let word = word.to_lowercase();
            let stem = word.split('\'').next().unwrap_or("");
            if QUESTION_WORDS.contains(&stem) {
                return true;
            }
            if HELPING_VERBS.contains(&word.as_str()) {
                return false;
            }
            if !QUESTION_OPENERS.contains(&word.as_str()) {
                break;
            }
        }
    }
    false
}

/// For each sentence in the tokens, whether it tells of the past, which
/// makes "read" the past form: "He was educated at Eton, where he read
/// history."
fn past_sentences(tokens: &[Token]) -> Vec<bool> {
    let mut sentences = Vec::new();
    let mut words: Vec<String> = Vec::new();
    for token in tokens {
        match token {
            Token::Word(word) => words.push(word.to_lowercase()),
            Token::Boundary(Boundary::Period | Boundary::Question | Boundary::WhQuestion | Boundary::Exclamation) => {
                let list: Vec<&str> = words.iter().map(String::as_str).collect();
                sentences.push(homographs::narrative_past(&list));
                words.clear();
            }
            _ => {}
        }
    }
    let list: Vec<&str> = words.iter().map(String::as_str).collect();
    sentences.push(homographs::narrative_past(&list));
    sentences
}

/// The most content words a stretch without punctuation may have before it
/// is split into phrases.
const LONGEST_PHRASE: usize = 6;
/// The fewest content words before a split, so that a short subject stays
/// with its verb ("Green roofs are excellent insulators"), and after one, so
/// that a short ending stays attached ("She had caught an early morning
/// train from London").
const SHORTEST_BEFORE: usize = 3;
const SHORTEST_AFTER: usize = 2;
/// Words that tie themselves to what follows, so no break comes after them:
/// "if and only if", "not even".
const NO_BREAK_AFTER: &[&str] = &["also", "both", "either", "even", "just", "neither", "not", "only", "rather"];

/// How strongly a word can start a new phrase, from 0 (not at all) to 3.
/// Words that open a clause start one most clearly; a helping verb after
/// its subject comes next; a preposition last. "Of", "to" and "as" bind too
/// closely to their neighbors to start one.
fn starter(word: &str) -> usize {
    match word {
        "and" | "or" | "nor" | "but" | "because" | "although" | "though" | "while" | "whereas" | "unless" | "where"
        | "when" | "which" | "who" | "whom" | "whose" | "whether" | "if" | "what" | "whatever" => 3,
        "that" | "since" | "is" | "are" | "was" | "were" | "has" | "have" | "had" | "will" | "would" | "can"
        | "could" | "should" | "must" | "may" | "might" | "shall" | "does" | "did" => 2,
        "about" | "above" | "across" | "after" | "against" | "along" | "among" | "around" | "at" | "before"
        | "behind" | "below" | "beneath" | "beside" | "between" | "beyond" | "by" | "during" | "for" | "from"
        | "in" | "into" | "on" | "onto" | "through" | "toward" | "towards" | "under" | "until" | "upon" | "with"
        | "within" | "without" => 1,
        _ => 0,
    }
}

/// How strongly word `j` starts a new phrase where it stands. "And", "or"
/// and "nor" start one only when they join clauses or phrases, shown by
/// the word after them ("and it is", "and is located", "and in the
/// closet"), not when they join two words ("the engines and boilers").
fn break_strength(written: &[Written], j: usize) -> usize {
    let next = written.get(j + 1).map_or("", |w| w.text.as_str());
    let opens_group = starter(next) > 0
        || matches!(
            next,
            "i" | "you" | "he" | "she" | "it" | "we" | "they" | "then" | "so" | "thus" | "therefore" | "yet" | "still"
        );
    match written[j].text.as_str() {
        "and" | "or" | "nor" if !opens_group => 0,
        word => starter(word),
    }
}

/// Where a stretch of words with no punctuation inside should be broken
/// into phrases: the indices of the words that start a new one.
///
/// A long stretch spoken as one phrase runs on. Readers break it where a
/// new group starts, and balance the parts in length (Bachenko and
/// Fitzpatrick 1990; O'Shaughnessy 1989). A stretch with more than
/// `LONGEST_PHRASE` content words is split before a word that starts a
/// group after a content word, choosing the split that best combines the
/// word's strength with parts of equal length, and each part is split again
/// while still too long.
fn phrase_breaks(written: &[Written]) -> Vec<usize> {
    let strength: Vec<usize> = (0..written.len()).map(|j| break_strength(written, j)).collect();
    let mut counts = vec![0];
    for w in written {
        counts.push(counts.last().unwrap() + usize::from(!w.function && starter(&w.text) == 0));
    }
    let content = |a: usize, b: usize| counts[b] - counts[a];
    let mut breaks = Vec::new();
    let mut stretches = vec![(0, written.len())];
    while let Some((start, end)) = stretches.pop() {
        if content(start, end) <= LONGEST_PHRASE {
            continue;
        }
        let best = (start + 1..end)
            .filter(|&j| {
                let before = written[j - 1].text.as_str();
                strength[j] > 0 && !written[j - 1].function && starter(before) == 0 && !NO_BREAK_AFTER.contains(&before)
            })
            .filter(|&j| content(start, j) >= SHORTEST_BEFORE && content(j, end) >= SHORTEST_AFTER)
            .max_by_key(|&j| (3 * strength[j]) as isize - content(start, j).abs_diff(content(j, end)) as isize);
        if let Some(j) = best {
            breaks.push(j);
            stretches.push((start, j));
            stretches.push((j, end));
        }
    }
    breaks.sort_unstable();
    breaks
}

/// Ends a stretch of words at punctuation, as one phrase or, with phrase
/// prediction and when it is long, as several joined by breaks without a
/// pause.
fn close_stretch(
    mentioned: &mut HashSet<String>,

    phrases: &mut Vec<Phrase>,
    written: &[Written],
    past_sentence: bool,
    predict: bool,
    boundary: Boundary,
) {
    let breaks = if predict { phrase_breaks(written) } else { Vec::new() };
    let mut start = 0;
    for end in breaks.into_iter().chain([written.len()]) {
        let mut words = finish(&written[start..end], past_sentence, mentioned);
        close_phrase(phrases, &mut words, if end == written.len() { boundary } else { Boundary::Minor });
        start = end;
    }
}

/// Turns ordinary text into phrases ready for synthesis. With
/// `expand_abbreviations` false, abbreviations are read as written: "Dr." as
/// "D R" and "Inc." as "ink". With `phrase_prediction`, long stretches
/// without punctuation are broken into phrases (see `phrase_breaks`).
pub fn phrases_with(text: &str, expand_abbreviations: bool, phrase_prediction: bool) -> Vec<Phrase> {
    let mut tokens = abbreviations::resolve(tokenize(text), expand_abbreviations);
    // Whether the text is written mostly in capitals.
    let longer: Vec<&str> = tokens
        .iter()
        .filter_map(|t| match t {
            Token::Word(w) if w.chars().filter(|c| c.is_alphabetic()).count() > 1 => Some(w.as_str()),
            _ => None,
        })
        .collect();
    let shouting = longer.iter().filter(|w| !w.chars().any(char::is_lowercase)).count() * 2 > longer.len();
    mark_falling_questions(&mut tokens);
    let past_sentences = past_sentences(&tokens);
    let mut sentence = 0;
    let mut phrases = Vec::new();
    let mut written: Vec<Written> = Vec::new();
    let mut mentioned = HashSet::new();
    let mut sentence_start = true;
    for token in tokens {
        let past = past_sentences.get(sentence).copied().unwrap_or(false);
        match token {
            Token::Word(word) if word == "'s" && !written.is_empty() => {
                let last = written.last_mut().unwrap();
                last.pronunciation = attach(std::mem::take(&mut last.pronunciation), "s");
                last.text.push_str("'s");
            }
            Token::Word(word) => {
                let letters = two_letter_abbreviation(&word, shouting);
                let function = !letters && is_function_word(&word, sentence_start);
                let pronunciation = if letters { spell(&word.to_lowercase()) } else { pronounce(&word) };
                let mut chars = word.chars();
                let capitalized =
                    !sentence_start && chars.next().is_some_and(char::is_uppercase) && chars.any(char::is_lowercase);
                // Any capital makes it the month, even where capitals say
                // nothing about other words: "August is hot", "AUGUST".
                let capitalized = capitalized || (word != "august" && word.eq_ignore_ascii_case("august"));
                written.push(Written {
                    text: word.to_lowercase(),
                    pronunciation,
                    function,
                    capitalized,
                    plain: !letters,
                });
                sentence_start = false;
            }
            Token::Number { digits, ordinal } => {
                for word in number_words(&digits, ordinal) {
                    written.push(Written {
                        pronunciation: pronounce(&word),
                        text: word,
                        function: false,
                        capitalized: false,
                        plain: false,
                    });
                }
                sentence_start = false;
            }
            Token::Expanded(words) => {
                for word in words.iter().flat_map(|w| w.split_whitespace()) {
                    written.push(Written {
                        pronunciation: pronounce(word),
                        text: word.to_string(),
                        function: false,
                        // From a date or "Aug.", so the month, not the
                        // adjective, which the homograph table takes otherwise.
                        capitalized: word == "august",
                        plain: false,
                    });
                }
                sentence_start = false;
            }
            Token::Spelled(letters) | Token::Dotted(letters) => {
                let lower = letters.to_lowercase();
                written.push(Written {
                    pronunciation: spell(&lower),
                    text: lower,
                    function: false,
                    capitalized: false,
                    plain: false,
                });
                sentence_start = false;
            }
            Token::Literal(word) => {
                let lower = word.to_lowercase();
                let pronunciation = lts::predict(&lower).unwrap_or_else(|| spell(&lower));
                written.push(Written { pronunciation, text: lower, function: false, capitalized: false, plain: false });
                sentence_start = false;
            }
            Token::Boundary(boundary) => {
                close_stretch(
                    &mut mentioned,
                    &mut phrases,
                    &std::mem::take(&mut written),
                    past,
                    phrase_prediction,
                    boundary,
                );
                sentence_start = matches!(
                    boundary,
                    Boundary::Period | Boundary::Question | Boundary::WhQuestion | Boundary::Exclamation
                );
                sentence += usize::from(sentence_start);
            }
        }
    }
    let past = past_sentences.get(sentence).copied().unwrap_or(false);
    close_stretch(&mut mentioned, &mut phrases, &written, past, phrase_prediction, Boundary::Period);
    phrases
}

#[cfg(test)]
mod dash_tests {
    use super::*;

    /// The boundary after the first phrase of `text`.
    fn first_break(text: &str) -> Boundary {
        phrases_with(text, true, false)[0].boundary
    }

    #[test]
    fn a_dash_is_a_stronger_break_than_a_comma() {
        for text in [
            "Yes \u{2014} no.",
            "Yes\u{2014}no.",
            "Yes \u{2013} no.",
            "Yes -- no.",
            "Yes--no.",
            "Yes - no.",
            "Yes ---- no.",
        ] {
            assert_eq!(first_break(text), Boundary::Clause, "{text}");
            assert_eq!(phrases_with(text, true, false).len(), 2, "{text}");
        }
        assert_eq!(first_break("Yes, no."), Boundary::Comma);
        assert_eq!(first_break("Yes (no)."), Boundary::Comma);
    }

    #[test]
    fn a_dash_at_the_end_breaks_nothing() {
        for text in ["Yes -", "Yes --", "Yes - ", "Yes ----"] {
            assert_eq!(phrases_with(text, true, false).len(), 1, "{text}");
            assert_eq!(first_break(text), Boundary::Period, "{text}");
        }
    }

    #[test]
    fn hyphens_that_are_not_dashes_stay_as_they_were() {
        // In a word, and in numbers, a hyphen makes no break.
        for text in ["A well-known fact.", "It was -2 outside.", "From 1990 - 1995 here.", "From 1pm - 4pm here."] {
            assert_eq!(phrases_with(text, true, false).len(), 1, "{text}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utterance::notation;

    fn say(text: &str) -> String {
        notation(&phrases_with(text, true, false))
    }

    #[test]
    fn reads_letters_names_and_spellings() {
        // Two capitals are spelled in ordinary text, but not in a headline.
        assert_eq!(say("The US Army."), "[DH AH0] [Y UW2 EH1 S] [AA1 R M IY0].");
        assert_eq!(say("LET US KNOW."), "[L EH1 T] [AH0 S] [N OW1].");
        assert_eq!(say("Say NO."), "[S EY1] [N OW1].");
        // Greek letters by name, dots in names, and email addresses.
        assert_eq!(say("α"), "[AE1 L F AH0].");
        assert!(say("Games.com").contains("[D AA1 T]"));
        assert!(say("me@example.com").contains("[AE1 T]"));
        // British spellings sound like American ones.
        assert_eq!(say("criticised"), say("criticized"));
        assert_eq!(say("specialises"), say("specializes"));
    }

    #[test]
    fn reads_dictionary_words_with_punctuation() {
        assert_eq!(say("Hello, world!"), "[HH AH0 L OW1], [W ER1 L D]!");
        assert_eq!(say("Is it raining?"), "[IH0 Z] [IH0 T] [R EY1 N IH0 NG]?");
        assert_eq!(say(""), "");
    }

    /// The words of plain text, with "|" where it breaks without a pause.
    fn phrasing(text: &str) -> String {
        let mut words = text.trim_end_matches('.').split_whitespace();
        let phrases: Vec<String> = phrases_with(text, true, true)
            .iter()
            .map(|p| words.by_ref().take(p.words.len()).collect::<Vec<_>>().join(" "))
            .collect();
        phrases.join(" | ")
    }

    #[test]
    fn words_that_link_clauses_lose_their_accent() {
        assert!(say("The town where he lived.").contains("[W EH0 R]"));
        assert!(
            say("There is a committee which meets.")
                .starts_with("[DH EH0 R] [IH0 Z] [AH0] [K AH0 M IH1 T IY0] [W IH0 CH]")
        );
        assert!(say("He said that the house was old.").contains("[DH AE0 T] [DH AH0]"));
        assert!(say("It rained because the wind changed.").contains("[B IH0 K AO2 Z]"));
        // The same words keep it as questions, as pointers, at a phrase end,
        // or as a noun.
        assert!(say("Where are you going?").starts_with("[W EH1 R]"));
        assert!(say("I like that book.").contains("[DH AE1 T]"));
        assert!(say("Go there.").ends_with("[DH EH1 R]."));
        assert!(say("You went where?").ends_with("[W EH1 R]?"));
        assert!(say("For a while.").contains("[W AY1 L]"));
    }

    #[test]
    fn repeated_words_lose_their_accent() {
        let said = say("My son wants me to buy a dog, but I'm allergic to dogs.");
        assert!(said.contains("[D AO1 G],") && said.ends_with("[D AA2 G Z]."), "{said}");
        // Names and numbers keep theirs, and a phrase keeps at least one.
        assert_eq!(say("Microsoft Office and Microsoft Word.").matches("[M AY1").count(), 2);
        assert_eq!(say("Two plus two is four."), "[T UW1] [P L AH1 S] [T UW1] [IH0 Z] [F AO1 R].");
        assert_eq!(say("Dogs. Dogs!"), "[D AA1 G Z]. [D AA1 G Z]!");
    }

    #[test]
    fn long_stretches_break_into_phrases() {
        assert_eq!(
            phrasing(
                "The old man walked slowly down the long dusty road toward the small town where his brother had lived for many years."
            ),
            "The old man walked slowly down the long dusty road | toward the small town | where his brother had lived for many years"
        );
        // "And" joining two words is not a break, but before a verb it is.
        assert_eq!(
            phrasing(
                "The escarpment contains a wide variety of native flora and fauna and is a haven for many forms of wildlife."
            ),
            "The escarpment contains a wide variety of native flora and fauna | and is a haven for many forms of wildlife"
        );
        // A short subject stays with its verb.
        assert_eq!(
            phrasing(
                "These diodes can indefinitely sustain a moderate level of current while on the edge of breakdown."
            ),
            "These diodes can indefinitely sustain a moderate level of current | while on the edge of breakdown"
        );
        // Short stretches, and a short ending, are left whole.
        for whole in
            ["The birch canoe slid on the smooth planks.", "She had caught an early morning train from London."]
        {
            assert_eq!(phrasing(whole), whole.trim_end_matches('.'));
        }
        // Without phrase prediction, as screen readers usually have it, there
        // are no breaks.
        let long = "The ship has an opening dock to the rear which allows it to launch up to three landing craft.";
        assert!(phrasing(long).contains('|'));
        assert_eq!(phrases_with(long, true, false).len(), 1);
    }

    #[test]
    fn words_keep_one_main_stress() {
        // The dictionary gives these two. A compound keeps its first.
        assert_eq!(say("Microsoft."), "[M AY1 K R OW2 S AO2 F T].");
        assert_eq!(say("Microsoft Word."), "[M AY1 K R OW2 S AO2 F T] [W ER1 D].");
        // Level stress follows the rhythm: the last at the end of a phrase,
        // the first just before another stressed syllable.
        assert_eq!(say("She is nineteen."), "[SH IY0] [IH0 Z] [N AY2 N T IY1 N].");
        assert_eq!(say("It was 1914."), "[IH0 T] [W AH0 Z] [N AY1 N T IY2 N] [F AO2 R T IY1 N].");
        assert_eq!(say("The outside world."), "[DH IY0] [AW1 T S AY2 D] [W ER1 L D].");
        // No shift when an unstressed syllable comes between.
        assert_eq!(say("Engineering students."), "[EH2 N JH AH0 N IH1 R IH0 NG] [S T UW1 D AH0 N T S].");
        // A prefix is not the first part of a compound.
        assert_eq!(say("Dislocation."), "[D IH0 S L OW2 K EY1 SH AH0 N].");
    }

    #[test]
    fn questions_with_a_question_word_fall() {
        let ending = |text: &str| phrases_with(text, true, false).last().unwrap().boundary;
        for falling in [
            "Where are you going?",
            "What's that?",
            "So what?",
            "Where are you going, John?",
            "When the sun went down, where did you go?",
            "Who knows?!",
            "Where did it go!?",
        ] {
            assert_eq!(ending(falling), Boundary::WhQuestion, "{falling}");
        }
        for rising in [
            "Are you coming?",
            "Isn't it?",
            "When you're done, can you call me?",
            "You went where?",
            "Do you know where he went?",
            "Really?",
            "Is it far, John?",
        ] {
            assert_eq!(ending(rising), Boundary::Question, "{rising}");
        }
        // Each sentence is judged on its own.
        let phrases = phrases_with("Who called? Did they leave a message?", true, false);
        assert_eq!(phrases[0].boundary, Boundary::WhQuestion);
        assert_eq!(phrases[1].boundary, Boundary::Question);
    }

    #[test]
    fn reduces_function_words() {
        assert_eq!(say("the cat"), "[DH AH0] [K AE1 T].");
        assert_eq!(say("the apple"), "[DH IY0] [AE1 P AH0 L].");
        assert_eq!(say("a cup of tea"), "[AH0] [K AH1 P] [AH0 V] [T IY1].");
        assert_eq!(say("go to bed"), "[G OW1] [T AH0] [B EH1 D].");
        // At the end of a phrase they keep their full vowel, unstressed.
        assert_eq!(say("What is it for?"), "[W AH1 T] [IH0 Z] [IH0 T] [F AO0 R]?");
        assert_eq!(say("I want to, but"), "[AY0] [W AA1 N T] [T UW0], [B AH0 T].");
        assert_eq!(say("I saw them."), "[AY0] [S AO1] [DH AH0 M].");
        // After a determiner a helping verb is a noun.
        assert_eq!(say("a can of paint"), "[AH0] [K AE1 N] [AH0 V] [P EY1 N T].");
    }

    #[test]
    fn expands_numbers() {
        assert_eq!(number_words("0", false), ["zero"]);
        assert_eq!(number_words("21", false), ["twenty", "one"]);
        assert_eq!(number_words("1,205", false), ["one", "thousand", "two", "hundred", "five"]);
        assert_eq!(number_words("1999", false), ["nineteen", "ninety", "nine"]);
        assert_eq!(number_words("1905", false), ["nineteen", "oh", "five"]);
        assert_eq!(number_words("2024", false), ["twenty", "twenty", "four"]);
        assert_eq!(number_words("2005", false), ["two", "thousand", "five"]);
        assert_eq!(number_words("2,024", false), ["two", "thousand", "twenty", "four"]);
        assert_eq!(number_words(".25", false), ["point", "two", "five"]);
        assert_eq!(number_words("3.14", false), ["three", "point", "one", "four"]);
        assert_eq!(number_words("007", false), ["zero", "zero", "seven"]);
        assert_eq!(number_words("1000000", false), ["one", "million"]);
        assert_eq!(number_words("21", true), ["twenty", "first"]);
        assert_eq!(number_words("12", true), ["twelfth"]);
        assert_eq!(number_words("40", true), ["fortieth"]);
        assert!(say("the 3rd of May").contains("[TH ER1 D]"));
    }

    #[test]
    fn a_very_long_run_of_letters_is_cut_into_words() {
        let words = |text: &str| -> Vec<String> {
            tokenize(text)
                .into_iter()
                .map(|t| match t {
                    Token::Word(word) => word,
                    other => panic!("{other:?}"),
                })
                .collect()
        };
        // The longest word in the dictionary, and the longest that stays whole.
        assert_eq!(words("antidisestablishmentarianism").len(), 1);
        assert_eq!(words(&"x".repeat(LONGEST_WORD)).len(), 1);
        // Nothing is lost.
        let parts = words(&"x".repeat(2 * LONGEST_WORD + 1));
        assert_eq!(parts.iter().map(String::len).collect::<Vec<_>>(), [LONGEST_WORD, LONGEST_WORD, 1]);
        // So also for runs of dotted letters, which are spelled.
        let dotted = tokenize(&"a.".repeat(2 * LONGEST_WORD));
        assert!(dotted.iter().all(|t| !matches!(t, Token::Dotted(d) if d.len() > LONGEST_WORD)));
    }

    #[test]
    fn spells_unknown_abbreviations() {
        // NVDA is not in the dictionary: spelled, stress on the last letter.
        assert_eq!(say("NVDA"), "[EH2 N V IY2 D IY2 EY1].");
        assert_eq!(say("NVDA's"), "[EH2 N V IY2 D IY2 EY1 Z].");
        // Plurals, and short ones that cannot be words though a name is
        // spelled the same.
        assert_eq!(say("CDs"), "[S IY2 D IY1 Z].");
        assert_eq!(say("NGO"), "[EH2 N JH IY2 OW1].");
        assert_eq!(say("NASA"), "[N AE1 S AH0].");
    }

    #[test]
    fn folds_accents_and_spells_by_character() {
        assert_eq!(say("café"), say("cafe"));
        assert_eq!(notation(&spelled_phrases("a")), "[EY1].");
        assert_eq!(notation(&spelled_phrases("B2")), "[B IY1] [T UW1].");
        assert_eq!(notation(&spelled_phrases("é")), "[IY1].");
        assert_eq!(notation(&spelled_phrases("?")), "");
    }

    #[test]
    fn read_depends_on_context() {
        assert_eq!(say("Read more"), "[R IY1 D] [M AO1 R].");
        assert_eq!(say("mark as read"), "[M AA1 R K] [AH0 Z] [R EH1 D].");
        assert_eq!(say("I have read it"), "[AY0] [HH AE0 V] [R EH1 D] [IH0 T].");
    }

    #[test]
    fn capitalized_function_words_are_names() {
        assert_eq!(say("in May"), "[IH0 N] [M EY1].");
        assert_eq!(say("we may"), "[W IY0] [M EY0].");
        assert_eq!(say("May I?"), "[M EY0] [AY0]?");
    }

    #[test]
    fn handles_suffixes_on_known_words() {
        // Possessives and inflections of dictionary words.
        let klatt = notation(&[Phrase {
            words: vec![Word { written: Vec::new(), phonemes: with_suffix("dogs").unwrap(), ending: false }],
            boundary: Boundary::Period,
        }]);
        assert!(klatt.ends_with("G Z]."), "{klatt}");
        assert_eq!(attach(arpabet("K AE1 T"), "s"), arpabet("K AE1 T S"));
        assert_eq!(attach(arpabet("D AO1 G"), "s"), arpabet("D AO1 G Z"));
        assert_eq!(attach(arpabet("B AA1 K S"), "s"), arpabet("B AA1 K S IH0 Z"));
        assert_eq!(attach(arpabet("W AO1 N T"), "ed"), arpabet("W AO1 N T IH0 D"));
    }

    #[test]
    fn august_is_the_month_unless_written_in_lowercase() {
        let month = "[AA1 G AH0 S T]";
        for text in ["It was August 5.", "In August, it rained.", "August is hot.", "AUGUST", "Aug. 5", "Aug 5, 2014"] {
            assert!(say(text).contains(month), "{text}: {}", say(text));
        }
        for text in ["08/05/2014", "08-25-2015", "25.08.2014", "2014-08-05"] {
            assert!(say(text).starts_with(month), "{text}: {}", say(text));
        }
        // The adjective has its stress on the second syllable.
        assert_eq!(say("an august body"), "[AH0 N] [AA2 G AH1 S T] [B AA1 D IY0].");
    }

    #[test]
    fn words_added_by_hand_are_said_as_written_there() {
        // Whatever the capitals, and with the usual endings.
        assert_eq!(say("YouTube"), "[Y UW1 T UW2 B].");
        assert_eq!(say("COVID"), "[K OW1 V IH0 D].");
        assert_eq!(say("Klattara"), "[K L AE0 T AA1 R AH0].");
        assert_eq!(say("chatbots"), "[CH AE1 T B AA2 T S].");
        // The dictionary's own plural and possessive give way too.
        assert_eq!(say("memes"), "[M IY1 M Z].");
        assert_eq!(say("Mozart's"), "[M OW1 T S AA2 R T S].");
        // A rule would spell these capitals, as it does "HTML".
        assert_eq!(say("JSON"), "[JH EY1 S AH0 N].");
        assert_eq!(say("JPEG"), "[JH EY1 P EH2 G].");
        assert_ne!(say("HTML"), "[HH T M L].");
    }

    #[test]
    fn suffixes_find_the_stem_with_its_silent_e() {
        let suffixed = |word: &str| with_suffix(word).map(|sounds| sounds.iter().map(|&(p, _)| p).collect::<Vec<_>>());
        let sounds = |text: &str| Some(arpabet(text).iter().map(|&(p, _)| p).collect::<Vec<_>>());
        // Not "cod" and "fac", though the dictionary lists them.
        assert_eq!(suffixed("coded"), sounds("K OW1 D IH0 D"));
        assert_eq!(suffixed("rides"), sounds("R AY1 D Z"));
        // A stem that really has no e, and a doubled consonant undone.
        assert_eq!(suffixed("visited"), sounds("V IH1 Z IH0 T IH0 D"));
        assert_eq!(suffixed("sunned"), sounds("S AH1 N D"));
        // -es needs a hissing sound or a stem's own e, and y restored counts.
        assert_eq!(suffixed("boxes"), sounds("B AA1 K S IH0 Z"));
        assert_eq!(suffixed("tries"), sounds("T R AY1 Z"));
        assert_eq!(suffixed("zzzzes"), None);
        assert_ne!(suffixed("memes"), sounds("M EH1 M Z"));
        // Left to the letter-to-sound model, which does better on these.
        assert_eq!(suffixed("reading"), None);
        assert_eq!(suffixed("writer"), None);
    }

    #[test]
    fn compounds_only_where_the_model_is_unsure() {
        let compound_sounds = |word: &str, shortest: usize, related: usize| {
            let phrase = |phonemes| Phrase {
                words: vec![Word { phonemes, ending: false, written: Vec::new() }],
                boundary: Boundary::Period,
            };
            compound(word, shortest, related).map(|phonemes| notation(&[phrase(phonemes)]))
        };
        // The parts of a real compound, with the main stress on the first.
        assert_eq!(compound_sounds("batgirl", 3, 1), Some("[B AE1 T G ER2 L].".to_string()));
        assert_eq!(compound_sounds("bookstore", 4, 2), Some("[B UH1 K S T AO2 R].".to_string()));
        // Name fragments are not parts, however well they spell the word.
        assert_eq!(compound_sounds("carolina", 4, 2), None);
        // "rather" could be "rat" and "her", but the model is sure of it.
        let (_, sureness) = lts::predict_scored("rather").unwrap();
        assert!(sureness > UNSURE, "{sureness}");
        // Unsure of "webcam", and so it is said as its two words.
        assert_eq!(say("webcam"), "[W EH1 B K AE2 M].");
    }

    #[test]
    fn weekdays_end_in_day() {
        assert_eq!(say("Monday"), "[M AH1 N D EY2].");
        assert_eq!(say("Saturday"), "[S AE1 T ER0 D EY2].");
        assert_eq!(say("Fridays"), say("Fridays").replace("IY0", "EY2"));
        assert_eq!(say("Thursday"), "[TH ER1 Z D EY2].");
    }

    #[test]
    fn marks_endings_by_spelling() {
        let ending = |word: &str| phrases_with(word, true, false)[0].words[0].ending;
        assert!(ending("songs") && ending("turned") && ending("Klatt's"));
        assert!(!ending("bronze") && !ending("bend") && !ending("song"));
    }

    #[test]
    fn tokenizes_apostrophes_and_dashes() {
        assert_eq!(say("don't"), "[D OW1 N T].");
        assert_eq!(say("well-known"), "[W EH1 L] [N OW1 N].");
    }
}
