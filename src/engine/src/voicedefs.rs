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

/// One voice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceDef {
    pub name: &'static str,
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
        gender: Gender::Male,
        pitch: 110.0,
        inflection: 1.0,
        open_quotient: 0.55,
        tilt: 0.0,
        breathiness: 0.0,
        flutter: 25.0,
        level: 1.0,
    },
    // The open quotient and tilt are in the range Stevens (1991) reports for
    // matching real voices: 70% and 2 dB for the woman in his example, whose
    // voice had a prominent first harmonic.
    VoiceDef {
        name: "Brittany",
        gender: Gender::Female,
        pitch: 210.0,
        inflection: 1.0,
        open_quotient: 0.65,
        tilt: 3.0,
        breathiness: 36.0,
        flutter: 25.0,
        // More pulses per second carry more power: 1.9 dB more than Steve.
        level: 0.8,
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
