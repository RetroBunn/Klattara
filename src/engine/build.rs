//! Compiles the CMU Pronouncing Dictionary into a compact binary table that
//! the engine embeds, so nothing is parsed at startup.
//!
//! Layout, little-endian: entry count (u32); one u32 offset per entry into
//! the entry area; then the entries, sorted by word and variant, each as
//! word length (u8), word bytes, variant number (u8), phoneme count (u8),
//! and one byte per phoneme (see src/arpabet.rs).

use std::path::Path;
use std::{env, fs};

#[path = "src/arpabet.rs"]
mod arpabet;

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let source = Path::new(&manifest).join("../data/cmudict/cmudict.dict");
    println!("cargo::rerun-if-changed={}", source.display());
    println!("cargo::rerun-if-changed=src/arpabet.rs");

    let text = fs::read_to_string(&source).unwrap_or_else(|e| panic!("cannot read {}: {e}", source.display()));
    let mut entries: Vec<(String, u8, Vec<u8>)> = Vec::new();
    for (number, line) in text.lines().enumerate() {
        // Some entries end in a comment, such as "# place, danish".
        let line = line.split('#').next().unwrap_or("").trim();
        let mut parts = line.split_whitespace();
        let Some(head) = parts.next() else { continue };
        let (word, variant) = match head.find('(') {
            Some(open) => (&head[..open], head[open + 1..head.len() - 1].parse().expect("variant number")),
            None => (head, 1),
        };
        let phonemes: Vec<u8> = parts.map(|symbol| encode(symbol, number + 1)).collect();
        assert!(word.len() < 256 && phonemes.len() < 256, "line {} is too long", number + 1);
        entries.push((word.to_string(), variant, phonemes));
    }
    entries.sort();

    let mut offsets = Vec::with_capacity(entries.len());
    let mut body = Vec::new();
    for (word, variant, phonemes) in &entries {
        offsets.push(body.len() as u32);
        body.push(word.len() as u8);
        body.extend_from_slice(word.as_bytes());
        body.push(*variant);
        body.push(phonemes.len() as u8);
        body.extend_from_slice(phonemes);
    }
    let mut out = Vec::with_capacity(4 + 4 * offsets.len() + body.len());
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for offset in offsets {
        out.extend_from_slice(&offset.to_le_bytes());
    }
    out.extend_from_slice(&body);
    let target = Path::new(&env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("cmudict.bin");
    fs::write(&target, out).unwrap_or_else(|e| panic!("cannot write {}: {e}", target.display()));
}

fn encode(symbol: &str, line: usize) -> u8 {
    let (name, stress) = match symbol.as_bytes().last() {
        Some(&digit @ b'0'..=b'2') => (&symbol[..symbol.len() - 1], digit - b'0' + 1),
        _ => (symbol, 0),
    };
    let index = arpabet::SYMBOLS
        .iter()
        .position(|s| *s == name)
        .unwrap_or_else(|| panic!("unknown phoneme {symbol} on line {line}"));
    index as u8 | (stress << arpabet::STRESS_SHIFT)
}
