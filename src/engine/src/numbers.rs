//! Numbers written with symbols: times ("3:00 pm"), decades ("the 1990s"),
//! ranges ("1990-1995"), phone and ISBN numbers ("555-1234"), dates
//! ("2014-03-05", "3/5/2014"), simple fractions ("3/4") and money
//! ("$22,750", "$3.50", "$80 million"). Each is turned into the tokens
//! for its words; plain numbers are left to the number reader in text.rs.

use crate::text::{ONES, TENS, Token};
use crate::utterance::Boundary;

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// Plurals of the tens, for decades: "the nineties".
const TENS_PLURAL: [&str; 10] =
    ["", "tens", "twenties", "thirties", "forties", "fifties", "sixties", "seventies", "eighties", "nineties"];

/// Fractions with small denominators: "one half", "three quarters".
const FRACTIONS: [(&str, &str); 11] = [
    ("", ""),
    ("", ""),
    ("half", "halves"),
    ("third", "thirds"),
    ("quarter", "quarters"),
    ("fifth", "fifths"),
    ("sixth", "sixths"),
    ("seventh", "sevenths"),
    ("eighth", "eighths"),
    ("ninth", "ninths"),
    ("tenth", "tenths"),
];

/// A currency sign's unit and subunit, singular and plural.
fn currency(sign: char) -> Option<[&'static str; 4]> {
    match sign {
        '$' => Some(["dollar", "dollars", "cent", "cents"]),
        '£' => Some(["pound", "pounds", "penny", "pence"]),
        '€' => Some(["euro", "euros", "cent", "cents"]),
        '¥' => Some(["yen", "yen", "", ""]),
        _ => None,
    }
}

/// Whether a number-like expression starts at `chars[i]`.
pub fn starts(chars: &[char], i: usize) -> bool {
    let at = |k: usize| chars.get(k).copied().unwrap_or(' ');
    let before = i.checked_sub(1).map_or(' ', |k| chars[k]);
    at(i).is_ascii_digit()
        || (at(i) == '.' && at(i + 1).is_ascii_digit() && !before.is_alphanumeric())
        || (currency(at(i)).is_some() && at(i + 1).is_ascii_digit())
}

/// Digits from `i`, with commas and points between digits: "1,024", "3.14".
fn digits_at(chars: &[char], mut i: usize) -> (String, usize) {
    let start = i;
    let at = |k: usize| chars.get(k).copied().unwrap_or(' ');
    while at(i).is_ascii_digit() || (matches!(at(i), ',' | '.') && at(i + 1).is_ascii_digit()) {
        i += 1;
    }
    (chars[start..i].iter().collect(), i)
}

/// Words for a whole number below 100, from the tables.
fn small(n: usize) -> Vec<&'static str> {
    match n {
        0..=19 => vec![ONES[n]],
        _ if n.is_multiple_of(10) => vec![TENS[n / 10]],
        _ => vec![TENS[n / 10], ONES[n % 10]],
    }
}

fn plain(digits: &str) -> Option<usize> {
    digits.chars().all(|c| c.is_ascii_digit()).then(|| digits.parse().ok()).flatten()
}

/// The word after `i`, skipping spaces, and the index after it.
fn word_after(chars: &[char], i: usize) -> (String, usize) {
    let mut k = i;
    while chars.get(k) == Some(&' ') {
        k += 1;
    }
    let start = k;
    while chars.get(k).is_some_and(|c| c.is_alphabetic()) {
        k += 1;
    }
    (chars[start..k].iter().collect(), k)
}

/// "am" or "pm" after a time or an hour: "3:00 pm", "8pm". Dotted "a.m." is
/// spelled as any dotted abbreviation is.
fn am_pm(chars: &[char], i: usize, tokens: &mut Vec<Token>) -> usize {
    let (word, end) = word_after(chars, i);
    if matches!(word.to_lowercase().as_str(), "am" | "pm") {
        tokens.push(Token::Spelled(word.to_uppercase()));
        end
    } else {
        i
    }
}

/// A decade: "1990s" is "nineteen nineties", "2000s" "two thousands",
/// "1900s" "nineteen hundreds", "80s" "eighties".
fn decade(n: usize, len: usize) -> Option<Vec<&'static str>> {
    if !n.is_multiple_of(10) {
        return None;
    }
    match len {
        2 => Some(vec![TENS_PLURAL[n / 10]]),
        4 if n.is_multiple_of(1000) => Some(vec![ONES[n / 1000], "thousands"]),
        4 => {
            let mut words = small(n / 100);
            words.push(if n.is_multiple_of(100) { "hundreds" } else { TENS_PLURAL[n % 100 / 10] });
            Some(words)
        }
        _ => None,
    }
}

/// Reads the number-like expression at `chars[i]` (see [`starts`]), adding
/// its tokens, and returns the index after it.
pub fn scan(chars: &[char], i: usize, tokens: &mut Vec<Token>) -> usize {
    let at = |k: usize| chars.get(k).copied().unwrap_or(' ');
    if let Some(units) = currency(chars[i]) {
        return money(chars, i + 1, units, tokens);
    }
    let (first, mut end) = digits_at(chars, i);
    let value = plain(&first);

    // A time: "3:00", "09:45", "1:00pm".
    if let Some(hour) = value.filter(|&h| h <= 23)
        && at(end) == ':'
        && at(end + 1).is_ascii_digit()
        && at(end + 2).is_ascii_digit()
        && !at(end + 3).is_ascii_digit()
    {
        let minute: usize = chars[end + 1..end + 3].iter().collect::<String>().parse().unwrap_or(0);
        if minute <= 59 {
            let mut words = small(hour);
            match minute {
                0 => {}
                1..=9 => words.extend(["oh", ONES[minute]]),
                _ => words.extend(small(minute)),
            }
            let before = tokens.len();
            tokens.push(Token::Expanded(words));
            let after = am_pm(chars, end + 3, tokens);
            if minute == 0 && tokens.len() == before + 1 {
                tokens.push(Token::Expanded(vec!["o'clock"]));
            }
            return after;
        }
    }

    // A decade: "1990s", "80s".
    if let Some(n) = value
        && at(end) == 's'
        && !at(end + 1).is_alphabetic()
        && let Some(words) = decade(n, first.len())
    {
        tokens.push(Token::Expanded(words));
        return end + 1;
    }

    // Groups joined by hyphens or dashes: ranges, phone numbers, ISBNs, dates.
    let mut groups = vec![first.clone()];
    loop {
        let mut k = end;
        while chars.get(k) == Some(&' ') {
            k += 1;
        }
        if !matches!(at(k), '-' | '\u{2013}') {
            break;
        }
        k += 1;
        while chars.get(k) == Some(&' ') {
            k += 1;
        }
        if !at(k).is_ascii_digit() {
            break;
        }
        let (next, next_end) = digits_at(chars, k);
        if plain(&next).is_none() || plain(groups.last().unwrap()).is_none() {
            break;
        }
        groups.push(next);
        end = next_end;
    }
    if groups.len() > 1 {
        let lengths: Vec<usize> = groups.iter().map(String::len).collect();
        if lengths == [4, 2, 2]
            && let Some(date) = date(&groups[1], &groups[2], &groups[0])
        {
            tokens.extend(date);
            return end;
        }
        let digit_groups = groups.len() >= 3
            || groups.iter().any(|g| g.len() > 1 && g.starts_with('0'))
            || (lengths[0] <= 3 && lengths[1] >= 4);
        if digit_groups {
            // Read digit by digit, with a pause between groups.
            for (n, group) in groups.iter().enumerate() {
                if n > 0 {
                    tokens.push(Token::Boundary(Boundary::Comma));
                }
                tokens.push(Token::Expanded(group.bytes().map(|d| ONES[(d - b'0') as usize]).collect()));
            }
        } else {
            // A range: "1990-1995", "pages 93 - 102".
            tokens.push(Token::Number { digits: groups[0].clone(), ordinal: false });
            tokens.push(Token::Word("to".to_string()));
            tokens.push(Token::Number { digits: groups[1].clone(), ordinal: false });
        }
        return end;
    }

    // Slashes: dates ("3/5/2014") and simple fractions ("3/4").
    if value.is_some() && at(end) == '/' && at(end + 1).is_ascii_digit() {
        let (second, second_end) = digits_at(chars, end + 1);
        if at(second_end) == '/' && at(second_end + 1).is_ascii_digit() {
            let (third, third_end) = digits_at(chars, second_end + 1);
            if let Some(date) = date(&first, &second, &third) {
                tokens.extend(date);
                return third_end;
            }
        } else if let (Some(top), Some(bottom)) = (value, plain(&second))
            && (2..=10).contains(&bottom)
            && (1..bottom).contains(&top)
        {
            let (one, many) = FRACTIONS[bottom];
            let mut words = small(top);
            words.push(if top == 1 { one } else { many });
            tokens.push(Token::Expanded(words));
            return second_end;
        }
    }

    // A plain number, maybe ordinal ("21st") or an hour ("8pm").
    let suffix: String = chars[end..].iter().take_while(|c| c.is_alphabetic()).collect();
    let ordinal = matches!(suffix.to_lowercase().as_str(), "st" | "nd" | "rd" | "th");
    tokens.push(Token::Number { digits: first, ordinal });
    if ordinal {
        return end + 2;
    }
    if value.is_some_and(|h| (1..=12).contains(&h)) { am_pm(chars, end, tokens) } else { end }
}

/// A date from month, day and year as written: "March fifth, twenty
/// fourteen". `None` unless the month and day are plausible.
fn date(month: &str, day: &str, year: &str) -> Option<Vec<Token>> {
    let (m, d) = (plain(month)?, plain(day)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || !matches!(year.len(), 2 | 4) {
        return None;
    }
    let year = match (year.len(), plain(year)?) {
        (2, y @ 0..=9) => Token::Expanded(vec!["oh", ONES[y]]),
        _ => Token::Number { digits: year.to_string(), ordinal: false },
    };
    Some(vec![
        Token::Expanded(vec![MONTHS[m - 1]]),
        Token::Number { digits: d.to_string(), ordinal: true },
        Token::Boundary(Boundary::Comma),
        year,
    ])
}

/// Money after its currency sign: "$22,750" is "twenty two thousand seven
/// hundred fifty dollars", "$3.50" "three dollars and fifty cents", "$80
/// million" "eighty million dollars".
fn money(chars: &[char], i: usize, units: [&'static str; 4], tokens: &mut Vec<Token>) -> usize {
    let (amount, end) = digits_at(chars, i);
    let [one, many, cent, cents] = units;
    let (written_scale, after_scale) = word_after(chars, end);
    // A scale in full, or short and written against the amount: "$5M",
    // "$10bn", "€3k".
    let attached = chars.get(end).is_some_and(|c| c.is_alphabetic());
    let scale = match written_scale.to_lowercase().as_str() {
        word @ ("thousand" | "million" | "billion" | "trillion") => word.to_string(),
        "k" if attached => "thousand".to_string(),
        "m" | "mn" | "mil" if attached => "million".to_string(),
        "b" | "bn" if attached => "billion".to_string(),
        "tn" if attached => "trillion".to_string(),
        _ => String::new(),
    };
    let scaled = !scale.is_empty();
    let (whole, fraction) = amount.split_once('.').unwrap_or((&amount, ""));
    let whole_value: usize = whole.replace(',', "").parse().unwrap_or(0);
    if scaled || fraction.len() != 2 || cent.is_empty() {
        // "$80 million", "$1.5 billion", "$22,750".
        tokens.push(Token::Number { digits: amount.clone(), ordinal: false });
        if scaled {
            tokens.push(Token::Word(scale));
        }
        let single = !scaled && amount == "1";
        tokens.push(Token::Expanded(vec![if single { one } else { many }]));
        return if scaled { after_scale } else { end };
    }
    let cents_value: usize = fraction.parse().unwrap_or(0);
    if whole_value > 0 || cents_value == 0 {
        tokens.push(Token::Number { digits: whole.to_string(), ordinal: false });
        tokens.push(Token::Expanded(vec![if whole_value == 1 { one } else { many }]));
    }
    if cents_value > 0 {
        if whole_value > 0 {
            tokens.push(Token::Word("and".to_string()));
        }
        tokens.push(Token::Expanded(small(cents_value)));
        tokens.push(Token::Expanded(vec![if cents_value == 1 { cent } else { cents }]));
    }
    end
}

#[cfg(test)]
mod tests {
    use crate::text::{Token, tokenize};
    use crate::utterance::Boundary;

    /// The tokens as plain text: words, numbers ("5th" for ordinals),
    /// spelled letters in capitals, and commas for pauses.
    fn read(text: &str) -> String {
        let parts: Vec<String> = tokenize(text)
            .into_iter()
            .map(|t| match t {
                Token::Expanded(words) => words.join(" "),
                Token::Number { digits, ordinal: true } => format!("{digits}th"),
                Token::Number { digits, .. } | Token::Word(digits) | Token::Spelled(digits) => digits,
                Token::Boundary(Boundary::Comma) => ",".to_string(),
                Token::Boundary(_) => ".".to_string(),
                other => format!("{other:?}"),
            })
            .collect();
        parts.join(" ")
    }

    #[test]
    fn symbols_beside_numbers() {
        assert_eq!(read("$5M, $10bn and €3k"), "5 million dollars , 10 billion dollars and 3 thousand euros");
        assert_eq!(read("US$200 and A$7"), "200 U.S. dollars and 7 Australian dollars");
        assert_eq!(read("7½ and ¾"), "7 and a half and three quarters");
        assert_eq!(read("-2 and 20°C"), "minus 2 and 20 degrees Celsius");
        assert_eq!(read("1984's and # 3"), "1984 's and number 3");
        assert_eq!(read("1pm - 4pm"), "1 PM to 4 PM");
    }

    #[test]
    fn times() {
        assert_eq!(read("3:00 pm"), "three PM");
        assert_eq!(read("at 9:45"), "at nine forty five");
        assert_eq!(read("10:05"), "ten oh five");
        assert_eq!(read("7:00"), "seven o'clock");
        assert_eq!(read("8pm"), "8 PM");
    }

    #[test]
    fn decades_and_ranges() {
        assert_eq!(read("the 1990s"), "the nineteen nineties");
        assert_eq!(read("the 2000s and 80s"), "the two thousands and eighties");
        assert_eq!(read("1990-1995"), "1990 to 1995");
        assert_eq!(read("pages 93 - 102"), "pages 93 to 102");
        assert_eq!(read("a 5-year-old"), "a 5 year old");
    }

    #[test]
    fn digit_groups_dates_and_fractions() {
        assert_eq!(read("555-1234"), "five five five , one two three four");
        assert_eq!(read("2014-03-05"), "march 5th , 2014");
        assert_eq!(read("3/5/2014"), "march 5th , 2014");
        assert_eq!(read("3/4 cup"), "three quarters cup");
        assert_eq!(read("1/2"), "one half");
        assert_eq!(read("24/7"), "24 7");
    }

    #[test]
    fn money_and_symbols() {
        assert_eq!(read("$22,750"), "22,750 dollars");
        assert_eq!(read("$1"), "1 dollar");
        assert_eq!(read("$3.50"), "3 dollars and fifty cents");
        assert_eq!(read("$0.99"), "ninety nine cents");
        assert_eq!(read("$80 million"), "80 million dollars");
        assert_eq!(read("£3.4 million"), "3.4 million pounds");
        assert_eq!(read("Tom & Jerry, #5, 60%, .267"), "Tom and Jerry , number 5 , 60 percent , .267");
    }

    #[test]
    fn numbers_at_the_end_of_the_text() {
        // Looking past the end must stop: this once looped forever.
        assert_eq!(read("in 2024"), "in 2024");
        assert_eq!(read("5 -"), "5");
        assert_eq!(read("$"), "");
    }
}
