//! Text files: reading them whatever their encoding, and saving them.

use std::path::Path;

/// What Windows-1252 has in place of the control characters 0x80 to 0x9F, the only
/// bytes where it differs from Latin-1. Unused bytes stay as they are.
const WINDOWS_1252: [char; 32] = [
    '\u{20AC}', '\u{81}', '\u{201A}', '\u{192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{2C6}', '\u{2030}',
    '\u{160}', '\u{2039}', '\u{152}', '\u{8D}', '\u{17D}', '\u{8F}', '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}',
    '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{2DC}', '\u{2122}', '\u{161}', '\u{203A}', '\u{153}', '\u{9D}',
    '\u{17E}', '\u{178}',
];

/// Reads a text file's bytes: UTF-16 or UTF-8 if it starts by saying so, UTF-8 if
/// it is valid as that, and otherwise Windows-1252, as older files often are.
pub fn decode(bytes: &[u8]) -> String {
    let utf16 = |rest: &[u8], unit: fn([u8; 2]) -> u16| {
        let units: Vec<u16> = rest.as_chunks::<2>().0.iter().map(|&pair| unit(pair)).collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest, u16::from_le_bytes);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(rest, u16::from_be_bytes);
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes
            .iter()
            .map(|&byte| if (0x80..0xA0).contains(&byte) { WINDOWS_1252[byte as usize - 0x80] } else { byte as char })
            .collect(),
    }
}

/// Gives text the line endings a Windows edit control needs.
pub fn with_windows_lines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n").replace('\n', "\r\n")
}

/// The window's title for a document.
pub fn title(path: Option<&Path>, modified: bool) -> String {
    let star = if modified { "*" } else { "" };
    format!("{star}{} - KlattPad", name(path))
}

/// A document's name, for showing to the user.
pub fn name(path: Option<&Path>) -> String {
    path.and_then(Path::file_name).map_or_else(|| "Untitled".to_string(), |name| name.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_encoding_it_should() {
        let text = "caf\u{E9} \u{2014} na\u{EF}ve";
        let utf16 = |unit: fn(u16) -> [u8; 2]| text.encode_utf16().flat_map(unit).collect::<Vec<u8>>();
        assert_eq!(decode(text.as_bytes()), text);
        assert_eq!(decode(&[&[0xEF, 0xBB, 0xBF], text.as_bytes()].concat()), text);
        assert_eq!(decode(&[&[0xFF, 0xFE], &utf16(u16::to_le_bytes)[..]].concat()), text);
        assert_eq!(decode(&[&[0xFE, 0xFF], &utf16(u16::to_be_bytes)[..]].concat()), text);
    }

    #[test]
    fn older_files_are_read_as_windows_1252() {
        assert_eq!(decode(b"caf\xE9 \x97 \x93quote\x94 \x80"), "caf\u{E9} \u{2014} \u{201C}quote\u{201D} \u{20AC}");
    }

    #[test]
    fn an_empty_file_is_empty() {
        assert_eq!(decode(b""), "");
        assert_eq!(decode(&[0xFF, 0xFE]), "");
    }

    #[test]
    fn line_endings_become_the_windows_kind() {
        assert_eq!(with_windows_lines("a\nb\r\nc\rd"), "a\r\nb\r\nc\r\nd");
    }

    #[test]
    fn the_title_names_the_document_and_marks_changes() {
        assert_eq!(title(None, false), "Untitled - KlattPad");
        assert_eq!(title(None, true), "*Untitled - KlattPad");
        assert_eq!(title(Some(Path::new("folder/song.txt")), false), "song.txt - KlattPad");
    }
}
