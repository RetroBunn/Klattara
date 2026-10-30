//! Renders demonstration WAV files: sustained vowels, three voice qualities,
//! diphthong glides, and pitch contours.
//!
//! Usage: cargo run --release --example vowels [output folder]

use klattara::{Frame, Synth, wav};

const SAMPLE_RATE: u32 = 22050;

/// American English vowel formants (F1, F2, F3) for adult men, from the
/// averages in Peterson & Barney (1952), keyed by an example word.
const HEED: [f64; 3] = [270.0, 2290.0, 3010.0];
const HID: [f64; 3] = [390.0, 1990.0, 2550.0];
const HEAD: [f64; 3] = [530.0, 1840.0, 2480.0];
const HAD: [f64; 3] = [660.0, 1720.0, 2410.0];
const HOD: [f64; 3] = [730.0, 1090.0, 2440.0];
const HAWED: [f64; 3] = [570.0, 840.0, 2410.0];
const HOOD: [f64; 3] = [440.0, 1020.0, 2240.0];
const WHOD: [f64; 3] = [300.0, 870.0, 2240.0];
const HUD: [f64; 3] = [640.0, 1190.0, 2390.0];
const HEARD: [f64; 3] = [490.0, 1350.0, 1690.0];

/// Voice-quality settings of the glottal source.
#[derive(Clone, Copy)]
struct Quality {
    oq: f64,
    tl: f64,
    ah: f64,
}

const BRIGHT: Quality = Quality { oq: 0.40, tl: 0.0, ah: 0.0 };
const MEDIUM: Quality = Quality { oq: 0.55, tl: 6.0, ah: 0.0 };
const SOFT: Quality = Quality { oq: 0.70, tl: 14.0, ah: 50.0 };

/// A voiced vowel frame.
fn vowel(formants: [f64; 3], f0: f64, quality: Quality) -> Frame {
    let mut frame = Frame { f0, av: 60.0, oq: quality.oq, tl: quality.tl, ah: quality.ah, ..Frame::default() };
    frame.freq[..3].copy_from_slice(&formants);
    frame.bw[..3].copy_from_slice(&[70.0, 100.0, 150.0]);
    frame
}

/// The same configuration with the sound turned off, so that fading in or
/// out changes only loudness.
fn silent(frame: Frame) -> Frame {
    Frame { av: 0.0, ah: 0.0, ..frame }
}

/// A track is a list of (milliseconds, frame): glide to `frame` over that
/// many milliseconds. A duration of 0 jumps straight to the frame.
type Track = Vec<(f64, Frame)>;

/// Wraps voiced targets in a 40 ms onset, a 60 ms fade-out and a pause.
fn utterance(targets: &[(f64, Frame)]) -> Track {
    let first = targets[0].1;
    let last = targets[targets.len() - 1].1;
    let mut track = vec![(0.0, silent(first)), (40.0, first)];
    track.extend_from_slice(targets);
    track.push((60.0, silent(last)));
    track.push((250.0, silent(last)));
    track
}

/// A sustained vowel with a gentle statement-like pitch fall.
fn sustained(formants: [f64; 3], quality: Quality) -> Track {
    utterance(&[
        (0.0, vowel(formants, 115.0, quality)),
        (100.0, vowel(formants, 125.0, quality)),
        (260.0, vowel(formants, 100.0, quality)),
    ])
}

/// A diphthong: hold the first vowel, glide to the second, hold briefly.
fn diphthong(from: [f64; 3], to: [f64; 3]) -> Track {
    utterance(&[
        (0.0, vowel(from, 120.0, MEDIUM)),
        (80.0, vowel(from, 122.0, MEDIUM)),
        (170.0, vowel(to, 106.0, MEDIUM)),
        (60.0, vowel(to, 100.0, MEDIUM)),
    ])
}

fn render(synth: &mut Synth, tracks: &[Track]) -> Vec<f32> {
    synth.reset();
    let mut out = Vec::new();
    for &(ms, frame) in tracks.iter().flatten() {
        let start = out.len();
        out.resize(start + (ms * SAMPLE_RATE as f64 / 1000.0).round() as usize, 0.0);
        synth.render(&frame, &mut out[start..]);
    }
    out
}

fn main() -> std::io::Result<()> {
    let folder = std::env::args().nth(1).unwrap_or_else(|| "samples".to_string());
    std::fs::create_dir_all(&folder)?;

    let all_vowels = [HEED, HID, HEAD, HAD, HOD, HAWED, HOOD, WHOD, HUD, HEARD];
    let quality_vowels = [HOD, HEED, WHOD, HAD];
    let pitch_ah = |f0: f64, av: f64| Frame { av, ..vowel(HOD, f0, MEDIUM) };

    let files: Vec<(&str, Vec<Track>)> = vec![
        ("01_vowels", all_vowels.iter().map(|&v| sustained(v, MEDIUM)).collect()),
        ("02_quality_bright", quality_vowels.iter().map(|&v| sustained(v, BRIGHT)).collect()),
        ("03_quality_medium", quality_vowels.iter().map(|&v| sustained(v, MEDIUM)).collect()),
        ("04_quality_soft", quality_vowels.iter().map(|&v| sustained(v, SOFT)).collect()),
        ("05_diphthongs", vec![diphthong(HOD, HID), diphthong(HOD, HOOD), diphthong(HAWED, HID), diphthong(HEAD, HID)]),
        (
            "06_pitch",
            vec![
                // Statement: small rise, then a full fall.
                utterance(&[
                    (0.0, pitch_ah(120.0, 60.0)),
                    (120.0, pitch_ah(135.0, 60.0)),
                    (330.0, pitch_ah(90.0, 60.0)),
                ]),
                // Yes/no question: low and level, then a rise at the end.
                utterance(&[
                    (0.0, pitch_ah(110.0, 60.0)),
                    (200.0, pitch_ah(105.0, 60.0)),
                    (250.0, pitch_ah(165.0, 60.0)),
                ]),
                // Exclamation: high peak, louder, steep fall.
                utterance(&[
                    (0.0, pitch_ah(130.0, 63.0)),
                    (120.0, pitch_ah(180.0, 63.0)),
                    (280.0, pitch_ah(85.0, 60.0)),
                ]),
            ],
        ),
    ];

    let mut synth = Synth::new(SAMPLE_RATE);
    for (name, tracks) in &files {
        let audio = render(&mut synth, tracks);
        let peak = audio.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        let rms = (audio.iter().map(|x| x * x).sum::<f32>() / audio.len() as f32).sqrt();
        let path = std::path::Path::new(&folder).join(format!("{name}.wav"));
        wav::write_wav(&path, &audio, SAMPLE_RATE)?;
        println!(
            "{}: {:.1} s, peak {:.1} dBFS, RMS {:.1} dBFS",
            path.display(),
            audio.len() as f32 / SAMPLE_RATE as f32,
            20.0 * peak.log10(),
            20.0 * rms.log10()
        );
    }
    Ok(())
}
