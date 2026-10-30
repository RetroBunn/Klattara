//! Measures a letter-to-sound model on held-out dictionary words.
//!
//! Usage: cargo run --release --example lts_eval -- target/lts-eval [guesses.tsv]
//!
//! The folder holds model.bin and heldout.tsv, both written by
//! `src/tools/train_lts.py --evaluate`. With a second argument, each word's
//! guess is written there, for looking at the errors.

use klattara::{JointModel, Stress};

fn arpabet(word: &[(klattara::Phoneme, Stress)]) -> Vec<String> {
    word.iter()
        .map(|&(p, s)| match (p.is_vowel(), s) {
            (false, _) => format!("{p:?}"),
            (true, Stress::Unstressed) => format!("{p:?}0"),
            (true, Stress::Primary) => format!("{p:?}1"),
            (true, Stress::Secondary) => format!("{p:?}2"),
        })
        .collect()
}

fn strip(phones: &[String]) -> Vec<String> {
    phones.iter().map(|p| p.trim_end_matches(['0', '1', '2']).to_string()).collect()
}

fn edit_distance(a: &[String], b: &[String]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for x in a {
        let mut previous = row[0];
        row[0] += 1;
        for (j, y) in b.iter().enumerate() {
            let substitute = previous + usize::from(x != y);
            previous = row[j + 1];
            row[j + 1] = (row[j + 1] + 1).min(row[j] + 1).min(substitute);
        }
    }
    row[b.len()]
}

fn main() {
    let folder = std::env::args().nth(1).unwrap_or_else(|| "target/lts-eval".to_string());
    let folder = std::path::Path::new(&folder);
    let bytes = std::fs::read(folder.join("model.bin")).expect("model.bin");
    let endings = std::fs::read_to_string(folder.join("stress_endings.tsv")).unwrap_or_default();
    let model = JointModel::parse(&bytes).with_stress_endings(&endings);
    let list = std::fs::read_to_string(folder.join("heldout.tsv")).expect("heldout.tsv");
    let mut guesses = String::new();
    let started = std::time::Instant::now();
    let (mut words, mut exact, mut any, mut no_stress, mut right, mut total) = (0, 0, 0, 0, 0, 0);
    for line in list.lines() {
        let Some((word, listed)) = line.split_once('\t') else { continue };
        let variants: Vec<Vec<String>> =
            listed.split(" | ").map(|v| v.split_whitespace().map(String::from).collect()).collect();
        let guess = arpabet(&model.predict(word).unwrap_or_default());
        guesses.push_str(&format!("{word}\t{}\n", guess.join(" ")));
        words += 1;
        exact += usize::from(guess == variants[0]);
        any += usize::from(variants.contains(&guess));
        no_stress += usize::from(variants.iter().any(|v| strip(v) == strip(&guess)));
        right += variants[0].len().saturating_sub(edit_distance(&guess, &variants[0]));
        total += variants[0].len();
    }
    if let Some(path) = std::env::args().nth(2) {
        std::fs::write(path, guesses).expect("guesses");
    }
    let percent = |n: usize, of: usize| 100.0 * n as f64 / of as f64;
    println!("{words} held-out words, {:.1} ms per word", started.elapsed().as_secs_f64() * 1000.0 / words as f64);
    println!("  exactly right, with stress: {:.1}%", percent(exact, words));
    println!("  matching any listed pronunciation, with stress: {:.1}%", percent(any, words));
    println!("  matching any listed pronunciation, ignoring stress: {:.1}%", percent(no_stress, words));
    println!("  phonemes right: {:.1}%", percent(right, total));
}
