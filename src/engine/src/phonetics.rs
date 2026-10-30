//! Acoustic targets and timing data for each phoneme.
//!
//! Durations are Klatt's inherent and minimum durations (Klatt 1979, Table 1).
//! Vowel formants are medians for the 45 men of Hillenbrand et al. (1995);
//! /æ/ and /ɑ/ are moved partway toward Peterson & Barney (1952), because
//! Hillenbrand's Michigan talkers raise and front them. /o/ and /u/ glide as
//! in Klatt's (1980) synthesizer, with Hillenbrand's F3: his talkers hold
//! them nearly steady, as Upper Midwest speakers do, which sounded off
//! against General American. Consonant loci follow the usual
//! acoustic-phonetic descriptions of place of articulation.

use crate::phoneme::{Manner, Phoneme, Place};

/// F6 when no segment asks for a higher one.
pub const DEFAULT_F6: f64 = 4900.0;
/// The nasal pole frequency. Setting the nasal zero to the same value
/// cancels both, which is how non-nasal sounds switch nasality off.
pub const NASAL_POLE: f64 = 270.0;
/// The nasal zero of a nasalized vowel. Raised above the nasal pole, it
/// adds a low pole-zero pair that flattens the F1 region (Klatt 1980).
pub const NASALIZED_ZERO: f64 = 480.0;
/// The steady vowel of "four" and "corn" before R, between /o/ and /ɔ/
/// (Klatt's OR nucleus).
pub const OR_NUCLEUS: [f64; 3] = [520.0, 920.0, 2450.0];
/// R after a vowel in its syllable ("car", "there"): r-coloring of the vowel,
/// near the vowel of "heard", rather than the tighter R of "red".
pub const CODA_R: [f64; 3] = [470.0, 1300.0, 1650.0];
/// F2 and F3 of the velar locus next to a front vowel ("big", "keep").
pub const FRONT_VELAR: [f64; 2] = [2250.0, 2350.0];

/// The source and parallel-branch settings of a segment: everything about
/// its sound except the main formant frequencies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sound {
    /// Amplitudes in dB, as in [`crate::Frame`].
    pub av: f64,
    pub ah: f64,
    pub af: f64,
    pub ab: f64,
    pub par_amp: [f64; 6],
    /// F6, raised for sharp sibilants.
    pub f6: f64,
    /// Nasal pole and zero frequencies; both equal to [`NASAL_POLE`] when
    /// not nasal, so they cancel.
    pub fnp: f64,
    pub fnz: f64,
    /// Nasal pole bandwidth.
    pub bnp: f64,
    /// Spectral tilt added to the voice's own, dB.
    pub tl: f64,
}

pub const SILENT: Sound = Sound {
    av: 0.0,
    ah: 0.0,
    af: 0.0,
    ab: 0.0,
    par_amp: [0.0; 6],
    f6: DEFAULT_F6,
    fnp: NASAL_POLE,
    fnz: NASAL_POLE,
    bnp: 100.0,
    tl: 0.0,
};

impl Sound {
    /// Interpolates toward `other`. Amplitudes move linearly in the linear
    /// domain, matching how the synthesizer glides between frames.
    pub fn lerp(&self, other: &Sound, t: f64) -> Sound {
        let mix = |a: f64, b: f64| a + (b - a) * t;
        let mix_db = |a: f64, b: f64| gain_to_db(mix(db_to_gain(a), db_to_gain(b)));
        Sound {
            av: mix_db(self.av, other.av),
            ah: mix_db(self.ah, other.ah),
            af: mix_db(self.af, other.af),
            ab: mix_db(self.ab, other.ab),
            par_amp: std::array::from_fn(|k| mix_db(self.par_amp[k], other.par_amp[k])),
            f6: mix(self.f6, other.f6),
            fnp: mix(self.fnp, other.fnp),
            fnz: mix(self.fnz, other.fnz),
            bnp: mix(self.bnp, other.bnp),
            tl: mix(self.tl, other.tl),
        }
    }
}

fn db_to_gain(db: f64) -> f64 {
    if db <= 0.0 { 0.0 } else { 10f64.powf(db / 20.0) }
}

fn gain_to_db(gain: f64) -> f64 {
    if gain <= 1.0 { 0.0 } else { 20.0 * gain.log10() }
}

/// The release of a stop or affricate: a burst of frication, then (for
/// voiceless stops) aspiration until voicing begins.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Release {
    pub burst: Sound,
    /// Duration of the burst, ms.
    pub burst_ms: f64,
    /// Voice onset time when fully aspirated (stressed syllable onset), ms.
    pub vot_ms: f64,
}

/// Everything the rules need to know about one phoneme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spec {
    /// Klatt's inherent and minimum durations, ms.
    pub inherent: f64,
    pub minimum: f64,
    /// F1 to F3: the target of vowels and sonorants, the locus of obstruents.
    pub formants: [f64; 3],
    /// Where a diphthong glides to.
    pub glide_to: Option<[f64; 3]>,
    pub bandwidths: [f64; 3],
    /// How far a neighbour pulls each formant at the shared boundary when
    /// this segment dominates it: 0 keeps this segment's own value, 1 takes
    /// the neighbour's.
    pub coarticulation: [f64; 3],
    /// Dominance at boundaries. The higher-ranked segment sets the boundary
    /// value; the other one moves toward it.
    pub rank: u8,
    /// Formant transition time inside a neighbour when this segment
    /// dominates, and inside this segment itself, ms.
    pub transition_out: f64,
    pub transition_in: f64,
    /// Steady-state sound. For stops this is the closure.
    pub sound: Sound,
    pub release: Option<Release>,
    /// For affricates, the fraction of the duration that is closure.
    pub closure_fraction: f64,
}

const VOWEL_BANDWIDTHS: [f64; 3] = [70.0, 100.0, 150.0];

fn vowel(inherent: f64, minimum: f64, formants: [f64; 3]) -> Spec {
    Spec {
        inherent,
        minimum,
        formants,
        glide_to: None,
        bandwidths: VOWEL_BANDWIDTHS,
        coarticulation: [0.5; 3],
        rank: 0,
        transition_out: 40.0,
        transition_in: 40.0,
        sound: Sound { av: 60.0, ..SILENT },
        release: None,
        closure_fraction: 0.0,
    }
}

fn diphthong(inherent: f64, minimum: f64, from: [f64; 3], to: [f64; 3]) -> Spec {
    Spec { glide_to: Some(to), ..vowel(inherent, minimum, from) }
}

fn sonorant(inherent: f64, minimum: f64, formants: [f64; 3], bandwidths: [f64; 3], av: f64) -> Spec {
    Spec {
        bandwidths,
        coarticulation: [0.3, 0.35, 0.35],
        rank: 2,
        transition_out: 40.0,
        transition_in: 35.0,
        sound: Sound { av, ..SILENT },
        ..vowel(inherent, minimum, formants)
    }
}

/// Formant loci and coarticulation for each place of articulation.
///
/// These give larger formant movements than the natural locus equations of
/// Sussman, McCaffrey & Matthews (1991). With the natural values, Whisper
/// recognized our speech worse, especially at double speed (46% of words
/// wrong instead of 39%). Synthetic speech seems to need its place cues
/// exaggerated, as DECtalk's are. The pull toward the neighbour is halved
/// again at the boundary (`tracks::PULL`) for the same reason.
fn locus(place: Place) -> ([f64; 3], [f64; 3]) {
    match place {
        Place::Labial => ([250.0, 900.0, 2100.0], [0.3, 0.6, 0.6]),
        Place::Labiodental => ([300.0, 1100.0, 2200.0], [0.35, 0.7, 0.7]),
        Place::Dental => ([300.0, 1500.0, 2600.0], [0.35, 0.6, 0.6]),
        Place::Alveolar => ([250.0, 1800.0, 2700.0], [0.3, 0.5, 0.5]),
        Place::Postalveolar => ([280.0, 2000.0, 2650.0], [0.3, 0.55, 0.55]),
        // F2 and F3 come together at the boundary (the "velar pinch").
        Place::Velar => ([250.0, 2000.0, 2100.0], [0.3, 0.5, 0.3]),
        Place::None => ([500.0, 1500.0, 2500.0], [0.5; 3]),
    }
}

fn obstruent(phoneme: Phoneme, inherent: f64, minimum: f64, sound: Sound) -> Spec {
    let (formants, coarticulation) = locus(phoneme.place());
    let (rank, transition_out) = match phoneme.manner() {
        Manner::Stop | Manner::Affricate => (5, 50.0),
        _ => (4, 40.0),
    };
    Spec {
        bandwidths: [90.0, 110.0, 150.0],
        coarticulation,
        rank,
        transition_out,
        transition_in: 12.0,
        sound,
        ..vowel(inherent, minimum, formants)
    }
}

/// A nasal murmur: one strong low resonance (F1), weak and heavily damped
/// higher ones, and a pole-zero pair from the nasal and oral cavities that
/// depends on place of articulation: for /m/ a zero near 900 Hz under a
/// resonance near 1300, for /n/ a zero near 1600 under one near 2000.
fn nasal(phoneme: Phoneme, inherent: f64, minimum: f64, pole: f64, zero: f64) -> Spec {
    let (loci, coarticulation) = locus(phoneme.place());
    Spec {
        formants: [280.0, loci[1], loci[2]],
        bandwidths: [100.0, 300.0, 300.0],
        coarticulation,
        rank: 3,
        transition_out: 45.0,
        transition_in: 15.0,
        sound: Sound { av: 55.0, fnp: pole, fnz: zero, bnp: 300.0, ..SILENT },
        ..vowel(inherent, minimum, loci)
    }
}

/// Frication spectra, as parallel formant amplitudes A1 to A6 plus bypass.
fn frication(af: f64, par_amp: [f64; 6], ab: f64) -> Sound {
    Sound { af, par_amp, ab, ..SILENT }
}

const S_NOISE: [f64; 6] = [0.0, 0.0, 0.0, 0.0, 42.0, 60.0];
const SH_NOISE: [f64; 6] = [0.0, 0.0, 54.0, 56.0, 52.0, 46.0];
const T_BURST: [f64; 6] = [0.0, 0.0, 0.0, 46.0, 52.0, 58.0];
const K_BURST: [f64; 6] = [0.0, 52.0, 60.0, 40.0, 0.0, 0.0];

/// Voicing during voiced obstruents: weaker, and with less high-frequency energy.
fn voiced(sound: Sound) -> Sound {
    Sound { av: 48.0, tl: 10.0, af: sound.af - 3.0, ..sound }
}

fn stop(phoneme: Phoneme, inherent: f64, minimum: f64, burst: Sound, burst_ms: f64, vot_ms: f64) -> Spec {
    let burst = if phoneme.is_voiced() { Sound { af: burst.af - 5.0, ..burst } } else { burst };
    Spec { release: Some(Release { burst, burst_ms, vot_ms }), ..obstruent(phoneme, inherent, minimum, SILENT) }
}

fn affricate(phoneme: Phoneme, closure: (f64, f64), frication: (f64, f64), noise: Sound) -> Spec {
    let sound = if phoneme.is_voiced() { voiced(noise) } else { noise };
    Spec {
        closure_fraction: closure.0 / (closure.0 + frication.0),
        ..obstruent(phoneme, closure.0 + frication.0, closure.1 + frication.1, sound)
    }
}

/// The specification of a phoneme.
pub fn spec(phoneme: Phoneme) -> Spec {
    use Phoneme::*;
    // Frication levels sit 2 to 6 dB above the relative phonetic powers of
    // natural speech (Fletcher 1953): weak sounds get a little extra so they
    // stay audible, without overpowering the vowels.
    let s_noise = Sound { f6: 5500.0, ..frication(54.0, S_NOISE, 36.0) };
    let sh_noise = frication(60.0, SH_NOISE, 0.0);
    let f_noise = frication(53.0, [0.0; 6], 45.0);
    let th_noise = frication(53.0, [0.0, 0.0, 0.0, 0.0, 0.0, 42.0], 42.0);
    match phoneme {
        IY => vowel(160.0, 50.0, [338.0, 2319.0, 2945.0]),
        IH => vowel(130.0, 40.0, [433.0, 2013.0, 2687.0]),
        EH => vowel(150.0, 60.0, [574.0, 1805.0, 2603.0]),
        AE => vowel(230.0, 60.0, [670.0, 1730.0, 2450.0]),
        AA => vowel(240.0, 80.0, [750.0, 1200.0, 2500.0]),
        AO => vowel(240.0, 80.0, [630.0, 960.0, 2520.0]),
        UH => vowel(160.0, 50.0, [468.0, 1106.0, 2459.0]),
        UW => diphthong(210.0, 60.0, [350.0, 1250.0, 2370.0], [320.0, 900.0, 2370.0]),
        AH => vowel(140.0, 50.0, [622.0, 1168.0, 2560.0]),
        ER => vowel(180.0, 60.0, [477.0, 1386.0, 1714.0]),
        AX => vowel(120.0, 40.0, [500.0, 1450.0, 2500.0]),
        EY => diphthong(190.0, 70.0, [530.0, 1980.0, 2620.0], [400.0, 2250.0, 2720.0]),
        OW => diphthong(220.0, 70.0, [540.0, 1100.0, 2460.0], [450.0, 900.0, 2440.0]),
        AY => diphthong(250.0, 90.0, [750.0, 1250.0, 2550.0], [460.0, 1900.0, 2600.0]),
        AW => diphthong(260.0, 100.0, [750.0, 1300.0, 2500.0], [470.0, 1050.0, 2450.0]),
        OY => diphthong(280.0, 110.0, [560.0, 900.0, 2500.0], [450.0, 1900.0, 2600.0]),

        W => sonorant(80.0, 60.0, [290.0, 610.0, 2150.0], [50.0, 80.0, 150.0], 56.0),
        Y => sonorant(80.0, 40.0, [260.0, 2070.0, 3020.0], [50.0, 100.0, 200.0], 56.0),
        R => sonorant(80.0, 30.0, [310.0, 1060.0, 1380.0], [70.0, 100.0, 120.0], 57.0),
        L => sonorant(80.0, 40.0, [310.0, 1050.0, 2880.0], [50.0, 100.0, 250.0], 54.0),
        LX => sonorant(90.0, 70.0, [450.0, 850.0, 2600.0], [60.0, 80.0, 150.0], 55.0),

        M => nasal(M, 70.0, 60.0, 1300.0, 900.0),
        N => nasal(N, 65.0, 35.0, 2000.0, 1600.0),
        NG => nasal(NG, 80.0, 50.0, 2300.0, 3000.0),

        F => obstruent(F, 120.0, 60.0, f_noise),
        TH => obstruent(TH, 110.0, 40.0, th_noise),
        S => obstruent(S, 125.0, 50.0, s_noise),
        SH => obstruent(SH, 125.0, 50.0, sh_noise),
        V => obstruent(V, 60.0, 40.0, voiced(f_noise)),
        DH => obstruent(DH, 50.0, 30.0, voiced(th_noise)),
        Z => obstruent(Z, 75.0, 40.0, voiced(s_noise)),
        ZH => obstruent(ZH, 70.0, 40.0, voiced(sh_noise)),

        P => stop(P, 85.0, 50.0, frication(60.0, [0.0; 6], 49.0), 6.0, 45.0),
        B => stop(B, 80.0, 50.0, frication(60.0, [0.0; 6], 52.0), 6.0, 45.0),
        T => stop(T, 65.0, 40.0, Sound { f6: 5000.0, ..frication(60.0, T_BURST, 42.0) }, 8.0, 55.0),
        D => stop(D, 65.0, 40.0, Sound { f6: 5000.0, ..frication(60.0, T_BURST, 42.0) }, 8.0, 55.0),
        K => stop(K, 65.0, 50.0, frication(60.0, K_BURST, 0.0), 18.0, 60.0),
        G => stop(G, 65.0, 50.0, frication(60.0, K_BURST, 0.0), 18.0, 60.0),
        CH => affricate(CH, (70.0, 50.0), (60.0, 40.0), sh_noise),
        JH => affricate(JH, (70.0, 50.0), (30.0, 20.0), sh_noise),

        HH => Spec { sound: Sound { ah: 55.0, ..SILENT }, rank: 0, ..vowel(80.0, 20.0, [500.0, 1500.0, 2500.0]) },
        // The flap of "thirty" and "water": a brief tongue tap, about 25 ms in
        // natural American speech, that nearly closes the mouth. Voicing
        // dips 16 dB below a stressed vowel; at 8 dB it was barely heard.
        DX => Spec {
            coarticulation: [0.5, 0.6, 0.6],
            rank: 1,
            transition_out: 25.0,
            transition_in: 8.0,
            sound: Sound { av: 44.0, ..SILENT },
            ..vowel(25.0, 25.0, [270.0, 1700.0, 2600.0])
        },
        Pause => Spec { sound: SILENT, ..vowel(0.0, 0.0, [500.0, 1500.0, 2500.0]) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_have_releases_and_others_do_not() {
        assert!(spec(Phoneme::K).release.is_some());
        assert!(spec(Phoneme::S).release.is_none());
        assert!(spec(Phoneme::AA).release.is_none());
    }

    #[test]
    fn voiced_obstruents_keep_some_voicing() {
        assert!(spec(Phoneme::Z).sound.av > 0.0 && spec(Phoneme::S).sound.av == 0.0);
        assert!(spec(Phoneme::Z).sound.af < spec(Phoneme::S).sound.af);
    }

    #[test]
    fn sound_lerp_is_linear_in_amplitude() {
        let loud = Sound { av: 60.0, ..SILENT };
        let half = SILENT.lerp(&loud, 0.5);
        // Half the linear amplitude is about 6 dB down.
        assert!((half.av - 54.0).abs() < 0.1, "{}", half.av);
        assert_eq!(SILENT.lerp(&loud, 0.0).av, 0.0);
        assert_eq!(SILENT.lerp(&loud, 1.0).av, 60.0);
    }

    #[test]
    fn affricates_split_closure_and_frication() {
        let ch = spec(Phoneme::CH);
        assert_eq!(ch.inherent, 130.0);
        assert!((ch.closure_fraction - 70.0 / 130.0).abs() < 1e-9);
    }
}
