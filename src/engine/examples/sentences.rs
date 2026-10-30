//! Renders demonstration sentences from ordinary text, some at fast rates.
//!
//! Usage: cargo run --release --example sentences [output folder]

use klattara::{Voice, speak_text, wav};

const SAMPLE_RATE: u32 = 22050;

const PARAGRAPH: &str = "Screen readers speak quickly, so every sound has to stay clear. \
    The quick brown fox jumps over the lazy dog. Is this easy to follow? \
    Tell me which words are hard to understand.";

const SENTENCES: [(&str, &str); 8] = [
    ("07_hello_world", "Hello world."),
    // Klatt's (1979) example sentence.
    ("08_old_man", "The old man sat in a rocker."),
    // Harvard sentences, used in intelligibility testing.
    ("09_birch_canoe", "The birch canoe slid on the smooth planks."),
    ("10_glue_the_sheet", "Glue the sheet to the dark blue background."),
    ("11_depth_of_a_well", "It's easy to tell the depth of a well."),
    ("12_quick_brown_fox", "The quick brown fox jumps over the lazy dog."),
    ("13_punctuation", "Is it raining? Yes, it is! Bring an umbrella."),
    ("14_numbers", "On May 3rd, 1999, NVDA's 12 users read 1,205 pages."),
];

fn main() -> std::io::Result<()> {
    let folder = std::env::args().nth(1).unwrap_or_else(|| "samples".to_string());
    std::fs::create_dir_all(&folder)?;
    for (name, text) in SENTENCES {
        let audio = speak_text(text, &Voice::default(), SAMPLE_RATE);
        let path = std::path::Path::new(&folder).join(format!("{name}.wav"));
        wav::write_wav(&path, &audio, SAMPLE_RATE)?;
        println!("{}: {:.2} s", path.display(), audio.len() as f32 / SAMPLE_RATE as f32);
    }
    // The paragraph at a ladder of rates, each file named by its measured
    // speed: words per minute from the first sound to the last.
    let words = PARAGRAPH.split_whitespace().count() as f32;
    for (n, rate) in [1.0, 1.66, 2.2, 2.8, 3.5, 4.2, 5.0].into_iter().enumerate() {
        let audio = speak_text(PARAGRAPH, &Voice { rate, ..Voice::default() }, SAMPLE_RATE);
        let first = audio.iter().position(|x| x.abs() > 0.001).unwrap_or(0);
        let last = audio.iter().rposition(|x| x.abs() > 0.001).unwrap_or(audio.len());
        let wpm = words / ((last - first) as f32 / SAMPLE_RATE as f32) * 60.0;
        let path = std::path::Path::new(&folder).join(format!("{}_paragraph_{:.0}wpm.wav", 15 + n, wpm.round()));
        wav::write_wav(&path, &audio, SAMPLE_RATE)?;
        println!("{}: rate {rate}, {wpm:.0} words per minute", path.display());
    }
    Ok(())
}
