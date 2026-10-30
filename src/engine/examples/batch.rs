//! Renders every line of a test-sentence file to WAV files at one rate.
//!
//! Usage: cargo run --release --example batch -- sentences.tsv output-folder [rate]
//!
//! Each line of the file is an id and a text, separated by a tab, optionally
//! followed by an ARPAbet transcription to use instead of the text. Lines
//! starting with # are comments. The voice is the default, or the one named by
//! the KLATTARA_VOICE environment variable.

use klattara::{Voice, find_voice, speak_phonemes, speak_text, wav};

const SAMPLE_RATE: u32 = 22050;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [sentences, folder, rest @ ..] = args.as_slice() else {
        eprintln!("usage: batch sentences.tsv output-folder [rate]");
        std::process::exit(2);
    };
    let rate = rest.first().map_or(1.0, |r| r.parse().expect("rate must be a number"));
    let chosen = std::env::var("KLATTARA_VOICE")
        .map_or(Voice::default(), |name| Voice::from(find_voice(&name).expect("there is no voice of that name")));
    let voice = Voice { rate, ..chosen };
    std::fs::create_dir_all(folder).expect("could not create the output folder");
    let list = std::fs::read_to_string(sentences).expect("could not read the sentence file");
    for line in list.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')) {
        let fields: Vec<&str> = line.split('\t').collect();
        let audio = match fields[..] {
            [_, text] => speak_text(text, &voice, SAMPLE_RATE),
            [id, _, phonemes] => match speak_phonemes(phonemes, &voice, SAMPLE_RATE) {
                Ok(audio) => audio,
                Err(error) => {
                    eprintln!("{id}: {error}");
                    continue;
                }
            },
            _ => {
                eprintln!("skipping malformed line: {line}");
                continue;
            }
        };
        let path = std::path::Path::new(folder).join(format!("{}.wav", fields[0]));
        wav::write_wav(&path, &audio, SAMPLE_RATE).expect("could not write a WAV file");
    }
}
