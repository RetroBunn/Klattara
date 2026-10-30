//! Words spelled alike but said differently, chosen from the words around
//! them: "I object" and "what object", "will record" and "speed record",
//! "will read" and "have you read", "Polish" and "polish".
//!
//! The words, their pronunciations and their most common readings come from
//! src/data/homographs.tsv, built from the training half of the Wikipedia
//! homograph data (Gorman, Mazovetskiy and Nikolaev 2018) by
//! src/tools/build_homographs.py; a few more are listed here.
//!
//! Most pairs are a noun or adjective stressed on the first syllable and a
//! verb stressed on the second, or a noun with S against a verb with Z. A
//! verb follows a subject, a helping verb or "to"; a noun follows a
//! determiner, a preposition or a form of "be". After another word, the next
//! word decides: "Jermain lives on", "students record the talk". With nothing
//! before it, a word followed by "the", "it" and the like is a verb ("Record
//! the show"), and a lone "Record" or "Export" is too, as on a button.
//! Otherwise a word takes its most common reading, which for a word
//! capitalized inside a sentence is often a name: "Polish", "Nestle".

use std::collections::HashMap;
use std::sync::OnceLock;

/// What a reading is: a noun or adjective, a verb, or something else, such
/// as a name or a tense.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Noun,
    Verb,
    Other,
}

use Role::{Noun, Other, Verb};

/// A homograph's readings and which is most common, in lowercase and when
/// capitalized inside a sentence.
struct Entry {
    readings: Vec<(Role, &'static str)>,
    lower: usize,
    capital: usize,
}

impl Entry {
    fn reading(&self, role: Role) -> Option<usize> {
        self.readings.iter().position(|r| r.0 == role)
    }

    fn noun_and_verb(&self) -> bool {
        self.reading(Noun).is_some() && self.reading(Verb).is_some()
    }
}

static TABLE: &str = include_str!("../../data/homographs.tsv");

/// Homographs not in the data, as noun and verb pronunciations and the
/// reading without a clue.
const MORE: &[(&str, &str, &str, Role)] =
    &[("combat", "K AA1 M B AE0 T", "K AH0 M B AE1 T", Noun), ("survey", "S ER1 V EY2", "S ER0 V EY1", Noun)];

/// Words read as verbs when they stand alone, as on a button: "Record",
/// "Export", "Close".
const LABEL_VERBS: &[&str] = &[
    "close", "convert", "decrease", "excuse", "export", "extract", "import", "increase", "produce", "record", "refuse",
    "reject", "use",
];

fn table() -> &'static HashMap<&'static str, Entry> {
    static PARSED: OnceLock<HashMap<&'static str, Entry>> = OnceLock::new();
    PARSED.get_or_init(|| {
        let mut entries = HashMap::new();
        for line in TABLE.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
            let fields: Vec<&str> = line.split('\t').collect();
            let readings = fields[3..]
                .iter()
                .map(|f| {
                    let (role, arpabet) = f.split_once(':').expect("role:ARPAbet");
                    (
                        match role {
                            "noun" => Noun,
                            "verb" => Verb,
                            _ => Other,
                        },
                        arpabet,
                    )
                })
                .collect();
            let index = |f: &str| f.parse().expect("reading number");
            entries.insert(fields[0], Entry { readings, lower: index(fields[1]), capital: index(fields[2]) });
        }
        for &(word, noun, verb, default) in MORE {
            let usual = if default == Verb { 1 } else { 0 };
            entries.insert(word, Entry { readings: vec![(Noun, noun), (Verb, verb)], lower: usual, capital: usual });
        }
        entries
    })
}

/// Words after which a homograph is a verb: subjects, helping verbs, "to",
/// and adverbs that come before verbs.
const BEFORE_VERB: &[&str] = &[
    "always",
    "also",
    "actually",
    "can",
    "can't",
    "cannot",
    "could",
    "couldn't",
    "did",
    "didn't",
    "do",
    "doesn't",
    "don't",
    "does",
    "he'd",
    "he'll",
    "i",
    "i'd",
    "i'll",
    "it'll",
    "just",
    "let's",
    "may",
    "might",
    "must",
    "mustn't",
    "never",
    "not",
    "often",
    "please",
    "rarely",
    "really",
    "shall",
    "she'd",
    "she'll",
    "should",
    "shouldn't",
    "sometimes",
    "they",
    "they'd",
    "they'll",
    "to",
    "usually",
    "we",
    "we'd",
    "we'll",
    "will",
    "won't",
    "would",
    "wouldn't",
    "you",
    "you'd",
    "you'll",
];

/// Subjects after which a word ending in -s is a verb: "he objects".
const BEFORE_VERB_S: &[&str] = &[
    "everybody",
    "everyone",
    "he",
    "it",
    "nobody",
    "one",
    "she",
    "somebody",
    "someone",
    "that",
    "this",
    "which",
    "who",
];

/// Words that begin an object, after which a phrase-initial homograph is a
/// verb: "Record the show", "Close it".
const OBJECT_START: &[&str] = &[
    "a", "all", "an", "any", "every", "her", "him", "his", "it", "its", "me", "my", "our", "some", "that", "the",
    "their", "them", "these", "this", "those", "us", "your",
];

/// Words after which a homograph is a noun or adjective, whatever follows:
/// determiners, possessives, prepositions and forms of "be".
const BEFORE_NOUN: &[&str] = &[
    "a", "about", "am", "an", "another", "any", "are", "at", "be", "been", "by", "each", "every", "for", "from", "her",
    "his", "in", "into", "is", "its", "more", "most", "my", "no", "of", "on", "our", "so", "some", "the", "their",
    "these", "this", "those", "too", "very", "was", "were", "what", "which", "whose", "with", "your",
];

/// Words after which a verb ending in -s often comes: "lives on", "objects
/// to".
const AFTER_VERB_S: &[&str] =
    &["alone", "at", "by", "from", "here", "in", "into", "near", "on", "there", "to", "together", "with"];

/// Words joining two alike: the second homograph takes the first's reading.
const CONJUNCTIONS: &[&str] = &["and", "or", "nor"];

/// Forms of "be", "have" and "get" that make "read" the past form, "red".
const PAST_READ_HELPERS: &[&str] = &[
    "am", "are", "be", "been", "being", "get", "gets", "got", "had", "has", "have", "having", "i've", "is", "they've",
    "was", "we've", "were", "who've", "you've",
];

/// Words that may stand between such a helper and "read": "have you read",
/// "has she ever read".
const READ_SKIPPABLE: &[&str] = &[
    "all", "already", "also", "both", "ever", "finally", "he", "i", "it", "just", "never", "not", "once", "recently",
    "she", "they", "we", "you",
];

/// Helping words after which "read" is the present form: "will read", "did
/// you read", "to read".
const PRESENT_READ_HELPERS: &[&str] = &[
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
    "i'll",
    "let's",
    "may",
    "might",
    "must",
    "please",
    "shall",
    "should",
    "shouldn't",
    "they'll",
    "to",
    "we'll",
    "will",
    "won't",
    "would",
    "wouldn't",
    "you'll",
];

/// Past forms that mark a sentence as telling of the past: "He was educated
/// at Eton, where he read history". Regular past forms in -ed count too.
const PAST_FORMS: &[&str] = &[
    "ate",
    "became",
    "began",
    "bought",
    "brought",
    "built",
    "came",
    "caught",
    "chose",
    "drew",
    "drove",
    "fell",
    "felt",
    "flew",
    "fought",
    "found",
    "gave",
    "got",
    "grew",
    "had",
    "heard",
    "held",
    "kept",
    "knew",
    "left",
    "lost",
    "made",
    "meant",
    "met",
    "paid",
    "ran",
    "rode",
    "rose",
    "said",
    "sang",
    "sat",
    "saw",
    "sent",
    "sold",
    "spent",
    "spoke",
    "stood",
    "struck",
    "taught",
    "thought",
    "threw",
    "told",
    "took",
    "understood",
    "was",
    "went",
    "were",
    "won",
    "wore",
    "wrote",
];

/// Present forms that mark a sentence as about the present or future.
const PRESENT_FORMS: &[&str] = &[
    "am", "are", "aren't", "can", "can't", "do", "does", "doesn't", "don't", "he's", "i'm", "is", "isn't", "it's",
    "let's", "may", "must", "please", "shall", "she's", "that's", "there's", "they're", "we're", "what's", "will",
    "won't", "you're",
];

/// Words ending in -ed that are not past forms.
const NOT_PAST: &[&str] = &[
    "bed", "beloved", "crooked", "hundred", "jagged", "kindred", "naked", "ragged", "red", "rugged", "sacred", "shed",
    "sled", "wed", "wicked",
];

fn contains(list: &[&str], word: &str) -> bool {
    list.contains(&word)
}

/// Whether a sentence's words tell of the past: more past forms than
/// present ones.
pub fn narrative_past(words: &[&str]) -> bool {
    let past = words
        .iter()
        .filter(|w| {
            contains(PAST_FORMS, w)
                || (w.len() > 3
                    && w.ends_with("ed")
                    && !(w.ends_with("eed") && !matches!(**w, "agreed" | "freed"))
                    && !contains(NOT_PAST, w))
        })
        .count();
    let present = words.iter().filter(|w| contains(PRESENT_FORMS, w)).count();
    past > present
}

/// The entry for a word and whether it carries an -s ending: "records" is
/// "record" plus -s, unless the table has "records" itself.
fn lookup(word: &str) -> Option<(&'static Entry, bool)> {
    let table = table();
    if let Some(entry) = table.get(word) {
        // A listed -s form of a verb ("uses", "lives") still takes the cues of one.
        let with_s =
            word.ends_with('s') && entry.reading(Verb).is_some_and(|r| entry.readings[r].1.ends_with(['S', 'Z']));
        return Some((entry, with_s));
    }
    let stem = word.strip_suffix("es").filter(|s| table.contains_key(s)).or_else(|| word.strip_suffix('s'))?;
    table.get(stem).filter(|e| e.noun_and_verb()).map(|entry| (entry, true))
}

/// Whether the context makes `words[i]`, a noun-or-verb homograph, a verb or
/// a noun; `None` when it gives no clue.
fn role(words: &[&str], i: usize, roles: &[Option<Role>], with_s: bool, word: &str) -> Option<Role> {
    let before = i.checked_sub(1).map(|j| words[j]);
    let after = words.get(i + 1).copied();
    let object_next = !with_s && after.is_some_and(|a| contains(OBJECT_START, a));
    match before {
        // "record and export it": like the homograph before the conjunction.
        Some(b) if contains(CONJUNCTIONS, b) && i >= 2 && roles[i - 2].is_some() => roles[i - 2],
        Some(b) if !contains(CONJUNCTIONS, b) => {
            let verb = if with_s {
                contains(BEFORE_VERB_S, b)
            } else {
                // "close to perfect" is an adjective, though "to" usually marks a verb.
                contains(BEFORE_VERB, b) && !(b == "to" && word == "perfect")
            };
            if verb {
                Some(Verb)
            } else if contains(BEFORE_NOUN, b) {
                Some(Noun)
            } else if object_next || (with_s && after.is_some_and(|a| contains(AFTER_VERB_S, a))) {
                // After another word, such as a name or a noun, the next word
                // decides: "Jermain lives on", "students record the talk".
                Some(Verb)
            } else {
                None
            }
        }
        _ if object_next => Some(Verb),
        // A lone label, as on a button.
        _ if after.is_none() && !with_s && contains(LABEL_VERBS, word) => Some(Verb),
        _ => None,
    }
}

/// Whether "read" at `words[i]` is the past form, "red". Never right after a
/// helping word such as "will" or "to", possibly with a subject between
/// ("did you read"). Otherwise after a form of "have" or "be" ("have you
/// read"), after "already" or "as", in a phrase that says "yesterday" or
/// "ago", or in a sentence that tells of the past.
fn past_read(words: &[&str], i: usize, past_sentence: bool) -> bool {
    for j in (0..i).rev().take(3) {
        let w = words[j];
        if contains(PRESENT_READ_HELPERS, w) {
            return false;
        }
        if contains(PAST_READ_HELPERS, w) || (j + 1 == i && (w == "already" || w == "as")) {
            return true;
        }
        if !contains(READ_SKIPPABLE, w) {
            break;
        }
    }
    past_sentence || words.iter().any(|w| *w == "yesterday" || *w == "ago")
}

/// A chosen pronunciation: ARPAbet for the base form, and whether to add the
/// -s ending to it.
pub struct Choice {
    pub arpabet: &'static str,
    pub add_s: bool,
}

/// Chooses the pronunciations of the homographs in a phrase's words
/// (lowercase), given which words are capitalized inside the sentence and
/// whether the sentence tells of the past; `None` for other words.
pub fn choose(words: &[&str], capitalized: &[bool], past_sentence: bool) -> Vec<Option<Choice>> {
    let mut roles: Vec<Option<Role>> = vec![None; words.len()];
    let mut choices = Vec::with_capacity(words.len());
    for (i, &word) in words.iter().enumerate() {
        let after = words.get(i + 1).copied();
        let before = i.checked_sub(1).map(|j| words[j]);
        let choice = match word {
            "read" => Some(Choice {
                arpabet: if past_read(words, i, past_sentence) { "R EH1 D" } else { "R IY1 D" },
                add_s: false,
            }),
            // "lead" is "led" only for the metal: "made of lead", "lead pipe".
            "lead" | "leads" => {
                let metal = before == Some("of")
                    || after.is_some_and(|a| ["paint", "pipe", "pipes", "pencil", "poisoning", "weight"].contains(&a));
                Some(Choice { arpabet: if metal { "L EH1 D" } else { "L IY1 D" }, add_s: word == "leads" })
            }
            // "used to" in the sense of "formerly": "I used to live there".
            // After "be" it is the passive: "it was used to cut".
            "used" if after == Some("to") && !before.is_some_and(|b| contains(PAST_READ_HELPERS, b)) => {
                Some(Choice { arpabet: "Y UW1 S T", add_s: false })
            }
            _ => lookup(word).map(|(entry, with_s)| {
                let capital = capitalized.get(i).copied().unwrap_or(false);
                let usual = if capital { entry.capital } else { entry.lower };
                // A name stays a name: "Nestle" is not the verb.
                let name = capital && entry.readings[usual].0 == Other;
                let index = match entry.noun_and_verb() && !name {
                    true => role(words, i, &roles, with_s, word).and_then(|r| entry.reading(r)).unwrap_or(usual),
                    false => usual,
                };
                let (role, arpabet) = entry.readings[index];
                roles[i] = Some(role).filter(|r| *r != Other);
                Choice { arpabet, add_s: with_s && !table().contains_key(word) }
            }),
        };
        choices.push(choice);
    }
    choices
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choices(sentence: &str, past: bool) -> Vec<Choice> {
        let words: Vec<&str> = sentence.split_whitespace().collect();
        choose(&words, &vec![false; words.len()], past).into_iter().flatten().collect()
    }

    /// The role of each homograph's chosen reading, from the table.
    fn roles(sentence: &str) -> Vec<Role> {
        let words: Vec<&str> = sentence.split_whitespace().collect();
        let chosen = choose(&words, &vec![false; words.len()], false);
        let entry = |w: &str, arpabet| lookup(w).unwrap().0.readings.iter().find(|r| r.1 == arpabet).unwrap().0;
        words.iter().zip(chosen).filter_map(|(w, c)| c.map(|c| entry(w, c.arpabet))).collect()
    }

    fn said(sentence: &str, past: bool) -> Vec<&'static str> {
        choices(sentence, past).into_iter().map(|c| c.arpabet).collect()
    }

    #[test]
    fn chooses_noun_or_verb_from_context() {
        assert_eq!(roles("i object what object does this serve"), [Verb, Noun]);
        assert_eq!(roles("a new land speed record i hope you will record it"), [Noun, Verb]);
        assert_eq!(roles("the subject of the present"), [Noun, Noun]);
        assert_eq!(roles("close the door it is close"), [Verb, Noun]);
        assert_eq!(roles("we live in a live show"), [Verb, Noun]);
        assert_eq!(roles("he objects to their objects"), [Verb, Noun]);
        assert_eq!(roles("use the arrow keys it has no use"), [Verb, Noun]);
        assert_eq!(roles("saint jermain lives on their lives"), [Verb, Noun]);
        assert_eq!(roles("the speed record holder"), [Noun]);
        assert_eq!(roles("students record the talk"), [Verb]);
        // Alone, as on a button.
        assert_eq!(roles("record"), [Verb]);
        assert_eq!(roles("subject"), [Noun]);
    }

    #[test]
    fn follows_the_word_before_a_conjunction() {
        assert_eq!(roles("to record and export it"), [Verb, Verb]);
        assert_eq!(roles("the import and export of goods"), [Noun, Noun]);
    }

    #[test]
    fn names_are_read_as_names() {
        let words = ["the", "polish", "language", "and", "shoe", "polish"];
        let said: Vec<&str> = choose(&words, &[false, true, false, false, false, false], false)
            .into_iter()
            .flatten()
            .map(|c| c.arpabet)
            .collect();
        assert_eq!(said, ["P OW1 L IH0 SH", "P AA1 L IH0 SH"]);
    }

    #[test]
    fn reads_read_by_tense() {
        assert_eq!(said("i will read this document", true), ["R IY1 D", "D AA1 K Y UW2 M AH0 N T"]);
        assert_eq!(said("have you read my latest book", false), ["R EH1 D"]);
        assert_eq!(said("i have to read it", true), ["R IY1 D"]);
        assert_eq!(said("i read it yesterday", false), ["R EH1 D"]);
        assert_eq!(said("did you read it", true), ["R IY1 D"]);
        assert_eq!(said("where he read history", true), ["R EH1 D"]);
        assert_eq!(said("read more", false), ["R IY1 D"]);
        assert!(narrative_past(&["he", "was", "educated", "at", "eton"]));
        assert!(!narrative_past(&["press", "enter", "to", "read", "it", "is", "ready"]));
    }

    #[test]
    fn handles_lead_and_used_to() {
        assert_eq!(said("lead the way", false), ["L IY1 D"]);
        assert_eq!(said("made of lead", false), ["L EH1 D"]);
        assert_eq!(said("i used to live there", false)[0], "Y UW1 S T");
        assert_eq!(said("it was used to cut", false), Vec::<&str>::new());
    }
}
