//! The voices: what each one is, and how a woman's voice differs from a man's.
//!
//! To add a voice, add it to [`VOICES`]. The C interface and the NVDA add-on
//! list whatever is here, in this order. The first voice is the default.
//!
//! A voice's gender chooses its model of the vocal tract. The phoneme tables in
//! `phonetics` describe a man's, and a shorter tract is not simply the same one
//! scaled: women's F1 is 7% to 29% higher than men's, depending on the vowel. So
//! the female model has a table of its own, measured from the same talkers as
//! the male one, and raising the pitch alone is not what makes a voice female.
//! At about 485 words per minute, 99% of the word pairs in src/tools were told
//! apart with this model, as with the male voice, against 96% with the male
//! model at the same pitch.
//!
//! How breathy or bright a voice sounds is its character, and is for the ear to
//! judge. A speech recognizer always prefers the clearer, brighter voice, so its
//! scores don't decide these settings.

use crate::phoneme::Phoneme;

/// Which model of the vocal tract a voice uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gender {
    Male,
    Female,
}

/// How loud each kind of sound is, as a share of normal: 1 is normal and 0 is off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceGains {
    /// Voicing: vowels and everything else the vocal folds sound.
    pub voicing: f64,
    /// Frication: the hiss of S and F, and the bursts of stops.
    pub frication: f64,
    /// Aspiration: H, and the puff of air after P, T and K. The breath that runs through
    /// a breathy voice has a setting of its own.
    pub aspiration: f64,
    /// The nasal consonants M, N and NG.
    pub nasal: f64,
}

impl SourceGains {
    pub const NORMAL: SourceGains = SourceGains { voicing: 1.0, frication: 1.0, aspiration: 1.0, nasal: 1.0 };
}

/// How a voice moves its pitch: the settings of DECtalk's that are named bf, hr, sr and qu,
/// on its scale, where these normal values are those of its Perfect Paul. They aren't quite
/// Hz here, since pitch moves here in proportion to the voice's own: `baseline_fall` at 18
/// is about 13 Hz at Steve's pitch, and about 2 semitones at any other.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PitchShape {
    /// How far the baseline falls through a phrase, half of it above the middle and half
    /// below. 0 is flat.
    pub baseline_fall: f64,
    /// How high the slow rise over each phrase goes. 0 is none.
    pub hat_rise: f64,
    /// How large the accent on each stressed word is. 0 is none.
    pub stress_rise: f64,
    /// How quickly the pitch responds, as a percentage. A larger larynx is slower.
    pub quickness: f64,
}

impl PitchShape {
    pub const NORMAL: PitchShape =
        PitchShape { baseline_fall: 18.0, hat_rise: 18.0, stress_rise: 32.0, quickness: 40.0 };
}

/// One voice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceDef {
    /// The name that commands and programs use.
    pub name: &'static str,
    /// The name to show people, in the style of DECtalk's "Perfect Paul".
    pub full_name: &'static str,
    pub gender: Gender,
    /// Base pitch, Hz.
    pub pitch: f64,
    /// How much the pitch moves: 0 is monotone, 1 normal, 2.5 the most. Pitch
    /// contours are in semitones, so 1 keeps the same intervals at any pitch.
    pub inflection: f64,
    /// Glottal open quotient: lower sounds brighter and buzzier.
    pub open_quotient: f64,
    /// Spectral tilt of the voice source, dB at 3 kHz: higher sounds softer.
    pub tilt: f64,
    /// Breath noise heard through voicing, dB on the same scale as voicing
    /// itself. 0 is none.
    pub breathiness: f64,
    /// Head size, as a percentage of normal, as in [`crate::Voice::head_size`].
    pub head_size: f64,
    /// Roughness, as in [`crate::Voice::roughness`]. 0 is none.
    pub roughness: f64,
    /// F4 to F6, as in [`crate::Voice::upper_formants`]. None uses the gender's model.
    pub upper_formants: [Option<f64>; 3],
    /// Their bandwidths, as in [`crate::Voice::upper_bandwidths`].
    pub upper_bandwidths: [Option<f64>; 3],
    /// How loud each kind of sound is, as in [`crate::Voice::gains`].
    pub gains: SourceGains,
    /// Breath at the ends of sentences and before voiceless sounds, as in
    /// [`crate::Voice::lax_breathiness`].
    pub lax_breathiness: f64,
    /// How the voice moves its pitch.
    pub pitch_shape: PitchShape,
    /// Slow pitch wobble, as in [`crate::VoiceSettings::flutter`].
    pub flutter: f64,
    /// Output gain, which brings the voice to the level of the others. 1 is none.
    pub level: f64,
}

/// Every voice, the default first.
pub const VOICES: &[VoiceDef] = &[
    // An adult male pitch, with the medium open quotient, between the
    // DECtalk-like buzz and a soft, breathy voice, and no extra spectral tilt.
    // With a tilt of 6 dB the voice had 3 to 5 dB less energy from 1 to 4 kHz
    // than Eloquence or DECtalk, and sounded breathy at fast rates; the user
    // chose this brighter tone by ear.
    VoiceDef {
        name: "Steve",
        full_name: "Standard Steve",
        gender: Gender::Male,
        pitch: 110.0,
        inflection: 1.0,
        open_quotient: 0.55,
        tilt: 0.0,
        breathiness: 0.0,
        head_size: 100.0,
        roughness: 0.0,
        upper_formants: [None; 3],
        upper_bandwidths: [None; 3],
        gains: SourceGains::NORMAL,
        lax_breathiness: 0.0,
        pitch_shape: PitchShape::NORMAL,
        flutter: 25.0,
        level: 1.0,
    },
    // The open quotient and tilt are in the range Stevens (1991) reports for
    // matching real voices: 70% and 2 dB for the woman in his example, whose
    // voice had a prominent first harmonic.
    VoiceDef {
        name: "Brittany",
        full_name: "Breezy Brittany",
        gender: Gender::Female,
        pitch: 210.0,
        inflection: 1.0,
        open_quotient: 0.65,
        tilt: 3.0,
        breathiness: 36.0,
        head_size: 100.0,
        roughness: 0.0,
        upper_formants: [None; 3],
        upper_bandwidths: [None; 3],
        gains: SourceGains::NORMAL,
        // Lively and relaxed: a small fall through each phrase, taller accents, the quick
        // larynx of a smaller voice, and breath as her sentences end. Her rise over each
        // phrase is a little smaller, so that the accents carry the movement.
        lax_breathiness: 65.0,
        pitch_shape: PitchShape { baseline_fall: 12.0, hat_rise: 16.0, stress_rise: 36.0, quickness: 58.0 },
        flutter: 25.0,
        // More pulses per second carry more power: 1.9 dB more than Steve.
        level: 0.8,
    },
    // A deep, giant voice, the same idea as DECtalk's Huge Harry, built its own way. Low
    // pitch and a long vocal tract are what listeners take for a large speaker (Smith and
    // Patterson 2005). Against Harry, he is lower (78 Hz to 89) with a longer tract (120%
    // to 115%), and has a much longer open phase, so he is full where Harry is buzzy, with
    // a little roughness where Harry has none. F4 and F5 lie close together, about 2830 and
    // 3040 Hz once the head size has lowered them, and wide enough to overlap, which makes
    // one broad resonance rather than two narrow ones. F6 is raised so that the head size
    // doesn't dull S, which peaks at about 5240 Hz, close to Steve's 5500. All this is
    // chosen for its character, and is for the ear to judge.
    VoiceDef {
        name: "Gary",
        full_name: "Giant Gary",
        gender: Gender::Male,
        pitch: 78.0,
        inflection: 0.85,
        open_quotient: 0.6,
        tilt: 2.0,
        breathiness: 0.0,
        head_size: 120.0,
        roughness: 8.0,
        upper_formants: [Some(3400.0), Some(3650.0), Some(5600.0)],
        upper_bandwidths: [Some(300.0), Some(300.0), None],
        gains: SourceGains::NORMAL,
        // A large larynx responds slowly, so his pitch moves heavily: a broad rise over each
        // phrase, with smaller accents on top of it. His folds stay tense, so he has very
        // little lax breath, and a steady voice falls little through a phrase.
        lax_breathiness: 8.0,
        pitch_shape: PitchShape { baseline_fall: 10.0, hat_rise: 22.0, stress_rise: 26.0, quickness: 14.0 },
        flutter: 25.0,
        // Fewer pulses per second carry less power.
        level: 1.08,
    },
    // A frail older man, the same idea as DECtalk's Frail Frank, built its own way. Men's
    // speaking pitch rises again in old age (Hollien and Shipp 1972), so he is above Steve
    // at 138 Hz, though below Frank's 155. His vocal folds no longer close firmly, so the
    // open quotient is long and breath runs through the voice. Against Frank, he has less
    // spectral tilt (7 dB to about 12), which keeps consonants from standing out too much,
    // and a head nearer normal (94% to 90%), with F4 and F5 lower and further apart. The
    // resonances are wide and damped, as leaking breath makes them. Most his own is the
    // shake: flutter of 60 where the others have 25, and more roughness than Frank's slight
    // creak, for a voice that wavers. All this is chosen for its character, and is for the
    // ear to judge. He is as loud as the others, since a screen reader's voices must be, so his
    // weakness is in the sound and not the level.
    VoiceDef {
        name: "David",
        full_name: "Delicate David",
        gender: Gender::Male,
        pitch: 138.0,
        inflection: 0.8,
        open_quotient: 0.7,
        tilt: 7.0,
        breathiness: 52.0,
        head_size: 94.0,
        roughness: 16.0,
        upper_formants: [Some(3450.0), Some(4100.0), None],
        upper_bandwidths: [Some(340.0), Some(380.0), None],
        gains: SourceGains::NORMAL,
        // The weakest and slowest movement of all: he doesn't have the strength for large
        // rises, his larynx is sluggish, and his voice gives out in breath at the ends of
        // sentences and before voiceless sounds.
        lax_breathiness: 70.0,
        pitch_shape: PitchShape { baseline_fall: 8.0, hat_rise: 14.0, stress_rise: 20.0, quickness: 6.0 },
        flutter: 60.0,
        // Breath and a soft source take some power, and his pitch gives some back.
        level: 1.0,
    },
    // A child, the same idea as DECtalk's Kit the Kid, built its own way, and on the female
    // model of the vocal tract as Kit is, since a child's is closer to a woman's. The
    // children of Hillenbrand et al. (1995), aged 10 to 12, are close to women: 7% higher in
    // pitch at 234 Hz, and 8% in formants, the same as a head size of 93. A voice with only
    // that would be close to Brittany, so this is about 8 to 10, Kit's age, further the same
    // way. Against
    // Kit, a little lower (270 Hz to 306) and with F4 and F5 in place, where Kit has none.
    // Children's folds are small and quick and their speech is lively, so the pitch moves
    // more than anyone's, and it falls least through a phrase. They have a little breath, and
    // the brightest source of the voices after Steve. All this is chosen for its
    // character, and is for the ear to judge. Children's /u/ is further forward than this
    // model makes it: F2 is 25% above women's in the data, where the model raises it 12%.
    VoiceDef {
        name: "Taylor",
        full_name: "Tiny Taylor",
        gender: Gender::Female,
        pitch: 270.0,
        inflection: 1.15,
        open_quotient: 0.58,
        tilt: 1.0,
        breathiness: 24.0,
        head_size: 89.0,
        roughness: 0.0,
        upper_formants: [None; 3],
        upper_bandwidths: [None; 3],
        gains: SourceGains::NORMAL,
        lax_breathiness: 30.0,
        pitch_shape: PitchShape { baseline_fall: 5.0, hat_rise: 20.0, stress_rise: 42.0, quickness: 72.0 },
        flutter: 25.0,
        // More pulses per second carry more power, as with Brittany.
        level: 0.75,
    },
];

/// The voice with this name, ignoring case.
pub fn find_voice(name: &str) -> Option<&'static VoiceDef> {
    VOICES.iter().find(|voice| voice.name.eq_ignore_ascii_case(name))
}

/// How far women's F1 to F3 lie above men's for sounds with no measurement of
/// their own, consonant loci among them: the mean over the 12 vowels below.
const FEMALE_MEAN: [f64; 3] = [1.17, 1.16, 1.135];
/// F4 to F6 and the nasal resonances. F4 is the measured median ratio.
const FEMALE_UPPER: f64 = 1.17;
/// Formant bandwidths: vocal folds open wider on average damp the resonances.
/// Stevens (1991) matched a woman's vowel with a first-formant bandwidth of
/// about 300 Hz and a man's with 100 Hz. This much smaller step is chosen by ear.
const FEMALE_BANDWIDTHS: f64 = 1.25;

const FEMALE_IH: [f64; 3] = [1.12, 1.18, 1.13];
const FEMALE_UH: [f64; 3] = [1.08, 1.07, 1.14];
const FEMALE_AA: [f64; 3] = [1.22, 1.17, 1.12];
const FEMALE_AO: [f64; 3] = [1.24, 1.15, 1.12];
const FEMALE_ER: [f64; 3] = [1.07, 1.13, 1.12];

/// Women's F1 to F3 as a share of men's, at the start and end of a sound: the
/// medians of the 48 women over the medians of the 45 men of Hillenbrand et
/// al. (1995). Monophthongs use the steady state. /e/, /o/ and /u/ use the
/// points 20% and 80% through, as they move. The other diphthongs were not
/// measured, and take the shares of the vowels they join.
fn female_shares(phoneme: Phoneme) -> ([f64; 3], [f64; 3]) {
    use Phoneme::*;
    let steady = |shares: [f64; 3]| (shares, shares);
    match phoneme {
        IY => steady([1.29, 1.19, 1.13]),
        IH => steady(FEMALE_IH),
        EH => steady([1.25, 1.14, 1.13]),
        AE => steady([1.14, 1.20, 1.14]),
        AA => steady(FEMALE_AA),
        AO => steady(FEMALE_AO),
        UH => steady(FEMALE_UH),
        AH => steady([1.23, 1.21, 1.14]),
        ER | R => steady(FEMALE_ER),
        UW => ([1.23, 1.10, 1.15], [1.19, 1.07, 1.17]),
        EY => ([1.10, 1.22, 1.14], [1.09, 1.21, 1.13]),
        OW => ([1.22, 1.14, 1.14], [1.09, 1.12, 1.14]),
        AY => (FEMALE_AA, FEMALE_IH),
        AW => (FEMALE_AA, FEMALE_UH),
        OY => (FEMALE_AO, FEMALE_IH),
        _ => steady(FEMALE_MEAN),
    }
}

impl Gender {
    /// The shares of the male table's F1 to F3 that this gender's formants
    /// take at the start and end of `phoneme`.
    pub(crate) fn formant_shares(self, phoneme: Phoneme) -> ([f64; 3], [f64; 3]) {
        match self {
            Gender::Male => ([1.0; 3], [1.0; 3]),
            Gender::Female => female_shares(phoneme),
        }
    }

    /// The share of the male F4 to F6 and nasal resonances.
    pub(crate) fn upper_share(self) -> f64 {
        match self {
            Gender::Male => 1.0,
            Gender::Female => FEMALE_UPPER,
        }
    }

    /// The share of the male formant bandwidths.
    pub(crate) fn bandwidth_share(self) -> f64 {
        match self {
            Gender::Male => 1.0,
            Gender::Female => FEMALE_BANDWIDTHS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phonetics::spec;

    #[test]
    fn the_default_voice_comes_first_and_names_are_unique() {
        assert_eq!(VOICES[0].name, "Steve");
        for (i, voice) in VOICES.iter().enumerate() {
            assert_eq!(find_voice(voice.name), Some(voice));
            assert!(VOICES[..i].iter().all(|other| other.name != voice.name));
        }
        assert_eq!(find_voice("BRITTANY").map(|v| v.pitch), Some(210.0));
        assert_eq!(find_voice("nobody"), None);
    }

    #[test]
    fn each_voice_moves_its_pitch_its_own_way_and_steve_is_the_normal_one() {
        assert_eq!(VOICES[0].pitch_shape, PitchShape::NORMAL);
        assert_eq!(VOICES[0].lax_breathiness, 0.0);
        for (i, voice) in VOICES.iter().enumerate() {
            for other in &VOICES[..i] {
                let (a, b) = (voice.pitch_shape, other.pitch_shape);
                assert_ne!(a.baseline_fall, b.baseline_fall, "{} {}", voice.name, other.name);
                assert_ne!(a.hat_rise, b.hat_rise, "{} {}", voice.name, other.name);
                assert_ne!(a.stress_rise, b.stress_rise, "{} {}", voice.name, other.name);
                assert_ne!(a.quickness, b.quickness, "{} {}", voice.name, other.name);
                assert_ne!(voice.lax_breathiness, other.lax_breathiness, "{} {}", voice.name, other.name);
            }
        }
    }

    #[test]
    fn full_names_end_in_the_name_and_start_with_its_letter() {
        for voice in VOICES {
            let (first, last) = voice.full_name.split_once(' ').unwrap();
            assert_eq!(last, voice.name);
            assert_eq!(first.chars().next(), voice.name.chars().next(), "{}", voice.full_name);
        }
        // A full name is for showing, and commands don't know it.
        assert_eq!(find_voice("Giant Gary"), None);
    }

    #[test]
    fn the_default_voice_is_the_first() {
        assert_eq!(crate::Voice::default(), crate::Voice::from(&VOICES[0]));
    }

    #[test]
    fn every_voice_is_about_as_loud_as_the_first() {
        let text = "Nine people walked along the river until the sun went down.";
        let level = |def: &VoiceDef| {
            let audio = crate::speak_text(text, &crate::Voice::from(def), 22050);
            10.0 * (audio.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / audio.len() as f64).log10()
        };
        for voice in VOICES {
            let difference = level(voice) - level(&VOICES[0]);
            assert!(difference.abs() < 1.0, "{}: {difference:.1} dB", voice.name);
        }
    }

    #[test]
    fn the_male_model_changes_nothing() {
        assert_eq!(Gender::Male.formant_shares(Phoneme::IY), ([1.0; 3], [1.0; 3]));
        assert_eq!((Gender::Male.upper_share(), Gender::Male.bandwidth_share()), (1.0, 1.0));
    }

    #[test]
    fn female_vowels_stay_in_order_and_below_the_upper_formants() {
        use Phoneme::*;
        let f4 = crate::Frame::default().freq[3] * FEMALE_UPPER;
        for phoneme in [IY, IH, EH, AE, AA, AO, UH, UW, AH, ER, AX, EY, OW, AY, AW, OY] {
            let table = spec(phoneme);
            let (start, end) = female_shares(phoneme);
            let scaled = |f: [f64; 3], s: [f64; 3]| [f[0] * s[0], f[1] * s[1], f[2] * s[2]];
            for f in [scaled(table.formants, start), scaled(table.glide_to.unwrap_or(table.formants), end)] {
                assert!(f[0] < f[1] && f[1] < f[2] && f[2] < f4, "{phoneme:?}: {f:?}");
            }
        }
    }
}
