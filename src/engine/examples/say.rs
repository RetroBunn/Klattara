//! Speaks text, or bracketed ARPAbet input, to a WAV file.
//!
//! Usage: cargo run --release --example say -- [--as-written] [--phrase-prediction] "Hello, world." [output.wav] [rate]
//!        cargo run --release --example say -- "[HH AH0 L OW1] [W ER1 L D]." [output.wav] [rate]
//!
//! For text, the pronunciation used is printed first. Rate 1.0 is about 180
//! words per minute; 2.0 is twice as fast. With --as-written, abbreviations
//! are read as written instead of in full. With --phrase-prediction, long
//! stretches without punctuation are broken into phrases.

use klattara::{Voice, speak_phonemes, speak_text, transcribe_with, wav};

const SAMPLE_RATE: u32 = 22050;

fn main() {
    let (flags, args): (Vec<String>, Vec<String>) = std::env::args().skip(1).partition(|a| a.starts_with("--"));
    let flag = |name: &str| flags.iter().any(|f| f == name);
    let mut args = args.into_iter();
    let Some(input) = args.next() else {
        eprintln!("usage: say [--as-written] [--phrase-prediction] \"Hello, world.\" [output.wav] [rate]");
        std::process::exit(2);
    };
    let output = args.next().unwrap_or_else(|| "samples/say.wav".to_string());
    let rate = args.next().map_or(1.0, |r| r.parse().expect("rate must be a number"));
    let voice = Voice {
        rate,
        expand_abbreviations: !flag("--as-written"),
        phrase_prediction: flag("--phrase-prediction"),
        ..Voice::default()
    };
    let audio = if input.trim_start().starts_with('[') {
        speak_phonemes(&input, &voice, SAMPLE_RATE).unwrap_or_else(|error| {
            eprintln!("error: {error}");
            std::process::exit(1);
        })
    } else {
        println!("{}", transcribe_with(&input, &voice));
        speak_text(&input, &voice, SAMPLE_RATE)
    };
    if let Some(folder) = std::path::Path::new(&output).parent() {
        std::fs::create_dir_all(folder).expect("could not create the output folder");
    }
    wav::write_wav(&output, &audio, SAMPLE_RATE).expect("could not write the WAV file");
    println!("{output}: {:.2} s", audio.len() as f64 / SAMPLE_RATE as f64);
}
