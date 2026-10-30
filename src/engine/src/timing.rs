//! Segment durations, by the rules of Klatt (1979), "Synthesis by rule of
//! segmental durations in English sentences".
//!
//! Each segment starts from an inherent duration and a minimum duration. A
//! series of rules multiply a percentage that compresses or stretches only
//! the part above the minimum:
//!
//! duration = (inherent − minimum) × percentage + minimum
//!
//! so segments never shrink below what keeps them recognizable.

use crate::phoneme::{Manner, Phoneme, Stress};
use crate::phonetics::spec;
use crate::utterance::{Boundary, Segment};

/// Lead-in silence before speech starts, ms.
const LEAD_IN: f64 = 10.0;

/// Pause lengths at normal rate, ms. Klatt used 200 ms at clause and comma
/// boundaries; sentence ends get a little more.
fn pause_ms(boundary: Option<Boundary>) -> f64 {
    match boundary {
        None => LEAD_IN,
        Some(Boundary::Minor) => 0.0,
        Some(Boundary::Comma) => 180.0,
        Some(Boundary::Clause) => 220.0,
        Some(Boundary::Period | Boundary::Question | Boundary::WhQuestion | Boundary::Exclamation) => 280.0,
    }
}

/// Assigns durations and start times at the normal speaking rate (Klatt's
/// nominal 180 words per minute). Pauses are multiplied by `pause_scale`.
/// Other rates are made by compressing the finished timeline evenly; see
/// [`crate::speech`].
pub fn assign_durations(segments: &mut [Segment], pause_scale: f64) {
    for i in 0..segments.len() {
        segments[i].duration = if segments[i].phoneme == Phoneme::Pause {
            pause_ms(segments[i].boundary) * pause_scale
        } else {
            let (compressible, minimum) = rule_duration(segments, i);
            compressible + minimum
        };
    }
    let mut t = 0.0;
    for seg in segments.iter_mut() {
        seg.start = t;
        t += seg.duration;
    }
}

/// Neighbour within the same phrase, ignoring word boundaries.
fn phrase_neighbour(segments: &[Segment], i: usize, delta: isize) -> Option<&Segment> {
    let other = segments.get(i.checked_add_signed(delta)?)?;
    (other.phoneme != Phoneme::Pause && other.phrase == segments[i].phrase).then_some(other)
}

/// The duration of segment `i` by Klatt's rules, split into the part above
/// the minimum and the minimum itself.
fn rule_duration(segments: &[Segment], i: usize) -> (f64, f64) {
    let seg = &segments[i];
    let phoneme = seg.phoneme;
    let spec = spec(phoneme);
    let syllabic = phoneme.is_vowel();
    let coda = phoneme.is_consonant() && !seg.onset;
    let mut minimum = spec.minimum;
    let mut percent = 1.0;

    // Rule 2: clause-final lengthening of the last syllable before a pause.
    if seg.in_clause_final_syllable && (syllabic || coda) {
        percent *= 1.4;
    }
    // Rule 3: non-phrase-final shortening; phrase-final liquids and nasals
    // after the vowel are lengthened.
    if syllabic && !seg.in_phrase_final_syllable {
        percent *= 0.6;
    }
    if coda && seg.in_phrase_final_syllable && matches!(phoneme.manner(), Manner::Liquid | Manner::Nasal) {
        percent *= 1.4;
    }
    // Rule 4: non-word-final shortening.
    if syllabic && !seg.in_word_final_syllable {
        percent *= 0.85;
    }
    // Rule 5: polysyllabic shortening.
    if syllabic && seg.polysyllabic_word {
        percent *= 0.8;
    }
    // Rule 6: non-initial consonant shortening.
    if phoneme.is_consonant() && !seg.word_initial {
        percent *= 0.85;
    }
    // Rule 7: unstressed segments are more compressible, and both they and
    // secondary-stressed ones are shortened.
    if !seg.stressed {
        minimum /= 2.0;
    }
    if !seg.stressed || seg.stress == Stress::Secondary {
        let next_is_vowel = phrase_neighbour(segments, i, 1).is_some_and(|n| n.phoneme.is_vowel());
        percent *= if syllabic && seg.in_word_medial_syllable {
            0.5
        } else if syllabic {
            0.7
        } else if matches!(phoneme.manner(), Manner::Liquid | Manner::Glide) && next_is_vowel {
            0.1
        } else {
            0.7
        };
    }
    // Rule 8: emphasized vowels are lengthened. Klatt used 1.4; a little
    // less keeps exclamations from dragging at fast rates.
    if seg.emphasized {
        percent *= 1.3;
    }
    // Rule 9: the consonant after a vowel affects its length. Klatt kept 30%
    // of the effect outside the end of a phrase; 40% matches connected
    // speech, where a vowel before a voiced final stop is 1.15 to 1.27 times
    // as long as before a voiceless one (Luce and Charles-Luce 1985), and
    // told voicing apart before a consonant at 485 words per minute in 86%
    // of word pairs instead of 81%.
    if syllabic {
        let effect = postvocalic_effect(segments, i);
        percent *= if seg.in_clause_final_syllable { effect } else { 0.6 + 0.4 * effect };
    }
    // Rule 10: shortening in clusters.
    let prev = phrase_neighbour(segments, i, -1).map(|s| s.phoneme);
    let next = phrase_neighbour(segments, i, 1).map(|s| s.phoneme);
    if syllabic {
        if next.is_some_and(Phoneme::is_vowel) {
            percent *= 1.2;
        }
        if prev.is_some_and(Phoneme::is_vowel) {
            percent *= 0.7;
        }
    } else {
        match (prev.is_some_and(Phoneme::is_consonant), next.is_some_and(Phoneme::is_consonant)) {
            (true, true) => percent *= 0.5,
            (true, false) | (false, true) => percent *= 0.7,
            (false, false) => {}
        }
    }
    let mut compressible = (spec.inherent - minimum) * percent;
    // Rule 11: a stressed vowel or sonorant after an aspirated plosive is
    // lengthened by 25 ms, which holds the aspiration.
    let after_aspirated = i > 0 && {
        let p = &segments[i - 1];
        p.released && !p.phoneme.is_voiced() && p.vot >= 0.9 * spec_vot(p.phoneme)
    };
    if after_aspirated && seg.stressed && phoneme.is_sonorant() {
        compressible += 25.0;
    }
    (compressible, minimum)
}

fn spec_vot(phoneme: Phoneme) -> f64 {
    spec(phoneme).release.map_or(f64::INFINITY, |r| r.vot_ms)
}

/// Rule 9's factor for the consonants after a vowel in the same word.
fn postvocalic_effect(segments: &[Segment], i: usize) -> f64 {
    let in_word = |j: usize| segments.get(j).filter(|s| s.phoneme != Phoneme::Pause && !s.word_initial);
    let Some(next) = in_word(i + 1) else {
        return 1.2; // open syllable, word-final
    };
    if next.phoneme.is_vowel() || next.stressed {
        return 1.0;
    }
    // In a sonorant + obstruent cluster, the obstruent decides. Klatt's rule
    // counts only consonants of the same morpheme, though: an ending leaves
    // the vowel of "songs" and "turned" as long as in "song" and "turn".
    // A flap counts as the T or D it came from: the vowel of "rider" is
    // longer than that of "writer", which is how listeners tell them apart.
    let mut consonant = next.underlying;
    if matches!(consonant.manner(), Manner::Liquid | Manner::Nasal)
        && let Some(after) = in_word(i + 2)
        && after.phoneme.is_obstruent()
        && !after.stressed
        && !after.ending
    {
        consonant = after.underlying;
    }
    use Phoneme::*;
    match consonant {
        V | DH | Z | ZH => 1.6,
        B | D | G | JH => 1.2,
        M | N | NG => 0.85,
        P | T | K | CH => 0.7,
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utterance::{parse, segments};

    fn timed(input: &str) -> Vec<Segment> {
        let mut segs = segments(&parse(input).unwrap());
        assign_durations(&mut segs, 1.0);
        segs
    }

    fn duration_of(segs: &[Segment], phoneme: Phoneme) -> f64 {
        segs.iter().find(|s| s.phoneme == phoneme).unwrap().duration
    }

    #[test]
    fn matches_klatts_worked_example() {
        // "...a rocker." Klatt (1979) gives AA 140, K 50 and RR 175 ms,
        // rounded up to 5 ms.
        let segs = timed("[AH0] [R AA1 K ER0].");
        for (phoneme, klatt) in [(Phoneme::AA, 140.0), (Phoneme::K, 50.0), (Phoneme::ER, 175.0)] {
            let ours = duration_of(&segs, phoneme);
            assert!(ours <= klatt && ours > klatt - 5.0, "{phoneme:?}: {ours} vs {klatt}");
        }
    }

    #[test]
    fn phrase_final_syllables_are_longer() {
        let segs = timed("[M AE1 N] [S AE1 T].");
        let vowels: Vec<f64> = segs.iter().filter(|s| s.phoneme == Phoneme::AE).map(|s| s.duration).collect();
        assert!(vowels[1] > 1.3 * vowels[0], "{vowels:?}");
    }

    #[test]
    fn endings_do_not_lengthen_the_vowel() {
        // "songs" keeps the vowel of "song"; "bend" is still longer than "Ben".
        let vowel = |input: &str, phoneme| duration_of(&timed(input), phoneme);
        let (song, songs) = (vowel("[S AO1 NG].", Phoneme::AO), vowel("[S AO1 NG Z].", Phoneme::AO));
        assert!((songs - song).abs() < 1e-9, "{song} {songs}");
        assert!(vowel("[B EH1 N D].", Phoneme::EH) > vowel("[B EH1 N].", Phoneme::EH));
    }

    #[test]
    fn a_flap_keeps_the_vowel_length_of_its_t_or_d() {
        // "rider" has a longer vowel than "writer", though both have a flap.
        let (writer, rider) = (timed("[R AY1 T ER0]."), timed("[R AY1 D ER0]."));
        assert!(writer.iter().chain(&rider).any(|s| s.phoneme == Phoneme::DX));
        assert!(duration_of(&rider, Phoneme::AY) > 1.05 * duration_of(&writer, Phoneme::AY));
    }

    #[test]
    fn segments_never_go_below_half_their_minimum() {
        let segs = timed("[IH0 N] [DH AH0] [B IH1 G IH0 N IH0 NG], [IH1 T] [W AA1 Z]!");
        for seg in segs.iter().filter(|s| s.phoneme != Phoneme::Pause) {
            assert!(seg.duration >= spec(seg.phoneme).minimum / 2.0 - 1e-9, "{:?}", seg.phoneme);
        }
    }

    #[test]
    fn pauses_scale_and_starts_are_contiguous() {
        let input = "[HH AH0 L OW1], [W ER1 L D].";
        let normal = timed(input);
        let mut short = segments(&parse(input).unwrap());
        assign_durations(&mut short, 0.5);
        let pause = |s: &[Segment]| s.iter().find(|x| x.boundary == Some(Boundary::Comma)).unwrap().duration;
        assert!((pause(&short) - 0.5 * pause(&normal)).abs() < 1e-9);
        for pair in normal.windows(2) {
            assert!((pair[0].end() - pair[1].start).abs() < 1e-9);
        }
    }
}
