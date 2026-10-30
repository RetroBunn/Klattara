//! Klattara: a Klatt-style formant speech synthesizer.
//!
//! The signal-processing core is a cascade/parallel formant synthesizer after
//! Klatt (1980), "Software for a cascade/parallel formant synthesizer", with
//! the voicing source, spectral tilt and flutter of KLSYN88 (Klatt & Klatt
//! 1990). It turns frames of acoustic parameters into audio.
//!
//! On top of it, the phonetic layer turns ARPAbet phoneme strings into those
//! frames: syllable structure and allophones, Klatt's (1979) duration rules,
//! a pitch contour, and formant transitions between segments.

#![forbid(unsafe_code)]

mod abbreviations;
mod arpabet;
mod frame;
mod homographs;
mod intonation;
mod lexicon;
mod lts;
mod numbers;
mod phoneme;
mod phonetics;
mod resonator;
mod source;
mod speech;
mod synth;
#[cfg(test)]
mod test_util;
mod text;
mod timing;
mod tracks;
mod utterance;
pub mod wav;

pub use frame::{Frame, NUM_FORMANTS};
#[doc(hidden)]
pub use lts::JointModel;
pub use phoneme::{Phoneme, Stress};
pub use speech::{
    Reading, Voice, phoneme_frames, speak_phonemes, speak_text, speak_with, text_frames, transcribe, transcribe_with,
};
pub use synth::{Synth, VoiceSettings};
pub use utterance::ParseError;
