//! The phoneme inventory: ARPAbet symbols, as used by the CMU Pronouncing
//! Dictionary, plus the allophones and pause that the phonetic rules insert.

/// A phonetic segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(clippy::upper_case_acronyms)]
pub enum Phoneme {
    // Vowels.
    AA,
    AE,
    AH,
    AO,
    AW,
    AY,
    EH,
    ER,
    EY,
    IH,
    IY,
    OW,
    OY,
    UH,
    UW,
    /// Schwa, the reduced vowel of "about". Unstressed AH becomes AX.
    AX,
    // Consonants.
    B,
    CH,
    D,
    DH,
    F,
    G,
    HH,
    JH,
    K,
    L,
    M,
    N,
    NG,
    P,
    R,
    S,
    SH,
    T,
    TH,
    V,
    W,
    Y,
    Z,
    ZH,
    /// The flap of American "butter", inserted for T or D between vowels.
    DX,
    /// Dark (velarized) L, as in "bill", inserted after vowels.
    LX,
    /// Silence between phrases.
    Pause,
}

/// Broad articulatory classes, which decide how a segment is synthesized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Manner {
    Vowel,
    Diphthong,
    Liquid,
    Glide,
    Nasal,
    Fricative,
    Stop,
    Affricate,
    Aspirate,
    Flap,
    Silence,
}

/// Place of articulation of consonants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Labial,
    Labiodental,
    Dental,
    Alveolar,
    Postalveolar,
    Velar,
    None,
}

/// Lexical stress, as marked by CMUdict digits: 0, 1 and 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stress {
    Unstressed,
    Secondary,
    Primary,
}

impl Phoneme {
    /// Parses an ARPAbet symbol with an optional stress digit, such as "AH0"
    /// or "K". Case is ignored. Vowels without a digit count as primary stress.
    pub fn parse(symbol: &str) -> Option<(Phoneme, Stress)> {
        let upper = symbol.to_ascii_uppercase();
        let (name, stress) = match upper.as_bytes().last() {
            Some(b'0') => (&upper[..upper.len() - 1], Stress::Unstressed),
            Some(b'1') => (&upper[..upper.len() - 1], Stress::Primary),
            Some(b'2') => (&upper[..upper.len() - 1], Stress::Secondary),
            _ => (upper.as_str(), Stress::Primary),
        };
        use Phoneme::*;
        let phoneme = match name {
            "AA" => AA,
            "AE" => AE,
            "AH" => AH,
            "AO" => AO,
            "AW" => AW,
            "AY" => AY,
            "EH" => EH,
            "ER" => ER,
            "EY" => EY,
            "IH" => IH,
            "IY" => IY,
            "OW" => OW,
            "OY" => OY,
            "UH" => UH,
            "UW" => UW,
            "AX" => AX,
            "B" => B,
            "CH" => CH,
            "D" => D,
            "DH" => DH,
            "F" => F,
            "G" => G,
            "HH" => HH,
            "JH" => JH,
            "K" => K,
            "L" => L,
            "M" => M,
            "N" => N,
            "NG" => NG,
            "P" => P,
            "R" => R,
            "S" => S,
            "SH" => SH,
            "T" => T,
            "TH" => TH,
            "V" => V,
            "W" => W,
            "Y" => Y,
            "Z" => Z,
            "ZH" => ZH,
            "DX" => DX,
            "LX" => LX,
            _ => return None,
        };
        Some((phoneme, stress))
    }

    pub fn manner(self) -> Manner {
        use Phoneme::*;
        match self {
            AA | AE | AH | AO | EH | ER | IH | IY | UH | UW | AX => Manner::Vowel,
            AW | AY | EY | OW | OY => Manner::Diphthong,
            L | LX | R => Manner::Liquid,
            W | Y => Manner::Glide,
            M | N | NG => Manner::Nasal,
            DH | F | S | SH | TH | V | Z | ZH => Manner::Fricative,
            B | D | G | K | P | T => Manner::Stop,
            CH | JH => Manner::Affricate,
            HH => Manner::Aspirate,
            DX => Manner::Flap,
            Pause => Manner::Silence,
        }
    }

    pub fn place(self) -> Place {
        use Phoneme::*;
        match self {
            P | B | M | W => Place::Labial,
            F | V => Place::Labiodental,
            TH | DH => Place::Dental,
            T | D | N | S | Z | L | LX | DX => Place::Alveolar,
            SH | ZH | CH | JH | R | Y => Place::Postalveolar,
            K | G | NG => Place::Velar,
            _ => Place::None,
        }
    }

    /// Vowels and diphthongs: the nuclei of syllables.
    pub fn is_vowel(self) -> bool {
        matches!(self.manner(), Manner::Vowel | Manner::Diphthong)
    }

    pub fn is_consonant(self) -> bool {
        !self.is_vowel() && self != Phoneme::Pause
    }

    /// Whether the vocal folds vibrate during the segment.
    pub fn is_voiced(self) -> bool {
        use Phoneme::*;
        !matches!(self, P | T | K | F | TH | S | SH | CH | HH | Pause)
    }

    /// Vowels, liquids, glides, nasals and the flap.
    pub fn is_sonorant(self) -> bool {
        matches!(
            self.manner(),
            Manner::Vowel | Manner::Diphthong | Manner::Liquid | Manner::Glide | Manner::Nasal | Manner::Flap
        )
    }

    pub fn is_obstruent(self) -> bool {
        matches!(self.manner(), Manner::Fricative | Manner::Stop | Manner::Affricate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_symbols_with_and_without_stress() {
        assert_eq!(Phoneme::parse("AH0"), Some((Phoneme::AH, Stress::Unstressed)));
        assert_eq!(Phoneme::parse("ow1"), Some((Phoneme::OW, Stress::Primary)));
        assert_eq!(Phoneme::parse("EY2"), Some((Phoneme::EY, Stress::Secondary)));
        assert_eq!(Phoneme::parse("NG"), Some((Phoneme::NG, Stress::Primary)));
        assert_eq!(Phoneme::parse("XX"), None);
        assert_eq!(Phoneme::parse(""), None);
    }

    #[test]
    fn classes_are_consistent() {
        assert!(Phoneme::AY.is_vowel());
        assert!(!Phoneme::S.is_voiced() && Phoneme::Z.is_voiced());
        assert!(Phoneme::M.is_sonorant() && !Phoneme::M.is_obstruent());
        assert!(Phoneme::CH.is_obstruent() && !Phoneme::CH.is_voiced());
    }
}
