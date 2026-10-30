//! The 39 ARPAbet phonemes of the CMU Pronouncing Dictionary, in the order
//! used to encode them in the compiled dictionary. Shared by the build script
//! and the engine, so both always agree on the codes.

#[cfg_attr(not(test), allow(dead_code))] // read by build.rs, and checked by the tests
pub const SYMBOLS: [&str; 39] = [
    "AA", "AE", "AH", "AO", "AW", "AY", "B", "CH", "D", "DH", "EH", "ER", "EY", "F", "G", "HH", "IH", "IY", "JH", "K",
    "L", "M", "N", "NG", "OW", "OY", "P", "R", "S", "SH", "T", "TH", "UH", "UW", "V", "W", "Y", "Z", "ZH",
];

/// Each phoneme is stored in one byte: the symbol index in the low six bits,
/// and the stress in the top two (0 for consonants, else CMUdict digit + 1).
pub const STRESS_SHIFT: u32 = 6;
