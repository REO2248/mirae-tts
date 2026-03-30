//! Korean syllable decomposition and Jamo processing.
//!
//! VoiceInfo.pkg phoneme IDs use KPS 9566 ordering with coda neutralization:
//!   bits 14-10: final group                    → 0,2,5,6,14,15,18,27
//!   bits 9-5:   medium vowel (KPS 9566)        → 0-20
//!   bits 4-0:   initial consonant (KPS 9566)   → 0-18
//!
//! Coda groups (Korean coda neutralization / 소리마디자체내에서 일어나는 말소리변화):
//!   0  = ㄱ group (ㄱ,ㄲ,ㄳ,ㄺ,ㅋ)
//!   2  = ㄴ group (ㄴ,ㄵ,ㄶ)
//!   5  = ㄷ group (ㄷ,ㅅ,ㅆ,ㅈ,ㅊ,ㅌ,ㅎ)
//!   6  = ㄹ group (ㄹ,ㄽ,ㄾ,ㅀ)
//!   14 = ㅁ group (ㅁ,ㄻ)
//!   15 = ㅂ group (ㅂ,ㄼ,ㄿ,ㅄ,ㅍ)
//!   18 = ㅇ group (ㅇ)
//!   27 = no coda / vowel boundary
//!
//! Non-Korean characters have bit 15 set (negative as i16),
//! encoded as: 0x8000 | (ord(c) - 0x14)

/// Unicode Korean Syllables block: U+AC00 to U+D7A3
const KOREAN_SYLLABLE_BASE: u32 = 0xAC00;
const KOREAN_SYLLABLE_END: u32 = 0xD7A3;

/// Number of final consonants including none
const FINAL_COUNT: u32 = 28;
/// Number of medial vowels
const MEDIUM_COUNT: u32 = 21;

/// Initial consonant mapping.
/// Unicode initial index (0-18) → KPS 9566 onset index (0-18).
/// KPS 9566 order: ㄱ,ㄴ,ㄷ,ㄹ,ㅁ,ㅂ,ㅅ,ㅈ,ㅊ,ㅋ,ㅌ,ㅍ,ㅎ,ㄲ,ㄸ,ㅃ,ㅆ,ㅉ,ㅇ
const INITIAL_MAP: [u8; 19] = [
    0,  // ㄱ (U+0) → KPS 0
    13, // ㄲ (U+1) → KPS 13
    1,  // ㄴ (U+2) → KPS 1
    2,  // ㄷ (U+3) → KPS 2
    14, // ㄸ (U+4) → KPS 14
    3,  // ㄹ (U+5) → KPS 3
    4,  // ㅁ (U+6) → KPS 4
    5,  // ㅂ (U+7) → KPS 5
    15, // ㅃ (U+8) → KPS 15
    6,  // ㅅ (U+9) → KPS 6
    16, // ㅆ (U+10) → KPS 16
    18, // ㅇ (U+11) → KPS 18
    7,  // ㅈ (U+12) → KPS 7
    17, // ㅉ (U+13) → KPS 17
    8,  // ㅊ (U+14) → KPS 8
    9,  // ㅋ (U+15) → KPS 9
    10, // ㅌ (U+16) → KPS 10
    11, // ㅍ (U+17) → KPS 11
    12, // ㅎ (U+18) → KPS 12
];

/// Medium vowel mapping.
/// Unicode medium index (0-20) → KPS 9566 vowel index (0-20).
/// KPS 9566 order: ㅏ,ㅑ,ㅓ,ㅕ,ㅗ,ㅛ,ㅜ,ㅠ,ㅡ,ㅣ,ㅐ,ㅒ,ㅔ,ㅖ,ㅚ,ㅟ,ㅢ,ㅘ,ㅝ,ㅙ,ㅞ
const MEDIUM_MAP: [u8; 21] = [
    0,  // ㅏ (U+0) → KPS 0
    10, // ㅐ (U+1) → KPS 10
    1,  // ㅑ (U+2) → KPS 1
    11, // ㅒ (U+3) → KPS 11
    2,  // ㅓ (U+4) → KPS 2
    12, // ㅔ (U+5) → KPS 12
    3,  // ㅕ (U+6) → KPS 3
    13, // ㅖ (U+7) → KPS 13
    4,  // ㅗ (U+8) → KPS 4
    17, // ㅘ (U+9) → KPS 17
    19, // ㅙ (U+10) → KPS 19
    14, // ㅚ (U+11) → KPS 14
    5,  // ㅛ (U+12) → KPS 5
    6,  // ㅜ (U+13) → KPS 6
    18, // ㅝ (U+14) → KPS 18
    20, // ㅞ (U+15) → KPS 20
    15, // ㅟ (U+16) → KPS 15
    7,  // ㅠ (U+17) → KPS 7
    8,  // ㅡ (U+18) → KPS 8
    16, // ㅢ (U+19) → KPS 16
    9,  // ㅣ (U+20) → KPS 9
];

/// Final consonant mapping.
/// Unicode final index (0-27) → KPS 9566 raw final index (0-27).
/// Raw indices preserve compound coda identity for G2P processing.
/// After G2P, CODA_REMAP converts raw → neutralized groups for VoiceInfo.
///
/// KPS final ordering: ㄱ,ㄳ,ㄴ,ㄵ,ㄶ,ㄷ,ㄹ,ㄺ,ㄻ,ㄼ,ㄽ,ㄾ,ㄿ,ㅀ,ㅁ,ㅂ,ㅄ,ㅅ,ㅇ,ㅈ,ㅊ,ㅋ,ㅌ,ㅍ,ㅎ,ㄲ,ㅆ
const FINAL_MAP: [u8; 28] = [
    27, // (none) (0) → raw 27 (no coda)
    0,  // ㄱ  (1)  → raw 0
    25, // ㄲ  (2)  → raw 25
    1,  // ㄳ  (3)  → raw 1
    2,  // ㄴ  (4)  → raw 2
    3,  // ㄵ  (5)  → raw 3
    4,  // ㄶ  (6)  → raw 4
    5,  // ㄷ  (7)  → raw 5
    6,  // ㄹ  (8)  → raw 6
    7,  // ㄺ  (9)  → raw 7
    8,  // ㄻ  (10) → raw 8
    9,  // ㄼ  (11) → raw 9
    10, // ㄽ  (12) → raw 10
    11, // ㄾ  (13) → raw 11
    12, // ㄿ  (14) → raw 12
    13, // ㅀ  (15) → raw 13
    14, // ㅁ  (16) → raw 14
    15, // ㅂ  (17) → raw 15
    16, // ㅄ  (18) → raw 16
    17, // ㅅ  (19) → raw 17
    26, // ㅆ  (20) → raw 26
    18, // ㅇ  (21) → raw 18
    19, // ㅈ  (22) → raw 19
    20, // ㅊ  (23) → raw 20
    21, // ㅋ  (24) → raw 21
    22, // ㅌ  (25) → raw 22
    23, // ㅍ  (26) → raw 23
    24, // ㅎ  (27) → raw 24
];

/// Coda neutralization remap table.
/// Maps raw KPS final consonant index (0-27) → neutralized coda group for VoiceInfo.
/// Applied after G2P rules to produce final phoneme IDs.
pub const CODA_REMAP: [u8; 28] = [
    0,  //  0: ㄱ  → ㄱ group (0)
    0,  //  1: ㄳ  → ㄱ group (0)
    2,  //  2: ㄴ  → ㄴ group (2)
    2,  //  3: ㄵ  → ㄴ group (2)
    2,  //  4: ㄶ  → ㄴ group (2)
    5,  //  5: ㄷ  → ㄷ group (5)
    6,  //  6: ㄹ  → ㄹ group (6)
    0,  //  7: ㄺ  → ㄱ group (0)
    14, //  8: ㄻ  → ㅁ group (14)
    15, //  9: ㄼ  → ㅂ group (15)
    6,  // 10: ㄽ  → ㄹ group (6)
    6,  // 11: ㄾ  → ㄹ group (6)
    15, // 12: ㄿ  → ㅂ group (15)
    6,  // 13: ㅀ  → ㄹ group (6)
    14, // 14: ㅁ  → ㅁ group (14)
    15, // 15: ㅂ  → ㅂ group (15)
    15, // 16: ㅄ  → ㅂ group (15)
    5,  // 17: ㅅ  → ㄷ group (5)
    18, // 18: ㅇ  → ㅇ group (18)
    5,  // 19: ㅈ  → ㄷ group (5)
    5,  // 20: ㅊ  → ㄷ group (5)
    0,  // 21: ㅋ  → ㄱ group (0)
    5,  // 22: ㅌ  → ㄷ group (5)
    15, // 23: ㅍ  → ㅂ group (15)
    5,  // 24: ㅎ  → ㄷ group (5)
    0,  // 25: ㄲ  → ㄱ group (0)
    5,  // 26: ㅆ  → ㄷ group (5)
    27, // 27: none → no coda (27)
];

/// A decomposed Korean syllable in VoiceInfo's KPS 9566 representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KoreanJamo {
    /// Initial consonant in KPS 9566 order: 0-18
    pub initial: u8,
    /// Medium vowel in KPS 9566 order: 0-20
    pub medium: u8,
    /// Raw KPS 9566 final index (0-27).
    /// After G2P + CODA_REMAP, becomes neutralized group for VoiceInfo.
    pub final_index: u8,
}

/// Vowel type flags, indexed by KPS 9566 medium vowel order.
/// 1 = "atomic" vowel (direct VoiceInfo lookup possible)
/// 0 = "compound" vowel (needs substitution or decomposition)
pub const VOWEL_FLAGS: [u8; 21] = [
    1, // KPS 0  ㅏ - atomic
    0, // KPS 1  ㅑ - needs substitution
    1, // KPS 2  ㅓ - atomic
    0, // KPS 3  ㅕ - needs substitution
    1, // KPS 4  ㅗ - atomic
    0, // KPS 5  ㅛ - needs substitution
    1, // KPS 6  ㅜ - atomic
    0, // KPS 7  ㅠ - needs substitution
    1, // KPS 8  ㅡ - atomic
    1, // KPS 9  ㅣ - atomic
    1, // KPS 10 ㅐ - atomic
    0, // KPS 11 ㅒ - needs substitution
    1, // KPS 12 ㅔ - atomic
    0, // KPS 13 ㅖ - needs substitution
    1, // KPS 14 ㅚ - atomic
    1, // KPS 15 ㅟ - atomic
    0, // KPS 16 ㅢ - needs substitution
    0, // KPS 17 ㅘ - needs substitution
    0, // KPS 18 ㅝ - needs substitution
    0, // KPS 19 ㅙ - needs substitution
    0, // KPS 20 ㅞ - needs substitution
];

/// First vowel component for consonant+vowel split.
/// Indexed by KPS 9566 medium vowel order, values in KPS 9566 medium vowel order.
pub const VOWEL_FIRST_COMPONENT: [u8; 21] = [
    0,  // KPS 0  ㅏ → ㅏ(0)
    9,  // KPS 1  ㅑ → ㅣ(9)
    2,  // KPS 2  ㅓ → ㅓ(2)
    9,  // KPS 3  ㅕ → ㅣ(9)
    4,  // KPS 4  ㅗ → ㅗ(4)
    9,  // KPS 5  ㅛ → ㅣ(9)
    6,  // KPS 6  ㅜ → ㅜ(6)
    9,  // KPS 7  ㅠ → ㅣ(9)
    8,  // KPS 8  ㅡ → ㅡ(8)
    9,  // KPS 9  ㅣ → ㅣ(9)
    10, // KPS 10 ㅐ → ㅐ(10)
    9,  // KPS 11 ㅒ → ㅣ(9)
    12, // KPS 12 ㅔ → ㅔ(12)
    9,  // KPS 13 ㅖ → ㅣ(9)
    14, // KPS 14 ㅚ → ㅚ(14)
    6,  // KPS 15 ㅟ → ㅜ(6)
    8,  // KPS 16 ㅢ → ㅡ(8)
    4,  // KPS 17 ㅘ → ㅗ(4)
    6,  // KPS 18 ㅝ → ㅜ(6)
    4,  // KPS 19 ㅙ → ㅗ(4)
    6,  // KPS 20 ㅞ → ㅜ(6)
];

/// Vowel substitution table.
/// Indexed by KPS 9566 medium vowel order, values in KPS 9566 medium vowel order.
pub const VOWEL_SUBSTITUTE: [u8; 21] = [
    0,  // KPS 0  ㅏ → ㅏ(0)
    0,  // KPS 1  ㅑ → ㅏ(0)
    2,  // KPS 2  ㅓ → ㅓ(2)
    2,  // KPS 3  ㅕ → ㅓ(2)
    4,  // KPS 4  ㅗ → ㅗ(4)
    4,  // KPS 5  ㅛ → ㅗ(4)
    6,  // KPS 6  ㅜ → ㅜ(6)
    6,  // KPS 7  ㅠ → ㅜ(6)
    8,  // KPS 8  ㅡ → ㅡ(8)
    9,  // KPS 9  ㅣ → ㅣ(9)
    10, // KPS 10 ㅐ → ㅐ(10)
    10, // KPS 11 ㅒ → ㅐ(10)
    12, // KPS 12 ㅔ → ㅔ(12)
    12, // KPS 13 ㅖ → ㅔ(12)
    14, // KPS 14 ㅚ → ㅚ(14)
    9,  // KPS 15 ㅟ → ㅣ(9)
    9,  // KPS 16 ㅢ → ㅣ(9)
    0,  // KPS 17 ㅘ → ㅏ(0)
    2,  // KPS 18 ㅝ → ㅓ(2)
    14, // KPS 19 ㅙ → ㅚ(14)
    12, // KPS 20 ㅞ → ㅔ(12)
];

/// Coda group value for "no coda / vowel boundary".
/// Used as bits 10-14 for syllables without a final consonant, and for
/// the vowel unit in consonant+vowel split mode.
/// 54% of VoiceInfo entries (37,790 out of 70,150) use this coda value.
pub const CODA_NO_CODA: u8 = 27; // 0x1B

/// KPS 9566 onset consonant index for ㅇ (ieung / silent onset).
/// Used as the initial consonant value for consonant units in consonant+vowel split mode,
/// Matches consonant+vowel split mode (`| 0x12` in packed IDs).
pub const IEUNG_INITIAL: u8 = 18; // 0x12

/// Backward compatibility alias.
pub const CHO_VOWEL_MARKER: u8 = CODA_NO_CODA;

/// Result of decomposing a single character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecomposedChar {
    /// Korean syllable decomposed into Korean Jamo
    KoreanSyllable(KoreanJamo),
    /// Non-Korean character (space, punctuation, etc.)
    /// Stored as Mirae's internal encoding
    Other(u16),
    /// Whitespace / sentence boundary marker
    Space,
    /// Punctuation that generates a pause
    Pause(PauseType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseType {
    /// Period (。/ .)
    Period,
    /// Comma (、/ ,)
    Comma,
    /// Question mark (?)
    Question,
    /// Exclamation mark (!)
    Exclamation,
    /// Other break
    Break,
}

/// Decompose a Unicode character into Mirae's internal representation.
pub fn decompose_char(ch: char) -> DecomposedChar {
    let cp = ch as u32;

    // Check if it's a Korean syllable
    if (KOREAN_SYLLABLE_BASE..=KOREAN_SYLLABLE_END).contains(&cp) {
        let offset = cp - KOREAN_SYLLABLE_BASE;
        let initial_idx = (offset / (MEDIUM_COUNT * FINAL_COUNT)) as usize;
        let medium_idx = ((offset % (MEDIUM_COUNT * FINAL_COUNT)) / FINAL_COUNT) as usize;
        let final_idx = (offset % FINAL_COUNT) as usize;

        return DecomposedChar::KoreanSyllable(KoreanJamo {
            initial: INITIAL_MAP[initial_idx],
            medium: MEDIUM_MAP[medium_idx],
            final_index: FINAL_MAP[final_idx],
        });
    }

    // Check for pause-inducing punctuation
    match ch {
        '.' | '。' => DecomposedChar::Pause(PauseType::Period),
        ',' | '、' => DecomposedChar::Pause(PauseType::Comma),
        '?' => DecomposedChar::Pause(PauseType::Question),
        '!' => DecomposedChar::Pause(PauseType::Exclamation),
        ' ' | '\t' => DecomposedChar::Space,
        // Line breaks: same pause class as period (`.`).
        '\n' | '\r' => DecomposedChar::Pause(PauseType::Period),
        _ => {
            // Non-Korean: encode as Mirae's special format
            // bit 15 set, with offset -0x14
            let code = (cp as u16).wrapping_sub(0x14) | 0x8000;
            DecomposedChar::Other(code)
        }
    }
}

/// Pack a Jamo triple into Mirae's 16-bit phoneme ID.
/// Layout: (coda_group << 10) | (medium_kps << 5) | initial_kps
/// Packing order matches VoiceInfo tooling / reference encoder tests.
pub fn pack_syllable(jamo: &KoreanJamo) -> u16 {
    ((jamo.final_index as u16) << 10) | ((jamo.medium as u16) << 5) | (jamo.initial as u16)
}

/// Unpack a 16-bit phoneme ID back to Jamo triple.
/// Bits 14-10: coda group (Final), Bits 9-5: Medium vowel (KPS), Bits 4-0: Initial consonant (KPS).
pub fn unpack_syllable(packed: u16) -> KoreanJamo {
    if packed & 0x8000 != 0 {
        // Non-Korean character
        KoreanJamo {
            initial: 0,
            medium: 0,
            final_index: 0,
        }
    } else {
        KoreanJamo {
            initial: (packed & 0x1F) as u8,
            medium: ((packed >> 5) & 0x1F) as u8,
            final_index: ((packed >> 10) & 0x1F) as u8,
        }
    }
}

/// Convert a Jamo triple to TTS phoneme byte sequence (for the internal text format).
/// Byte layout for internal phoneme stream:
///   coda_group → raw byte (no offset, written first)
///   jung_kps   → value + 0x13 (written second)
///   cho_kps    → value + 0x28 (written third)
pub fn jamo_to_phoneme_bytes(jamo: &KoreanJamo) -> Vec<u8> {
    let mut result = Vec::with_capacity(3);

    // Coda group (Final field) - written first with no offset
    if jamo.final_index != 0 && jamo.final_index != CODA_NO_CODA {
        result.push(jamo.final_index);
    }

    // Medium vowel (0-20 → byte 0x13-0x27)
    if jamo.medium > 0 {
        result.push(jamo.medium + 0x13);
    }

    // Initial consonant (0-18 → byte 0x28-0x3A)
    if jamo.initial > 0 {
        result.push(jamo.initial + 0x28);
    }

    result
}

/// Pack a 3-character phoneme string into a 16-bit key for VoiceInfo lookup.
/// Matches the `pack_string` function from decode_voice_info_keys.py:
///   val = (c0 << 10) | (c1 << 5) | c2
///   For c2 >= 'E': val = (val | 0x8000 | (c2 - 0x14))
pub fn pack_phoneme_string(c0: u8, c1: u8, c2: u8) -> u16 {
    let mut val: u16 = 0;

    if c0 != b' ' {
        val = c0 as u16;
    }
    val <<= 5;

    if c1 != b' ' {
        val |= c1 as u16;
    }
    val <<= 5;

    if c2 >= b'E' {
        val | 0x8000 | ((c2 as u16) - 0x14)
    } else if c2 != b' ' {
        val | (c2 as u16)
    } else {
        val
    }
}

/// Decompose a Korean text string into a sequence of Jamo triples and breaks.
///
/// Normalizes line endings so `\r\n` and lone `\r` become a single logical newline
/// (one [`PauseType::Period`] each, not two — same pause as `.`).
pub fn decompose_text(text: &str) -> Vec<DecomposedChar> {
    let mut out = Vec::with_capacity(text.len());
    let mut it = text.chars().peekable();
    while let Some(ch) = it.next() {
        if ch == '\r' {
            if it.peek() == Some(&'\n') {
                it.next();
            }
            out.push(DecomposedChar::Pause(PauseType::Period));
        } else {
            out.push(decompose_char(ch));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decompose_korean_syllable() {
        let result = decompose_char('한');
        match result {
            DecomposedChar::KoreanSyllable(j) => {
                assert_eq!(j.initial, 12);
                assert_eq!(j.medium, 0);
                assert_eq!(j.final_index, 2);
            }
            _ => panic!("Expected Korean syllable decomposition"),
        }
    }

    #[test]
    fn test_decompose_ga() {
        let result = decompose_char('가');
        match result {
            DecomposedChar::KoreanSyllable(j) => {
                assert_eq!(j.initial, 0);
                assert_eq!(j.medium, 0);
                assert_eq!(j.final_index, 27);
                assert_eq!(pack_syllable(&j), 0x6C00);
            }
            _ => panic!("Expected Korean syllable decomposition"),
        }
    }

    #[test]
    fn test_decompose_ha() {
        let result = decompose_char('하');
        match result {
            DecomposedChar::KoreanSyllable(j) => {
                assert_eq!(j.initial, 12);
                assert_eq!(j.medium, 0);
                assert_eq!(j.final_index, 27);
                assert_eq!(pack_syllable(&j), 0x6C0C);
            }
            _ => panic!("Expected Korean syllable decomposition"),
        }
    }

    #[test]
    fn test_pack_unpack() {
        let jamo = KoreanJamo {
            initial: 12,
            medium: 0,
            final_index: 2,
        };
        let packed = pack_syllable(&jamo);
        assert_eq!(packed, 0x080C);
        let unpacked = unpack_syllable(packed);
        assert_eq!(jamo, unpacked);
    }

    #[test]
    fn test_verify_voiceinfo_sample() {
        let packed: u16 = 0x6D86;
        let j = unpack_syllable(packed);
        assert_eq!(j.initial, 6);
        assert_eq!(j.medium, 12);
        assert_eq!(j.final_index, 27);
        assert_eq!(pack_syllable(&j), packed);

        let packed2: u16 = 0x4861;
        let j2 = unpack_syllable(packed2);
        assert_eq!(j2.initial, 1);
        assert_eq!(j2.medium, 3);
        assert_eq!(j2.final_index, 18);
    }

    #[test]
    fn test_decompose_an_nyeong() {
        let an = decompose_char('안');
        match an {
            DecomposedChar::KoreanSyllable(j) => {
                assert_eq!(j.initial, 18);
                assert_eq!(j.medium, 0);
                assert_eq!(j.final_index, 2);
                assert_eq!(pack_syllable(&j), 0x0812);
            }
            _ => panic!("Expected Korean syllable"),
        }

        let nyeong = decompose_char('녕');
        match nyeong {
            DecomposedChar::KoreanSyllable(j) => {
                assert_eq!(j.initial, 1);
                assert_eq!(j.medium, 3);
                assert_eq!(j.final_index, 18);
                assert_eq!(pack_syllable(&j), 0x4861);
            }
            _ => panic!("Expected Korean syllable"),
        }
    }

    #[test]
    fn decompose_text_newline_is_single_pause_period() {
        let a = decompose_text("a\nb");
        assert!(
            matches!(
                a.as_slice(),
                [
                    DecomposedChar::Other(_),
                    DecomposedChar::Pause(PauseType::Period),
                    DecomposedChar::Other(_),
                ]
            ),
            "{a:?}"
        );
        assert_eq!(decompose_text("x\r\ny").len(), 3);
        assert!(matches!(
            decompose_text("x\r\ny").get(1),
            Some(DecomposedChar::Pause(PauseType::Period))
        ));
    }

    #[test]
    fn test_vowel_tables() {
        assert_eq!(VOWEL_FLAGS.len(), 21);
        assert_eq!(VOWEL_FIRST_COMPONENT.len(), 21);
        assert_eq!(VOWEL_SUBSTITUTE.len(), 21);
        assert_eq!(VOWEL_FLAGS[0], 1);
        assert_eq!(VOWEL_FLAGS[1], 0);
        assert_eq!(VOWEL_SUBSTITUTE[1], 0);
        assert_eq!(VOWEL_FLAGS[9], 1);
    }
}
