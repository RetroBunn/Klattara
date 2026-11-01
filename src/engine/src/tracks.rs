//! Parameter tracks: from timed segments to synthesizer frames.
//!
//! Three groups of parameters move on their own schedules:
//!
//! - Formants F1 to F3 glide slowly between segment targets. At each
//!   boundary the higher-ranked segment (a consonant over a vowel) sets the
//!   boundary value near its own locus, pulled partway toward its neighbour,
//!   in the manner of Holmes, Mattingly & Shearme (1964). Transitions are
//!   squeezed, never dropped, when a segment is short.
//! - Sound (amplitudes, frication spectrum, nasality) changes quickly at
//!   boundaries, with stop bursts and aspiration placed at releases.
//! - Pitch comes from the intonation rules.
//!
//! Frames are produced at every keyframe of every track, so each parameter
//! follows its own piecewise-linear path exactly.

use crate::frame::Frame;
use crate::phoneme::{Manner, Phoneme, Stress};
use crate::phonetics::{CODA_R, DEFAULT_F6, FRONT_VELAR, NASALIZED_ZERO, OR_NUCLEUS, SILENT, Sound, spec};
use crate::singing::{VIBRATO_FADE, is_note};
use crate::speech::Voice;
use crate::synth::Synth;
use crate::utterance::{Boundary, Segment, syllabic_consonant};
use crate::voicedefs::Gender;

/// How long before the release a voiced stop's voicing resumes when it
/// follows a voiceless sound, ms.
const PREVOICING: f64 = 20.0;
/// The longest a glide (W, Y) holds its own target, ms.
const GLIDE_HOLD: f64 = 10.0;
/// Aspiration level after the burst of a voiceless stop, dB.
const ASPIRATION: f64 = 57.0;
/// How far a consonant's boundary values are pulled toward its neighbour,
/// as a share of its own coarticulation setting. Half the pull leaves the
/// formants farther from the vowel at the boundary, so they bend more
/// sharply into each consonant, which marks its place more clearly. At
/// about 485 words per minute, final-consonant pairs went from 82% to 87%
/// told apart and sentence words from 51% to 53% recognized; at normal
/// speed, final consonants from 95% to 97%. The idea comes from TGSpeechBox
/// (Tamas Geczy), which weakens coarticulation at high speech rates.
const PULL: f64 = 0.5;
/// Voicing heard through the closure of a voiced stop ("voice bar"), 16 dB
/// below a stressed vowel. At 22 dB below, a final /d/ before a vowel was
/// heard as /t/ at fast rates ("tide" as "tight").
const VOICE_BAR: Sound = Sound { av: 44.0, tl: 18.0, ..SILENT };
/// Voicing through the burst of a voiced stop released into a vowel or
/// other sonorant. Between vowels, /b d g/ stay voiced through the release;
/// a voiceless gap there sounds like /p t k/. With this and the stronger
/// voice bar, voicing pairs before a vowel were told apart at 485 words per
/// minute in 95% of cases instead of 90%.
const VOICED_RELEASE: f64 = 40.0;
/// Creak before a glottalized /t/ builds up toward the end of the sound before
/// it, where natural glottalization is strongest (Garellek & Seyfarth 2016):
/// over this many ms, but never more than [`CREAK_SHARE`] of that sound.
/// When the /t/ is not released, as in "button", the sound after it starts
/// at [`CREAK_AFTER`] and is clear again within [`CREAK_FADE`] ms.
const CREAK_LEAD: f64 = 90.0;
const CREAK_SHARE: f64 = 0.7;
const CREAK_AFTER: f64 = 0.5;
const CREAK_FADE: f64 = 20.0;
/// Lax breathiness, as in DECtalk's lx: as the folds relax, aspiration comes up through
/// the voicing and the voice's high frequencies fall away. At full strength, it takes this
/// many ms to arrive, rising from the first level to the first plus the second, in dB, and
/// the tilt rises by [`LAX_TILT`] dB. Only while voicing is above [`LAX_MIN_VOICING`] dB.
const LAX_RISE: f64 = 84.0;
const LAX_BREATH_START: f64 = 30.0;
const LAX_BREATH_RISE: f64 = 27.0;
const LAX_TILT: f64 = 16.0;
const LAX_MIN_VOICING: f64 = 40.0;
/// Before a voiceless sound, never more than this share of the voiced sound.
const LAX_SHARE: f64 = 0.7;
/// How far a dark L pulls /ɪ/ and /ɛ/ down and back: F1 rises by this many Hz
/// and F2 shrinks to this share. With their usual targets, "fill" stood too
/// close to "feel", and F2's plunge to the L blurred them both. Ten pairs like
/// "seal" and "sill" scored +34 against +17 at 485 words per minute and +73
/// against +42 at normal speed, recorded one at a time.
const DARK_L_F1_RISE: f64 = 60.0;
const DARK_L_F2_SHARE: f64 = 0.82;

trait Interpolate: Copy {
    fn interpolate(&self, other: &Self, t: f64) -> Self;
}

impl Interpolate for f64 {
    fn interpolate(&self, other: &Self, t: f64) -> Self {
        self + (other - self) * t
    }
}

impl Interpolate for Sound {
    fn interpolate(&self, other: &Self, t: f64) -> Self {
        self.lerp(other, t)
    }
}

/// Frequencies and bandwidths of F1 to F3.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Formants {
    freq: [f64; 3],
    bw: [f64; 3],
}

impl Interpolate for Formants {
    fn interpolate(&self, other: &Self, t: f64) -> Self {
        let mix = |a: &[f64; 3], b: &[f64; 3]| std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t);
        Formants { freq: mix(&self.freq, &other.freq), bw: mix(&self.bw, &other.bw) }
    }
}

/// A piecewise-linear path through keyframes, held constant beyond its ends.
struct Track<T> {
    points: Vec<(f64, T)>,
}

impl<T: Interpolate> Track<T> {
    fn new() -> Self {
        Track { points: Vec::new() }
    }

    fn push(&mut self, t: f64, value: T) {
        self.points.push((t, value));
    }

    fn sort(&mut self) {
        self.points.sort_by(|a, b| a.0.total_cmp(&b.0));
    }

    fn at(&self, t: f64) -> T {
        let i = self.points.partition_point(|p| p.0 <= t);
        if i == 0 {
            return self.points[0].1;
        }
        if i == self.points.len() {
            return self.points[i - 1].1;
        }
        let (t0, v0) = self.points[i - 1];
        let (t1, v1) = self.points[i];
        if t1 - t0 < 1e-9 { v1 } else { v0.interpolate(&v1, (t - t0) / (t1 - t0)) }
    }
}

/// The start and end formant targets of segment `i`, for a voice of `gender`.
fn targets(segments: &[Segment], i: usize, gender: Gender) -> (Formants, Formants) {
    let seg = &segments[i];
    if seg.phoneme == Phoneme::HH {
        // /h/ is breath through the vocal tract shape of its neighbour,
        // usually the vowel that follows.
        if segments.get(i + 1).is_some_and(|n| n.phoneme.is_sonorant()) {
            let (start, _) = targets(segments, i + 1, gender);
            return (start, start);
        }
        if i > 0 && segments[i - 1].phoneme.is_sonorant() {
            let (_, end) = targets(segments, i - 1, gender);
            return (end, end);
        }
    }
    let (start, end) = table_targets(segments, i);
    let (start_shares, end_shares) = gender.formant_shares(seg.phoneme);
    let scaled = |f: Formants, shares: [f64; 3]| Formants { freq: std::array::from_fn(|k| f.freq[k] * shares[k]), ..f };
    // A sound held steady in this context stays steady, between its two shares.
    if start == end {
        let middle = std::array::from_fn(|k| (start_shares[k] + end_shares[k]) / 2.0);
        return (scaled(start, middle), scaled(end, middle));
    }
    (scaled(start, start_shares), scaled(end, end_shares))
}

/// The start and end formant targets of segment `i` in the phoneme tables,
/// which describe a man's vocal tract.
fn table_targets(segments: &[Segment], i: usize) -> (Formants, Formants) {
    let seg = &segments[i];
    // The schwa of a syllabic L or N takes the consonant's own shape.
    if let Some(consonant) = syllabic_consonant(segments, i) {
        let spec = spec(consonant);
        let held = Formants { freq: spec.formants, bw: spec.bandwidths };
        return (held, held);
    }
    let spec = spec(seg.phoneme);
    // Before R in the same word, /o/ holds steady instead of gliding: "four"
    // is not "foh-oor".
    if seg.phoneme == Phoneme::OW && segments.get(i + 1).is_some_and(|n| n.phoneme == Phoneme::R && !n.word_initial) {
        let held = Formants { freq: OR_NUCLEUS, bw: spec.bandwidths };
        return (held, held);
    }
    if is_soft_r(segments, i) {
        let soft = Formants { freq: CODA_R, bw: spec.bandwidths };
        return (soft, soft);
    }
    if front_velar(segments, i) {
        let front = Formants { freq: [spec.formants[0], FRONT_VELAR[0], FRONT_VELAR[1]], bw: spec.bandwidths };
        return (front, front);
    }
    let before_dark_l = segments.get(i + 1).is_some_and(|n| n.phoneme == Phoneme::LX);
    if before_dark_l && matches!(seg.phoneme, Phoneme::IH | Phoneme::EH) {
        let [f1, f2, f3] = spec.formants;
        let backed = Formants { freq: [f1 + DARK_L_F1_RISE, f2 * DARK_L_F2_SHARE, f3], bw: spec.bandwidths };
        return (backed, backed);
    }
    // Before a dark L, /u/ holds its back target instead of gliding to it, or
    // "fool" starts too far forward. Nine pairs like "fool" and "full" scored
    // +50 against +42 at 485 words per minute, and +79 against +75 at normal speed.
    if before_dark_l && seg.phoneme == Phoneme::UW {
        let held = Formants { freq: spec.glide_to.unwrap_or(spec.formants), bw: spec.bandwidths };
        return (held, held);
    }
    let start = Formants { freq: spec.formants, bw: spec.bandwidths };
    let end = Formants { freq: spec.glide_to.unwrap_or(spec.formants), bw: spec.bandwidths };
    (start, end)
}

/// Whether segment `i` is an R that colors the vowel before it rather than
/// closing in to the full R of "red": after its vowel in the syllable ("car",
/// "there"), or between vowels before one without primary stress ("very",
/// "Mary"). The full R sounded forced in both. Before a stressed vowel the R
/// is full: "arrive".
fn is_soft_r(segments: &[Segment], i: usize) -> bool {
    let Some(seg) = segments.get(i) else { return false };
    let between_vowels = !seg.word_initial
        && i > 0
        && segments[i - 1].phoneme.is_vowel()
        && segments.get(i + 1).is_none_or(|n| n.stress != Stress::Primary);
    seg.phoneme == Phoneme::R && (!seg.onset || between_vowels)
}

/// Whether segment `i` is K, G or NG next to a front vowel, where the tongue
/// meets the palate further forward (MITalk's front velars). F2 and F3 then
/// come together near 2300 Hz: "big", "keep", "egg". With the one velar
/// locus, G after "ih" moved almost as D does, and "rid" and "rig" were
/// confused at speed. The vowel that follows decides, or else the one before.
fn front_velar(segments: &[Segment], i: usize) -> bool {
    use Phoneme::*;
    let Some(seg) = segments.get(i) else { return false };
    if !matches!(seg.phoneme, K | G | NG) {
        return false;
    }
    let front = |j: usize| segments.get(j).is_some_and(|n| matches!(n.phoneme, IY | IH | EY | EH | AE));
    if segments.get(i + 1).is_some_and(|n| n.phoneme.is_vowel()) { front(i + 1) } else { i > 0 && front(i - 1) }
}

/// The meeting point of two neighbouring segments: the shared formant value
/// and how long each side takes to move between it and its own target.
struct Join {
    value: Formants,
    left_ms: f64,
    right_ms: f64,
}

/// The join between segment `i` and the next, unless a pause separates them.
fn join(segments: &[Segment], i: usize, gender: Gender) -> Option<Join> {
    let (left, right) = (segments.get(i)?, segments.get(i + 1)?);
    if left.phoneme == Phoneme::Pause || right.phoneme == Phoneme::Pause {
        return None;
    }
    let (mut a, mut b) = (spec(left.phoneme), spec(right.phoneme));
    // A front velar keeps its pinch, pulled less toward the vowel.
    if front_velar(segments, i) {
        a.coarticulation = [0.3; 3];
    }
    if front_velar(segments, i + 1) {
        b.coarticulation = [0.3; 3];
    }
    a.coarticulation = a.coarticulation.map(|k| k * PULL);
    b.coarticulation = b.coarticulation.map(|k| k * PULL);
    let (left_end, right_start) = (targets(segments, i, gender).1, targets(segments, i + 1, gender).0);
    let pull = |own: Formants, other: Formants, k: [f64; 3]| Formants {
        freq: std::array::from_fn(|j| own.freq[j] + k[j] * (other.freq[j] - own.freq[j])),
        bw: std::array::from_fn(|j| own.bw[j] + 0.5 * (other.bw[j] - own.bw[j])),
    };
    Some(if a.rank == b.rank {
        Join { value: left_end.interpolate(&right_start, 0.5), left_ms: a.transition_in, right_ms: b.transition_in }
    } else if a.rank > b.rank {
        Join {
            value: pull(left_end, right_start, a.coarticulation),
            left_ms: a.transition_in,
            right_ms: a.transition_out,
        }
    } else {
        Join {
            value: pull(right_start, left_end, b.coarticulation),
            left_ms: b.transition_out,
            right_ms: b.transition_in,
        }
    })
}

fn formant_track(segments: &[Segment], gender: Gender) -> Track<Formants> {
    let mut track = Track::new();
    for (i, seg) in segments.iter().enumerate() {
        if seg.phoneme == Phoneme::Pause {
            continue;
        }
        let (start, end) = targets(segments, i, gender);
        let left = i.checked_sub(1).and_then(|j| join(segments, j, gender));
        let right = join(segments, i, gender);
        let (s, e, d) = (seg.start, seg.end(), seg.duration);
        let mut lead = left.as_ref().map_or(0.0, |j| j.right_ms);
        let mut trail = right.as_ref().map_or(0.0, |j| j.left_ms);
        // Squeeze transitions that would overlap; a diphthong keeps room to glide.
        let room = if spec(seg.phoneme).glide_to.is_some() { 0.7 * d } else { d };
        if lead + trail > room {
            let scale = room / (lead + trail);
            lead *= scale;
            trail *= scale;
        }
        // A vowel before a soft R glides into it over its second half, as
        // one r-colored nucleus ("car" is "ah-er", not "ca" + "r").
        if seg.phoneme.is_vowel() && is_soft_r(segments, i + 1) {
            trail = trail.max(0.5 * d).min(d - lead);
        }
        // A glide is nearly all movement: it touches its target briefly and
        // spends the rest of its time heading for the next sound.
        if seg.phoneme.manner() == Manner::Glide && d - lead - trail > GLIDE_HOLD {
            if right.is_some() {
                trail = d - lead - GLIDE_HOLD;
            } else if left.is_some() {
                lead = d - trail - GLIDE_HOLD;
            }
        }
        if let Some(j) = &left {
            track.push(s, j.value);
        }
        track.push(s + lead, start);
        if start != end {
            let hold = s + 0.3 * d;
            if hold > s + lead && hold < e - trail {
                track.push(hold, start);
            }
        }
        track.push(e - trail, end);
        if let Some(j) = &right {
            track.push(e, j.value);
        }
    }
    if track.points.is_empty() {
        let (start, _) = targets(segments, 0, gender);
        track.push(0.0, start);
    }
    track.sort();
    track
}

/// How long amplitudes take to change at a segment's edges, ms.
fn edge_ms(manner: Manner) -> f64 {
    match manner {
        Manner::Vowel | Manner::Diphthong => 10.0,
        Manner::Liquid | Manner::Glide => 16.0,
        Manner::Fricative | Manner::Aspirate => 14.0,
        Manner::Stop | Manner::Affricate => 8.0,
        Manner::Nasal | Manner::Flap | Manner::Silence => 6.0,
    }
}

/// A level in dB at this share of normal. Normal leaves it exactly as it is, and
/// silence stays silent at any share.
fn scaled(db: f64, share: f64) -> f64 {
    if share == 1.0 || db <= 0.0 {
        db
    } else if share <= 0.0 {
        0.0
    } else {
        (db + 20.0 * share.log10()).max(0.0)
    }
}

/// The steady sound of segment `i` in its context, with nasals at this share of normal.
fn steady_sound(segments: &[Segment], i: usize, nasal: f64) -> Sound {
    let seg = &segments[i];
    // A syllabic N is the nasal murmur, at the level of an unstressed vowel.
    if syllabic_consonant(segments, i) == Some(Phoneme::N) {
        return Sound { av: scaled(57.0, nasal), ..spec(Phoneme::N).sound };
    }
    let mut sound = spec(seg.phoneme).sound;
    let after_voicing = i > 0 && segments[i - 1].phoneme.is_voiced();
    match seg.phoneme.manner() {
        Manner::Vowel | Manner::Diphthong => {
            sound.av = match seg.stress {
                Stress::Primary => 60.0,
                Stress::Secondary => 59.0,
                Stress::Unstressed => 57.0,
            };
            // Exclamations are louder, most of all on the emphasized word.
            if seg.emphasized {
                sound.av += 4.0;
            } else if seg.exclaimed {
                sound.av += 2.0;
            }
        }
        Manner::Nasal => sound.av = scaled(sound.av, nasal),
        Manner::Stop if seg.phoneme.is_voiced() && after_voicing => sound = VOICE_BAR,
        // Part of the vowel, so nearly as loud as one.
        Manner::Liquid if is_soft_r(segments, i) => sound.av = 59.0,
        _ => {}
    }
    sound
}

fn sound_track(segments: &[Segment], nasal: f64) -> Track<Sound> {
    let mut track = Track::new();
    for (i, seg) in segments.iter().enumerate() {
        let (s, e, d) = (seg.start, seg.end(), seg.duration);
        let manner = seg.phoneme.manner();
        let edge = edge_ms(manner).min(d / 2.0);
        let steady = steady_sound(segments, i, nasal);
        let mut begin = s + edge / 2.0;

        // The release of a stop before this segment: burst, then aspiration
        // until voicing starts.
        if let Some(prev) = i.checked_sub(1).map(|j| &segments[j])
            && prev.released
            && let Some(release) = spec(prev.phoneme).release
        {
            // Aspiration never takes more than half the segment: the rest is voiced.
            let vot = prev.vot.min((0.5 * d).max(release.burst_ms + 3.0));
            let burst_end = s + release.burst_ms.min(vot);
            let mut burst = release.burst;
            if prev.phoneme.is_voiced() && seg.phoneme.is_sonorant() {
                burst.av = VOICED_RELEASE;
                burst.tl = VOICE_BAR.tl;
            }
            track.push(s, burst);
            track.push(burst_end, Sound { af: burst.af - 6.0, ..burst });
            if !prev.phoneme.is_voiced() && vot > release.burst_ms + 3.0 {
                let breath = Sound { ah: ASPIRATION, fnp: steady.fnp, fnz: steady.fnz, ..SILENT };
                track.push(burst_end + 1.0, breath);
                track.push(s + vot - 2.0, Sound { ah: ASPIRATION - 3.0, ..breath });
            }
            begin = s + vot + 2.0;
        }
        let last = (e - edge / 2.0).max(s);
        let begin = begin.min(last);

        match manner {
            Manner::Silence => {
                // Hold silence to the end, so the next sound starts on time
                // instead of fading in across the whole pause.
                track.push(begin, SILENT);
                track.push(last, SILENT);
            }
            Manner::Stop
                if seg.phoneme.is_voiced()
                    && steady == SILENT
                    && i > 0
                    && segments[i - 1].phoneme != Phoneme::Pause =>
            {
                // A voiced stop after a voiceless sound: voicing starts again
                // shortly before the release, a strong cue that it is /b d g/
                // and not /p t k/. At the start of a phrase it stays silent.
                let voicing_from = (e - PREVOICING).max(begin);
                track.push(begin, SILENT);
                track.push(voicing_from, SILENT);
                track.push((voicing_from + 5.0).min(e - 0.5), VOICE_BAR);
                track.push(e - 0.5, VOICE_BAR);
            }
            Manner::Affricate => {
                let closure_end = s + d * spec(seg.phoneme).closure_fraction;
                let closure = if seg.phoneme.is_voiced() && i > 0 && segments[i - 1].phoneme.is_voiced() {
                    VOICE_BAR
                } else {
                    SILENT
                };
                track.push(begin, closure);
                track.push(closure_end - 0.5, closure);
                track.push(closure_end, Sound { af: steady.af + 3.0, ..steady });
                track.push((closure_end + 6.0).min(e), steady);
                track.push(last.max(closure_end + 6.0).min(e), steady);
            }
            _ => {
                let mut start_sound = steady;
                let mut end_sound = steady;
                // Voicing fades a little at the end of a sentence.
                if seg.phoneme.is_vowel() && seg.in_clause_final_syllable && !seg.singing {
                    end_sound.av -= 4.0;
                }
                // English vowels are nasalized next to nasal consonants: over
                // the second half before one, and briefly after one.
                let is_nasal = |j: usize| segments.get(j).is_some_and(|n| n.phoneme.manner() == Manner::Nasal);
                let oral_vowel = seg.phoneme.is_vowel() && syllabic_consonant(segments, i) != Some(Phoneme::N);
                let nasal_before = oral_vowel && i > 0 && is_nasal(i - 1);
                let nasal_after = oral_vowel && is_nasal(i + 1);
                if nasal_before {
                    start_sound.fnz = NASALIZED_ZERO;
                }
                if nasal_after {
                    end_sound.fnz = NASALIZED_ZERO;
                }
                track.push(begin, start_sound);
                let oral_from = begin + 0.3 * (last - begin);
                if nasal_before {
                    track.push(oral_from, steady);
                }
                if nasal_after {
                    track.push((begin + 0.5 * (last - begin)).max(oral_from), steady);
                }
                track.push(last, end_sound);
            }
        }
    }
    track.sort();
    track
}

/// Creaky voice around each glottalized /t/: rising to full creak at the
/// closure, and fading over the start of a sonorant after an unreleased one.
fn creak_track(segments: &[Segment]) -> Track<f64> {
    let mut track = Track::new();
    track.push(0.0, 0.0);
    for (i, seg) in segments.iter().enumerate() {
        if !seg.glottalized || seg.singing || i == 0 {
            continue;
        }
        let before = &segments[i - 1];
        track.push(before.end() - CREAK_LEAD.min(CREAK_SHARE * before.duration), 0.0);
        track.push(before.end(), 1.0);
        match segments.get(i + 1) {
            Some(after) if after.phoneme.is_sonorant() && !seg.released => {
                track.push(seg.end() - 0.5, 1.0);
                track.push(seg.end(), CREAK_AFTER);
                track.push(seg.end() + CREAK_FADE.min(after.duration / 2.0), 0.0);
            }
            _ => track.push(seg.end(), 0.0),
        }
    }
    track.sort();
    track
}

/// How far the folds have relaxed over time, 0 to 1: through the last syllable of a
/// sentence, from its vowel to the end of its voicing, and at the end of a voiced sound
/// before a voiceless one. Those are the places DECtalk's manual gives for lax breathiness.
fn lax_track(segments: &[Segment]) -> Track<f64> {
    let ends_sentence = |i: usize| {
        let next_pause = segments[i..].iter().find(|s| s.phoneme == Phoneme::Pause);
        matches!(
            next_pause.and_then(|s| s.boundary),
            Some(Boundary::Period | Boundary::Question | Boundary::WhQuestion | Boundary::Exclamation)
        )
    };
    let mut track = Track::new();
    track.push(0.0, 0.0);
    let mut i = 0;
    while i < segments.len() {
        let seg = &segments[i];
        if seg.singing || !seg.phoneme.is_voiced() || seg.phoneme == Phoneme::Pause {
            i += 1;
            continue;
        }
        if seg.phoneme.is_vowel() && seg.in_clause_final_syllable && ends_sentence(i) {
            // To the end of the sentence: the voicing gate takes care of what isn't voiced.
            let last = segments[i..].iter().take_while(|s| s.phoneme != Phoneme::Pause).last().unwrap_or(seg);
            let arrives = (seg.start + LAX_RISE).min(last.end());
            track.push(seg.start, 0.0);
            track.push(arrives, (arrives - seg.start) / LAX_RISE);
            track.push(last.end(), (arrives - seg.start) / LAX_RISE);
            track.push(last.end() + 1.0, 0.0);
            i += segments[i..].iter().take_while(|s| s.phoneme != Phoneme::Pause).count();
            continue;
        }
        let next = segments.get(i + 1);
        if next.is_some_and(|n| n.phoneme != Phoneme::Pause && !n.phoneme.is_voiced()) {
            let lead = LAX_RISE.min(LAX_SHARE * seg.duration);
            track.push(seg.end() - lead, 0.0);
            track.push(seg.end(), lead / LAX_RISE);
            track.push(seg.end() + 1.0, 0.0);
        }
        i += 1;
    }
    track.sort();
    track
}

/// Two noise levels in dB, from one noise source, heard together.
fn added_db(a: f64, b: f64) -> f64 {
    if a <= 0.0 {
        b
    } else if b <= 0.0 {
        a
    } else {
        20.0 * (10f64.powf(a / 20.0) + 10f64.powf(b / 20.0)).log10()
    }
}

/// How quickly singing's vibrato and steadiness change, ms: at once, in effect, but
/// never across the whole gap to the next frame.
const SINGING_EDGE: f64 = 1.0;

/// Vibrato and steadiness over time. A sung stretch is steady throughout, with vibrato
/// while its latest pitch is a note and none while it is in Hz. Vibrato fades in, after
/// the voice's delay. With a delay, each new note starts still again; with none, the
/// vibrato carries on across notes.
fn singing_tracks(segments: &[Segment], voice: &Voice) -> (Track<f64>, Track<f64>) {
    let (mut vibrato, mut steady) = (Track::new(), Track::new());
    vibrato.push(0.0, 0.0);
    steady.push(0.0, 0.0);
    let mut i = 0;
    while i < segments.len() {
        if !segments[i].singing {
            i += 1;
            continue;
        }
        let start = i;
        while segments.get(i).is_some_and(|s| s.singing) {
            i += 1;
        }
        let stretch = &segments[start..i];
        let (begin, end) = (stretch[0].start, stretch[stretch.len() - 1].end());
        steady.push(begin - SINGING_EDGE, 0.0);
        steady.push(begin, 1.0);
        steady.push(end, 1.0);
        steady.push(end + SINGING_EDGE, 0.0);
        let depth = voice.vibrato_depth;
        let delay = voice.vibrato_delay;
        let pitched: Vec<&Segment> = stretch.iter().filter(|s| s.written.pitch.is_some()).collect();
        let mut now = 0.0;
        vibrato.push(begin - SINGING_EDGE, 0.0);
        vibrato.push(begin, 0.0);
        for (i, seg) in pitched.iter().enumerate() {
            let note = seg.written.pitch.is_some_and(is_note);
            if note && now == depth && delay == 0.0 {
                continue;
            }
            if now > 0.0 {
                vibrato.push(seg.start, now);
                vibrato.push(seg.start + SINGING_EDGE, 0.0);
                now = 0.0;
            }
            if note && depth > 0.0 {
                let until = pitched.get(i + 1).map_or(end, |next| next.start);
                let (from, full) = (seg.start + delay, seg.start + delay + VIBRATO_FADE);
                if from < until {
                    vibrato.push(from, 0.0);
                    now = if full <= until { depth } else { depth * (until - from) / VIBRATO_FADE };
                    vibrato.push(full.min(until), now);
                }
            }
        }
        vibrato.push(end, now);
        vibrato.push(end + SINGING_EDGE, 0.0);
    }
    vibrato.sort();
    steady.sort();
    (vibrato, steady)
}

/// Aspiration at this share of normal, or breath noise through voicing at this voice's
/// level, whichever is stronger. Breath follows the voicing, so it fades with it and is
/// gone when the voicing stops. It follows the voicing before the voice's gain for
/// voicing, so that with that off, what is left is a whisper.
fn breath(s: &Sound, breathiness: f64, aspiration: f64) -> f64 {
    let ah = scaled(s.ah, aspiration);
    if breathiness > 0.0 && s.av > 0.0 { ah.max(breathiness + s.av - 60.0) } else { ah }
}

/// Builds synthesizer frames from timed segments and pitch keyframes, for the
/// voice's gender and quality.
pub fn frames(segments: &[Segment], pitch: &[(f64, f64)], voice: &Voice) -> Vec<(f64, Frame)> {
    let formants = formant_track(segments, voice.gender);
    let gains = voice.gains;
    let sound = sound_track(segments, gains.nasal);
    let creak = creak_track(segments);
    let (vibrato, steady) = singing_tracks(segments, voice);
    let lax_share = voice.lax_breathiness / 100.0;
    let lax = if lax_share > 0.0 { lax_track(segments) } else { Track::new() };
    let mut f0 = Track::new();
    for &(t, hz) in pitch {
        f0.push(t, hz);
    }
    if f0.points.is_empty() {
        f0.push(0.0, 0.0);
    }

    let end = segments.last().map_or(0.0, Segment::end);
    let mut times: Vec<f64> = formants
        .points
        .iter()
        .map(|p| p.0)
        .chain(sound.points.iter().map(|p| p.0))
        .chain(f0.points.iter().map(|p| p.0))
        .chain(creak.points.iter().map(|p| p.0))
        .chain(vibrato.points.iter().map(|p| p.0))
        .chain(steady.points.iter().map(|p| p.0))
        .chain(lax.points.iter().map(|p| p.0))
        .chain([0.0, end])
        .filter(|&t| (0.0..=end).contains(&t))
        .collect();
    times.sort_by(f64::total_cmp);
    times.dedup_by(|a, b| (*a - *b).abs() < 0.05);

    let defaults = Frame::default();
    let upper = voice.gender.upper_share();
    let widths = voice.gender.bandwidth_share();
    // One factor for every resonance and its bandwidth, so that the whole spectrum
    // stretches evenly and keeps its shape.
    let head = 100.0 / voice.head_size;
    times
        .into_iter()
        .map(|t| {
            let f = formants.at(t);
            let s = sound.at(t);
            let relaxed = if lax_share > 0.0 && s.av > LAX_MIN_VOICING { lax.at(t) } else { 0.0 };
            let ah = breath(&s, voice.breathiness, gains.aspiration);
            let frame = Frame {
                f0: f0.at(t),
                av: scaled(s.av, gains.voicing),
                oq: voice.open_quotient,
                tl: voice.tilt + s.tl + lax_share * LAX_TILT * relaxed,
                creak: creak.at(t),
                vibrato: vibrato.at(t),
                steady: steady.at(t),
                ah: if relaxed > 0.0 {
                    added_db(ah, lax_share * (LAX_BREATH_START + LAX_BREATH_RISE * relaxed))
                } else {
                    ah
                },
                af: scaled(s.af, gains.frication),
                freq: [
                    f.freq[0] * head,
                    f.freq[1] * head,
                    f.freq[2] * head,
                    voice.upper_formants[0].unwrap_or(defaults.freq[3] * upper) * head,
                    voice.upper_formants[1].unwrap_or(defaults.freq[4] * upper) * head,
                    voice.upper_formants[2].map_or(s.f6 * upper, |hz| hz * s.f6 / DEFAULT_F6) * head,
                ],
                bw: [
                    f.bw[0] * widths * head,
                    f.bw[1] * widths * head,
                    f.bw[2] * widths * head,
                    voice.upper_bandwidths[0].unwrap_or(defaults.bw[3] * widths) * head,
                    voice.upper_bandwidths[1].unwrap_or(defaults.bw[4] * widths) * head,
                    voice.upper_bandwidths[2].unwrap_or(defaults.bw[5] * widths) * head,
                ],
                fnp: s.fnp * upper * head,
                bnp: s.bnp * head,
                fnz: s.fnz * upper * head,
                bnz: 100.0 * head,
                par_amp: s.par_amp,
                par_bw: defaults.par_bw.map(|width| width * head),
                ab: s.ab,
            };
            (t, frame)
        })
        .collect()
}

/// Audio is handed out in chunks of at least this many samples (about 46 ms
/// at 22050 Hz), so playback can start long before a long text is finished.
const CHUNK: usize = 1024;

/// Renders frames (time in ms, frame) to audio, gliding between them, and
/// passes it to `on_audio` chunk by chunk as it is made. If `on_audio`
/// returns false, rendering stops at once and this returns false.
pub fn render_with(synth: &mut Synth, frames: &[(f64, Frame)], on_audio: &mut dyn FnMut(&[f32]) -> bool) -> bool {
    let Some((_, first)) = frames.first() else { return true };
    let samples_per_ms = synth.sample_rate() as f64 / 1000.0;
    synth.reset();
    synth.jump_to(first);
    let mut buffer = Vec::with_capacity(2 * CHUNK);
    let mut position = 0;
    for (t, frame) in &frames[1..] {
        let end = ((t * samples_per_ms).round() as usize).max(position);
        let start = buffer.len();
        buffer.resize(start + end - position, 0.0);
        synth.render(frame, &mut buffer[start..]);
        position = end;
        if buffer.len() >= CHUNK {
            if !on_audio(&buffer) {
                return false;
            }
            buffer.clear();
        }
    }
    buffer.is_empty() || on_audio(&buffer)
}

/// Renders frames (time in ms, frame) to audio, gliding between them.
pub fn render(synth: &mut Synth, frames: &[(f64, Frame)]) -> Vec<f32> {
    let mut out = Vec::new();
    render_with(synth, frames, &mut |chunk| {
        out.extend_from_slice(chunk);
        true
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::NUM_FORMANTS;
    use crate::intonation::pitch_keyframes;
    use crate::singing::VIBRATO_DEPTH;
    use crate::timing::assign_durations;
    use crate::utterance::{parse, segments};

    fn plan(input: &str) -> (Vec<Segment>, Vec<(f64, Frame)>) {
        let mut segs = segments(&parse(input).unwrap());
        assign_durations(&mut segs, 1.0);
        let pitch = pitch_keyframes(&segs, 110.0, 1.0);
        let frames = frames(&segs, &pitch, &Voice { open_quotient: 0.4, ..Voice::default() });
        (segs, frames)
    }

    /// The same as [`plan`], for the female model.
    fn female_plan(input: &str) -> (Vec<Segment>, Vec<(f64, Frame)>) {
        let mut segs = segments(&parse(input).unwrap());
        assign_durations(&mut segs, 1.0);
        let pitch = pitch_keyframes(&segs, 210.0, 1.0);
        let frames = frames(&segs, &pitch, &Voice { gender: Gender::Female, breathiness: 36.0, ..Voice::default() });
        (segs, frames)
    }

    fn frame_at(frames: &[(f64, Frame)], t: f64) -> Frame {
        frames.iter().rev().find(|f| f.0 <= t).unwrap().1
    }

    #[test]
    fn frames_cover_the_utterance_in_order() {
        let (segs, frames) = plan("[DH AH0] [K W IH1 K] [B R AW1 N] [F AA1 K S].");
        assert_eq!(frames[0].0, 0.0);
        assert!((frames.last().unwrap().0 - segs.last().unwrap().end()).abs() < 1e-9);
        assert!(frames.windows(2).all(|w| w[1].0 > w[0].0));
        assert!(frames.iter().all(|(_, f)| f.freq.iter().chain(&f.bw).all(|x| x.is_finite() && *x > 0.0)));
    }

    #[test]
    fn notes_have_vibrato_and_hz_have_none() {
        let at_vowel = |input: &str| {
            let (segs, frames) = plan(input);
            let vowel = segs.iter().find(|s| s.phoneme == Phoneme::AA).unwrap();
            frame_at(&frames, vowel.start + vowel.duration / 2.0)
        };
        let note = at_vowel("[AA1<1000,39>].");
        assert_eq!((note.vibrato, note.steady, note.f0), (VIBRATO_DEPTH, 1.0, 523.25));
        let hz = at_vowel("[AA1<1000,40>].");
        assert_eq!((hz.vibrato, hz.steady, hz.f0), (0.0, 1.0, 40.0));
        let top = at_vowel("[AA1<1000,550>].");
        assert_eq!((top.vibrato, top.steady, top.f0), (0.0, 1.0, 550.0));
        // Ordinary speech has neither.
        let (_, frames) = plan("[P AA1 T].");
        assert!(frames.iter().all(|(_, f)| f.vibrato == 0.0 && f.steady == 0.0));
    }

    #[test]
    fn vibrato_changes_with_the_kind_of_pitch() {
        let (segs, frames) = plan("[AA1<500,20> IY1<500,200> AA1<500,20>].");
        let mid = |i: usize| frame_at(&frames, segs[i].start + segs[i].duration / 2.0).vibrato;
        assert_eq!((mid(1), mid(2), mid(3)), (VIBRATO_DEPTH, 0.0, VIBRATO_DEPTH));
    }

    /// Plans sung input with vibrato settings, and gives a way to read the depth at a
    /// time after the start of a segment.
    fn sung_depths(input: &str, delay: f64, depth: f64) -> (Vec<Segment>, Vec<(f64, Frame)>) {
        let mut segs = segments(&parse(input).unwrap());
        assign_durations(&mut segs, 1.0);
        let pitch = pitch_keyframes(&segs, 110.0, 1.0);
        let voice = Voice { vibrato_delay: delay, vibrato_depth: depth, ..Voice::default() };
        let frames = frames(&segs, &pitch, &voice);
        (segs, frames)
    }

    fn depth_at(frames: &[(f64, Frame)], t: f64) -> f64 {
        let i = frames.partition_point(|f| f.0 <= t);
        let ((t0, a), (t1, b)) = (frames[i - 1], frames[i]);
        a.vibrato + (b.vibrato - a.vibrato) * (t - t0) / (t1 - t0)
    }

    #[test]
    fn vibrato_fades_in_rather_than_starting_at_once() {
        let (segs, frames) = sung_depths("[AA1<1000,20>].", 0.0, 25.0);
        let s = segs[1].start;
        assert_eq!(depth_at(&frames, s + 0.5), depth_at(&frames, s + 0.5).min(1.0));
        assert!((depth_at(&frames, s + VIBRATO_FADE / 2.0) - 12.5).abs() < 0.01);
        assert_eq!(depth_at(&frames, s + VIBRATO_FADE + 1.0), 25.0);
    }

    #[test]
    fn vibrato_delay_holds_each_note_still_first() {
        let (segs, frames) = sung_depths("[AA1<800,20> AA1<800,22>].", 300.0, 25.0);
        for seg in &segs[1..3] {
            assert_eq!(depth_at(&frames, seg.start + 100.0), 0.0);
            assert_eq!(depth_at(&frames, seg.start + 299.0), 0.0);
            assert!((depth_at(&frames, seg.start + 300.0 + VIBRATO_FADE / 2.0) - 12.5).abs() < 0.01);
            assert_eq!(depth_at(&frames, seg.start + 500.0), 25.0);
        }
    }

    #[test]
    fn with_no_delay_vibrato_carries_on_across_notes() {
        let (segs, frames) = sung_depths("[AA1<500,20> AA1<500,22>].", 0.0, 25.0);
        assert_eq!(depth_at(&frames, segs[2].start + 1.0), 25.0);
    }

    #[test]
    fn a_note_shorter_than_its_delay_has_no_vibrato() {
        let (_, frames) = sung_depths("[AA1<200,20> AA1<200,22>].", 300.0, 25.0);
        assert!(frames.iter().all(|(_, f)| f.vibrato == 0.0));
    }

    #[test]
    fn a_note_shorter_than_its_fade_gets_part_of_the_way() {
        let (segs, frames) = sung_depths("[AA1<60,20> AA1<500,300>].", 0.0, 24.0);
        assert!((depth_at(&frames, segs[1].end() - 0.01) - 12.0).abs() < 0.1);
        assert_eq!(depth_at(&frames, segs[2].start + 100.0), 0.0);
    }

    #[test]
    fn a_depth_of_nothing_is_no_vibrato_at_all() {
        let (_, frames) = sung_depths("[AA1<800,20>].", 0.0, 0.0);
        assert!(frames.iter().all(|(_, f)| f.vibrato == 0.0));
    }

    #[test]
    fn sung_vowels_hold_their_level_and_never_creak() {
        let (segs, frames) = plan("[B AE1<800,20> T].");
        let vowel = segs.iter().find(|s| s.phoneme == Phoneme::AE).unwrap();
        assert_eq!(frame_at(&frames, vowel.end() - 10.0).av, frame_at(&frames, vowel.start + 100.0).av);
        assert!(frames.iter().all(|(_, f)| f.creak == 0.0));
    }

    /// Plans with a head size, at the male model's usual settings.
    fn with_head(input: &str, head_size: f64) -> Vec<(f64, Frame)> {
        let mut segs = segments(&parse(input).unwrap());
        assign_durations(&mut segs, 1.0);
        let pitch = pitch_keyframes(&segs, 110.0, 1.0);
        frames(&segs, &pitch, &Voice { head_size, ..Voice::default() })
    }

    /// Plans with a voice as given.
    fn with_voice(input: &str, voice: &Voice) -> (Vec<Segment>, Vec<(f64, Frame)>) {
        let mut segs = segments(&parse(input).unwrap());
        assign_durations(&mut segs, 1.0);
        let pitch = pitch_keyframes(&segs, 110.0, 1.0);
        let frames = frames(&segs, &pitch, voice);
        (segs, frames)
    }

    fn upper_voice() -> Voice {
        Voice {
            upper_formants: [Some(3600.0), Some(4200.0), Some(6000.0)],
            upper_bandwidths: [Some(300.0), Some(400.0), Some(1500.0)],
            ..Voice::default()
        }
    }

    #[test]
    fn upper_formants_and_bandwidths_are_what_was_set() {
        let (_, frames) = with_voice("[AA1 AA1].", &upper_voice());
        for (_, f) in &frames {
            assert_eq!((f.freq[3], f.freq[4], f.freq[5]), (3600.0, 4200.0, 6000.0));
            assert_eq!((f.bw[3], f.bw[4], f.bw[5]), (300.0, 400.0, 1500.0));
        }
    }

    #[test]
    fn upper_formants_leave_everything_else_alone() {
        let input = "[M AA1 S IY1 K AH0 N].";
        let (_, normal) = with_voice(input, &Voice::default());
        let (_, changed) = with_voice(input, &upper_voice());
        assert_eq!(normal.len(), changed.len());
        for ((t0, a), (t1, b)) in normal.iter().zip(&changed) {
            assert_eq!(t0, t1);
            assert_eq!(a.freq[..3], b.freq[..3]);
            assert_eq!(a.bw[..3], b.bw[..3]);
            assert_eq!((a.f0, a.av, a.af, a.ah, a.fnp, a.fnz), (b.f0, b.av, b.af, b.ah, b.fnp, b.fnz));
        }
    }

    #[test]
    fn a_set_formant_is_the_same_on_either_gender_model() {
        let female = Voice {
            gender: Gender::Female,
            upper_formants: [Some(3600.0), None, None],
            upper_bandwidths: [Some(300.0), None, None],
            ..Voice::default()
        };
        let (_, frames) = with_voice("[AA1 AA1].", &female);
        let f = frames[3].1;
        assert_eq!((f.freq[3], f.bw[3]), (3600.0, 300.0));
        // Those not set still come from the model.
        assert!((f.freq[4] - 3750.0 * 1.17).abs() < 1e-9 && (f.bw[4] - 200.0 * 1.25).abs() < 1e-9);
    }

    #[test]
    fn head_size_still_moves_a_set_formant() {
        let voice = Voice { head_size: 125.0, ..upper_voice() };
        let (_, frames) = with_voice("[AA1 AA1].", &voice);
        let f = frames[3].1;
        assert_eq!((f.freq[3], f.freq[4], f.freq[5]), (3600.0 * 0.8, 4200.0 * 0.8, 6000.0 * 0.8));
        assert_eq!((f.bw[3], f.bw[4], f.bw[5]), (300.0 * 0.8, 400.0 * 0.8, 1500.0 * 0.8));
    }

    #[test]
    fn sharp_sounds_keep_their_higher_f6_when_it_is_set() {
        let voice = Voice { upper_formants: [None, None, Some(6000.0)], ..Voice::default() };
        let f6_during_s = |voice: &Voice| {
            let (segs, frames) = with_voice("[AA1 S AA1].", voice);
            let s = segs.iter().find(|seg| seg.phoneme == Phoneme::S).unwrap();
            frame_at(&frames, s.start + s.duration / 2.0).freq[5]
        };
        // /s/ raises F6 from 4900 to 5500, and by that share of any value set.
        assert_eq!(f6_during_s(&Voice::default()), 5500.0);
        assert!((f6_during_s(&voice) - 6000.0 * 5500.0 / 4900.0).abs() < 1e-9);
    }

    /// The frame halfway through the first of these sounds.
    fn middle_of(segs: &[Segment], frames: &[(f64, Frame)], phoneme: Phoneme) -> Frame {
        let seg = segs.iter().find(|s| s.phoneme == phoneme).unwrap();
        frame_at(frames, seg.start + seg.duration / 2.0)
    }

    #[test]
    fn lax_breath_comes_at_the_end_of_a_sentence_and_before_voiceless_sounds() {
        let lax = Voice { lax_breathiness: 100.0, ..Voice::default() };
        let input = "[M AA1 M AA1 S AA1 M], [M AA1 M AA1 S M AA1 M].";
        let (segs, frames) = with_voice(input, &lax);
        let (_, plain) = with_voice(input, &Voice::default());
        let vowels: Vec<&Segment> = segs.iter().filter(|s| s.phoneme == Phoneme::AA).collect();
        let late = |seg: &Segment| seg.end() - 5.0;
        // Not in a vowel before a voiced sound.
        assert_eq!(frame_at(&frames, late(vowels[0])).ah, 0.0);
        assert_eq!(frame_at(&frames, late(vowels[0])).tl, frame_at(&plain, late(vowels[0])).tl);
        // Before S, yes: breath comes up and the voice's high frequencies fall away.
        let before_s = frame_at(&frames, late(vowels[1]));
        assert!(before_s.ah > 35.0, "{}", before_s.ah);
        assert!(before_s.tl > frame_at(&plain, late(vowels[1])).tl + 5.0, "{}", before_s.tl);
        // Not at a comma, which isn't the end of a sentence.
        assert_eq!(frame_at(&frames, late(vowels[2])).ah, 0.0);
        // At the end of the sentence, the most.
        let end = frame_at(&frames, late(vowels[5]));
        assert!(end.ah > 45.0, "{}", end.ah);
        assert!(end.tl > frame_at(&plain, late(vowels[5])).tl + 9.0, "{}", end.tl);
        // Just before it starts, none.
        assert_eq!(frame_at(&frames, vowels[5].start - 1.0).ah, 0.0);
    }

    #[test]
    fn lax_breath_adds_to_a_voices_breath_and_scales_with_the_setting() {
        let input = "[M AA1 M].";
        let end_breath = |voice: &Voice| {
            let (segs, frames) = with_voice(input, voice);
            let vowel = segs.iter().find(|s| s.phoneme == Phoneme::AA).unwrap();
            frame_at(&frames, vowel.end() - 5.0).ah
        };
        let breathy = Voice { breathiness: 40.0, ..Voice::default() };
        let more = Voice { lax_breathiness: 100.0, ..breathy };
        assert!(end_breath(&more) > end_breath(&breathy) + 3.0);
        let half = Voice { lax_breathiness: 50.0, ..Voice::default() };
        let full = Voice { lax_breathiness: 100.0, ..Voice::default() };
        assert!(end_breath(&half) < end_breath(&full) - 10.0);
        assert_eq!(added_db(0.0, 40.0), 40.0);
        assert_eq!(added_db(40.0, 0.0), 40.0);
        assert!((added_db(40.0, 40.0) - 46.02).abs() < 0.01);
    }

    #[test]
    fn no_lax_breath_while_singing() {
        let lax = Voice { lax_breathiness: 100.0, ..Voice::default() };
        let input = "[AA1<500,25> S AA1<500,25>].";
        let (segs, frames) = with_voice(input, &lax);
        let first = segs.iter().find(|s| s.phoneme == Phoneme::AA).unwrap();
        assert_eq!(frame_at(&frames, first.end() - 5.0).ah, 0.0);
    }

    #[test]
    fn each_gain_moves_its_own_sound_and_no_other() {
        use crate::voicedefs::SourceGains;
        let input = "[HH AE1 S M AA1].";
        let (segs, normal) = with_voice(input, &Voice::default());
        let at = |gains: SourceGains, phoneme: Phoneme| {
            let (_, frames) = with_voice(input, &Voice { gains, ..Voice::default() });
            middle_of(&segs, &frames, phoneme)
        };
        let usual = |phoneme: Phoneme| middle_of(&segs, &normal, phoneme);
        let half_db = 20.0 * 0.5f64.log10();
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        let half = |voicing, frication, aspiration, nasal| SourceGains { voicing, frication, aspiration, nasal };

        let voicing = half(0.5, 1.0, 1.0, 1.0);
        assert!(near(at(voicing, Phoneme::AE).av, usual(Phoneme::AE).av + half_db));
        assert!(near(at(voicing, Phoneme::M).av, usual(Phoneme::M).av + half_db));
        assert_eq!(at(voicing, Phoneme::S).af, usual(Phoneme::S).af);
        assert_eq!(at(voicing, Phoneme::HH).ah, usual(Phoneme::HH).ah);

        let frication = half(1.0, 0.5, 1.0, 1.0);
        assert!(near(at(frication, Phoneme::S).af, usual(Phoneme::S).af + half_db));
        assert_eq!(at(frication, Phoneme::AE).av, usual(Phoneme::AE).av);
        assert_eq!(at(frication, Phoneme::HH).ah, usual(Phoneme::HH).ah);

        let aspiration = half(1.0, 1.0, 0.5, 1.0);
        assert!(near(at(aspiration, Phoneme::HH).ah, usual(Phoneme::HH).ah + half_db));
        assert_eq!(at(aspiration, Phoneme::S).af, usual(Phoneme::S).af);
        assert_eq!(at(aspiration, Phoneme::AE).av, usual(Phoneme::AE).av);

        let nasal = half(1.0, 1.0, 1.0, 0.5);
        assert!(near(at(nasal, Phoneme::M).av, usual(Phoneme::M).av + half_db));
        assert_eq!(at(nasal, Phoneme::AE).av, usual(Phoneme::AE).av);
        assert_eq!(at(nasal, Phoneme::S).af, usual(Phoneme::S).af);

        // Zero is off.
        assert_eq!(at(half(0.0, 1.0, 1.0, 1.0), Phoneme::AE).av, 0.0);
        assert_eq!(at(half(1.0, 0.0, 1.0, 1.0), Phoneme::S).af, 0.0);
        assert_eq!(at(half(1.0, 1.0, 0.0, 1.0), Phoneme::HH).ah, 0.0);
        assert_eq!(at(half(1.0, 1.0, 1.0, 0.0), Phoneme::M).av, 0.0);
    }

    #[test]
    fn a_gain_leaves_silence_silent() {
        use crate::voicedefs::SourceGains;
        let gains = SourceGains { voicing: 2.0, frication: 2.0, aspiration: 2.0, nasal: 2.0 };
        let input = "[AA1 T AA1].";
        let (segs, frames) = with_voice(input, &Voice { gains, ..Voice::default() });
        let closure = segs.iter().find(|s| s.phoneme == Phoneme::T).unwrap();
        let f = frame_at(&frames, closure.start + closure.duration / 2.0);
        assert_eq!((f.av, f.af, f.ah), (0.0, 0.0, 0.0));
        assert_eq!(scaled(0.0, 2.0), 0.0);
        assert_eq!(scaled(60.0, 1.0), 60.0);
        assert!((scaled(60.0, 2.0) - 66.0206).abs() < 0.001);
    }

    #[test]
    fn aspiration_leaves_a_voices_breath_alone_and_no_voicing_leaves_a_whisper() {
        use crate::voicedefs::SourceGains;
        let input = "[HH AE1 S].";
        let quiet = SourceGains { aspiration: 0.0, ..SourceGains::NORMAL };
        let breathy = Voice { breathiness: 50.0, gains: quiet, ..Voice::default() };
        let (segs, frames) = with_voice(input, &breathy);
        // No H, and the breath through the vowel as it was.
        assert!(middle_of(&segs, &frames, Phoneme::AE).ah > 45.0);
        assert_eq!(middle_of(&segs, &frames, Phoneme::HH).ah, 0.0);

        let silent = SourceGains { voicing: 0.0, ..SourceGains::NORMAL };
        let whisper = Voice { breathiness: 60.0, gains: silent, ..Voice::default() };
        let (segs, frames) = with_voice(input, &whisper);
        let vowel = middle_of(&segs, &frames, Phoneme::AE);
        assert_eq!(vowel.av, 0.0);
        assert!(vowel.ah > 50.0, "{}", vowel.ah);
    }

    #[test]
    fn head_size_stretches_the_whole_spectrum_by_one_factor() {
        let input = "[M AA1 S IY1 K AH0 N].";
        let normal = with_head(input, 100.0);
        for head_size in [64.0, 80.0, 125.0, 135.0] {
            let scaled = with_head(input, head_size);
            let share = 100.0 / head_size;
            assert_eq!(normal.len(), scaled.len());
            for ((t0, a), (t1, b)) in normal.iter().zip(&scaled) {
                assert_eq!(t0, t1);
                let same = |x: f64, y: f64| (x * share - y).abs() < 1e-9;
                assert!((0..NUM_FORMANTS).all(|k| same(a.freq[k], b.freq[k])), "{:?} {:?}", a.freq, b.freq);
                assert!((0..NUM_FORMANTS).all(|k| same(a.bw[k], b.bw[k])), "{:?} {:?}", a.bw, b.bw);
                assert!((0..NUM_FORMANTS).all(|k| same(a.par_bw[k], b.par_bw[k])));
                assert!(same(a.fnp, b.fnp) && same(a.fnz, b.fnz) && same(a.bnp, b.bnp) && same(a.bnz, b.bnz));
                // Nothing else about the sound moves.
                assert_eq!((a.f0, a.av, a.ah, a.af, a.oq, a.tl), (b.f0, b.av, b.ah, b.af, b.oq, b.tl));
            }
        }
    }

    #[test]
    fn a_larger_head_is_lower_and_a_smaller_one_higher() {
        let f1 = |head_size: f64| with_head("[AA1 AA1 AA1].", head_size)[3].1.freq[0];
        assert!(f1(135.0) < f1(100.0) && f1(100.0) < f1(64.0));
    }

    #[test]
    fn the_nasal_zero_still_cancels_outside_nasals_at_any_head_size() {
        for (_, f) in with_head("[AA1 S AA1].", 80.0) {
            assert_eq!(f.fnp, f.fnz);
        }
    }

    #[test]
    fn the_female_model_follows_its_own_table() {
        let f = |input: &str, vowel: Phoneme| {
            let (segs, frames) = female_plan(input);
            let seg = segs.iter().find(|s| s.phoneme == vowel).unwrap();
            frame_at(&frames, seg.start + seg.duration / 2.0)
        };
        // /i/ rises more in F1 than /ɪ/, as measured.
        let iy = f("[P IY1 P].", Phoneme::IY);
        assert!((iy.freq[0] - 338.0 * 1.29).abs() < 2.0 && (iy.freq[1] - 2319.0 * 1.19).abs() < 3.0, "{:?}", iy.freq);
        let ih = f("[P IH1 P].", Phoneme::IH);
        assert!((ih.freq[0] - 433.0 * 1.12).abs() < 2.0, "{:?}", ih.freq);
        assert!(iy.freq[3] > 3800.0 && iy.bw[0] > 70.0);
        // Nasal resonances rise together, so they still cancel outside nasals.
        assert_eq!(iy.fnp, iy.fnz);
        let n = f("[AA1 N AA1].", Phoneme::N);
        assert!(n.fnz > 1600.0 && n.fnp != n.fnz);
    }

    #[test]
    fn the_female_model_scales_once_and_holds_what_is_held() {
        // /h/ takes the shape of its vowel, not that scaled again.
        let (segs, frames) = female_plan("[HH IY1].");
        let h = frame_at(&frames, segs[1].start + 1.0);
        assert!((h.freq[1] - 2319.0 * 1.19).abs() < 3.0, "{:?}", h.freq);
        // "four" stays steady before the R, with no glide from the two shares.
        let held = targets(&segments(&parse("[F OW1 R].").unwrap()), 1, Gender::Female);
        assert_eq!(held.0.freq, held.1.freq);
    }

    #[test]
    fn breath_noise_follows_voicing() {
        let (segs, frames) = female_plan("[P AA1 S AA1].");
        let vowel = segs.iter().find(|s| s.phoneme == Phoneme::AA).unwrap();
        let mid = frame_at(&frames, vowel.start + vowel.duration / 2.0);
        assert!(mid.ah > 30.0 && mid.ah < 40.0, "{}", mid.ah);
        let s = segs.iter().find(|s| s.phoneme == Phoneme::S).unwrap();
        assert_eq!(frame_at(&frames, s.start + s.duration / 2.0).ah, 0.0);
        // The male voice has none.
        let (segs, frames) = plan("[P AA1 S AA1].");
        let vowel = segs.iter().find(|s| s.phoneme == Phoneme::AA).unwrap();
        assert_eq!(frame_at(&frames, vowel.start + vowel.duration / 2.0).ah, 0.0);
    }

    #[test]
    fn vowels_reach_their_targets_and_stops_are_silent() {
        let (segs, frames) = plan("[P AA1 P AA1].");
        let vowel = segs.iter().find(|s| s.phoneme == Phoneme::AA).unwrap();
        let mid = frame_at(&frames, vowel.start + vowel.duration / 2.0);
        assert!((mid.freq[0] - 750.0).abs() < 30.0 && (mid.freq[1] - 1200.0).abs() < 60.0, "{:?}", mid.freq);
        assert!(mid.av >= 59.0);
        let closure = segs.iter().filter(|s| s.phoneme == Phoneme::P).nth(1).unwrap();
        let shut = frame_at(&frames, closure.start + closure.duration / 2.0);
        assert_eq!((shut.av, shut.af, shut.ah), (0.0, 0.0, 0.0));
    }

    /// Formant `k` at time `t`, interpolated between frames.
    fn formant_at(frames: &[(f64, Frame)], k: usize, t: f64) -> f64 {
        let i = frames.partition_point(|f| f.0 <= t);
        let ((t0, a), (t1, b)) = (frames[i - 1], frames[i]);
        a.freq[k] + (b.freq[k] - a.freq[k]) * (t - t0) / (t1 - t0)
    }

    #[test]
    fn r_after_a_vowel_is_soft_and_glides_in() {
        let (segs, frames) = plan("[K AA1 R] [R EH1 D].");
        let find = |p: Phoneme| segs.iter().find(|s| s.phoneme == p).unwrap();
        let (vowel, coda) = (find(Phoneme::AA), find(Phoneme::R));
        // The R of "car" is milder than the R of "red"...
        let soft = formant_at(&frames, 2, coda.start + 0.6 * coda.duration);
        let onset = segs.iter().rfind(|s| s.phoneme == Phoneme::R).unwrap();
        let full = formant_at(&frames, 2, onset.start + 0.5 * onset.duration);
        assert!((soft - CODA_R[2]).abs() < 50.0 && full < 1500.0, "{soft} {full}");
        // ...and colors the second half of the vowel.
        let late = formant_at(&frames, 2, vowel.start + 0.8 * vowel.duration);
        assert!(late < 2300.0, "{late}");
        // Between vowels it is soft before an unstressed one ("very"), full
        // before a stressed one ("arrive").
        let segs = segments(&parse("[V EH1 R IY0] [ER0 AY1 V].").unwrap());
        let rs: Vec<usize> = (0..segs.len()).filter(|&i| segs[i].phoneme == Phoneme::R).collect();
        assert!(is_soft_r(&segs, rs[0]));
        let segs = segments(&parse("[AH0 R AY1 V].").unwrap());
        let r = segs.iter().position(|s| s.phoneme == Phoneme::R).unwrap();
        assert!(!is_soft_r(&segs, r));
    }

    #[test]
    fn velars_move_forward_next_to_front_vowels() {
        // "rig": F2 rises and F3 falls into the G, unlike "rid"; "dog" keeps
        // the back locus, well below the front one.
        let f2_f3_at_close = |input: &str, stop: Phoneme| {
            let (segs, frames) = plan(input);
            let s = segs.iter().find(|s| s.phoneme == stop).unwrap();
            let f = frame_at(&frames, s.start);
            (f.freq[1], f.freq[2])
        };
        let (g2, g3) = f2_f3_at_close("[R IH1 G].", Phoneme::G);
        let (d2, _) = f2_f3_at_close("[R IH1 D].", Phoneme::D);
        assert!(g2 > 2100.0 && g3 < 2550.0 && d2 < 1950.0, "{g2} {g3} {d2}");
        let (back, _) = f2_f3_at_close("[D AO1 G].", Phoneme::G);
        assert!(back < 1900.0, "{back}");
    }

    #[test]
    fn a_flap_is_a_clear_dip() {
        // "butter": the tap is heard as a dip of about 16 dB, with F1 low.
        let (segs, frames) = plan("[B AH1 T ER0].");
        let flap = segs.iter().find(|s| s.phoneme == Phoneme::DX).unwrap();
        let mid = frame_at(&frames, flap.start + 0.5 * flap.duration);
        assert!(mid.av <= 45.0 && mid.freq[0] < 300.0, "{} {}", mid.av, mid.freq[0]);
    }

    #[test]
    fn syllabic_l_has_no_vowel_before_it() {
        // "bottle" is [B AA1 T AH0 L] in the dictionary, but the last
        // syllable is a dark L alone: its F2 stays low, never a schwa's.
        let (segs, frames) = plan("[B AA1 T AH0 L].");
        let schwa = segs.iter().find(|s| s.phoneme == Phoneme::AX).unwrap();
        let f2 = formant_at(&frames, 1, schwa.start + 0.5 * schwa.duration);
        assert!((f2 - spec(Phoneme::LX).formants[1]).abs() < 100.0, "{f2}");
        // After a vowel the schwa stays: "trial".
        let trial = segments(&parse("[T R AY1 AH0 L].").unwrap());
        let schwa = trial.iter().position(|s| s.phoneme == Phoneme::AX).unwrap();
        assert_eq!(syllabic_consonant(&trial, schwa), None);
    }

    #[test]
    fn syllabic_n_follows_an_unreleased_t() {
        // "button": no flap, no burst, and the last syllable is the N murmur.
        let (segs, frames) = plan("[B AH1 T AH0 N].");
        let t = segs.iter().find(|s| s.phoneme.manner() == Manner::Stop && s.phoneme != Phoneme::B).unwrap();
        assert!(t.phoneme == Phoneme::T && !t.released);
        let schwa = segs.iter().find(|s| s.phoneme == Phoneme::AX).unwrap();
        let mid = frame_at(&frames, schwa.start + 0.5 * schwa.duration);
        assert_eq!((mid.fnp, mid.fnz), (spec(Phoneme::N).sound.fnp, spec(Phoneme::N).sound.fnz));
        assert!(mid.af == 0.0 && mid.ah == 0.0);
    }

    #[test]
    fn a_dark_l_pulls_the_vowel_before_it_back() {
        let f2 = |input: &str, vowel: Phoneme| {
            let (segs, frames) = plan(input);
            let seg = segs.iter().find(|s| s.phoneme == vowel).unwrap();
            formant_at(&frames, 1, seg.start + 0.3 * seg.duration)
        };
        // "fill" and "fell" stand farther back than "fit" and "fet".
        assert!(f2("[F IH1 L].", Phoneme::IH) < f2("[F IH1 T].", Phoneme::IH) - 200.0);
        assert!(f2("[F EH1 L].", Phoneme::EH) < f2("[F EH1 T].", Phoneme::EH) - 200.0);
        // "feel" keeps its target, so the gap to "fill" widens.
        assert!(f2("[F IY1 L].", Phoneme::IY) > f2("[F IH1 L].", Phoneme::IH) + 400.0);
        // "fool" holds its back target; "food" glides to it.
        assert!(f2("[F UW1 L].", Phoneme::UW) < f2("[F UW1 D].", Phoneme::UW) - 100.0);
        // An L that starts a syllable changes nothing: "fillet" isn't dark.
        assert_eq!(f2("[F IH1 L AH0 T].", Phoneme::IH), f2("[F IH1 R AH0 T].", Phoneme::IH));
    }

    #[test]
    fn the_voice_creaks_into_a_glottalized_t() {
        // "bat now": clear at the start of the vowel, creaky at its end, and
        // clear again after the release.
        let (segs, frames) = plan("[B AE1 T] [N AW1].");
        let vowel = segs.iter().find(|s| s.phoneme == Phoneme::AE).unwrap();
        let t = segs.iter().find(|s| s.phoneme == Phoneme::T).unwrap();
        assert!(t.glottalized && t.released);
        assert_eq!(frame_at(&frames, vowel.start + 5.0).creak, 0.0);
        assert!(frame_at(&frames, vowel.end() - 5.0).creak > 0.8);
        assert_eq!(frame_at(&frames, t.end() + 5.0).creak, 0.0);
        // No creak without a glottalized /t/.
        let (_, frames) = plan("[B AE1 D] [N AW1].");
        assert!(frames.iter().all(|(_, f)| f.creak == 0.0));
    }

    #[test]
    fn aspiration_follows_a_voiceless_release() {
        let (segs, frames) = plan("[T IY1].");
        let vowel = segs.iter().find(|s| s.phoneme == Phoneme::IY).unwrap();
        let during = frame_at(&frames, vowel.start + 20.0);
        assert!(during.ah > 50.0 && during.av == 0.0, "{during:?}");
        let later = frame_at(&frames, vowel.start + 80.0);
        assert!(later.av > 55.0);
    }

    #[test]
    fn fricatives_stay_well_below_vowels() {
        // Natural English: /s/ is about 16 dB and /f/ about 21 dB weaker than
        // /ɑ/ (Fletcher 1953). Ours may be a little stronger, never close.
        for (consonant, floor, ceiling) in [("S", -16.0, -10.0), ("F", -21.0, -15.0)] {
            let (_, frames) = plan(&format!("[AA1 {consonant} AA1]."));
            let audio = render(&mut Synth::new(22050), &frames);
            let span = |pick: &dyn Fn(&Frame) -> bool| {
                let times: Vec<f64> = frames.iter().filter(|f| pick(&f.1)).map(|f| f.0).collect();
                let (a, b) = ((times[0] * 22.05) as usize, (times[times.len() - 1] * 22.05) as usize);
                (audio[a..b].iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / (b - a) as f64).sqrt()
            };
            let vowel = span(&|f| f.av >= 59.9);
            let noise = span(&|f| f.af > 50.0 && f.av == 0.0);
            let db = 20.0 * (noise / vowel).log10();
            assert!(db > floor && db < ceiling, "{consonant}: {db:.1} dB");
        }
    }

    #[test]
    fn pauses_are_silent_throughout() {
        let (segs, frames) = plan("[AA1], [AA1].");
        let audio = render(&mut Synth::new(22050), &frames);
        let pause = segs.iter().find(|s| s.boundary.is_some_and(|b| b.has_pause())).unwrap();
        let quiet = (pause.start + 30.0) * 22.05..(pause.end() - 5.0) * 22.05;
        let loudest = audio[quiet.start as usize..quiet.end as usize].iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(loudest < 0.01, "{loudest}");
    }

    #[test]
    fn nasals_open_the_nasal_zero() {
        let (segs, frames) = plan("[AA1 N AA1].");
        let n = segs.iter().find(|s| s.phoneme == Phoneme::N).unwrap();
        let mid = frame_at(&frames, n.start + n.duration / 2.0);
        assert!(mid.fnz > 1000.0 && mid.fnp != mid.fnz);
        let vowel = frame_at(&frames, segs[1].start + segs[1].duration / 2.0);
        assert_eq!(vowel.fnz, vowel.fnp);
    }

    #[test]
    fn renders_audio_of_the_right_length() {
        let (segs, frames) = plan("[HH AH0 L OW1] [W ER1 L D].");
        let mut synth = Synth::new(22050);
        let audio = render(&mut synth, &frames);
        let expected = segs.last().unwrap().end() * 22.05;
        assert!((audio.len() as f64 - expected).abs() <= 1.0);
        assert!(audio.iter().any(|x| x.abs() > 0.1));
    }

    #[test]
    fn streams_in_chunks_and_stops_on_request() {
        let (_, frames) = plan("[HH AH0 L OW1] [W ER1 L D].");
        let whole = render(&mut Synth::new(22050), &frames);
        let mut chunks = Vec::new();
        assert!(render_with(&mut Synth::new(22050), &frames, &mut |chunk| {
            chunks.push(chunk.to_vec());
            true
        }));
        assert!(chunks.len() > 3 && chunks.concat() == whole);
        let mut calls = 0;
        assert!(!render_with(&mut Synth::new(22050), &frames, &mut |_| {
            calls += 1;
            false
        }));
        assert_eq!(calls, 1);
    }
}
