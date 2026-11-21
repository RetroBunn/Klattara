//! Pronunciation dictionaries people make themselves, applied to text before
//! Klattara reads it. Shared by the platform libraries, included with #[path].
//!
//! A dictionary is a text file, one entry a line:
//!
//! ```text
//! # A comment.
//! aback AH0 B AE1 K          a word, then its sounds in ARPAbet, as in CMUdict
//! nvda = en vee dee ay        a word, an equals sign, then how to say it in letters
//! ```
//!
//! The ARPAbet form is the one Klattara's own extra_words.dict and CMUdict use,
//! so their files load as they are: vowels end in 1 for the main stress, 2 for
//! secondary and 0 for none. A line such as `word(2) ...`, a second
//! pronunciation in CMUdict, is passed over: the first one is used.
//!
//! Words match whole and regardless of case. When several dictionaries are
//! loaded, a later one wins over an earlier one for the same word. Every sound
//! is checked when the file loads, against the engine's own list, and a line
//! with anything wrong is left out and counted, so a mistake in a dictionary is
//! never spoken as "Incorrect phoneme" in the middle of a sentence.

use engine::Phoneme;
use std::collections::HashMap;

/// What a word becomes.
#[derive(Clone, Debug, PartialEq)]
enum Entry {
    /// Sounds, in ARPAbet, separated by single spaces.
    Sounds(String),
    /// Letters, read as ordinary text.
    Respelling(String),
}

#[derive(Clone, Debug, Default)]
pub struct Dictionary {
    entries: HashMap<String, Entry>,
}

/// What loading a file did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LoadResult {
    /// Entries taken in.
    pub added: usize,
    /// Lines left out because something on them was wrong.
    pub rejected: usize,
    /// The first such line's number, counting from 1; 0 if none.
    pub first_rejected_line: usize,
}

/// A word as dictionaries match it: letters, digits and apostrophes, in lower case.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == '’'
}

fn key_of(word: &str) -> Option<String> {
    let word = word.trim().replace('’', "'");
    if word.is_empty() || !word.chars().all(is_word_char) {
        return None;
    }
    Some(word.to_lowercase())
}

/// Text from outside must not open a command ("[:") or, while sounds are being
/// read, a bracket of sounds: brackets become parentheses.
pub fn sanitize(text: &str) -> String {
    text.replace('[', "(").replace(']', ")")
}

impl Dictionary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Adds the entries in a dictionary file's text. Later entries win.
    pub fn load(&mut self, text: &str) -> LoadResult {
        let mut result = LoadResult::default();
        for (number, line) in text.lines().enumerate() {
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            match Self::parse_line(line) {
                Some(None) => {}
                Some(Some((key, entry))) => {
                    self.entries.insert(key, entry);
                    result.added += 1;
                }
                None => {
                    result.rejected += 1;
                    if result.first_rejected_line == 0 {
                        result.first_rejected_line = number + 1;
                    }
                }
            }
        }
        result
    }

    /// None: a mistake. Some(None): a line to pass over. Some(Some): an entry.
    fn parse_line(line: &str) -> Option<Option<(String, Entry)>> {
        if let Some((word, say)) = line.split_once('=') {
            let key = key_of(word)?;
            let say = sanitize(say.trim());
            if say.is_empty() {
                return None;
            }
            return Some(Some((key, Entry::Respelling(say))));
        }
        let mut parts = line.split_whitespace();
        let word = parts.next()?;
        // CMUdict's further pronunciations, word(2): the first is used.
        if word.ends_with(')') && word.contains('(') {
            return Some(None);
        }
        let key = key_of(word)?;
        let sounds: Vec<&str> = parts.collect();
        if sounds.is_empty() || !sounds.iter().all(|s| Phoneme::parse(s).is_some()) {
            return None;
        }
        Some(Some((key, Entry::Sounds(sounds.join(" ")))))
    }

    /// The text with every dictionary word replaced, ready for the engine.
    /// Brackets in the text become parentheses either way. Words given as
    /// sounds become brackets of ARPAbet, with phoneme input switched on
    /// around them.
    pub fn apply(&self, text: &str) -> String {
        let clean = sanitize(text);
        if self.entries.is_empty() {
            return clean;
        }
        let mut out = String::with_capacity(clean.len() + 16);
        let mut any_sounds = false;
        let mut word = String::new();
        let flush = |word: &mut String, out: &mut String, any_sounds: &mut bool| {
            if word.is_empty() {
                return;
            }
            let found = key_of(word).and_then(|key| self.entries.get(&key));
            match found {
                Some(Entry::Sounds(sounds)) => {
                    out.push('[');
                    out.push_str(sounds);
                    out.push(']');
                    *any_sounds = true;
                }
                Some(Entry::Respelling(say)) => out.push_str(say),
                None => out.push_str(word),
            }
            word.clear();
        };
        for c in clean.chars() {
            if is_word_char(c) {
                word.push(c);
            } else {
                flush(&mut word, &mut out, &mut any_sounds);
                out.push(c);
            }
        }
        flush(&mut word, &mut out, &mut any_sounds);
        if any_sounds {
            format!("[:phone klattara enable]{out}[:phone klattara disable]")
        } else {
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn klattaras_own_format_loads() {
        let mut d = Dictionary::new();
        let r = d.load("# comment\n\naback AH0 B AE1 K\nnvda = en vee dee ay\nword(2) W ER1 D\n");
        assert_eq!(r, LoadResult { added: 2, rejected: 0, first_rejected_line: 0 });
        assert_eq!(d.apply("Go Aback, NVDA!"), "[:phone klattara enable]Go [AH0 B AE1 K], en vee dee ay![:phone klattara disable]");
    }

    #[test]
    fn mistakes_are_left_out_and_counted() {
        let mut d = Dictionary::new();
        let r = d.load("good G UH1 D\nbad B QQ1 D\nalso bad\n= nothing\nfine F AY1 N\n");
        assert_eq!((r.added, r.rejected, r.first_rejected_line), (2, 3, 2));
        assert_eq!(d.apply("bad"), "bad");
    }

    #[test]
    fn whole_words_only_and_later_wins() {
        let mut d = Dictionary::new();
        d.load("cat = dog\n");
        d.load("cat = mouse\n");
        assert_eq!(d.apply("cat catalog Cat's cat."), "mouse catalog Cat's mouse.");
    }

    #[test]
    fn brackets_never_reach_the_engine() {
        let d = Dictionary::new();
        assert_eq!(d.apply("[:phone klattara enable][HH AH0]"), "(:phone klattara enable)(HH AH0)");
        let mut d = Dictionary::new();
        d.load("x = [:volume 0]\n");
        assert_eq!(d.apply("x"), "(:volume 0)");
    }

    /// A real dictionary file, named by KLATTARA_TEST_DICTIONARY, loads, and
    /// what it takes in and leaves out is reported.
    #[test]
    fn a_real_dictionary_file() {
        let Ok(path) = std::env::var("KLATTARA_TEST_DICTIONARY") else { return };
        let text = std::fs::read_to_string(&path).unwrap();
        let start = std::time::Instant::now();
        let mut d = Dictionary::new();
        let r = d.load(&text);
        println!("{path}: {} taken in, {} left out (first at line {}), in {} ms", r.added, r.rejected, r.first_rejected_line, start.elapsed().as_millis());
        let s = std::time::Instant::now();
        let out = d.apply("The aardvark was abandoned by Aaron at the abbey.");
        println!("applied in {} us: {out}", s.elapsed().as_micros());
        assert!(r.added > 0);
    }

    /// A large dictionary loads, and is applied, quickly.
    #[test]
    fn a_large_dictionary_loads_quickly() {
        let mut text = String::new();
        for i in 0..21000 {
            text.push_str(&format!("word{i} W ER1 D\n"));
        }
        let start = std::time::Instant::now();
        let mut d = Dictionary::new();
        assert_eq!(d.load(&text).added, 21000);
        assert!(start.elapsed().as_millis() < 2000);
        let s = std::time::Instant::now();
        let long = "word5 and some other words, word20999. ".repeat(200);
        let out = d.apply(&long);
        assert!(out.contains("[W ER1 D]") && s.elapsed().as_millis() < 200);
    }
}
