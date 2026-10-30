//! Pitch contours.
//!
//! A simplified target-and-interpolation model in the spirit of
//! Pierrehumbert (1981), "Synthesizing intonation": each intonational phrase
//! has a gently declining baseline, every primary-stressed vowel gets a pitch
//! accent that steps down a little from the one before, though never below
//! half height, and the phrase ends
//! with a contour chosen by its punctuation, and for a question by its
//! wording (see [`Boundary::WhQuestion`]). Pitch is computed in semitones
//! from the voice's base pitch, so the inflection setting scales every
//! movement evenly.

use crate::phoneme::Phoneme;
use crate::utterance::{Boundary, Segment};

/// Pitch-shape settings, in semitones above or below the base pitch.
const BASELINE_START: f64 = 1.0;
const BASELINE_END: f64 = -1.0;
const ACCENT_HEIGHT: f64 = 4.0;
/// Each later accent in a phrase is this fraction of the one before.
const DOWNSTEP: f64 = 0.85;
/// Accents step down no lower than this, half of `ACCENT_HEIGHT`, so the later
/// accents of a long phrase do not fade into a monotone.
const ACCENT_FLOOR: f64 = 2.0;
/// Exclamations: every accent is this much taller, the whole phrase sits
/// this many semitones higher, and the emphasized word's accent is taller
/// again by `EMPHASIS`.
const EXCLAMATION_BOOST: f64 = 1.6;
const EXCLAMATION_REGISTER: f64 = 2.0;
const EMPHASIS: f64 = 1.4;

/// Where each kind of phrase ends, relative to the base pitch.
fn final_target(boundary: Boundary) -> f64 {
    match boundary {
        Boundary::Period | Boundary::WhQuestion => -4.0,
        Boundary::Clause => -2.5,
        Boundary::Exclamation => -6.0,
        Boundary::Comma => 2.5,
        // A break without a pause falls a little, far less than a statement's
        // end, and the next phrase starts afresh. The user chose this over a
        // comma-like rise by ear (samples 44_breaks_fall and _rise).
        Boundary::Minor => -1.5,
        Boundary::Question => 7.0,
    }
}

/// Pitch keyframes (time in ms, F0 in Hz) for timed segments.
pub fn pitch_keyframes(segments: &[Segment], base_hz: f64, inflection: f64) -> Vec<(f64, f64)> {
    let to_hz = |semitones: f64| base_hz * 2f64.powf(semitones * inflection / 12.0);
    let mut keyframes = Vec::new();
    let mut i = 0;
    while i < segments.len() {
        if segments[i].phoneme == Phoneme::Pause {
            i += 1;
            continue;
        }
        let start = i;
        while i < segments.len()
            && segments[i].phoneme != Phoneme::Pause
            && segments[i].phrase == segments[start].phrase
        {
            i += 1;
        }
        // A phrase that ends without a pause ends at a break (`|`).
        let boundary = match segments.get(i) {
            Some(next) if next.phoneme != Phoneme::Pause => Boundary::Minor,
            next => next.and_then(|s| s.boundary).unwrap_or(Boundary::Period),
        };
        for (t, semitones) in phrase_contour(&segments[start..i], boundary) {
            keyframes.push((t, to_hz(semitones)));
        }
    }
    keyframes
}

/// The contour of one intonational phrase, in semitones.
fn phrase_contour(phrase: &[Segment], boundary: Boundary) -> Vec<(f64, f64)> {
    let begin = phrase[0].start;
    let end = phrase[phrase.len() - 1].end();
    let span = (end - begin).max(1.0);
    let register = if boundary == Boundary::Exclamation { EXCLAMATION_REGISTER } else { 0.0 };
    let baseline = |t: f64| register + BASELINE_START + (BASELINE_END - BASELINE_START) * (t - begin) / span;
    let rising_end = matches!(boundary, Boundary::Question | Boundary::Comma);

    let mut points = vec![(begin, baseline(begin) - 0.5)];
    let accents: Vec<&Segment> = phrase.iter().filter(|s| s.accented).collect();
    let boost = if boundary == Boundary::Exclamation { EXCLAMATION_BOOST } else { 1.0 };
    for (n, vowel) in accents.iter().enumerate() {
        let (s, d) = (vowel.start, vowel.duration);
        let emphasis = if vowel.emphasized { EMPHASIS } else { 1.0 };
        let height = (ACCENT_HEIGHT * DOWNSTEP.powi(n as i32)).max(ACCENT_FLOOR) * boost * emphasis;
        let nuclear = n == accents.len() - 1;
        if nuclear && rising_end {
            // A low accent, so the final rise starts from below the baseline.
            let dip = if boundary == Boundary::Question { 1.5 } else { 0.7 };
            points.push((s, baseline(s)));
            points.push((s + 0.5 * d, baseline(s + 0.5 * d) - dip));
        } else {
            let peak = s + 0.45 * d;
            points.push((s, baseline(s) + 0.2 * height));
            points.push((peak, baseline(peak) + height));
            if !nuclear {
                points.push((s + d, baseline(s + d) + 0.4 * height));
            }
        }
    }
    points.push((end, final_target(boundary)));
    // Keep times strictly increasing; a contour point never moves back in time.
    let mut last = f64::NEG_INFINITY;
    points.retain(|&(t, _)| {
        let keep = t > last && t <= end;
        if keep {
            last = t;
        }
        keep
    });
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timing::assign_durations;
    use crate::utterance::{parse, segments};

    fn contour(input: &str) -> Vec<(f64, f64)> {
        let mut segs = segments(&parse(input).unwrap());
        assign_durations(&mut segs, 1.0);
        pitch_keyframes(&segs, 110.0, 1.0)
    }

    #[test]
    fn statements_fall_and_questions_rise() {
        let statement = contour("[IH1 T] [R EY1 N Z].");
        let question = contour("[IH1 T] [R EY1 N Z]?");
        let last = |c: &[(f64, f64)]| c[c.len() - 1].1;
        assert!(last(&statement) < 90.0, "{statement:?}");
        assert!(last(&question) > 150.0, "{question:?}");
    }

    #[test]
    fn wh_questions_fall_like_statements() {
        let contour_as = |boundary: Boundary| {
            let mut phrases = parse("[W EH1 R] [ER0] [Y UW0] [G OW1 IH0 NG].").unwrap();
            phrases[0].boundary = boundary;
            let mut segs = segments(&phrases);
            assign_durations(&mut segs, 1.0);
            pitch_keyframes(&segs, 110.0, 1.0)
        };
        assert_eq!(contour_as(Boundary::WhQuestion), contour_as(Boundary::Period));
        assert!(contour_as(Boundary::Question).last().unwrap().1 > 150.0);
    }

    #[test]
    fn exclamations_reach_higher_and_fall_lower() {
        let peak = |c: &[(f64, f64)]| c.iter().map(|p| p.1).fold(0.0, f64::max);
        let statement = contour("[IH0 T] [R EY1 N Z] [AH0 G EH1 N].");
        let exclamation = contour("[IH0 T] [R EY1 N Z] [AH0 G EH1 N]!");
        // At least 4 semitones (26%) higher at the peak, and lower at the end.
        assert!(peak(&exclamation) > 1.26 * peak(&statement), "{statement:?} {exclamation:?}");
        assert!(exclamation.last().unwrap().1 < statement.last().unwrap().1);
    }

    #[test]
    fn accents_raise_pitch_above_the_start() {
        let c = contour("[M AE1 N].");
        let peak = c.iter().map(|p| p.1).fold(0.0, f64::max);
        assert!(peak > c[0].1 * 1.2);
    }

    #[test]
    fn long_phrases_keep_their_accents() {
        let words = ["[M AE1 N]"; 10].join(" ");
        let mut segs = segments(&parse(&format!("{words}.")).unwrap());
        assign_durations(&mut segs, 1.0);
        let c = pitch_keyframes(&segs, 110.0, 1.0);
        let semitones = |t: f64| 12.0 * (c.iter().find(|p| p.0 >= t - 1e-9).unwrap().1 / 110.0).log2();
        // How far each accent rises within its vowel.
        let rises: Vec<f64> = segs
            .iter()
            .filter(|s| s.accented)
            .map(|s| semitones(s.start + 0.45 * s.duration) - semitones(s.start))
            .collect();
        assert_eq!(rises.len(), 10);
        // The first accents step down; from the sixth on they stay at half height.
        assert!(rises[..6].windows(2).all(|w| w[1] < w[0]), "{rises:?}");
        assert!(rises[5..].iter().all(|&r| (r - rises[9]).abs() < 0.1 && r > 0.45 * rises[0]), "{rises:?}");
    }

    #[test]
    fn keyframes_move_forward_in_time() {
        let c = contour("[DH AH0] [K W IH1 K] [B R AW1 N] [F AA1 K S], [JH AH1 M P S]!");
        assert!(c.windows(2).all(|w| w[1].0 > w[0].0));
    }

    #[test]
    fn breaks_without_a_pause_start_a_new_contour() {
        let last_peak = |input: &str| {
            let mut segs = segments(&parse(input).unwrap());
            assign_durations(&mut segs, 1.0);
            let c = pitch_keyframes(&segs, 110.0, 1.0);
            let last = segs.iter().rev().find(|s| s.accented).unwrap();
            c.iter().filter(|p| p.0 >= last.start && p.0 <= last.end()).map(|p| p.1).fold(0.0, f64::max)
        };
        // After the break, accents start again from full height.
        let joined = last_peak("[M AE1 N] [M AE1 N] [M AE1 N] [M AE1 N].");
        let broken = last_peak("[M AE1 N] [M AE1 N] | [M AE1 N] [M AE1 N].");
        assert!(broken > joined * 1.05, "{joined} {broken}");
    }

    #[test]
    fn inflection_zero_is_monotone() {
        let mut segs = segments(&parse("[M AE1 N]?").unwrap());
        assign_durations(&mut segs, 1.0);
        assert!(pitch_keyframes(&segs, 110.0, 0.0).iter().all(|p| (p.1 - 110.0).abs() < 1e-9));
    }
}
