//! KPS9566: range index → class 0–7 (Mirae table). Fullwidth ASCII normalize; 1- or 2-byte width.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KpsCharClass {
    Unknown = 0,
    KoreanSyllable = 1,
    Symbol = 2,
    FullwidthDigit = 3,
    FullwidthLetter = 4,
    MiscSymbol = 5,
    ExtSymbol = 6,
    KoreanJamo = 7,
}

impl From<u8> for KpsCharClass {
    fn from(v: u8) -> Self {
        match v {
            0 => KpsCharClass::Unknown,
            1 => KpsCharClass::KoreanSyllable,
            2 => KpsCharClass::Symbol,
            3 => KpsCharClass::FullwidthDigit,
            4 => KpsCharClass::FullwidthLetter,
            5 => KpsCharClass::MiscSymbol,
            6 => KpsCharClass::ExtSymbol,
            7 => KpsCharClass::KoreanJamo,
            _ => KpsCharClass::Unknown,
        }
    }
}

/// Index from `kps_char_range_index` → class byte (line comments = KPS ranges).
const CHAR_CLASS_TABLE: [u8; 27] = [
    0x00, // [0] no match
    0x02, // [1] a1a1-a1f3
    0x02, // [2] a2a1-a2dc
    0x02, // [3] a2dd-a2fe
    0x03, // [4] a3b0-a3b9 fullwidth digits
    0x04, // [5] a3c1-a3da fullwidth uppercase
    0x04, // [6] a3e1-a3fa fullwidth lowercase
    0x07, // [7] a4a1-a4d3 Korean jamo
    0x01, // [8] a4e8-a4ed special Korean Character
    0x00, // [9]
    0x00, // [10]
    0x00, // [11]
    0x00, // [12]
    0x00, // [13]
    0x00, // [14]
    0x05, // [15] a7a1-a7be
    0x05, // [16] a7c1-a7ce
    0x05, // [17] a7d1-a7de
    0x00, // [18]
    0x00, // [19]
    0x06, // [20] a8a1-a8fe extended symbols
    0x00, // [21]
    0x00, // [22]
    0x00, // [23]
    0x00, // [24]
    0x01, // [25] b0a1-cccf Korean syllables
    0x00, // [26] cda1+ Hanja/unclassified
];

pub fn kps_char_range_index(kps: u16) -> u8 {
    let p = kps;
    if p > 0xa1a0 && p < 0xa1f4 {
        return 1;
    }
    if p > 0xa2a0 && p < 0xa2dd {
        return 2;
    }
    if p > 0xa2dc && p < 0xa2ff {
        return 3;
    }
    if p > 0xa3af && p < 0xa3ba {
        return 4;
    } // fullwidth digits
    if p > 0xa3c0 && p < 0xa3db {
        return 5;
    } // fullwidth uppercase
    if p > 0xa3e0 && p < 0xa3fb {
        return 6;
    } // fullwidth lowercase
    if p > 0xa4a0 && p < 0xa4d4 {
        return 7;
    } // Korean jamo
    if p > 0xa4e7 && p < 0xa4ee {
        return 8;
    } // special Korean Character
    if p > 0xa5a0 && p < 0xa5c2 {
        return 9;
    }
    if p > 0xa5d0 && p < 0xa5f2 {
        return 10;
    }
    if p > 0xa6a0 && p < 0xa6b9 {
        return 11;
    }
    if p > 0xa6c0 && p < 0xa6d9 {
        return 12;
    }
    if p > 0xa6e0 && p < 0xa6eb {
        return 13;
    }
    if p > 0xa6f0 && p < 0xa6fb {
        return 14;
    }
    if p > 0xa7a0 && p < 0xa7bf {
        return 15;
    }
    if p > 0xa7c0 && p < 0xa7cf {
        return 16;
    }
    if p > 0xa7d0 && p < 0xa7df {
        return 17;
    }
    if p > 0xa7df && p < 0xa7ef {
        return 18;
    }
    if p > 0xa7ef && p < 0xa7ff {
        return 19;
    }
    if p > 0xa8a0 && p < 0xa8ff {
        return 20;
    }
    if p > 0xa9a0 && p < 0xa9e5 {
        return 21;
    }
    if p > 0xaaa0 && p < 0xaaf4 {
        return 22;
    }
    if p > 0xaba0 && p < 0xabf7 {
        return 23;
    }
    if p > 0xaca0 && p < 0xace1 {
        return 24;
    }
    if p > 0xb0a0 && p < 0xccd0 {
        return 25;
    } // Korean syllables
    if p > 0xcda0 && p < 0xfed0 {
        return 26;
    } // Hanja / extended
    0
}

/// Fullwidth digit → 1 byte `second+0x80`; fullwidth letter → `second&0x7f`; range index 8 = special KPS substitutions. ASCII passes through.
pub fn classify_and_normalize(input: &[u8]) -> Option<(KpsCharClass, [u8; 3])> {
    if input.is_empty() {
        return None;
    }

    let first = input[0];

    if first < 0xa1 {
        if first < 0x80 {
            let mut out = [0u8; 3];
            out[0] = first;
            return Some((KpsCharClass::Unknown, out));
        }
        return None;
    }

    if input.len() < 2 {
        return None;
    }
    let second = input[1];
    let kps: u16 = ((first as u16) << 8) | (second as u16);

    let range_idx = kps_char_range_index(kps);
    let class_raw = if (range_idx as usize) < CHAR_CLASS_TABLE.len() {
        CHAR_CLASS_TABLE[range_idx as usize]
    } else {
        0
    };
    let class = KpsCharClass::from(class_raw);

    if class_raw == 0 {
        return None;
    }

    let mut out = [0u8; 3];
    match range_idx {
        4 => {
            out[0] = second.wrapping_add(0x80);
        }
        5 | 6 => {
            out[0] = second & 0x7f;
        }
        8 => {
            // Special KPS: a4e8/eb→란 b1ae, e9/ed→일 cbce, ea→만 bac2, ec→원 bcb3
            let (hi, lo) = match second {
                0xe8 | 0xeb => (0xb1, 0xae),
                0xe9 | 0xed => (0xcb, 0xce),
                0xea => (0xba, 0xc2),
                0xec => (0xbc, 0xb3),
                _ => (first, second),
            };
            out[0] = hi;
            out[1] = lo;
        }
        _ => {
            out[0] = first;
            out[1] = second;
        }
    }

    Some((class, out))
}

pub fn classify_next_char(input: &[u8]) -> (KpsCharClass, usize) {
    if input.is_empty() {
        return (KpsCharClass::Unknown, 0);
    }

    let first = input[0];

    if first < 0x80 {
        return (KpsCharClass::Unknown, 1);
    }

    if first < 0xa1 || input.len() < 2 {
        return (KpsCharClass::Unknown, 1);
    }

    let second = input[1];
    let kps: u16 = ((first as u16) << 8) | (second as u16);
    let range_idx = kps_char_range_index(kps);
    let class_raw = if (range_idx as usize) < CHAR_CLASS_TABLE.len() {
        CHAR_CLASS_TABLE[range_idx as usize]
    } else {
        0
    };

    (KpsCharClass::from(class_raw), 2)
}

pub fn is_korean_syllable(bytes: &[u8]) -> bool {
    matches!(classify_next_char(bytes).0, KpsCharClass::KoreanSyllable)
}

pub fn is_fullwidth_digit(bytes: &[u8]) -> bool {
    matches!(classify_next_char(bytes).0, KpsCharClass::FullwidthDigit)
}

pub fn is_fullwidth_letter(bytes: &[u8]) -> bool {
    matches!(classify_next_char(bytes).0, KpsCharClass::FullwidthLetter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kps_char_range_index() {
        // Fullwidth digits: 0xa3b0-0xa3b9
        assert_eq!(kps_char_range_index(0xa3b1), 4); // '１'
        assert_eq!(kps_char_range_index(0xa3b9), 4); // '９'
                                                     // Fullwidth uppercase: 0xa3c1-0xa3da
        assert_eq!(kps_char_range_index(0xa3c1), 5); // 'Ａ'
                                                     // Fullwidth lowercase: 0xa3e1-0xa3fa
        assert_eq!(kps_char_range_index(0xa3e1), 6); // 'ａ'
                                                     // Korean jamo: 0xa4a1-0xa4d3
        assert_eq!(kps_char_range_index(0xa4a1), 7); // ㄱ
                                                     // Special Korean Character: 0xa4e8-0xa4ed
        assert_eq!(kps_char_range_index(0xa4e8), 8);
        // Korean syllable: 0xb0a1-0xcccf
        assert_eq!(kps_char_range_index(0xb0a1), 25); // 가
        assert_eq!(kps_char_range_index(0xccab), 25); // some late syllable
                                                      // Hanja: 0xcda1+
        assert_eq!(kps_char_range_index(0xcda1), 26);
        // Out-of-range: 0
        assert_eq!(kps_char_range_index(0xa000), 0);
    }

    #[test]
    fn test_classify_korean_syllable() {
        // 0xb0a1 = 가 (first Korean syllable in KPS 9566)
        let (class, consumed) = classify_next_char(&[0xb0, 0xa1]);
        assert_eq!(class, KpsCharClass::KoreanSyllable);
        assert_eq!(consumed, 2);
    }

    #[test]
    fn test_classify_fullwidth_digit() {
        // 0xa3b1 = '１'
        let (class, consumed) = classify_next_char(&[0xa3, 0xb1]);
        assert_eq!(class, KpsCharClass::FullwidthDigit);
        assert_eq!(consumed, 2);
    }

    #[test]
    fn test_classify_fullwidth_letter() {
        // 0xa3c1 = 'Ａ'
        let (class, consumed) = classify_next_char(&[0xa3, 0xc1]);
        assert_eq!(class, KpsCharClass::FullwidthLetter);
        assert_eq!(consumed, 2);
    }

    #[test]
    fn test_classify_korean_jamo() {
        // 0xa4a1 = ㄱ
        let (class, consumed) = classify_next_char(&[0xa4, 0xa1]);
        assert_eq!(class, KpsCharClass::KoreanJamo);
        assert_eq!(consumed, 2);
    }

    #[test]
    fn test_classify_hanja_unknown() {
        // 0xcda1 = first Hanja character → Unknown
        let (class, _) = classify_next_char(&[0xcd, 0xa1]);
        assert_eq!(class, KpsCharClass::Unknown);
    }

    #[test]
    fn test_normalize_fullwidth_digit() {
        // '１' (0xa3b1) → normalized to second_byte + 0x80 = 0xb1 + 0x80 = single byte 0x31 (which is '1' + 0x80)
        let result = classify_and_normalize(&[0xa3, 0xb1]);
        assert!(result.is_some());
        let (class, out) = result.unwrap();
        assert_eq!(class, KpsCharClass::FullwidthDigit);
        assert_eq!(out[0], 0xb1u8.wrapping_add(0x80));
    }

    #[test]
    fn test_normalize_fullwidth_letter() {
        // 'Ａ' (0xa3c1) → second_byte & 0x7f = 0xc1 & 0x7f = 0x41 = 'A'
        let result = classify_and_normalize(&[0xa3, 0xc1]);
        assert!(result.is_some());
        let (class, out) = result.unwrap();
        assert_eq!(class, KpsCharClass::FullwidthLetter);
        assert_eq!(out[0], b'A');
    }

    #[test]
    fn test_normalize_special_korean_ran() {
        // 0xa4e8 → 란 (KPS 0xb1ae)
        let result = classify_and_normalize(&[0xa4, 0xe8]);
        assert!(result.is_some());
        let (class, out) = result.unwrap();
        assert_eq!(class, KpsCharClass::KoreanSyllable);
        assert_eq!(&out[..2], &[0xb1, 0xae]);
    }

    #[test]
    fn test_normalize_special_korean_il() {
        // 0xa4e9 → 일 (KPS 0xcbce)
        let result = classify_and_normalize(&[0xa4, 0xe9]);
        assert!(result.is_some());
        let (class, out) = result.unwrap();
        assert_eq!(class, KpsCharClass::KoreanSyllable);
        assert_eq!(&out[..2], &[0xcb, 0xce]);
    }
}
