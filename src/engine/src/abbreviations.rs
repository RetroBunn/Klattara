//! Abbreviations, read by context.
//!
//! Each abbreviation is decided by its neighbours: "Dr." before a name is
//! "doctor" and after a street name "drive"; "St." likewise "saint" or
//! "street"; "Ft." is "fort" before a name and "feet" after a number; "No."
//! is "number" only before a digit. Units are read only after numbers, and
//! the rarer titles only with their period, so ordinary words are never
//! mistaken for abbreviations.
//!
//! A period after an abbreviation ends the sentence only when the next word
//! looks like the start of a new one ("...on Main St. It was late."), never
//! after a title, which always comes before a name.
//!
//! US states are read in full where they are clearly states: "Calif." and
//! the like anywhere, but "Mo.", "Pa." or "Miss.", which are also words or
//! other abbreviations, only in an address, after a city and a comma or
//! before a ZIP code; and postal codes such as "TX" only before a ZIP code.
//!
//! Roman numerals are read as numbers: after a monarch's or pope's name as
//! an ordinal ("Richard III" is "Richard the third"), and otherwise as a
//! count ("World War II" is "World War two", "Final Fantasy VII" "seven").
//! Like ZIP codes, they are read so whether or not abbreviations are
//! expanded.
//!
//! Expansion can be turned off. Every abbreviation is then read as written:
//! spelled if it has no vowel ("Dr." as "D R"), otherwise said as a word
//! ("Inc."), with the same sentence breaks.

use crate::text::{FUNCTION_WORDS, ONES, Token};
use crate::utterance::Boundary;

/// How an expanded abbreviation relates to the sentence around it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Comes before what it describes (a title, or "number" before a digit),
    /// so its period never ends a sentence.
    Before,
    /// May end a sentence.
    Other,
    /// Usually closes a list or sentence ("etc."), so any capitalized word
    /// after its period starts a new sentence.
    Closing,
}

/// What surrounds a word: the word before it (skipping an abbreviation's
/// period), whether a period follows it, the token after that, and whether
/// it follows a city and a comma, as a state does: "Springfield, Mo.".
struct Context<'a> {
    word: &'a str,
    period: bool,
    before: Option<&'a Token>,
    after: Option<&'a Token>,
    after_city: bool,
    /// The word before is capitalized inside a sentence, so most likely a
    /// name: "on Elm Dr.", but not "Ask Dr." or "Visit St.".
    name_before: bool,
}

fn capitalized(token: Option<&Token>) -> bool {
    matches!(token, Some(Token::Word(w)) if w.chars().next().is_some_and(char::is_uppercase))
}

fn number(token: Option<&Token>) -> Option<&str> {
    match token {
        Some(Token::Number { digits, .. }) => Some(digits),
        _ => None,
    }
}

impl Context<'_> {
    /// Written like a name: "Dr", not "dr" or "DR".
    fn title_case(&self) -> bool {
        let mut chars = self.word.chars();
        chars.next().is_some_and(char::is_uppercase) && chars.all(char::is_lowercase)
    }

    fn before_name(&self) -> bool {
        capitalized(self.after)
    }

    fn after_name(&self) -> bool {
        capitalized(self.before)
    }

    /// After a title, as "St." in "Dr. St. Jermain" is.
    fn after_title(&self) -> bool {
        matches!(self.before, Some(Token::Word(w)) if TITLES.contains(&w.to_lowercase().as_str()))
    }

    fn after_number(&self) -> bool {
        number(self.before).is_some()
    }

    fn before_number(&self) -> bool {
        number(self.after).is_some()
    }

    /// Before a Roman numeral in capitals: "vol IV".
    fn before_roman(&self) -> bool {
        matches!(self.after, Some(Token::Word(w)) if w.chars().all(|c| c.is_ascii_uppercase()) && roman_value(w).is_some())
    }

    /// Inside a sentence, after a word: "Shipyard No 1", but not "No 2
    /// people are alike".
    fn after_word(&self) -> bool {
        matches!(self.before, Some(Token::Word(_)))
    }

    fn before_zip(&self) -> bool {
        number(self.after).is_some_and(|digits| digits.len() == 5)
    }

    /// A title, as in "Mr. Smith" or "Mr Smith".
    fn title(&self, expansion: &'static str) -> Option<(Vec<&'static str>, Kind)> {
        ((self.period || self.title_case()) && self.before_name()).then(|| (vec![expansion], Kind::Before))
    }

    /// A rarer title, only with its period: "Gen. Grant", "Rev. King".
    fn formal_title(&self, expansion: &'static str) -> Option<(Vec<&'static str>, Kind)> {
        (self.period && self.before_name()).then(|| (vec![expansion], Kind::Before))
    }

    /// A unit after a number: singular after exactly one.
    fn unit(&self, singular: &'static str, plural: &'static str) -> Option<(Vec<&'static str>, Kind)> {
        let one = number(self.before) == Some("1");
        self.after_number().then(|| (vec![if one { singular } else { plural }], Kind::Other))
    }

    /// A word that follows a name or a number: "Main St.", "Smith Jr.".
    fn after(&self, expansion: &'static str) -> Option<(Vec<&'static str>, Kind)> {
        (self.after_name() || self.after_number()).then(|| (vec![expansion], Kind::Other))
    }

    /// Only with its period: "etc.", "approx.".
    fn with_period(&self, expansion: &'static [&'static str]) -> Option<(Vec<&'static str>, Kind)> {
        self.period.then(|| (expansion.to_vec(), Kind::Other))
    }

    /// Before a number, with its period: "No. 5", "Vol. 2".
    fn before_a_number(&self, expansion: &'static str) -> Option<(Vec<&'static str>, Kind)> {
        (self.period && self.before_number()).then(|| (vec![expansion], Kind::Before))
    }

    /// A month, as in "Jan. 5" or "Jan 5, 2024".
    fn month(&self, expansion: &'static str) -> Option<(Vec<&'static str>, Kind)> {
        if !self.title_case() {
            return None;
        }
        if self.before_number() {
            Some((vec![expansion], Kind::Before))
        } else {
            self.period.then(|| (vec![expansion], Kind::Other))
        }
    }

    /// A weekday, only with its period: "Sat." but not the verb "sat".
    fn weekday(&self, expansion: &'static str) -> Option<(Vec<&'static str>, Kind)> {
        (self.title_case() && self.period).then(|| (vec![expansion], Kind::Other))
    }
}

/// US states by their traditional abbreviations.
const STATES: &[(&str, &str)] = &[
    ("ala", "alabama"),
    ("ariz", "arizona"),
    ("ark", "arkansas"),
    ("calif", "california"),
    ("colo", "colorado"),
    ("conn", "connecticut"),
    ("del", "delaware"),
    ("fla", "florida"),
    ("ga", "georgia"),
    ("ill", "illinois"),
    ("ind", "indiana"),
    ("kan", "kansas"),
    ("kans", "kansas"),
    ("ky", "kentucky"),
    ("la", "louisiana"),
    ("mass", "massachusetts"),
    ("md", "maryland"),
    ("mich", "michigan"),
    ("minn", "minnesota"),
    ("miss", "mississippi"),
    ("mo", "missouri"),
    ("mont", "montana"),
    ("neb", "nebraska"),
    ("nebr", "nebraska"),
    ("nev", "nevada"),
    ("okla", "oklahoma"),
    ("ore", "oregon"),
    ("oreg", "oregon"),
    ("pa", "pennsylvania"),
    ("penn", "pennsylvania"),
    ("penna", "pennsylvania"),
    ("tenn", "tennessee"),
    ("tex", "texas"),
    ("va", "virginia"),
    ("vt", "vermont"),
    ("wash", "washington"),
    ("wis", "wisconsin"),
    ("wisc", "wisconsin"),
    ("wyo", "wyoming"),
];

/// State abbreviations that are also words, names or other abbreviations,
/// read as states only in an address: "Mo." may be Monday, "Miss." a title.
const AMBIGUOUS_STATES: &[&str] =
    &["ark", "del", "ill", "ind", "la", "mass", "miss", "mo", "ore", "pa", "penn", "wash"];

/// Postal codes, read as states only before a ZIP code: "OK", "OR", "IN" and
/// "ME" are also words.
const POSTAL_CODES: &[(&str, &str)] = &[
    ("ak", "alaska"),
    ("al", "alabama"),
    ("ar", "arkansas"),
    ("az", "arizona"),
    ("ca", "california"),
    ("co", "colorado"),
    ("ct", "connecticut"),
    ("de", "delaware"),
    ("fl", "florida"),
    ("ga", "georgia"),
    ("hi", "hawaii"),
    ("ia", "iowa"),
    ("id", "idaho"),
    ("il", "illinois"),
    ("in", "indiana"),
    ("ks", "kansas"),
    ("ky", "kentucky"),
    ("la", "louisiana"),
    ("ma", "massachusetts"),
    ("md", "maryland"),
    ("me", "maine"),
    ("mi", "michigan"),
    ("mn", "minnesota"),
    ("mo", "missouri"),
    ("ms", "mississippi"),
    ("mt", "montana"),
    ("nc", "north carolina"),
    ("nd", "north dakota"),
    ("ne", "nebraska"),
    ("nh", "new hampshire"),
    ("nj", "new jersey"),
    ("nm", "new mexico"),
    ("nv", "nevada"),
    ("ny", "new york"),
    ("oh", "ohio"),
    ("ok", "oklahoma"),
    ("or", "oregon"),
    ("pa", "pennsylvania"),
    ("ri", "rhode island"),
    ("sc", "south carolina"),
    ("sd", "south dakota"),
    ("tn", "tennessee"),
    ("tx", "texas"),
    ("ut", "utah"),
    ("va", "virginia"),
    ("vt", "vermont"),
    ("wa", "washington"),
    ("wi", "wisconsin"),
    ("wv", "west virginia"),
    ("wy", "wyoming"),
];

/// States written with periods between letters, read in full only in an
/// address: "Albany, N.Y.". "D.C." stays spelled, as it is said.
const DOTTED_STATES: &[(&str, &str)] = &[
    ("nc", "north carolina"),
    ("nd", "north dakota"),
    ("nh", "new hampshire"),
    ("nj", "new jersey"),
    ("nm", "new mexico"),
    ("ny", "new york"),
    ("ri", "rhode island"),
    ("sc", "south carolina"),
    ("sd", "south dakota"),
    ("wva", "west virginia"),
];

fn find(table: &[(&str, &'static str)], key: &str) -> Option<&'static str> {
    table.iter().find(|entry| entry.0 == key).map(|entry| entry.1)
}

/// A state, if the word is one here.
fn state(c: &Context) -> Option<(Vec<&'static str>, Kind)> {
    let lower = c.word.to_lowercase();
    // Between a city and a ZIP code: "Austin, TX 78701", "Austin TX 78701".
    let before_zip = c.before_zip() && (c.after_city || c.after_name());
    let postal = c.word.len() == 2 && c.word.chars().all(|ch| ch.is_ascii_uppercase());
    if postal {
        return find(POSTAL_CODES, &lower).filter(|_| before_zip).map(|name| (vec![name], Kind::Other));
    }
    let name = find(STATES, &lower)?;
    let in_address = (c.after_city && c.period) || before_zip;
    let clear = c.period && !AMBIGUOUS_STATES.contains(&lower.as_str());
    (c.title_case() && (clear || in_address)).then(|| (vec![name], Kind::Other))
}

/// The words an abbreviation stands for here, if it is one.
fn expand(c: &Context) -> Option<(Vec<&'static str>, Kind)> {
    expand_word(c).or_else(|| state(c))
}

fn expand_word(c: &Context) -> Option<(Vec<&'static str>, Kind)> {
    match c.word.to_lowercase().as_str() {
        // Read by their neighbours.
        // Between a name and a capitalized word, "Dr." ends a street name and
        // its period a sentence: "on Elm Dr. Open the door".
        // Without its period or a capital, only before a name: "dr Henry".
        "dr" if c.period || c.title_case() || c.before_name() => Some(if c.before_name() && !c.name_before {
            (vec!["doctor"], Kind::Before)
        } else if c.after_name() || c.after_number() {
            (vec!["drive"], if c.before_name() { Kind::Closing } else { Kind::Other })
        } else {
            (vec!["doctor"], Kind::Other)
        }),
        // "Main St. Louis" is a street before a name; "Dr. St. Jermain" a
        // saint after a title.
        "st" if c.period || c.title_case() || c.before_name() => {
            Some(if c.before_name() && (!c.name_before || c.after_title()) {
                (vec!["saint"], Kind::Before)
            } else {
                (vec!["street"], if c.before_name() { Kind::Closing } else { Kind::Other })
            })
        }
        "ft" => c.unit("foot", "feet").or_else(|| if c.period && c.before_name() { c.title("fort") } else { None }),
        "mt" if c.period || c.title_case() => {
            Some((vec!["mount"], if c.before_name() { Kind::Before } else { Kind::Other }))
        }
        // Titles.
        "mr" => c.title("mister"),
        "mrs" => c.title("missus"),
        "ms" => c.title("miz"),
        "prof" => c.title("professor"),
        "gov" => c.formal_title("governor"),
        "sen" => c.formal_title("senator"),
        "rep" => c.formal_title("representative"),
        "gen" => c.formal_title("general"),
        "lt" => c.title("lieutenant"),
        "col" => c.formal_title("colonel"),
        "capt" => c.formal_title("captain"),
        "sgt" => c.formal_title("sergeant"),
        "rev" => c.formal_title("reverend"),
        "hon" => c.formal_title("honorable"),
        "pres" => c.formal_title("president"),
        "fr" => c.formal_title("father"),
        // After names.
        "jr" if c.period || c.after_name() => Some((vec!["junior"], Kind::Other)),
        "sr" if c.period || c.after_name() => Some((vec!["senior"], Kind::Other)),
        // Streets and addresses.
        "ave" => c.after("avenue"),
        "blvd" => c.after("boulevard"),
        "rd" => c.after("road"),
        "ln" => c.after("lane"),
        "hwy" => c.after("highway"),
        "pkwy" => c.after("parkway"),
        "apt" => c.before_a_number("apartment"),
        "ste" => c.before_a_number("suite"),
        // Companies.
        "inc" if c.period || c.after_name() => Some((vec!["incorporated"], Kind::Other)),
        "ltd" if c.period || c.after_name() => Some((vec!["limited"], Kind::Other)),
        "corp" if c.period || c.after_name() => Some((vec!["corporation"], Kind::Other)),
        "co" if c.period && c.after_name() => Some((vec!["company"], Kind::Other)),
        "dept" => c.with_period(&["department"]),
        // Common Latin and general abbreviations.
        "etc" => Some((vec!["et cetera"], Kind::Closing)),
        "vs" => Some((vec!["versus"], Kind::Before)),
        "approx" => c.with_period(&["approximately"]),
        // Before numbers. "No" without its period only inside a sentence;
        // "vol", "pp" and "pg" are common without theirs, but "PG 13" is a
        // film rating.
        "no" if c.period || ((c.title_case() || capitalized(c.before)) && c.after_word()) => {
            c.before_number().then(|| (vec!["number"], Kind::Before))
        }
        "vol" if c.before_number() || c.before_roman() => Some((vec!["volume"], Kind::Before)),
        "vol" | "vols" if c.after_number() => c.unit("volume", "volumes"),
        "ch" => c.before_a_number("chapter"),
        "fig" => c.before_a_number("figure"),
        "p" => c.before_a_number("page"),
        "pg" if c.word != "PG" && c.before_number() => Some((vec!["page"], Kind::Before)),
        "pp" if c.before_number() => Some((vec!["pages"], Kind::Before)),
        "pp" if c.after_number() => Some((vec!["pages"], Kind::Other)),
        // Editors, in a book's details.
        "eds" if c.period || c.word == "eds" => Some((vec!["editors"], Kind::Other)),
        "ext" => c.before_a_number("extension"),
        // Units, after numbers.
        "lb" | "lbs" => c.unit("pound", "pounds"),
        "oz" => c.unit("ounce", "ounces"),
        "mi" => c.unit("mile", "miles"),
        "km" => c.unit("kilometer", "kilometers"),
        "kg" => c.unit("kilogram", "kilograms"),
        "cm" => c.unit("centimeter", "centimeters"),
        "mm" => c.unit("millimeter", "millimeters"),
        "mph" => c.unit("mile per hour", "miles per hour"),
        "hr" | "hrs" => c.unit("hour", "hours"),
        "min" | "mins" => c.unit("minute", "minutes"),
        "sec" | "secs" => c.unit("second", "seconds"),
        // Months and weekdays.
        "jan" => c.month("january"),
        "feb" => c.month("february"),
        "mar" => c.month("march"),
        "apr" => c.month("april"),
        "jun" => c.month("june"),
        "jul" => c.month("july"),
        "aug" => c.month("august"),
        "sep" | "sept" => c.month("september"),
        "oct" => c.month("october"),
        "nov" => c.month("november"),
        "dec" => c.month("december"),
        "mon" => c.weekday("monday"),
        "tue" | "tues" => c.weekday("tuesday"),
        "wed" => c.weekday("wednesday"),
        "thu" | "thur" | "thurs" => c.weekday("thursday"),
        "fri" => c.weekday("friday"),
        "sat" => c.weekday("saturday"),
        "sun" => c.weekday("sunday"),
        _ => None,
    }
}

/// Dotted abbreviations: "e.g." and "i.e." are read out, and states in an
/// address; others, such as "U.S." and "a.m.", are spelled.
fn expand_dotted(letters: &str, in_address: bool) -> Token {
    match letters.to_lowercase().as_str() {
        "eg" => Token::Expanded(vec!["for example"]),
        "ie" => Token::Expanded(vec!["that is"]),
        lower => match find(DOTTED_STATES, lower).filter(|_| in_address) {
            Some(name) => Token::Expanded(vec![name]),
            None => Token::Spelled(letters.to_string()),
        },
    }
}

/// Words the dictionary itself reads in full ("dr" as "drive", "etc" as "et
/// cetera"); with expansion off they are read as written wherever they are.
const DICTIONARY_EXPANSIONS: &[&str] = &[
    "ave", "aug", "blvd", "dr", "etc", "feb", "jr", "lb", "lbs", "ln", "ltd", "mr", "mrs", "ms", "mt", "sgt", "sr",
    "st", "thur",
];

/// An abbreviation read as written: spelled if it has no vowel ("Dr",
/// "lbs"), otherwise said as a word ("Inc", "Jan").
fn as_written(word: &str) -> Token {
    if word.chars().any(|c| "aeiouAEIOU".contains(c)) {
        Token::Literal(word.to_string())
    } else {
        Token::Spelled(word.to_string())
    }
}

/// Units of measure read after a number, with their singular and plural
/// names. They are matched as written, since case matters ("MB" is
/// megabytes, "mb" millibars), with a lowercase match for longer ones.
const UNITS: &[(&str, &str, &str)] = &[
    ("m", "meter", "meters"),
    ("km", "kilometer", "kilometers"),
    ("cm", "centimeter", "centimeters"),
    ("mm", "millimeter", "millimeters"),
    ("nm", "nanometer", "nanometers"),
    ("\u{b5}m", "micrometer", "micrometers"),
    ("\u{3bc}m", "micrometer", "micrometers"),
    ("mi", "mile", "miles"),
    ("ft", "foot", "feet"),
    ("yd", "yard", "yards"),
    ("ha", "hectare", "hectares"),
    ("kg", "kilogram", "kilograms"),
    ("g", "gram", "grams"),
    ("mg", "milligram", "milligrams"),
    ("lb", "pound", "pounds"),
    ("lbs", "pound", "pounds"),
    ("oz", "ounce", "ounces"),
    ("L", "liter", "liters"),
    ("mL", "milliliter", "milliliters"),
    ("ml", "milliliter", "milliliters"),
    ("W", "watt", "watts"),
    ("kW", "kilowatt", "kilowatts"),
    ("MW", "megawatt", "megawatts"),
    ("GW", "gigawatt", "gigawatts"),
    ("kWh", "kilowatt hour", "kilowatt hours"),
    ("V", "volt", "volts"),
    ("kV", "kilovolt", "kilovolts"),
    ("hp", "horsepower", "horsepower"),
    ("Hz", "hertz", "hertz"),
    ("kHz", "kilohertz", "kilohertz"),
    ("MHz", "megahertz", "megahertz"),
    ("GHz", "gigahertz", "gigahertz"),
    ("KB", "kilobyte", "kilobytes"),
    ("kB", "kilobyte", "kilobytes"),
    ("MB", "megabyte", "megabytes"),
    ("GB", "gigabyte", "gigabytes"),
    ("TB", "terabyte", "terabytes"),
    ("mph", "mile per hour", "miles per hour"),
    ("s", "second", "seconds"),
    ("sec", "second", "seconds"),
    ("secs", "second", "seconds"),
    ("ms", "millisecond", "milliseconds"),
    ("h", "hour", "hours"),
    ("hr", "hour", "hours"),
    ("hrs", "hour", "hours"),
    ("min", "minute", "minutes"),
    ("mins", "minute", "minutes"),
];

/// The singular and plural names of a unit of measure.
pub fn unit_name(written: &str) -> Option<(&'static str, &'static str)> {
    let exact = UNITS.iter().find(|(a, _, _)| *a == written);
    let folded = || {
        let lower = written.to_lowercase();
        UNITS.iter().find(|(a, _, _)| a.chars().count() > 1 && a.to_lowercase() == lower)
    };
    exact.or_else(|| if written.chars().count() > 1 { folded() } else { None }).map(|&(_, one, many)| (one, many))
}

/// A unit's power, from a "²" or "³", or a 2 or 3, written after it.
fn power(unit: &str) -> (Option<&'static str>, &str) {
    if let Some(base) = unit.strip_suffix(['\u{b2}', '2']) {
        (Some("square"), base)
    } else if let Some(base) = unit.strip_suffix(['\u{b3}', '3']) {
        (Some("cubic"), base)
    } else {
        (None, unit)
    }
}

/// How a unit written after a number is read: "km²" as "square
/// kilometers", "km/h" as "kilometers per hour", "/km²" as "per square
/// kilometer". `one` when the number is exactly one.
fn unit_reading(written: &str, one: bool) -> Option<Vec<&'static str>> {
    let (main, per) = match written.split_once('/') {
        Some((main, per)) => (main, Some(per)),
        None => (written, None),
    };
    let mut words = Vec::new();
    if !main.is_empty() {
        let (raised, base) = power(main);
        let (single, plural) = unit_name(base)?;
        words.extend(raised);
        words.push(if one { single } else { plural });
    }
    if let Some(per) = per {
        let (raised, base) = power(per);
        let (single, _) = unit_name(base)?;
        words.push("per");
        words.extend(raised);
        words.push(single);
    }
    (!words.is_empty()).then_some(words)
}

/// Titles that come before a name.
const TITLES: &[&str] = &[
    "capt", "col", "dr", "fr", "gen", "gov", "hon", "lt", "mr", "mrs", "ms", "pres", "prof", "rep", "rev", "sen", "sgt",
];

/// Words after which a Roman numeral is a count: "World War II", "Chapter
/// IV", "Super Bowl LVII".
const COUNTED: &[&str] = &[
    "act", "appendix", "article", "book", "bowl", "canto", "ch", "chapter", "class", "episode", "grade", "level",
    "mark", "no", "number", "page", "part", "phase", "pt", "round", "scene", "section", "stage", "title", "type",
    "unit", "vol", "volume", "war",
];

/// The value of a Roman numeral in its standard form ("XIV", not "XIIII"),
/// from 1 to 3999.
fn roman_value(numeral: &str) -> Option<u32> {
    const NUMERALS: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut rest = numeral;
    let mut value = 0;
    for (amount, letters) in NUMERALS {
        // Each numeral appears at most three times ("III", "XXX").
        for _ in 0..3 {
            match rest.strip_prefix(letters) {
                Some(remaining) => {
                    rest = remaining;
                    value += amount;
                }
                None => break,
            }
            if amount.to_string().starts_with(['4', '9', '5']) {
                break;
            }
        }
    }
    (rest.is_empty() && value > 0).then_some(value)
}

/// Names of monarchs and popes, lowercase and sorted by bytes: after these
/// a Roman numeral is "the" and an ordinal ("Henry VIII", "Pius XII"). They
/// are the names before a numeral in the English labels of people who held
/// a monarch's position or the papacy in Wikidata (CC0), with accents also
/// dropped ("wladyslaw"). After any other capitalized word the numeral is a
/// plain number, as in "Final Fantasy VII" or "Super Bowl XLII".
const REGNAL_NAMES: &[&str] = &[
    "abas",
    "abbad",
    "abbas",
    "abbo",
    "abdallah",
    "abdashtart",
    "abdullah",
    "abdulmecid",
    "abdülmecid",
    "abgar",
    "abidin",
    "abinergaios",
    "abinergaos",
    "achila",
    "achirimbi",
    "adabero",
    "adad",
    "adalbero",
    "adalbert",
    "adam",
    "adelard",
    "adeodatus",
    "adilin",
    "aditya",
    "adlan",
    "adolf",
    "adolphus",
    "adrian",
    "adélard",
    "aelfwald",
    "aethelberht",
    "aethelbert",
    "aethelred",
    "afonso",
    "agapetus",
    "agesilaus",
    "agesipolis",
    "aggabodhi",
    "agila",
    "agis",
    "agrippa",
    "agum",
    "agustin",
    "agustín",
    "ahhotep",
    "ahmad",
    "ahmose",
    "akenzua",
    "akiolu",
    "alam",
    "alamgir",
    "alan",
    "alaric",
    "alberico",
    "albero",
    "alberon",
    "albert",
    "alberto",
    "albrecht",
    "alcetas",
    "alexander",
    "alexandros",
    "alexios",
    "alfonso",
    "ali",
    "allah",
    "alois",
    "alpin",
    "alpín",
    "alvare",
    "alvaro",
    "amadeo",
    "amadeus",
    "amadocus",
    "amadu",
    "amasis",
    "amazasp",
    "ambrosio",
    "ambrósio",
    "amenemhat",
    "amenemhet",
    "amenhotep",
    "ammittamru",
    "amororo",
    "amyntas",
    "ana",
    "anastasios",
    "anastasius",
    "anaxandridas",
    "andani",
    "andre",
    "andrew",
    "andronikos",
    "andré",
    "ankhesenpepi",
    "anna",
    "anno",
    "anthony",
    "antigonus",
    "antiochus",
    "antipater",
    "antoine",
    "anton",
    "antonio",
    "apophis",
    "archelaus",
    "archidamus",
    "archil",
    "ardashir",
    "ardys",
    "ared",
    "aretas",
    "areus",
    "argaeus",
    "argishti",
    "ariarathes",
    "ariobarzanes",
    "aristobulus",
    "arnold",
    "arnuwanda",
    "aromolaran",
    "arsaces",
    "arsames",
    "arshak",
    "arsinoe",
    "arslan",
    "artabanus",
    "artabazos",
    "artavasdes",
    "artavazde",
    "artaxerxes",
    "artaxias",
    "asen",
    "ashot",
    "ashur",
    "ashurnasirpal",
    "aspacures",
    "attalus",
    "attambelos",
    "august",
    "augustus",
    "aziz",
    "baal",
    "baalmilk",
    "bacurius",
    "badi",
    "bagrat",
    "bahadır",
    "bahram",
    "bahu",
    "bakar",
    "balderic",
    "baldwin",
    "ballala",
    "balsa",
    "balša",
    "barisone",
    "barnim",
    "basarab",
    "basil",
    "bassey",
    "baudouin",
    "beatrice",
    "bela",
    "beli",
    "benedek",
    "benedict",
    "berengar",
    "berenguer",
    "berenice",
    "bermudo",
    "bernard",
    "bernardo",
    "berthold",
    "bhadravarman",
    "bharmal",
    "bhavavarman",
    "bhuvanaikabahu",
    "bhuvanekabahu",
    "blanche",
    "bocchus",
    "bodhi",
    "bohemond",
    "boleslaw",
    "bolesław",
    "boniface",
    "bonsu",
    "boris",
    "borommarachathirat",
    "brawijaya",
    "bridei",
    "bruno",
    "buqa",
    "burchard",
    "buriash",
    "burkhard",
    "burnaburiash",
    "béla",
    "callixtus",
    "cambyses",
    "cangrande",
    "cankili",
    "canute",
    "carl",
    "carlo",
    "carloman",
    "carlos",
    "carol",
    "casimir",
    "catherine",
    "celestine",
    "ceolwulf",
    "cesare",
    "chan",
    "chandragupta",
    "charibert",
    "charles",
    "cheliyan",
    "chestha",
    "chettha",
    "childebert",
    "childeric",
    "chilperic",
    "chlothar",
    "chola",
    "christian",
    "christopher",
    "ciniod",
    "clement",
    "cleombrotus",
    "cleomenes",
    "cleopatra",
    "clothar",
    "clovis",
    "comita",
    "conrad",
    "constans",
    "constantine",
    "constantius",
    "cosimo",
    "costanza",
    "costanzo",
    "cotys",
    "cwa",
    "cyaxares",
    "cyrus",
    "dagan",
    "dagbara",
    "dagobert",
    "damasus",
    "daniel",
    "dappula",
    "darius",
    "datayola",
    "daud",
    "david",
    "dawit",
    "dedumose",
    "deidamia",
    "demetrius",
    "denis",
    "deshal",
    "deshalji",
    "devlet",
    "dewa",
    "dharanindravarman",
    "dharmasokaraja",
    "dietmar",
    "dietrich",
    "diogo",
    "dlamini",
    "dmitriy",
    "domitianus",
    "domonkos",
    "donald",
    "drest",
    "dua",
    "dumnagual",
    "duncan",
    "dyfnwal",
    "eadberht",
    "eadbert",
    "eadwulf",
    "ecgberht",
    "edmund",
    "edward",
    "egbert",
    "el",
    "eleonora",
    "elizabeth",
    "emanuele",
    "emmanuel",
    "emund",
    "engelbert",
    "engelhard",
    "enlil",
    "enrique",
    "eric",
    "erishum",
    "ermolao",
    "ernest",
    "eser",
    "eshmunazar",
    "eudamidas",
    "eudes",
    "eugein",
    "eugene",
    "eumenes",
    "eurydice",
    "evagoras",
    "eweka",
    "ewuare",
    "eystein",
    "ezer",
    "fa",
    "faisal",
    "falcon",
    "farouk",
    "federico",
    "felice",
    "felipe",
    "felix",
    "feodor",
    "ferdinand",
    "fetih",
    "folch",
    "francis",
    "francisco",
    "franz",
    "frederick",
    "frederik",
    "fruela",
    "fryderyk",
    "fuad",
    "gagik",
    "gajabahu",
    "garcia",
    "garcía",
    "gariba",
    "gartnait",
    "gaston",
    "gazı",
    "gebhard",
    "gelasius",
    "georg",
    "george",
    "georgios",
    "gerard",
    "gerhard",
    "geza",
    "ghalib",
    "ghazan",
    "giordano",
    "giorgi",
    "giovanni",
    "giyorgis",
    "godji",
    "gohil",
    "gonario",
    "gordian",
    "gottfried",
    "govinda",
    "gregorio",
    "gregory",
    "guaram",
    "guido",
    "guillaume",
    "guisad",
    "gunther",
    "gustaf",
    "gustav",
    "guy",
    "gwari",
    "géza",
    "günther",
    "haakon",
    "haci",
    "hacı",
    "hadad",
    "hakam",
    "hakim",
    "hamad",
    "hamengkubuwana",
    "hamengkubuwono",
    "hamilcar",
    "hammurabi",
    "hanno",
    "happi",
    "harald",
    "harbe",
    "harivarman",
    "harshavarman",
    "harthacnut",
    "hasdrubal",
    "hashimi",
    "hassan",
    "hasyim",
    "hatto",
    "havea",
    "hayat",
    "haymanot",
    "heinrich",
    "hendrik",
    "henri",
    "henrique",
    "henry",
    "henryk",
    "heraclius",
    "herman",
    "hermann",
    "hethum",
    "hidayatullah",
    "hiero",
    "hippolyte",
    "hiram",
    "hisham",
    "honore",
    "honorius",
    "honoré",
    "hor",
    "horik",
    "hormizd",
    "hugh",
    "husayn",
    "hussein",
    "hyrcanus",
    "iacopo",
    "iakovos",
    "ibrahim",
    "idris",
    "igor",
    "iguru",
    "ikenwoli",
    "inaros",
    "indravarman",
    "inge",
    "ini",
    "innocent",
    "intef",
    "isa",
    "isaac",
    "isabella",
    "isanavarman",
    "ishanavarman",
    "iskandar",
    "islam",
    "ithobaal",
    "ittobaal",
    "iuput",
    "ivan",
    "iyasu",
    "iyoas",
    "iziaslav",
    "i̇brahim",
    "i̇brâhim",
    "i̇slâm",
    "jacopo",
    "jadwiga",
    "jah",
    "jahan",
    "jahangir",
    "jakob",
    "jalil",
    "jambe",
    "james",
    "jan",
    "jaromar",
    "jayabahu",
    "jayavaraman",
    "jayavarman",
    "jean",
    "jeroboam",
    "jifar",
    "joachim",
    "joan",
    "joanna",
    "joao",
    "johan",
    "johann",
    "johannes",
    "john",
    "jose",
    "josef",
    "joseph",
    "josé",
    "joão",
    "juana",
    "juba",
    "judoc",
    "julius",
    "justin",
    "justinian",
    "kaliman",
    "kama",
    "kamehameha",
    "kamnaskires",
    "kanishka",
    "kapova",
    "karl",
    "karma",
    "karunwi",
    "kashtiliash",
    "kashyapa",
    "kassapa",
    "kauʻulufonua",
    "kavad",
    "kavadh",
    "kaykaus",
    "kaykhusraw",
    "kaykubad",
    "kayqubad",
    "kenneth",
    "khan",
    "khengar",
    "khentkaus",
    "kheti",
    "khety",
    "khosrov",
    "khosrow",
    "khéty",
    "kidara",
    "kinnin",
    "kirti",
    "kirtivarman",
    "kitoro",
    "kiurike",
    "konrad",
    "konstantinos",
    "kotan",
    "kpɛma",
    "kresimir",
    "krešimir",
    "krishna",
    "krishnaraja",
    "kulasekaran",
    "kulimoetoke",
    "kumaragupta",
    "kuno",
    "kurigalzu",
    "kuvyon",
    "kyawswa",
    "ladislaus",
    "lambert",
    "lamboeny",
    "laodice",
    "lavelua",
    "leo",
    "leon",
    "leonardo",
    "leonidas",
    "leonor",
    "leopold",
    "leotychidas",
    "leszek",
    "letsie",
    "lim",
    "liuva",
    "lothair",
    "louis",
    "luarsab",
    "lucius",
    "ludvonga",
    "ludwig",
    "luis",
    "lupo",
    "luís",
    "madansinh",
    "magnus",
    "mago",
    "mahathammaracha",
    "mahendra",
    "mahendravarman",
    "mahinda",
    "mahmud",
    "mai",
    "malcolm",
    "malichus",
    "malik",
    "mamia",
    "mansur",
    "manuel",
    "marcellus",
    "margaret",
    "margrethe",
    "maria",
    "mariamne",
    "marianus",
    "marinus",
    "martin",
    "marwan",
    "mary",
    "maryam",
    "mastino",
    "masud",
    "matilda",
    "mattan",
    "mautamakia",
    "mautāmakia",
    "mavuso",
    "maximilian",
    "mehmed",
    "mehmet",
    "menander",
    "menelik",
    "menilek",
    "menkamin",
    "menli",
    "mentuhotep",
    "merikara",
    "mesud",
    "meñli",
    "michael",
    "mieszko",
    "miguel",
    "mihailo",
    "mihrdat",
    "milan",
    "mingi",
    "minkhaung",
    "mirian",
    "mithridates",
    "mitridate",
    "modjadji",
    "moggallana",
    "mohammed",
    "mojmir",
    "moshoeshoe",
    "mpisani",
    "mstislav",
    "mswati",
    "mudhoji",
    "muhammad",
    "muhàmmad",
    "mundhir",
    "mursili",
    "mustafa",
    "mustain",
    "mustakfi",
    "mustansir",
    "mustapha",
    "mutaga",
    "mutawakkil",
    "mutebi",
    "muteesa",
    "muwatalli",
    "muzaffar",
    "mwambutsa",
    "mwanga",
    "mwezi",
    "naga",
    "naimbanna",
    "namgyal",
    "napoleon",
    "narai",
    "narasaraja",
    "narasimha",
    "narasimhavarman",
    "nasir",
    "nebiriau",
    "nebiryraw",
    "nebuchadnezzar",
    "necho",
    "nectanebo",
    "nedunjeliyan",
    "neferhotep",
    "neferirkare",
    "neferkara",
    "neferkare",
    "neferkaure",
    "neferu",
    "nemtyemsaf",
    "nepherites",
    "ngwane",
    "nicholas",
    "nicolau",
    "nicomedes",
    "nikare",
    "nikephoros",
    "niko",
    "ninurta",
    "niqmaddu",
    "nkosi",
    "nofret",
    "non",
    "ntare",
    "nyamaa",
    "obodas",
    "obrenovic",
    "obrenović",
    "oengus",
    "ogbagba",
    "olaf",
    "olav",
    "olimi",
    "olubuse",
    "orabazes",
    "ordono",
    "ordoño",
    "orhue",
    "orodes",
    "orontes",
    "orzocorre",
    "oscar",
    "osorkon",
    "osred",
    "osulf",
    "otto",
    "ottokar",
    "outey",
    "pacorus",
    "pandyan",
    "parakkamabahu",
    "parakramabahu",
    "paramesvaravarman",
    "parantaka",
    "parnavaz",
    "parsman",
    "paschal",
    "paul",
    "paulinus",
    "pedro",
    "pedubast",
    "pelagius",
    "pellegrino",
    "pepi",
    "pepin",
    "pepple",
    "perdiccas",
    "peroz",
    "petar",
    "peter",
    "petubastis",
    "pharasmanes",
    "pharnavaz",
    "philip",
    "philippe",
    "phraates",
    "pierre",
    "pietro",
    "pileser",
    "pinedjem",
    "piotr",
    "pius",
    "polemon",
    "pragmal",
    "pragmalji",
    "pravarasena",
    "prempeh",
    "presian",
    "pressiyan",
    "prithivishena",
    "przemysl",
    "przemysł",
    "psammetichus",
    "psamtik",
    "psusennes",
    "ptolemy",
    "pulakeshi",
    "pulakesi",
    "putra",
    "qadir",
    "qaplan",
    "qasim",
    "qudsia",
    "rabat",
    "rabbel",
    "radama",
    "rafael",
    "raghoji",
    "raghuji",
    "rahman",
    "rainier",
    "raja",
    "rajaraja",
    "rajaram",
    "rajasimha",
    "rajasinghe",
    "rajasinha",
    "rajendravarman",
    "rama",
    "ramathibodi",
    "ramesses",
    "ramiro",
    "ranavalona",
    "rasasingan",
    "rayadhan",
    "reachea",
    "ream",
    "reamea",
    "reccared",
    "rev",
    "rhescuporis",
    "richard",
    "riderch",
    "robert",
    "roger",
    "roman",
    "romanos",
    "rostislav",
    "rudolf",
    "rudolph",
    "rudrasena",
    "rudravarman",
    "rukidi",
    "rupert",
    "rusa",
    "saadet",
    "sadalas",
    "sahib",
    "saif",
    "salentin",
    "salih",
    "salim",
    "salman",
    "salomon",
    "sambhaji",
    "sanatruk",
    "sanchez",
    "sancho",
    "sarduri",
    "sargon",
    "sarvasena",
    "satha",
    "saurmag",
    "sauromaces",
    "sava",
    "scorpion",
    "sebastien",
    "sechele",
    "seem",
    "sekhukhune",
    "selamet",
    "selassie",
    "seleucus",
    "selim",
    "selâmet",
    "sena",
    "senusret",
    "sergius",
    "seti",
    "setthathirath",
    "seuthes",
    "sewadjkare",
    "seyon",
    "shah",
    "shahu",
    "shalmaneser",
    "shapur",
    "shesh",
    "sheshonk",
    "shipak",
    "shivaji",
    "shoshenq",
    "siegfried",
    "sigebert",
    "sigeric",
    "sigismund",
    "sigurd",
    "sikandar",
    "simeon",
    "simhavarman",
    "simon",
    "sin",
    "singh",
    "sinhavarman",
    "sirajuddin",
    "sisebut",
    "siva",
    "sixtus",
    "smbat",
    "sneferka",
    "sobekemsaf",
    "sobekhotep",
    "sobhuza",
    "solomon",
    "soriyotei",
    "soriyoteï",
    "stasioikos",
    "stefan",
    "stephen",
    "sulaiman",
    "suleiman",
    "sultan",
    "suppiluliuma",
    "surinyavong",
    "suryavarman",
    "susenyos",
    "svatopluk",
    "sverker",
    "sviatopolk",
    "sviatoslav",
    "sweyn",
    "syah",
    "syaifuddin",
    "syaifudin",
    "sylvester",
    "sánchez",
    "sébastien",
    "sëëm",
    "tabnit",
    "takelot",
    "talal",
    "talorc",
    "talorgan",
    "tamar",
    "tamatoa",
    "tamjidullah",
    "tarabya",
    "tayyib",
    "tefnakht",
    "tegueste",
    "teimuraz",
    "teodos",
    "teqerideamani",
    "teres",
    "teriitaria",
    "tetricus",
    "tewodros",
    "thammaracha",
    "theobald",
    "theoderic",
    "theodore",
    "theodoric",
    "theodosios",
    "theodosius",
    "theonesios",
    "theudebert",
    "theuderic",
    "thierry",
    "thipadei",
    "thomas",
    "thoros",
    "thutmose",
    "tiberios",
    "tiberius",
    "tigranes",
    "tiraios",
    "tiridate",
    "tiridates",
    "tissa",
    "toghrul",
    "torchitorio",
    "tribhuvan",
    "tsimiaro",
    "tudhaliya",
    "tughril",
    "tugʻshoda",
    "tupou",
    "tutu",
    "ubaidullah",
    "udaya",
    "udayadityavarman",
    "udo",
    "ulrich",
    "umar",
    "umberto",
    "unsa",
    "upatissa",
    "urban",
    "uros",
    "uroš",
    "uthong",
    "uto",
    "uzana",
    "vakhtang",
    "valdemar",
    "valentinian",
    "valerian",
    "vameq",
    "varagunan",
    "varagunavarman",
    "vasili",
    "vasudeva",
    "veronica",
    "verónica",
    "vespasiano",
    "viacheslav",
    "victor",
    "vijayabahu",
    "vijayaraj",
    "vikramabahu",
    "vikramaditya",
    "vikrantavarman",
    "vimaladharmasuriya",
    "vimaladharmasurya",
    "vladimir",
    "vladislaus",
    "vologases",
    "vologese",
    "vologèse",
    "vonones",
    "voravongse",
    "vratislaus",
    "vsevolod",
    "wabi",
    "wadiyar",
    "wahid",
    "walgash",
    "walid",
    "ware",
    "wathiq",
    "wenceslas",
    "wenceslaus",
    "werner",
    "wilhelm",
    "william",
    "wladyslaw",
    "wodeyar",
    "władysław",
    "xah",
    "xerxes",
    "yahya",
    "yakup",
    "yamusa",
    "yaropolk",
    "yaroslav",
    "yasovarman",
    "yazdegerd",
    "yazid",
    "yeshaq",
    "yiadom",
    "yohannes",
    "yuhi",
    "yusuf",
    "zacharias",
    "zewditu",
    "zhanibek",
    "zog",
    "zyroslaw",
    "álvaro",
    "ælfwald",
    "æthelberht",
    "æthelbert",
    "æthelred",
    "óengus",
    "ğazı",
    "żyrosław",
];

/// How a Roman numeral is read after `before`, if it is one there. After
/// words like "chapter", "part" or "war" it counts ("World War II" is "two");
/// after a name in `REGNAL_NAMES` it is "the" and an ordinal, up to 30
/// ("Richard III" is "Richard the third"); after any other capitalized word
/// it is a plain number ("Final Fantasy VII"). Outside counts only I, V and X
/// are read, as "DC", "MD" and "XL" are other things, and not a lone I or X
/// ("Malcolm X"). A lone I counts only after a capitalized word or a short
/// form: "World War I", "vol I", but not "the war I fought".
fn roman_numeral(word: &str, before: Option<&Token>) -> Option<Vec<Token>> {
    let Some(Token::Word(previous)) = before else { return None };
    let previous = previous.to_lowercase();
    let upper = word.chars().all(|c| c.is_ascii_uppercase());
    let lower = word.chars().all(|c| c.is_ascii_lowercase());
    if COUNTED.contains(&previous.as_str()) {
        let short = matches!(previous.as_str(), "vol" | "no" | "ch" | "pt");
        if !(upper || lower) || (word == "I" && !capitalized(before) && !short) {
            return None;
        }
        let value = roman_value(&word.to_uppercase())?;
        return Some(vec![Token::Number { digits: value.to_string(), ordinal: false }]);
    }
    let numeral = capitalized(before)
        && FUNCTION_WORDS.binary_search(&previous.as_str()).is_err()
        && upper
        && word.chars().all(|c| matches!(c, 'I' | 'V' | 'X'))
        && word != "I"
        && word != "X";
    let value = roman_value(word).filter(|_| numeral)?;
    if REGNAL_NAMES.binary_search(&previous.as_str()).is_ok() && value <= 30 {
        Some(vec![Token::Word("the".to_string()), Token::Number { digits: value.to_string(), ordinal: true }])
    } else {
        Some(vec![Token::Number { digits: value.to_string(), ordinal: false }])
    }
}

/// Months, in full and abbreviated, for reading the day beside them.
const MONTH_WORDS: &[&str] = &[
    "apr",
    "april",
    "aug",
    "august",
    "dec",
    "december",
    "feb",
    "february",
    "jan",
    "january",
    "jul",
    "july",
    "jun",
    "june",
    "mar",
    "march",
    "may",
    "nov",
    "november",
    "oct",
    "october",
    "sep",
    "sept",
    "september",
];

/// Whether a token is a month, written as a name is: "April", "Jan".
fn month(token: Option<&Token>) -> bool {
    matches!(token, Some(Token::Word(w)) if capitalized(token) && MONTH_WORDS.contains(&w.to_lowercase().as_str()))
}

/// Words that greet or answer rather than name a place, so a state
/// abbreviation after them and a comma is not in an address: "Thanks, Pa."
const NOT_CITIES: &[&str] =
    &["bye", "dear", "goodbye", "hello", "hey", "hi", "oh", "okay", "please", "sorry", "thanks", "well"];

/// Whether the token after an abbreviation's period starts a new sentence:
/// the end of the text, or a capitalized word that usually opens one ("The",
/// "It", "We", "Then").
fn starts_sentence(after: Option<&Token>) -> bool {
    const OPENERS: &[&str] = &[
        "also", "however", "so", "then", "there", "these", "this", "those", "what", "when", "where", "who", "why",
        "how", "here", "now", "yes", "no", "that", "don't", "can't", "won't", "didn't", "doesn't", "isn't", "let's",
    ];
    match after {
        None => true,
        Some(Token::Word(w)) if capitalized(after) => {
            let lower = w.to_lowercase();
            // Contractions open sentences as their first word does: "It's", "We're".
            let first = lower.split('\'').next().unwrap_or(&lower);
            [lower.as_str(), first]
                .iter()
                .any(|word| FUNCTION_WORDS.binary_search(word).is_ok() || OPENERS.contains(word))
        }
        _ => false,
    }
}

/// Replaces abbreviations in a token stream with what they stand for, or
/// with how they are written when `expand` is false, and keeps an
/// abbreviation's period as a sentence end only where it is one.
pub fn resolve(tokens: Vec<Token>, expand: bool) -> Vec<Token> {
    let is_period = |t: Option<&Token>| matches!(t, Some(Token::Boundary(Boundary::Period)));
    let mut out = Vec::with_capacity(tokens.len());
    // Set after a state, so the ZIP code that follows is read digit by digit.
    let mut zip_next = false;
    let mut i = 0;
    while i < tokens.len() {
        if let (true, Token::Number { digits, .. }) = (std::mem::take(&mut zip_next), &tokens[i]) {
            out.push(Token::Expanded(digits.bytes().map(|d| ONES[(d - b'0') as usize]).collect()));
            i += 1;
            continue;
        }
        let period = is_period(tokens.get(i + 1));
        let after = tokens.get(i + if period { 2 } else { 1 });
        // Look past one period, so "King Jr. Dr." sees "Jr" before "Dr".
        let before = match i.checked_sub(1).map(|j| &tokens[j]) {
            Some(Token::Boundary(Boundary::Period)) => i.checked_sub(2).map(|j| &tokens[j]),
            other => other,
        };
        // A city and a comma before: "Springfield, Mo.".
        let after_city = matches!(i.checked_sub(1).map(|j| &tokens[j]), Some(Token::Boundary(Boundary::Comma)))
            && match i.checked_sub(2).map(|j| &tokens[j]) {
                Some(Token::Word(w)) => {
                    let lower = w.to_lowercase();
                    capitalized(Some(&tokens[i - 2]))
                        && !NOT_CITIES.contains(&lower.as_str())
                        && FUNCTION_WORDS.binary_search(&lower.as_str()).is_err()
                }
                _ => false,
            };
        // Judged on the tokens already read, where a period remains only
        // where a sentence ends.
        let name_before = capitalized(before)
            && out.len() >= 2
            && !matches!(
                out[out.len() - 2],
                Token::Boundary(Boundary::Period | Boundary::Question | Boundary::Exclamation)
            );
        // A day beside its month is an ordinal: "April 10" is "April tenth",
        // "4 March" "the fourth of March".
        if let Token::Number { digits, ordinal: false } = &tokens[i]
            && digits.parse::<u32>().is_ok_and(|day| (1..=31).contains(&day))
        {
            let day = Token::Number { digits: digits.clone(), ordinal: true };
            if month(before) {
                out.push(day);
                i += 1;
                continue;
            }
            if month(tokens.get(i + 1)) {
                out.extend([Token::Word("the".to_string()), day, Token::Word("of".to_string())]);
                i += 1;
                continue;
            }
        }
        // "1.17 sq mi": square miles.
        if let (Token::Word(sq), Some(digits), Some(Token::Word(unit))) = (&tokens[i], number(before), after)
            && expand
            && sq.eq_ignore_ascii_case("sq")
            && let Some((single, plural)) = unit_name(unit)
        {
            out.push(Token::Expanded(vec!["square", if digits == "1" { single } else { plural }]));
            i += if period { 3 } else { 2 };
            continue;
        }
        if let Token::Word(word) = &tokens[i]
            && let Some(reading) = roman_numeral(word, before)
        {
            out.extend(reading);
            i += 1;
            continue;
        }
        let replacement = match &tokens[i] {
            Token::Word(word) => {
                let context = Context { word, period, before, after, after_city, name_before };
                zip_next = context.before_zip() && state(&context).is_some();
                if let Some(words) = number(before).and_then(|digits| unit_reading(word, digits == "1")) {
                    Some((if expand { Token::Expanded(words) } else { as_written(word) }, Kind::Other))
                } else if matches!(word.as_str(), "x" | "X") && number(before).is_some() && number(after).is_some() {
                    // Sizes: "4 x 4", "6 x 14".
                    Some((if expand { Token::Expanded(vec!["by"]) } else { as_written(word) }, Kind::Before))
                } else if let Some((words, kind)) = self::expand(&context) {
                    Some((if expand { Token::Expanded(words) } else { as_written(word) }, kind))
                } else if !expand && DICTIONARY_EXPANSIONS.contains(&word.to_lowercase().as_str()) {
                    Some((as_written(word), Kind::Other))
                } else if period && word.chars().count() == 1 && capitalized(Some(&tokens[i])) && capitalized(after) {
                    // An initial, as in "J. R. R. Tolkien".
                    Some((Token::Spelled(word.clone()), Kind::Before))
                } else {
                    None
                }
            }
            Token::Dotted(letters) if expand => {
                let context = Context { word: letters, period, before, after, after_city, name_before };
                zip_next = context.before_zip() && find(DOTTED_STATES, &letters.to_lowercase()).is_some();
                Some((expand_dotted(letters, after_city || context.before_zip()), Kind::Other))
            }
            Token::Dotted(letters) => Some((Token::Spelled(letters.clone()), Kind::Other)),
            _ => None,
        };
        match replacement {
            Some((token, kind)) => {
                out.push(token);
                if period {
                    let ends = match kind {
                        Kind::Before => false,
                        Kind::Other => starts_sentence(after),
                        Kind::Closing => after.is_none() || capitalized(after),
                    };
                    if ends {
                        out.push(Token::Boundary(Boundary::Period));
                    }
                    i += 1;
                }
            }
            None => out.push(tokens[i].clone()),
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::tokenize;

    /// The resolved tokens as plain text: expansions as words, spelled
    /// letters in capitals, and periods only where a sentence ends.
    fn read(text: &str) -> String {
        read_with(text, true)
    }

    fn read_with(text: &str, expand: bool) -> String {
        let parts: Vec<String> = resolve(tokenize(text), expand)
            .into_iter()
            .map(|t| match t {
                Token::Word(w) => w,
                Token::Number { digits, ordinal: false } => digits,
                Token::Number { digits, ordinal: true } => format!("{digits}th"),
                Token::Expanded(words) => words.join(" "),
                Token::Spelled(letters) | Token::Dotted(letters) => letters.to_uppercase(),
                Token::Literal(word) => format!("<{word}>"),
                Token::Boundary(Boundary::Period) => ".".to_string(),
                Token::Boundary(_) => ",".to_string(),
            })
            .collect();
        parts.join(" ")
    }

    #[test]
    fn doctor_and_drive() {
        assert_eq!(read("Dr. Smith lives on Elm Dr."), "doctor Smith lives on Elm drive .");
        assert_eq!(read("Dr Jones met Mr. Lee"), "doctor Jones met mister Lee");
        assert_eq!(read("Martin Luther King Jr. Dr."), "Martin Luther King junior drive .");
    }

    #[test]
    fn saint_and_street() {
        assert_eq!(read("St. Louis is on Main St. It is big."), "saint Louis is on Main street . It is big .");
        assert_eq!(read("Main St. is closed"), "Main street is closed");
        assert_eq!(read("Turn onto 5th St. now"), "Turn onto 5th street now");
        assert_eq!(
            read("Dr. St. Jermain lives on St. Jermain Dr."),
            "doctor saint Jermain lives on saint Jermain drive ."
        );
        assert_eq!(
            read("He lives on St. Jermain Dr. Open the door."),
            "He lives on saint Jermain drive . Open the door ."
        );
        assert_eq!(read("Ask Dr. Smith. Visit St. Louis."), "Ask doctor Smith . Visit saint Louis .");
    }

    #[test]
    fn units_and_numbers_need_numbers() {
        assert_eq!(read("He is 6 ft. tall"), "He is 6 feet tall");
        assert_eq!(read("1 lb of rice"), "1 pound of rice");
        assert_eq!(read("Ft. Worth"), "fort Worth");
        assert_eq!(read("Room No. 5"), "Room number 5");
        assert_eq!(read("I said no. Then I left."), "I said no . Then I left .");
        assert_eq!(read("a lb"), "a lb");
    }

    #[test]
    fn dotted_abbreviations_and_initials() {
        assert_eq!(read("fruit, e.g. apples"), "fruit , for example apples");
        assert_eq!(read("the U.S. Army"), "the US Army");
        assert_eq!(read("I live in the U.S. It is big."), "I live in the US . It is big .");
        assert_eq!(read("J. R. R. Tolkien wrote"), "J R R Tolkien wrote");
    }

    #[test]
    fn dates_companies_and_latin() {
        assert_eq!(read("Jan. 5, 2024"), "january 5th , 2024");
        assert_eq!(read("Open Sat. and Sun."), "Open saturday and sunday .");
        assert_eq!(read("we sat down"), "we sat down");
        assert_eq!(read("Apple Inc. announced"), "Apple incorporated announced");
        assert_eq!(read("apples, pears, etc. The end."), "apples , pears , et cetera . The end .");
        assert_eq!(read("cats vs. dogs"), "cats versus dogs");
        assert_eq!(read("apples, etc. Vol. 3 is next"), "apples , et cetera . volume 3 is next");
    }

    #[test]
    fn contractions_can_start_sentences() {
        assert_eq!(read("It weighs 2 lbs. It's heavy."), "It weighs 2 pounds . It's heavy .");
        assert_eq!(read("Turn on Oak Rd. Don't stop."), "Turn on Oak road . Don't stop .");
    }

    #[test]
    fn states_where_they_are_clear() {
        assert_eq!(read("I moved to Calif. It is warm."), "I moved to california . It is warm .");
        assert_eq!(read("Springfield, Mo., is near"), "Springfield , missouri , is near");
        assert_eq!(read("Jackson, Miss. 39201"), "Jackson , mississippi three nine two zero one");
        assert_eq!(read("Austin, TX 78701"), "Austin , texas seven eight seven zero one");
        assert_eq!(read("Albany, N.Y. is cold"), "Albany , new york is cold");
        // Words and other abbreviations stay as they are.
        assert_eq!(read("See you Mo. and Tues."), "See you Mo . and tuesday .");
        assert_eq!(read("Thanks, Pa. It was fun."), "Thanks , Pa . It was fun .");
        assert_eq!(read("Miss. Smith arrived"), "Miss . Smith arrived");
        assert_eq!(read("It is OK 12345 times"), "It is OK 12345 times");
        assert_eq!(read("He is OK, IN fact"), "He is OK , IN fact");
        assert_eq!(read("the U.S. and D.C."), "the US and DC .");
    }

    #[test]
    fn expansion_can_be_turned_off() {
        assert_eq!(
            read_with("Dr. Smith of Apple Inc. met Mr. Lee on Main St. It was Jan. 5.", false),
            "DR Smith of Apple <Inc> met MR Lee on Main ST . It was <Jan> 5th ."
        );
        assert_eq!(read_with("fruit, e.g. apples, etc. The end.", false), "fruit , EG apples , <etc> . The end .");
        assert_eq!(read_with("it weighs 2 lbs and the dr said so", false), "it weighs 2 LBS and the DR said so");
        assert_eq!(read_with("Austin, TX 78701", false), "Austin , TX seven eight seven zero one");
    }

    #[test]
    fn units_with_powers_and_rates() {
        assert_eq!(read("It is 100 m long"), "It is 100 meters long");
        assert_eq!(read("A 1GB drive"), "A 1 gigabyte drive");
        assert_eq!(read("459.0/km² and 3.04 km2"), "459.0 per square kilometer and 3.04 square kilometers");
        assert_eq!(read("15 km/h and 854 m³/s"), "15 kilometers per hour and 854 cubic meters per second");
        assert_eq!(read("1.17 sq mi"), "1.17 square miles");
        assert_eq!(read("a 4 x 4 truck"), "a 4 by 4 truck");
        assert_eq!(read("2 vol and 4 pp"), "2 volumes and 4 pages");
    }

    #[test]
    fn short_forms_without_periods() {
        assert_eq!(read("dr Henry died in st Petersburg"), "doctor Henry died in saint Petersburg");
        assert_eq!(read("See pp 1 and pg 5, rated PG 13"), "See pages 1 and page 5 , rated PG 13");
        assert_eq!(read("Shipyard No 1. No 2 people are alike."), "Shipyard number 1 . No 2 people are alike .");
        assert_eq!(read("vol I and Lt Smith"), "volume 1 and lieutenant Smith");
    }

    #[test]
    fn roman_numerals_by_context() {
        assert_eq!(
            read("Open your textbook to Ch. 20, do you see the reference to Richard III?"),
            "Open your textbook to chapter 20 , do you see the reference to Richard the 3th ,"
        );
        assert_eq!(read("Henry VIII and Pope John Paul II"), "Henry the 8th and Pope John Paul the 2th");
        assert_eq!(read("World War II and World War I"), "World War 2 and World War 1");
        assert_eq!(read("Chapter IV, page xii, Super Bowl LVII"), "Chapter 4 , page 12 , Super Bowl 57");
        // After a word that is not a monarch's or pope's name, a plain number.
        assert_eq!(read("Final Fantasy VII and Journal XXX"), "Final Fantasy 7 and Journal 30");
        // Not numerals here.
        assert_eq!(read("Washington DC and Malcolm X"), "Washington DC and Malcolm X");
        assert_eq!(read("the war I fought"), "the war I fought");
        assert_eq!(read("Chapter IIII"), "Chapter IIII");
        assert_eq!(roman_value("MCMXCIX"), Some(1999));
        assert_eq!(roman_value("IC"), None);
    }

    #[test]
    fn days_beside_months_are_ordinals() {
        assert_eq!(read("Retrieved April 10, 2013."), "Retrieved April 10th , 2013 .");
        assert_eq!(read("Retrieved 4 March 2014."), "Retrieved the 4th of March 2014 .");
        assert_eq!(read("on Jan. 5"), "on january 5th");
        assert_eq!(read("May 2014 and 5 apples"), "May 2014 and 5 apples");
    }

    #[test]
    fn ordinary_words_are_left_alone() {
        assert_eq!(read("MS Word"), "MS Word");
        assert_eq!(read("the gen pool"), "the gen pool");
        assert_eq!(read("Rev up the engine"), "Rev up the engine");
    }
}
