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

/// Whether "am" or "pm" comes after `i`, as a word or with dots: "1.40 pm",
/// "1.40 p.m.".
fn am_pm_follows(chars: &[char], i: usize) -> bool {
    let (word, end) = word_after(chars, i);
    let at = |k: usize| chars.get(k).copied().unwrap_or(' ');
    match word.to_lowercase().as_str() {
        "am" | "pm" => true,
        "a" | "p" => at(end) == '.' && matches!(at(end + 1), 'm' | 'M'),
        _ => false,
    }
}

/// Says a time, with the "am" or "pm" after it, and "o'clock" on the hour if
/// there is neither: "nine forty five", "three PM", "seven o'clock".
fn time(hour: usize, minute: usize, chars: &[char], after: usize, tokens: &mut Vec<Token>) -> usize {
    let mut words = small(hour);
    match minute {
        0 => {}
        1..=9 => words.extend(["oh", ONES[minute]]),
        _ => words.extend(small(minute)),
    }
    tokens.push(Token::Expanded(words));
    if minute == 0 && !am_pm_follows(chars, after) {
        tokens.push(Token::Expanded(vec!["o'clock"]));
    }
    am_pm(chars, after, tokens)
}

/// A date written in numbers, read in American order. The order as written
/// is taken if it can be a date, and the other if only that one can:
/// "25/08/2014" is August twenty fifth.
fn date_either_way(first: &str, second: &str, year: &str, day_first: bool) -> Option<Vec<Token>> {
    if day_first {
        date(second, first, year).or_else(|| date(first, second, year))
    } else {
        date(first, second, year).or_else(|| date(second, first, year))
    }
}

/// Whether a year written with hyphens or dots looks like a year: groups like
/// "10-12-3456" are more likely part numbers, and "2.1.1040" a version.
fn likely_year(year: &str) -> bool {
    year.len() == 4 && plain(year).is_some_and(|y| (1800..=2199).contains(&y))
}

/// Whether a quote mark at `i` would close a quotation opened earlier in the
/// sentence, as in: she said "6". An apostrophe inside a word opens nothing.
fn closes_quote(chars: &[char], i: usize) -> bool {
    let mark = chars[i];
    let start = (0..i)
        .rev()
        .take(300)
        .find(|&k| matches!(chars[k], '.' | '!' | '?') && chars.get(k + 1).is_some_and(|c| c.is_whitespace()))
        .map_or(i.saturating_sub(300), |k| k + 1);
    let opening = |k: usize| chars[k] == mark && (mark == '"' || k == 0 || !chars[k - 1].is_alphanumeric());
    (start..i).filter(|&k| opening(k)).count() % 2 == 1
}

/// Feet and inches after a number: 6' 1", 3', 12". After degrees, the same
/// marks are minutes and seconds. `None` if the mark is a quote or apostrophe.
fn feet_and_inches(chars: &[char], first: &str, end: usize, tokens: &mut Vec<Token>) -> Option<usize> {
    let at = |k: usize| chars.get(k).copied().unwrap_or(' ');
    let angle = matches!(tokens.last(), Some(Token::Expanded(words)) if words[0].starts_with("degree"));
    let (one, many, one_small, many_small) =
        if angle { ("minute", "minutes", "second", "seconds") } else { ("foot", "feet", "inch", "inches") };
    // Before a noun the unit stays singular: "a 12 inch pizza".
    let before_noun = matches!(tokens.last(), Some(Token::Word(word))
        if matches!(word.to_lowercase().as_str(), "a" | "an" | "the"));
    let say = |digits: &str, one: &'static str, many: &'static str, tokens: &mut Vec<Token>| {
        tokens.push(Token::Number { digits: digits.to_string(), ordinal: false });
        tokens.push(Token::Expanded(vec![if digits == "1" || before_noun { one } else { many }]));
    };
    let straight = |k: usize, mark: char| at(k) == mark && !at(k + 1).is_alphabetic() && !closes_quote(chars, k);
    if at(end) == '\u{2032}' || straight(end, '\'') {
        say(first, one, many, tokens);
        let mut k = end + 1;
        while matches!(at(k), ' ' | '-') {
            k += 1;
        }
        if at(k).is_ascii_digit() {
            let (inches, inches_end) = digits_at(chars, k);
            if matches!(at(inches_end), '"' | '\u{201d}' | '\u{2033}') {
                say(&inches, one_small, many_small, tokens);
                return Some(inches_end + 1);
            }
        }
        return Some(end + 1);
    }
    if at(end) == '\u{2033}' || straight(end, '"') {
        say(first, one_small, many_small, tokens);
        return Some(end + 1);
    }
    None
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
            return time(hour, minute, chars, end + 3, tokens);
        }
    }

    // Only "am" or "pm" tells a time with a dot from a decimal: "1.40 pm".
    if let Some((hour, minute)) = first.split_once('.')
        && minute.len() == 2
        && let (Some(hour), Some(minute)) = (plain(hour).filter(|h| (1..=12).contains(h)), plain(minute))
        && minute <= 59
        && am_pm_follows(chars, end)
    {
        return time(hour, minute, chars, end, tokens);
    }

    // A date with dots: "25.08.2014". They are written day first.
    if let [day, month, year] = first.split('.').collect::<Vec<_>>()[..]
        && likely_year(year)
        && let Some(date) = date_either_way(day, month, year, true)
    {
        tokens.extend(date);
        return end;
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
        // A date with hyphens: "08-25-2015", "25-08-2015".
        if let [first, second, year] = &groups[..]
            && first.len() <= 2
            && second.len() <= 2
            && likely_year(year)
            && let Some(date) = date_either_way(first, second, year, false)
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
            if let Some(date) = date_either_way(&first, &second, &third, false) {
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

    if let Some(after) = feet_and_inches(chars, &first, end, tokens) {
        return after;
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
    fn times_with_a_dot_need_am_or_pm() {
        assert_eq!(read("1.40 pm"), "one forty PM");
        assert_eq!(read("1.40pm"), "one forty PM");
        assert_eq!(read("11.05 am"), "eleven oh five AM");
        assert_eq!(read("3.00 pm"), "three PM");
        assert_eq!(read("9.30am - 5.30pm"), "nine thirty AM to five thirty PM");
        // Dotted, the a.m. is read after it, with no "o'clock" before it.
        assert!(read("11.05 a.m.").starts_with("eleven oh five "), "{}", read("11.05 a.m."));
        assert!(!read("3:00 p.m.").contains("o'clock"), "{}", read("3:00 p.m."));
        // Otherwise it is a decimal, and these can't be times.
        assert_eq!(read("1.40"), "1.40");
        assert_eq!(read("13.40 pm"), "13.40 pm");
        assert_eq!(read("1.75 pm"), "1.75 pm");
        assert_eq!(read("1.4 pm"), "1.4 pm");
    }

    #[test]
    fn dates_in_either_order_and_with_dots() {
        // Month first, as American dates are, unless that can't be a date.
        assert_eq!(read("08-25-2015"), "august 25th , 2015");
        assert_eq!(read("25-08-2015"), "august 25th , 2015");
        assert_eq!(read("03-04-2015"), "march 4th , 2015");
        assert_eq!(read("25/08/2014"), "august 25th , 2014");
        assert_eq!(read("10/04/2014"), "october 4th , 2014");
        // Dates with dots are written day first.
        assert_eq!(read("10.04.2014"), "april 10th , 2014");
        assert_eq!(read("25.08.2014"), "august 25th , 2014");
        assert_eq!(read("12.25.2014"), "december 25th , 2014");
        // Not dates: too many digits, no such month, or no likely year.
        assert_eq!(read("1.2.3"), "1.2.3");
        assert_eq!(read("192.168.1.1"), "192.168.1.1");
        assert_eq!(read("2.1.1040"), "2.1.1040");
        assert_eq!(read("25.13.2014"), "25.13.2014");
        assert_eq!(read("10-12-3456"), "one zero , one two , three four five six");
        assert_eq!(read("555-12-2014"), "five five five , one two , two zero one four");
    }

    #[test]
    fn feet_and_inches() {
        assert_eq!(read("6' 1\""), "6 feet 1 inch");
        assert_eq!(read("6'1\""), "6 feet 1 inch");
        assert_eq!(read("5'-10\" tall"), "5 feet 10 inches tall");
        assert_eq!(read("6\u{2019} 1\u{201d}"), "6 feet 1 inch");
        assert_eq!(read("5\u{2032} 10\u{2033}"), "5 feet 10 inches");
        assert_eq!(read("3' long"), "3 feet long");
        assert_eq!(read("1' long"), "1 foot long");
        assert_eq!(read("12\" wide"), "12 inches wide");
        // Before a noun the unit stays singular.
        assert_eq!(read("a 3' board"), "a 3 foot board");
        assert_eq!(read("a 12\" pizza"), "a 12 inch pizza");
        assert_eq!(read("the 6' 2\" man"), "the 6 foot 2 inch man");
        // After degrees, minutes and seconds.
        assert_eq!(read("40\u{b0} 26' 46\""), "40 degrees 26 minutes 46 seconds");
        // Quote marks and apostrophes around a number stay what they are.
        assert_eq!(read("she said \"6\" and left"), "she said 6 and left");
        assert_eq!(read("'6' and"), "6 and");
        assert_eq!(read("1984's"), "1984 's");
        assert_eq!(read("the '90s"), "the nineties");
        assert_eq!(read("it's 5' long"), "it 's 5 feet long".replace("it 's", "it's"));
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
