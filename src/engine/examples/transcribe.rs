//! Prints how each line of standard input will be pronounced, in bracketed
//! ARPAbet, one output line per input line.
//!
//! Usage: cargo run --release --example transcribe [-- [--as-written] [--phrase-prediction]] < sentences.txt

use std::io::{BufRead, Write};

use klattara::{Voice, transcribe_with};

fn main() {
    let flag = |name: &str| std::env::args().skip(1).any(|a| a == name);
    let voice = Voice {
        expand_abbreviations: !flag("--as-written"),
        phrase_prediction: flag("--phrase-prediction"),
        ..Voice::default()
    };
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    for line in std::io::stdin().lock().lines() {
        let line = line.expect("could not read standard input");
        writeln!(out, "{}", transcribe_with(&line, &voice)).expect("could not write standard output");
    }
}
