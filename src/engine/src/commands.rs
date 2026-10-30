//! Commands and phonetic words inside text, in square brackets, as in DECtalk.
//!
//! `[:phone klattara enable]` turns phoneme input on and
//! `[:phone klattara disable]` turns it off. Case and extra spaces don't matter,
//! and the setting lasts until it is changed. While phoneme input is on, a
//! bracket holding valid ARPAbet is spoken as one word, in among ordinary text:
//! "Say [HH AH0 L OW1] now."
//!
//! Anything else in brackets is ordinary text, as it always was: a citation like
//! "[1]" read with phoneme input on, a slice like "a[:5]", a misspelled command.

use crate::speech::Voice;
use crate::text;
use crate::utterance::{Boundary, Phrase, parse};

/// The setting a bracket's contents ask for, if they are the phoneme input command.
fn phoneme_input_command(inner: &str) -> Option<bool> {
    let words: Vec<&str> = inner.strip_prefix(':')?.split_whitespace().collect();
    let [command, alphabet, state] = words[..] else { return None };
    if !command.eq_ignore_ascii_case("phone") || !alphabet.eq_ignore_ascii_case("klattara") {
        return None;
    }
    match state {
        s if s.eq_ignore_ascii_case("enable") => Some(true),
        s if s.eq_ignore_ascii_case("disable") => Some(false),
        _ => None,
    }
}

/// The boundary that punctuation at the start of `text` puts on the phrase before it.
fn leading_boundary(text: &str) -> Option<Boundary> {
    match text.trim_start().chars().next()? {
        ',' => Some(Boundary::Comma),
        ';' | ':' => Some(Boundary::Clause),
        '.' => Some(Boundary::Period),
        '?' => Some(Boundary::Question),
        '!' => Some(Boundary::Exclamation),
        _ => None,
    }
}

/// Whether `text` ends in punctuation that closes a phrase, closing quotes aside.
fn ends_a_phrase(text: &str) -> bool {
    let end = text.trim_end().trim_end_matches(['"', '\'', '\u{201D}', '\u{2019}']);
    end.ends_with([',', '(', ')', '\u{2013}', '\u{2014}', ';', ':', '.', '\u{2026}', '?', '!'])
}

/// Phrases gathered from stretches of text and phonetic words. A stretch with no
/// punctuation at its end runs on into the next, as one phrase with one contour.
#[derive(Default)]
struct Gathered {
    phrases: Vec<Phrase>,
    /// Whether the last phrase's ending is only the default one.
    open: bool,
}

impl Gathered {
    fn add(&mut self, mut phrases: Vec<Phrase>, ended: bool) {
        if phrases.is_empty() {
            return;
        }
        if self.open
            && let Some(last) = self.phrases.last_mut()
        {
            let first = phrases.remove(0);
            last.words.extend(first.words);
            last.boundary = first.boundary;
        }
        self.phrases.extend(phrases);
        self.open = !ended;
    }

    fn add_text(&mut self, text: &str, voice: &Voice) {
        if self.open
            && let Some(boundary) = leading_boundary(text)
            && let Some(last) = self.phrases.last_mut()
        {
            last.boundary = boundary;
            self.open = false;
        }
        let phrases = text::phrases_with(text, voice.expand_abbreviations, voice.phrase_prediction);
        self.add(phrases, ends_a_phrase(text));
    }
}

/// Reads text with the commands and phonetic words in it, which may change the
/// voice's phoneme input setting.
pub fn phrases(input: &str, voice: &mut Voice) -> Vec<Phrase> {
    let mut gathered = Gathered::default();
    // Where the text not yet read starts, and where to look for the next bracket.
    let (mut from, mut search) = (0, 0);
    while let Some(open) = input[search..].find('[').map(|at| at + search) {
        let Some(end) = input[open..].find(']').map(|at| at + open + 1) else { break };
        if let Some(enable) = phoneme_input_command(&input[open + 1..end - 1]) {
            gathered.add_text(&input[from..open], voice);
            voice.phoneme_input = enable;
            (from, search) = (end, end);
        } else if voice.phoneme_input
            && let Ok(word) = parse(&input[open..end])
        {
            gathered.add_text(&input[from..open], voice);
            gathered.add(word, false);
            (from, search) = (end, end);
        } else {
            search = open + 1;
        }
    }
    gathered.add_text(&input[from..], voice);
    gathered.phrases
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utterance::notation;

    /// How `input` is read, starting with phoneme input as given.
    fn read(input: &str, phoneme_input: bool) -> String {
        notation(&phrases(input, &mut Voice { phoneme_input, ..Voice::default() }))
    }

    /// How `input` is read by the text reader alone.
    fn plain(input: &str) -> String {
        notation(&text::phrases_with(input, true, false))
    }

    #[test]
    fn text_without_commands_is_read_as_ever() {
        for input in ["Hello there.", "See [1] for more.", "a[:5] is a slice.", "[HH AH0 L OW1] and [unclosed", ""] {
            assert_eq!(read(input, false), plain(input), "{input}");
        }
    }

    #[test]
    fn phoneme_input_is_off_until_enabled() {
        let word = "[HH AH0 L OW1]";
        assert_ne!(read(word, false), word.to_string() + ".");
        assert_eq!(read(&format!("[:phone klattara enable] {word}"), false), word.to_string() + ".");
        assert_eq!(read(word, true), word.to_string() + ".");
    }

    #[test]
    fn the_command_ignores_case_and_extra_spaces_and_says_nothing() {
        for command in ["[:phone klattara enable]", "[:phone Klattara enable]", "[:PHONE  KLATTARA\tENABLE ]"] {
            let mut voice = Voice::default();
            assert!(phrases(command, &mut voice).is_empty(), "{command}");
            assert!(voice.phoneme_input, "{command}");
        }
        let mut voice = Voice { phoneme_input: true, ..Voice::default() };
        assert!(phrases("[:phone Klattara disable]", &mut voice).is_empty());
        assert!(!voice.phoneme_input);
    }

    #[test]
    fn a_command_can_change_the_setting_partway_through() {
        let on = "[:phone klattara enable]";
        let off = "[:phone klattara disable]";
        assert_eq!(read(&format!("{on}[HH AY1] {off}[S AY1]."), false), format!("[HH AY1] {}", plain("[S AY1].")));
    }

    #[test]
    fn phonetic_words_join_the_sentence_around_them() {
        let spoken = read("Say [HH AH0 L OW1] now.", true);
        assert_eq!(spoken, format!("[S EY1] [HH AH0 L OW1] {}", plain("now.")));
        // One phrase, not three sentences.
        let mut voice = Voice { phoneme_input: true, ..Voice::default() };
        assert_eq!(phrases("Say [HH AH0 L OW1] now.", &mut voice).len(), 1);
        // A command in mid-sentence doesn't break it either.
        assert_eq!(phrases("Say [:phone klattara disable] it now.", &mut voice).len(), 1);
    }

    #[test]
    fn punctuation_after_a_phonetic_word_ends_its_phrase() {
        let mut voice = Voice { phoneme_input: true, ..Voice::default() };
        let got = phrases("[HH AH0 L OW1], [W ER1 L D]? Yes.", &mut voice);
        let endings: Vec<Boundary> = got.iter().map(|p| p.boundary).collect();
        assert_eq!(endings, [Boundary::Comma, Boundary::Question, Boundary::Period]);
    }

    #[test]
    fn punctuation_before_a_phonetic_word_keeps_its_place() {
        let mut voice = Voice { phoneme_input: true, ..Voice::default() };
        assert_eq!(phrases("Well, [HH AH0 L OW1].", &mut voice).len(), 2);
    }

    #[test]
    fn other_brackets_are_ordinary_text() {
        for input in ["See [1] for more.", "A [note] here.", "[]", "[:phone klattara maybe] Hi.", "[:rate 200] Hi."] {
            assert_eq!(read(input, true), plain(input), "{input}");
        }
        // Without its closing bracket, a command is not one.
        let mut voice = Voice::default();
        phrases("[:phone klattara enable", &mut voice);
        assert!(!voice.phoneme_input);
        // A bracket that isn't a command doesn't hide one after it.
        phrases("[:rate [:phone klattara enable]", &mut voice);
        assert!(voice.phoneme_input);
    }
}
