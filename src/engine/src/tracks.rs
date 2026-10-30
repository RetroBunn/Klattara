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
use crate::phonetics::{CODA_R, FRONT_VELAR, NASALIZED_ZERO, OR_NUCLEUS, SILENT, Sound, spec};
use crate::synth::Synth;
use crate::utterance::{Segment, syllabic_consonant};

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

/// The start and end formant targets of segment `i`.
fn targets(segments: &[Segment], i: usize) -> (Formants, Formants) {
    let seg = &segments[i];
    // The schwa of a syllabic L or N takes the consonant's own shape.
    if let Some(consonant) = syllabic_consonant(segments, i) {
        let spec = spec(consonant);
        let held = Formants { freq: spec.formants, bw: spec.bandwidths };
        return (held, held);
    }
    if seg.phoneme == Phoneme::HH {
        // /h/ is breath through the vocal tract shape of its neighbour,
        // usually the vowel that follows.
        if segments.get(i + 1).is_some_and(|n| n.phoneme.is_sonorant()) {
            let (start, _) = targets(segments, i + 1);
            return (start, start);
        }
        if i > 0 && segments[i - 1].phoneme.is_sonorant() {
            let (_, end) = targets(segments, i - 1);
            return (end, end);
        }
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
fn join(segments: &[Segment], i: usize) -> Option<Join> {
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
    let (left_end, right_start) = (targets(segments, i).1, targets(segments, i + 1).0);
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

fn formant_track(segments: &[Segment]) -> Track<Formants> {
    let mut track = Track::new();
    for (i, seg) in segments.iter().enumerate() {
        if seg.phoneme == Phoneme::Pause {
            continue;
        }
        let (start, end) = targets(segments, i);
        let left = i.checked_sub(1).and_then(|j| join(segments, j));
        let right = join(segments, i);
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
        let (start, _) = targets(segments, 0);
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

/// The steady sound of segment `i` in its context.
fn steady_sound(segments: &[Segment], i: usize) -> Sound {
    let seg = &segments[i];
    // A syllabic N is the nasal murmur, at the level of an unstressed vowel.
    if syllabic_consonant(segments, i) == Some(Phoneme::N) {
        return Sound { av: 57.0, ..spec(Phoneme::N).sound };
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
        Manner::Stop if seg.phoneme.is_voiced() && after_voicing => sound = VOICE_BAR,
        // Part of the vowel, so nearly as loud as one.
        Manner::Liquid if is_soft_r(segments, i) => sound.av = 59.0,
        _ => {}
    }
    sound
}

fn sound_track(segments: &[Segment]) -> Track<Sound> {
    let mut track = Track::new();
    for (i, seg) in segments.iter().enumerate() {
        let (s, e, d) = (seg.start, seg.end(), seg.duration);
        let manner = seg.phoneme.manner();
        let edge = edge_ms(manner).min(d / 2.0);
        let steady = steady_sound(segments, i);
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
                if seg.phoneme.is_vowel() && seg.in_clause_final_syllable {
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
        if !seg.glottalized || i == 0 {
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

/// Builds synthesizer frames from timed segments and pitch keyframes, using
/// the voice's open quotient and spectral tilt.
pub fn frames(segments: &[Segment], pitch: &[(f64, f64)], oq: f64, tl: f64) -> Vec<(f64, Frame)> {
    let formants = formant_track(segments);
    let sound = sound_track(segments);
    let creak = creak_track(segments);
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
        .chain([0.0, end])
        .filter(|&t| (0.0..=end).contains(&t))
        .collect();
    times.sort_by(f64::total_cmp);
    times.dedup_by(|a, b| (*a - *b).abs() < 0.05);

    let defaults = Frame::default();
    times
        .into_iter()
        .map(|t| {
            let f = formants.at(t);
            let s = sound.at(t);
            let frame = Frame {
                f0: f0.at(t),
                av: s.av,
                oq,
                tl: tl + s.tl,
                creak: creak.at(t),
                ah: s.ah,
                af: s.af,
                freq: [f.freq[0], f.freq[1], f.freq[2], defaults.freq[3], defaults.freq[4], s.f6],
                bw: [f.bw[0], f.bw[1], f.bw[2], defaults.bw[3], defaults.bw[4], defaults.bw[5]],
                fnp: s.fnp,
                bnp: s.bnp,
                fnz: s.fnz,
                bnz: 100.0,
                par_amp: s.par_amp,
                par_bw: defaults.par_bw,
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
    use crate::intonation::pitch_keyframes;
    use crate::timing::assign_durations;
    use crate::utterance::{parse, segments};

    fn plan(input: &str) -> (Vec<Segment>, Vec<(f64, Frame)>) {
        let mut segs = segments(&parse(input).unwrap());
        assign_durations(&mut segs, 1.0);
        let pitch = pitch_keyframes(&segs, 110.0, 1.0);
        let frames = frames(&segs, &pitch, 0.4, 0.0);
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
