//! Prints the synthesizer frames for a text: time, voicing, F1 to F3.
//!
//! Usage: cargo run --release --example formants -- "Say bit again." [rate]

use klattara::{Voice, text_frames};

fn main() {
    let mut args = std::env::args().skip(1);
    let text = args.next().expect("usage: formants \"text\" [rate]");
    let rate = args.next().map_or(1.0, |r| r.parse().expect("rate must be a number"));
    for (t, f) in text_frames(&text, &Voice { rate, ..Voice::default() }) {
        println!(
            "{t:7.1} av {:4.1} af {:4.1} ah {:4.1}  F {:5.0} {:5.0} {:5.0}",
            f.av, f.af, f.ah, f.freq[0], f.freq[1], f.freq[2]
        );
    }
}
