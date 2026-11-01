//! Commands and phonetic words inside text, in square brackets, as in DECtalk.
//!
//! Case and extra spaces don't matter, and what a command sets lasts until it is
//! changed.
//!
//! - `[:phone klattara enable]` turns phoneme input on and
//!   `[:phone klattara disable]` turns it off. While it is on, a bracket holding
//!   valid ARPAbet is spoken as one word, in among ordinary text: "Say [HH AH0 L OW1]
//!   now."
//! - `[:voice Brittany]` changes to a voice by name, and `[:vb]` by its first letter.
//! - `[:cv pi 150 br 40]` customizes the voice: see [`SETTINGS`], which also holds the
//!   values each setting can take. A value beyond them stops at the limit.
//! - `[:speed 400]` or `[:spd 400]` sets the speed in words per minute, `[:volume 50]`
//!   the volume, and `[:vibspeed 6]`, `[:vibdepth 30]` and `[:vibdelay 200]` the vibrato
//!   of sung notes: see [`SINGLE_SETTINGS`].
//! - `[:reset]` returns everything to how an engine starts.
//!
//! All of these but phoneme input apply from there on, and break the speech at that
//! point, because what comes before them is spoken as the voice was.
//!
//! A mistake is spoken aloud, as DECtalk's are, and what follows the mistake in its
//! bracket is skipped: see [`Alert`]. Whatever came before it in the bracket still
//! counts, the sounds of a phonetic word or the settings of a `[:cv]`. With phoneme input
//! off, a bracket with no colon is ordinary text, like the citation "[1]".
//!
//! Where DECtalk would skip everything up to the next closing bracket when one is
//! missing, which can be a long way, here the text goes on being read after the alert.

use crate::speech::{FASTEST_WPM, SLOWEST_WPM, Voice, rate_for_words_per_minute};
use crate::text;
use crate::utterance::{Boundary, Phrase, read_sounds};
use crate::voicedefs::{Gender, VOICES, VoiceDef, find_voice};

/// A mistake that is spoken aloud. These follow what sets off DECtalk's own alerts, such
/// as "Command error in phoneme".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Alert {
    /// With phoneme input on, a bracket with something in it that isn't a sound, or a
    /// bad length or pitch.
    Phoneme,
    /// A command that doesn't exist, or one with no closing bracket.
    Command,
    /// Something a command can't use: words in place of a number, a setting that doesn't
    /// exist, or more than the command takes.
    Parameter,
    /// A value that is missing, or isn't one of those the command can choose from.
    Value,
}

impl Alert {
    fn words(self) -> &'static str {
        match self {
            Alert::Phoneme => "Incorrect phoneme.",
            Alert::Command => "Incorrect command.",
            Alert::Parameter => "Incorrect parameter.",
            Alert::Value => "Incorrect value.",
        }
    }
}

/// One thing a command can set, and the values it can take.
struct Setting {
    name: &'static str,
    min: f64,
    max: f64,
    /// The only values it can take, by name, if it doesn't take a number in a range.
    choices: &'static [(&'static str, f64)],
    apply: fn(&mut Voice, f64),
}

impl Setting {
    const fn number(name: &'static str, min: f64, max: f64, apply: fn(&mut Voice, f64)) -> Setting {
        Setting { name, min, max, choices: &[], apply }
    }

    /// Reads a value for this setting. A number beyond the limits stops at the limit.
    fn read(&self, text: &str) -> Result<f64, Alert> {
        if !self.choices.is_empty() {
            let choice = self.choices.iter().find(|(name, _)| name.eq_ignore_ascii_case(text));
            return choice.map(|(_, value)| *value).ok_or(Alert::Value);
        }
        let value: f64 = text.parse().ok().filter(|value: &f64| value.is_finite()).ok_or(Alert::Parameter)?;
        Ok(value.clamp(self.min, self.max))
    }
}

/// The tilt at the most smoothness, dB at 3 kHz, as in DECtalk.
const SMOOTHEST_TILT: f64 = 25.0;
/// The open quotients at the least and the most richness: the whole of what the voice
/// source can use, so that every part of the range does something. In DECtalk, where
/// the open quotient is simply what richness leaves of 100%, the ends do nothing.
const LEAST_RICH_OPEN_QUOTIENT: f64 = 0.9;
const MOST_RICH_OPEN_QUOTIENT: f64 = 0.2;

/// What `[:cv]` can set:
///
/// - `pi`: average pitch, Hz.
/// - `in`: inflection, as a percentage of normal pitch movement.
/// - `hs`: head size, as a percentage of normal: see [`Voice::head_size`].
/// - `rg`: roughness, from none to the most: see [`Voice::roughness`].
/// - `br`: breathiness, dB: see [`Voice::breathiness`].
/// - `rc`: richness, as a percentage. It sets the open quotient, and 50 is Steve's. More
///   weakens the lowest harmonics, which sounds thinner and buzzier: from one end to the
///   other, the first harmonic goes from 7 dB above the second to 7 dB below it, while
///   energy above 2 kHz moves by less than 1 dB.
/// - `sm`: smoothness, as a percentage. It sets the spectral tilt, and 0 is Steve's. More
///   softens the voice, but only the voice: at 100, voiced sound has lost most of its high
///   frequencies, and consonants like /s/ stand out more against it.
/// - `ge`: gender, which chooses the model of the vocal tract: 0 or f for female, and 1
///   or m for male. It changes nothing else, so the pitch stays as it was.
/// - `f4`, `f5` and `f6`: F4 to F6, Hz: see [`Voice::upper_formants`]. DECtalk has f4 and
///   f5, and their limits are from it, though its top value turns a formant off, and here
///   it is only a high formant. `f6` is this engine's own, with limits chosen to match.
///   Set close together, formants add up: F4 and F5 both at 2600 Hz are 5.7 dB louder, and
///   1% of the samples reach the soft clipper. F4 and F5 at 3000 and 3400 add 0.3 dB.
/// - `b4`, `b5` and `b6`: their bandwidths, Hz.
/// - `fl`: flutter, as a percentage: how much the pitch wavers, slowly and unevenly. It is
///   the FL of KLSYN88 (Klatt and Klatt 1990), and 25 is what most voices have.
/// - `vvl`, `fvl`, `avl` and `nvl`: how loud voicing, frication, aspiration and nasals are,
///   as a percentage of normal, so 100 is normal, 50 is about 6 dB quieter, and 0 is off.
///   DECtalk's gv, gf and gh do the same job in dB, from defaults that differ for each voice
///   and each source, so a setting means little without knowing them. Here every voice
///   starts at 100. DECtalk's gn scales a point in the whole voiced path, not the nasals,
///   and its later versions leave it unused, so `nvl` is this engine's own and sets the
///   level of M, N and NG. Its g1 to g5 keep the signal in range in fixed-point
///   arithmetic, which this engine doesn't need.
/// - `fa`, `ha`, `st` and `qk`: the pitch shape, as in [`crate::PitchShape`]: baseline fall,
///   hat rise, stress rise and quickness, DECtalk's bf, hr, sr and qu, with its limits for
///   the fall and for quickness. Its hat and stress rises stop at 2 and 1, and here 0 means
///   none.
/// - `lx`: lax breathiness, as a percentage, DECtalk's lx.
/// - `oq`: the open quotient, as a percentage. `rc` sets the same thing from the other end.
static SETTINGS: [Setting; 25] = [
    Setting::number("pi", 40.0, 550.0, |voice, hz| voice.pitch = hz),
    Setting::number("in", 0.0, 250.0, |voice, percent| voice.inflection = percent / 100.0),
    Setting::number("hs", 64.0, 135.0, |voice, percent| voice.head_size = percent),
    Setting::number("rg", 0.0, 100.0, |voice, amount| voice.roughness = amount),
    Setting::number("br", 0.0, 72.0, |voice, db| voice.breathiness = db),
    Setting::number("rc", 0.0, 100.0, |voice, percent| {
        let share = percent / 100.0;
        voice.open_quotient = LEAST_RICH_OPEN_QUOTIENT + (MOST_RICH_OPEN_QUOTIENT - LEAST_RICH_OPEN_QUOTIENT) * share;
    }),
    Setting::number("sm", 0.0, 100.0, |voice, percent| voice.tilt = SMOOTHEST_TILT * percent / 100.0),
    Setting {
        name: "ge",
        min: 0.0,
        max: 1.0,
        choices: &[("0", 0.0), ("f", 0.0), ("1", 1.0), ("m", 1.0)],
        apply: |voice, value| voice.gender = if value == 1.0 { Gender::Male } else { Gender::Female },
    },
    Setting::number("f4", 2000.0, 6000.0, |voice, hz| voice.upper_formants[0] = Some(hz)),
    Setting::number("f5", 2500.0, 6000.0, |voice, hz| voice.upper_formants[1] = Some(hz)),
    Setting::number("f6", 3000.0, 9000.0, |voice, hz| voice.upper_formants[2] = Some(hz)),
    Setting::number("b4", 100.0, 4000.0, |voice, hz| voice.upper_bandwidths[0] = Some(hz)),
    Setting::number("b5", 100.0, 4000.0, |voice, hz| voice.upper_bandwidths[1] = Some(hz)),
    Setting::number("b6", 100.0, 4000.0, |voice, hz| voice.upper_bandwidths[2] = Some(hz)),
    Setting::number("fl", 0.0, 100.0, |voice, percent| voice.flutter = percent),
    Setting::number("vvl", 0.0, 200.0, |voice, percent| voice.gains.voicing = percent / 100.0),
    Setting::number("fvl", 0.0, 200.0, |voice, percent| voice.gains.frication = percent / 100.0),
    Setting::number("avl", 0.0, 200.0, |voice, percent| voice.gains.aspiration = percent / 100.0),
    Setting::number("nvl", 0.0, 200.0, |voice, percent| voice.gains.nasal = percent / 100.0),
    Setting::number("fa", 0.0, 90.0, |voice, fall| voice.pitch_shape.baseline_fall = fall),
    Setting::number("ha", 0.0, 100.0, |voice, rise| voice.pitch_shape.hat_rise = rise),
    Setting::number("st", 0.0, 100.0, |voice, rise| voice.pitch_shape.stress_rise = rise),
    Setting::number("qk", 0.0, 100.0, |voice, percent| voice.pitch_shape.quickness = percent),
    Setting::number("lx", 0.0, 100.0, |voice, percent| voice.lax_breathiness = percent),
    Setting::number("oq", 20.0, 90.0, |voice, percent| voice.open_quotient = percent / 100.0),
];

/// The commands that set one thing each, written `[:name value]`, with the values each
/// can take:
///
/// - `vibspeed`: how fast a note's vibrato swings, Hz.
/// - `vibdepth`: how far it swings the pitch either way, cents.
/// - `vibdelay`: how long a note holds still before its vibrato starts, ms.
/// - `speed` or `spd`: the speed of speech, words per minute.
/// - `volume`: the volume, as a percentage, where 100 is normal.
static SINGLE_SETTINGS: [Setting; 6] = [
    Setting::number("vibspeed", 0.0, 20.0, |voice, hz| voice.vibrato_speed = hz),
    Setting::number("vibdepth", 0.0, 200.0, |voice, cents| voice.vibrato_depth = cents),
    Setting::number("vibdelay", 0.0, 5000.0, |voice, ms| voice.vibrato_delay = ms),
    Setting::number("speed", SLOWEST_WPM, FASTEST_WPM, |voice, wpm| voice.rate = rate_for_words_per_minute(wpm)),
    Setting::number("spd", SLOWEST_WPM, FASTEST_WPM, |voice, wpm| voice.rate = rate_for_words_per_minute(wpm)),
    Setting::number("volume", 0.0, 200.0, |voice, percent| voice.volume = percent / 100.0),
];

/// A command understood from a bracket's contents.
enum Command {
    PhonemeInput(bool),
    ChangeVoice(&'static VoiceDef),
    Customize(Vec<(&'static Setting, f64)>),
    Reset,
}

/// The voice named by a command like "vb": a V and the voice's first letter.
fn voice_by_letter(word: &str) -> Option<&'static VoiceDef> {
    let mut letters = word.chars();
    let (v, letter) = (letters.next()?, letters.next()?);
    if !v.eq_ignore_ascii_case(&'v') || letters.next().is_some() {
        return None;
    }
    VOICES.iter().find(|voice| voice.name.chars().next().is_some_and(|first| first.eq_ignore_ascii_case(&letter)))
}

/// Finds a setting by name.
fn find_setting(from: &'static [Setting], name: &str) -> Option<&'static Setting> {
    from.iter().find(|setting| setting.name.eq_ignore_ascii_case(name))
}

/// Reads the settings of a `[:cv]` command, in pairs of a name and a value, as far as
/// they are valid. As in DECtalk, those before a mistake still count, and with no
/// settings at all the command simply does nothing.
fn read_settings(words: &[&str]) -> (Vec<(&'static Setting, f64)>, Option<Alert>) {
    let mut settings = Vec::new();
    for pair in words.chunks(2) {
        let Some(setting) = find_setting(&SETTINGS, pair[0]) else { return (settings, Some(Alert::Parameter)) };
        let Some(value) = pair.get(1) else { return (settings, Some(Alert::Value)) };
        match setting.read(value) {
            Ok(value) => settings.push((setting, value)),
            Err(alert) => return (settings, Some(alert)),
        }
    }
    (settings, None)
}

/// Reads what follows the colon of a command: the command, as far as it is valid, and
/// the mistake in it, if there is one.
fn read_command(inner: &str) -> (Option<Command>, Option<Alert>) {
    let words: Vec<&str> = inner.split_whitespace().collect();
    let is = |word: &str, name: &str| word.eq_ignore_ascii_case(name);
    let Some((&name, rest)) = words.split_first() else { return (None, Some(Alert::Command)) };
    if is(name, "phone") {
        return match rest {
            [alphabet, state] if is(alphabet, "klattara") && is(state, "enable") => {
                (Some(Command::PhonemeInput(true)), None)
            }
            [alphabet, state] if is(alphabet, "klattara") && is(state, "disable") => {
                (Some(Command::PhonemeInput(false)), None)
            }
            _ => (None, Some(Alert::Value)),
        };
    }
    if is(name, "voice") {
        return match find_voice(&rest.join(" ")) {
            Some(def) => (Some(Command::ChangeVoice(def)), None),
            None => (None, Some(Alert::Value)),
        };
    }
    if is(name, "cv") {
        let (settings, alert) = read_settings(rest);
        return ((!settings.is_empty()).then_some(Command::Customize(settings)), alert);
    }
    if is(name, "reset") {
        return if rest.is_empty() { (Some(Command::Reset), None) } else { (None, Some(Alert::Parameter)) };
    }
    if let Some(def) = voice_by_letter(name) {
        return if rest.is_empty() { (Some(Command::ChangeVoice(def)), None) } else { (None, Some(Alert::Parameter)) };
    }
    let Some(setting) = find_setting(&SINGLE_SETTINGS, name) else { return (None, Some(Alert::Command)) };
    match rest {
        [] => (None, Some(Alert::Value)),
        [value] => match setting.read(value) {
            Ok(value) => (Some(Command::Customize(vec![(setting, value)])), None),
            Err(alert) => (None, Some(alert)),
        },
        _ => (None, Some(Alert::Parameter)),
    }
}

impl Command {
    fn apply(&self, voice: &mut Voice) {
        match self {
            Command::PhonemeInput(enable) => voice.phoneme_input = *enable,
            Command::ChangeVoice(def) => voice.take_on(def),
            Command::Customize(settings) => {
                for (setting, value) in settings {
                    (setting.apply)(voice, *value);
                }
            }
            Command::Reset => *voice = Voice::default(),
        }
    }

    /// Whether the speech before this command is spoken differently from the speech
    /// after it, so that it can't be one stretch.
    fn changes_the_sound(&self) -> bool {
        !matches!(self, Command::PhonemeInput(_))
    }
}

/// Where the bracket opened at `open` closes. Another opening bracket first means this
/// one was never closed.
fn closing_bracket(input: &str, open: usize) -> Option<usize> {
    let rest = &input[open + 1..];
    let at = rest.find(['[', ']'])?;
    (rest.as_bytes()[at] == b']').then_some(open + 1 + at)
}

/// Applies the commands in text to the voice without reading it, for starting partway
/// through a document as if the part before had been read.
pub fn follow_commands(input: &str, voice: &mut Voice) {
    let mut search = 0;
    while let Some(open) = input[search..].find("[:").map(|at| at + search) {
        if let Some(close) = closing_bracket(input, open)
            && let (Some(command), _) = read_command(&input[open + 2..close])
        {
            command.apply(voice);
        }
        search = open + 1;
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

    /// Says a mistake aloud, as a sentence of its own.
    fn add_alert(&mut self, alert: Alert, voice: &Voice) {
        self.open = false;
        self.add(text::phrases_with(alert.words(), voice.expand_abbreviations, false), true);
    }
}

/// A stretch of speech, and the voice that speaks it.
pub struct Part {
    pub voice: Voice,
    pub phrases: Vec<Phrase>,
}

/// Reads text with the commands and phonetic words in it, which may change the voice.
/// There is always at least one part, though it may have no phrases.
pub fn parts(input: &str, voice: &mut Voice) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut gathered = Gathered::default();
    // Where the text not yet read starts, and where to look for the next bracket.
    let (mut from, mut search) = (0, 0);
    while let Some(open) = input[search..].find('[').map(|at| at + search) {
        let rest = &input[open + 1..];
        let is_command = rest.starts_with(':');
        // Ordinary text, or a bracket that only leads to the next one.
        if !is_command && (!voice.phoneme_input || rest.starts_with('[')) {
            search = open + 1;
            continue;
        }
        gathered.add_text(&input[from..open], voice);
        let Some(close) = closing_bracket(input, open) else {
            gathered.add_alert(if is_command { Alert::Command } else { Alert::Phoneme }, voice);
            from = open + if is_command { 2 } else { 1 };
            search = from;
            continue;
        };
        if is_command {
            let (command, alert) = read_command(&input[open + 2..close]);
            if let Some(command) = command {
                if command.changes_the_sound() && !gathered.phrases.is_empty() {
                    parts.push(Part { voice: *voice, phrases: std::mem::take(&mut gathered).phrases });
                }
                command.apply(voice);
            }
            if let Some(alert) = alert {
                gathered.add_alert(alert, voice);
            }
        } else {
            let (words, bad) = read_sounds(&input[open + 1..close]);
            if !words.is_empty() {
                gathered.add(vec![Phrase { words, boundary: Boundary::Period }], false);
            }
            if bad.is_some() {
                gathered.add_alert(Alert::Phoneme, voice);
            }
        }
        from = close + 1;
        search = from;
    }
    gathered.add_text(&input[from..], voice);
    if !gathered.phrases.is_empty() || parts.is_empty() {
        parts.push(Part { voice: *voice, phrases: gathered.phrases });
    }
    parts
}

/// All the phrases of text with commands in it, whatever speaks them.
pub fn phrases(input: &str, voice: &mut Voice) -> Vec<Phrase> {
    parts(input, voice).into_iter().flat_map(|part| part.phrases).collect()
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

    /// The voice after the commands in `input`, read from the default voice.
    fn after(input: &str) -> Voice {
        let mut voice = Voice::default();
        parts(input, &mut voice);
        voice
    }

    #[test]
    fn cv_sets_each_setting() {
        let voice = after("[:cv pi 150 in 120 hs 110 rg 30 br 40]");
        assert_eq!((voice.pitch, voice.inflection, voice.head_size), (150.0, 1.2, 110.0));
        assert_eq!((voice.roughness, voice.breathiness), (30.0, 40.0));
        // One at a time, the others stay as they were.
        let voice = after("[:cv br 20]");
        assert_eq!((voice.breathiness, voice.pitch), (20.0, Voice::default().pitch));
    }

    #[test]
    fn cv_stops_at_each_settings_limits() {
        let low = after("[:cv pi 1 in -5 hs 1 rg -5 br -5]");
        assert_eq!(
            (low.pitch, low.inflection, low.head_size, low.roughness, low.breathiness),
            (40.0, 0.0, 64.0, 0.0, 0.0)
        );
        let high = after("[:cv pi 9999 in 9999 hs 9999 rg 9999 br 9999]");
        assert_eq!(
            (high.pitch, high.inflection, high.head_size, high.roughness, high.breathiness),
            (550.0, 2.5, 135.0, 100.0, 72.0)
        );
        // The limits themselves are allowed.
        assert_eq!(after("[:cv hs 64]").head_size, 64.0);
        assert_eq!(after("[:cv hs 135]").head_size, 135.0);
    }

    #[test]
    fn the_single_commands_set_their_settings() {
        let voice = after("[:vibspeed 5.5][:vibdepth 40][:vibdelay 250][:volume 60]");
        assert_eq!(
            (voice.vibrato_speed, voice.vibrato_depth, voice.vibrato_delay, voice.volume),
            (5.5, 40.0, 250.0, 0.6)
        );
        assert_eq!(after("[:speed 400]").rate, after("[:spd 400]").rate);
        assert_eq!(after("[:speed 190]").rate, 1.0);
        assert!(after("[:SPEED 400]").rate > 1.9);
    }

    #[test]
    fn the_single_commands_stop_at_their_limits() {
        let low = after("[:vibspeed -1][:vibdepth -1][:vibdelay -1][:volume -1][:speed 1]");
        assert_eq!(
            (low.vibrato_speed, low.vibrato_depth, low.vibrato_delay, low.volume, low.rate),
            (0.0, 0.0, 0.0, 0.0, 0.25)
        );
        let high = after("[:vibspeed 999][:vibdepth 999][:vibdelay 99999][:volume 999][:speed 99999]");
        assert_eq!(
            (high.vibrato_speed, high.vibrato_depth, high.vibrato_delay, high.volume, high.rate),
            (20.0, 200.0, 5000.0, 2.0, 8.0)
        );
    }

    /// Checks that each of `inputs` says the alert and changes nothing.
    fn says_alert(alert: Alert, inputs: &[&str]) {
        for input in inputs {
            assert_eq!(read(input, false), plain(alert.words()), "{input}");
            assert_eq!(after(input), Voice::default(), "{input}");
        }
    }

    #[test]
    fn a_command_that_doesnt_exist_says_so() {
        says_alert(
            Alert::Command,
            &["[:foo]", "[:vq]", "[:vbb]", "[:v]", "[:]", "[: ]", "[:5]", "[:rate 200]", "[:pi 150]"],
        );
    }

    #[test]
    fn something_a_command_cant_use_says_so() {
        says_alert(
            Alert::Parameter,
            &[
                "[:speed fast]",
                "[:speed 100 200]",
                "[:volume loud]",
                "[:reset now]",
                "[:vb x]",
                "[:cv xx 5]",
                "[:cv pi high]",
                "[:cv pi inf]",
                "[:cv pi NaN]",
                "[:cv rc loud]",
            ],
        );
    }

    #[test]
    fn a_missing_or_impossible_value_says_so() {
        says_alert(
            Alert::Value,
            &[
                "[:speed]",
                "[:volume]",
                "[:vibspeed]",
                "[:voice]",
                "[:voice nobody]",
                "[:cv pi]",
                "[:phone]",
                "[:phone klattara]",
                "[:phone klattara maybe]",
                "[:phone other enable]",
                "[:cv ge 2]",
                "[:cv ge x]",
                "[:cv ge 1.0]",
                "[:cv ge]",
            ],
        );
    }

    #[test]
    fn cv_with_nothing_to_set_does_nothing() {
        assert_eq!(read("[:cv]", false), "");
        assert_eq!(after("[:cv]"), Voice::default());
    }

    #[test]
    fn settings_before_a_mistake_still_count() {
        for (input, alert) in [
            ("[:cv pi 150 xx 5 br 40]", Alert::Parameter),
            ("[:cv pi 150 br high in 50]", Alert::Parameter),
            ("[:cv pi 150 ge 2 br 40]", Alert::Value),
            ("[:cv pi 150 br]", Alert::Value),
        ] {
            let voice = after(input);
            assert_eq!((voice.pitch, voice.breathiness, voice.inflection), (150.0, 0.0, 1.0), "{input}");
            assert_eq!(read(input, false), plain(alert.words()), "{input}");
        }
    }

    #[test]
    fn an_alert_is_a_sentence_of_its_own_in_the_voice_of_the_moment() {
        let mut voice = Voice::default();
        let spoken = phrases("One [:foo] two.", &mut voice);
        assert_eq!(spoken.len(), 3);
        assert_eq!(notation(&spoken[1..2]), plain(Alert::Command.words()));
        // After a good setting, the alert comes in the new voice.
        let mut voice = Voice::default();
        let parts = parts("One. [:cv pi 300 xx 5] Two.", &mut voice);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1].voice.pitch, 300.0);
        assert_eq!(parts[1].phrases.len(), 2);
    }

    #[test]
    fn a_bad_phoneme_says_so_after_the_good_sounds_before_it() {
        for input in ["[QQ]", "[1]", "[note]", "[AA1<400,551>]", "[AA1<400]", "[AA1<x>]", "[_]", "[_<,20>]"] {
            assert_eq!(read(input, true), plain(Alert::Phoneme.words()), "{input}");
        }
        // The sounds before the mistake are spoken, and the rest of the bracket isn't.
        let expected = format!("[HH AH0]. {}", plain(Alert::Phoneme.words()));
        assert_eq!(read("[HH AH0 QQ L OW1]", true), expected);
        // The text goes on after it.
        assert_eq!(phrases("Say [QQ] now.", &mut Voice { phoneme_input: true, ..Voice::default() }).len(), 3);
    }

    #[test]
    fn empty_brackets_and_doubled_ones_are_no_mistake() {
        for input in ["[]", "[  ]"] {
            assert_eq!(read(input, true), "", "{input}");
        }
        assert_eq!(read("[[HH AY1]", true), "[HH AY1].");
    }

    #[test]
    fn with_phoneme_input_off_only_commands_can_be_mistakes() {
        for input in ["See [1] for more.", "A [note] here.", "[QQ]", "[]"] {
            assert_eq!(read(input, false), plain(input), "{input}");
        }
    }

    #[test]
    fn a_missing_closing_bracket_says_so_and_goes_on_reading() {
        // Nothing is lost: the words after it are read as text.
        let input = "[:phone klattara enable Hello there.";
        assert_eq!(
            read(input, false),
            format!("{} {}", plain(Alert::Command.words()), plain("phone klattara enable Hello there."))
        );
        assert!(!after(input).phoneme_input);
        assert_eq!(read("[HH AY1", true), format!("{} {}", plain(Alert::Phoneme.words()), plain("HH AY1")));
        // Another opening bracket first means this one was never closed, and doesn't
        // hide the command that follows it.
        let voice = after("[:foo [:vb] Hello.");
        assert_eq!(voice.pitch, 210.0);
    }

    #[test]
    fn a_slice_in_program_text_is_a_mistake_as_in_dectalk() {
        let mut voice = Voice::default();
        assert_eq!(phrases("a[:5] is a slice.", &mut voice).len(), 3);
    }

    #[test]
    fn a_space_before_the_colon_is_no_command() {
        // The NVDA add-on removes the opening of a command, which has to be enough.
        assert_eq!(read("[ :vb] Hello.", false), plain("[ :vb] Hello."));
        assert_eq!(after("[ :vb] Hello."), Voice::default());
    }

    #[test]
    fn richness_sets_the_open_quotient_across_its_whole_range() {
        let quotient = |input: &str| after(input).open_quotient;
        assert!((quotient("[:cv rc 0]") - 0.9).abs() < 1e-9);
        assert!((quotient("[:cv rc 100]") - 0.2).abs() < 1e-9);
        // Halfway is Steve's own.
        assert!((quotient("[:cv rc 50]") - Voice::default().open_quotient).abs() < 1e-9);
        assert!(quotient("[:cv rc 75]") < quotient("[:cv rc 25]"));
        assert_eq!(quotient("[:cv rc -5]"), quotient("[:cv rc 0]"));
        assert_eq!(quotient("[:cv rc 500]"), quotient("[:cv rc 100]"));
    }

    #[test]
    fn smoothness_sets_the_tilt() {
        assert_eq!(after("[:cv sm 0]").tilt, 0.0);
        assert_eq!(after("[:cv sm 40]").tilt, 10.0);
        assert_eq!(after("[:cv sm 100]").tilt, 25.0);
        assert_eq!(after("[:cv sm 500]").tilt, 25.0);
        assert_eq!(after("[:cv sm -5]").tilt, 0.0);
    }

    #[test]
    fn gender_takes_only_its_four_values() {
        for female in ["0", "f", "F"] {
            assert_eq!(after(&format!("[:cv ge {female}]")).gender, Gender::Female, "{female}");
        }
        for male in ["1", "m", "M"] {
            let voice = after(&format!("[:vb][:cv ge {male}]"));
            assert_eq!(voice.gender, Gender::Male, "{male}");
        }
        // It changes nothing else.
        assert_eq!(after("[:cv ge f]"), Voice { gender: Gender::Female, ..Voice::default() });
    }

    #[test]
    fn upper_formants_are_set_and_stop_at_their_limits() {
        let voice = after("[:cv f4 3600 f5 4200 f6 6000 b4 300 b5 400 b6 1500]");
        assert_eq!(voice.upper_formants, [Some(3600.0), Some(4200.0), Some(6000.0)]);
        assert_eq!(voice.upper_bandwidths, [Some(300.0), Some(400.0), Some(1500.0)]);
        let low = after("[:cv f4 1 f5 1 f6 1 b4 1 b5 1 b6 1]");
        assert_eq!(low.upper_formants, [Some(2000.0), Some(2500.0), Some(3000.0)]);
        assert_eq!(low.upper_bandwidths, [Some(100.0); 3]);
        let high = after("[:cv f4 99999 f5 99999 f6 99999 b4 99999 b5 99999 b6 99999]");
        assert_eq!(high.upper_formants, [Some(6000.0), Some(6000.0), Some(9000.0)]);
        assert_eq!(high.upper_bandwidths, [Some(4000.0); 3]);
        // They start unset, so that the gender's model decides.
        assert_eq!(Voice::default().upper_formants, [None; 3]);
        assert_eq!(Voice::default().upper_bandwidths, [None; 3]);
    }

    #[test]
    fn pitch_shape_lax_breath_and_open_quotient_are_set_and_stop_at_their_limits() {
        use crate::voicedefs::PitchShape;
        let voice = after("[:cv fa 9 ha 20 st 22 qk 10 lx 50 oq 70]");
        let shape = PitchShape { baseline_fall: 9.0, hat_rise: 20.0, stress_rise: 22.0, quickness: 10.0 };
        assert_eq!(voice.pitch_shape, shape);
        assert_eq!((voice.lax_breathiness, voice.open_quotient), (50.0, 0.7));
        let high = after("[:cv fa 999 ha 999 st 999 qk 999 lx 999 oq 999]");
        let top = PitchShape { baseline_fall: 90.0, hat_rise: 100.0, stress_rise: 100.0, quickness: 100.0 };
        assert_eq!(high.pitch_shape, top);
        assert_eq!((high.lax_breathiness, high.open_quotient), (100.0, 0.9));
        let low = after("[:CV FA 0 HA 0 ST 0 QK 0 LX 0 OQ 1]");
        let none = PitchShape { baseline_fall: 0.0, hat_rise: 0.0, stress_rise: 0.0, quickness: 0.0 };
        assert_eq!(low.pitch_shape, none);
        assert_eq!((low.lax_breathiness, low.open_quotient), (0.0, 0.2));
        // Steve's own values, as numbers you can type.
        assert_eq!(after("[:cv fa 18 ha 18 st 32 qk 40 lx 0 oq 55]"), Voice::default());
        for bad in ["[:cv fa low]", "[:cv ha low]", "[:cv st low]", "[:cv qk low]", "[:cv lx low]", "[:cv oq low]"] {
            assert_eq!(read(bad, false), plain(Alert::Parameter.words()), "{bad}");
        }
        assert_eq!(read("[:cv qk]", false), plain(Alert::Value.words()));
        // A voice brings its own, and a reset the first voice's.
        assert_eq!(after("[:cv fa 50 lx 50 oq 30][:vs]"), Voice::default());
        assert_eq!(after("[:cv fa 50 lx 50 oq 30][:reset]"), Voice::default());
        assert_eq!(after("[:vd]").pitch_shape.stress_rise, 20.0);
        // The last of rc and oq counts, since they set the same thing.
        assert_eq!(after("[:cv rc 50 oq 70]").open_quotient, 0.7);
        assert!((after("[:cv oq 70 rc 50]").open_quotient - 0.55).abs() < 1e-9);
    }

    #[test]
    fn gains_are_set_and_stop_at_their_limits() {
        use crate::voicedefs::SourceGains;
        assert_eq!(Voice::default().gains, SourceGains::NORMAL);
        let gains = after("[:cv vvl 150 fvl 50 avl 0 nvl 200]").gains;
        assert_eq!(gains, SourceGains { voicing: 1.5, frication: 0.5, aspiration: 0.0, nasal: 2.0 });
        assert_eq!(after("[:CV VVL 25]").gains.voicing, 0.25);
        let high = after("[:cv vvl 999 fvl 999 avl 999 nvl 999]").gains;
        assert_eq!(high, SourceGains { voicing: 2.0, frication: 2.0, aspiration: 2.0, nasal: 2.0 });
        for bad in ["[:cv vvl loud]", "[:cv fvl loud]", "[:cv avl loud]", "[:cv nvl loud]", "[:cv gvl 50]"] {
            assert_eq!(read(bad, false), plain(Alert::Parameter.words()), "{bad}");
        }
        assert_eq!(read("[:cv fvl]", false), plain(Alert::Value.words()));
        // A new voice or a reset brings them back.
        assert_eq!(after("[:cv vvl 50 fvl 50 avl 50 nvl 50][:vs]"), Voice::default());
        assert_eq!(after("[:cv vvl 50 fvl 50 avl 50 nvl 50][:reset]"), Voice::default());
    }

    #[test]
    fn flutter_is_set_and_stops_at_its_limits() {
        assert_eq!(Voice::default().flutter, 25.0);
        assert_eq!(after("[:cv fl 60]").flutter, 60.0);
        assert_eq!(after("[:CV FL 0]").flutter, 0.0);
        assert_eq!(after("[:cv fl 999]").flutter, 100.0);
        assert_eq!(read("[:cv fl shaky]", false), plain(Alert::Parameter.words()));
        assert_eq!(read("[:cv fl]", false), plain(Alert::Value.words()));
        // A voice brings its own.
        assert_eq!(after("[:cv fl 90][:vd]").flutter, 60.0);
        assert_eq!(after("[:vd][:cv fl 90][:vs]"), Voice::default());
        assert_eq!(after("[:cv fl 90][:reset]"), Voice::default());
    }

    #[test]
    fn bad_upper_formants_say_so() {
        for input in ["[:cv f4 high]", "[:cv b5 wide]"] {
            assert_eq!(read(input, false), plain(Alert::Parameter.words()), "{input}");
        }
        assert_eq!(read("[:cv f4]", false), plain(Alert::Value.words()));
        assert_eq!(read("[:cv f7 3000]", false), plain(Alert::Parameter.words()));
        assert_eq!(after("[:cv f4 high]"), Voice::default());
    }

    #[test]
    fn a_new_voice_or_a_reset_clears_the_upper_formants() {
        assert_eq!(after("[:cv f4 3600 b5 400][:vs]"), Voice::default());
        assert_eq!(after("[:cv f4 3600 b5 400][:reset]"), Voice::default());
        // A change of gender keeps them.
        assert_eq!(after("[:cv f4 3600][:cv ge f]").upper_formants[0], Some(3600.0));
    }

    #[test]
    fn a_new_voice_replaces_richness_smoothness_and_gender() {
        assert_eq!(after("[:cv rc 90 sm 90 ge f][:vs]"), Voice::default());
    }

    #[test]
    fn head_size_starts_at_100() {
        assert_eq!(Voice::default().head_size, 100.0);
        assert_eq!(Voice::from(find_voice("Brittany").unwrap()).head_size, 100.0);
    }

    #[test]
    fn commands_ignore_case_and_extra_spaces() {
        assert_eq!(after("[:CV  PI\t150 ]").pitch, 150.0);
        assert_eq!(after("[:VB]").pitch, 210.0);
        assert_eq!(after("[:VOICE  brittany ]").pitch, 210.0);
    }

    #[test]
    fn voices_change_by_letter_or_by_name() {
        for command in ["[:vb]", "[:voice Brittany]", "[:voice brittany]"] {
            let voice = after(command);
            assert_eq!((voice.pitch, voice.gender), (210.0, crate::voicedefs::Gender::Female), "{command}");
        }
        for command in ["[:vb][:vs]", "[:vb][:voice Steve]", "[:vg][:vs]", "[:vg][:voice steve]"] {
            assert_eq!(after(command), Voice::default(), "{command}");
        }
        let gary = Voice::from(find_voice("Gary").unwrap());
        for command in ["[:vg]", "[:VG]", "[:voice Gary]", "[:voice gary]"] {
            assert_eq!(after(command), gary, "{command}");
        }
        assert_eq!(gary.upper_formants, [Some(3400.0), Some(3650.0), Some(5600.0)]);
        let david = Voice::from(find_voice("David").unwrap());
        for command in ["[:vd]", "[:VD]", "[:voice David]", "[:voice david]"] {
            assert_eq!(after(command), david, "{command}");
        }
        assert_eq!(after("[:vd][:vs]"), Voice::default());
        let taylor = Voice::from(find_voice("Taylor").unwrap());
        for command in ["[:vt]", "[:VT]", "[:voice Taylor]", "[:voice taylor]"] {
            assert_eq!(after(command), taylor, "{command}");
        }
        assert_eq!(after("[:vt][:vs]"), Voice::default());
    }

    #[test]
    fn a_new_voice_replaces_the_customizing_and_keeps_the_rest() {
        let mut voice = Voice { rate: 2.0, volume: 0.5, phoneme_input: true, ..Voice::default() };
        parts("[:cv pi 300 hs 120 rg 50 br 40 in 200][:vs]", &mut voice);
        assert_eq!(voice, Voice { rate: 2.0, volume: 0.5, phoneme_input: true, ..Voice::default() });
    }

    #[test]
    fn a_change_of_voice_applies_from_there_on() {
        let mut voice = Voice::default();
        let spoken = parts("One. [:vb] Two. [:cv pi 300] Three.", &mut voice);
        let pitches: Vec<f64> = spoken.iter().map(|part| part.voice.pitch).collect();
        assert_eq!(pitches, [110.0, 210.0, 300.0]);
        assert!(spoken.iter().all(|part| part.phrases.len() == 1));
        // In mid-sentence too.
        let mut voice = Voice::default();
        assert_eq!(parts("One two [:vb] three four.", &mut voice).len(), 2);
    }

    #[test]
    fn commands_with_nothing_to_say_make_no_empty_parts() {
        for input in ["[:vb] Hello.", "[:cv pi 150] Hello.", "[:vb][:cv pi 150] Hello.", "Hello. [:vb]"] {
            assert_eq!(parts(input, &mut Voice::default()).len(), 1, "{input}");
        }
        // A command alone leaves one part with nothing in it.
        let only = parts("[:vb]", &mut Voice::default());
        assert!(only.len() == 1 && only[0].phrases.is_empty());
        assert_eq!(only[0].voice.pitch, 210.0);
    }

    #[test]
    fn phoneme_input_doesnt_break_the_speech() {
        assert_eq!(parts("Say [:phone klattara enable] it now.", &mut Voice::default()).len(), 1);
    }

    #[test]
    fn following_commands_applies_every_kind() {
        let mut voice = Voice::default();
        follow_commands("Some text [:vb] and [:cv br 50 hs 90] more [:phone klattara enable] text.", &mut voice);
        assert_eq!((voice.pitch, voice.breathiness, voice.head_size, voice.phoneme_input), (210.0, 50.0, 90.0, true));
    }

    #[test]
    fn commands_can_be_followed_without_reading() {
        let follow = |input: &str, start: bool| {
            let mut voice = Voice { phoneme_input: start, ..Voice::default() };
            follow_commands(input, &mut voice);
            voice.phoneme_input
        };
        assert!(follow("Some text [:phone klattara enable] more.", false));
        assert!(!follow("[:phone klattara enable] text [:phone Klattara disable]", true));
        assert!(follow("No commands [1] here.", true));
        assert!(!follow("No commands [1] here.", false));
        assert!(!follow("[:phone klattara enable", false));
        // As in reading: a bracket that isn't a command doesn't hide one after it.
        assert!(follow("[:rate [:phone klattara enable]", false));
    }

    #[test]
    fn text_without_commands_is_read_as_ever() {
        for input in ["Hello there.", "See [1] for more.", "[HH AH0 L OW1] and [unclosed", ""] {
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
    fn a_bracket_that_isnt_a_command_doesnt_hide_one_after_it() {
        let mut voice = Voice::default();
        phrases("[:rate [:phone klattara enable]", &mut voice);
        assert!(voice.phoneme_input);
    }
}
