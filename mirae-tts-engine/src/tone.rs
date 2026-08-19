//! Tone classes, sandhi rules, and 16-value normalization.
//! Sandhi (FUN_0044ca50): prev = prev class % 10; prev==5 -> prev=5, cur += 0x1e;
//! else cur += prev x 10; first record of text: (class % 10) + 0x28;
//! sentence-boundary linking per the original pump.
//! Normalization (DAT_0048bd40): TONE_CLASS_MAP rows; canonical classes
//! {0,1,3,4,10,11,13,14,30,31,33,34,40,41,43,44}.
use crate::record::{MARKER_SENTENCE_END, MARKER_SPECIAL, ProsodyRecord};

pub const TONE_CLASS_MAP: [u8; 256] = [
    40, 30, 41, 31, 10, 11, 0, 1, 3, 13, 4, 14, 33, 43, 34, 44, 4, 14, 3, 13, 1, 11, 0, 10, 30, 31,
    40, 41, 33, 34, 43, 44, 30, 40, 31, 41, 10, 11, 0, 1, 3, 13, 4, 14, 33, 43, 34, 44, 3, 13, 4,
    14, 1, 11, 0, 10, 30, 31, 40, 41, 33, 34, 43, 44, 10, 11, 30, 40, 31, 41, 0, 1, 3, 13, 4, 14,
    33, 43, 34, 44, 1, 11, 0, 10, 3, 13, 4, 14, 30, 31, 40, 41, 33, 34, 43, 44, 41, 31, 40, 30, 11,
    10, 0, 1, 3, 13, 4, 14, 33, 43, 34, 44, 14, 4, 13, 3, 11, 1, 0, 10, 30, 31, 40, 41, 33, 34, 43,
    44, 31, 41, 30, 40, 11, 10, 0, 1, 3, 13, 4, 14, 33, 43, 34, 44, 13, 3, 14, 4, 11, 10, 1, 0, 30,
    31, 40, 41, 33, 34, 43, 44, 11, 10, 1, 0, 30, 31, 3, 13, 40, 41, 4, 14, 33, 43, 34, 44, 33, 34,
    43, 41, 14, 31, 13, 30, 3, 40, 4, 11, 10, 1, 0, 44, 34, 33, 43, 14, 4, 13, 3, 41, 31, 11, 10,
    30, 40, 10, 0, 44, 43, 33, 34, 41, 40, 31, 30, 14, 13, 11, 10, 3, 4, 1, 0, 44, 44, 33, 43, 34,
    41, 14, 31, 13, 40, 4, 30, 3, 11, 0, 1, 10, 0, 11, 1, 10, 30, 31, 40, 41, 3, 13, 4, 14, 33, 43,
    34, 44,
];

/// The 16 canonical tone classes (row heads of `TONE_CLASS_MAP`): {0,1,3,4,10,11,13,14,30,31,33,34,40,41,43,44}.
pub const NORMALIZED_CLASSES: [u8; 16] =
    [40, 4, 30, 3, 10, 1, 41, 14, 31, 13, 11, 33, 34, 43, 44, 0];

#[inline]
pub const fn level(cls: u8) -> u8 {
    cls / 10
}

#[inline]
pub const fn tone(cls: u8) -> u8 {
    cls % 10
}

/// Initial tone class from the G2P marker byte `& 0x7f` (FUN_0044ca50): 0->0, 1->1, 2|5->3, 3->2, 6->5, 7->4.
#[inline]
pub const fn initial_tone_class(marker: u8) -> u8 {
    match marker & 0x7F {
        1 => 1,
        2 | 5 => 3,
        3 => 2,
        6 => 5,
        7 => 4,
        _ => 0,
    }
}

#[inline]
pub const fn initial_tone_class_marker4_stale() -> u8 {
    1
}

/// Build one sentence's 12B records from the G2P output (the pump's copy loop).
pub fn build_sentence(codes: &[u16], markers: &[u8]) -> Vec<ProsodyRecord> {
    assert_eq!(
        codes.len(),
        markers.len(),
        "G2P code/marker arrays must match"
    );
    let n = codes.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let mut r = ProsodyRecord::new(codes[i]);
        r.init_from_marker(markers[i], i + 1 == n);
        out.push(r);
    }
    out
}

pub fn normalize_class(cls: u8) -> Option<u8> {
    for i in 0..16u8 {
        if TONE_CLASS_MAP[i as usize * 16] == cls {
            return Some(i);
        }
    }
    None
}

pub fn normalize_voiceinfo_class(cls: u8) -> Option<u8> {
    let mut c = cls;
    if level(c) == 2 {
        c = tone(c) + 0x1E;
    }
    match tone(c) {
        2 => c = level(c) * 10 + 3,
        5 => c = level(c) * 10 + 4,
        _ => {}
    }
    normalize_class(c)
}

/// Append one sentence's records to the buffer, applying sandhi and boundary linking.
pub fn apply_sandhi(buf: &mut Vec<ProsodyRecord>, sentence: &mut [ProsodyRecord]) {
    let ac = buf.len(); // engine record count before this sentence
    let n = sentence.len();
    if n == 0 {
        return;
    }

    let prev_non_pause =
        |i: usize, buf: &[ProsodyRecord], sentence: &[ProsodyRecord]| -> (usize, u8) {
            if i == 0 {
                return (ac - 1, buf[ac - 1].tone_class % 10);
            }
            let mut j = i - 1;
            while j > 0 && sentence[j].code == 0x1486 {
                j -= 1;
            }
            if j == 0 && sentence[j].code == 0x1486 {
                (ac - 1, buf[ac - 1].tone_class % 10)
            } else {
                (j, sentence[j].tone_class % 10)
            }
        };
    let first = if ac == 0 { 1 } else { 0 };
    for i in first..n {
        if sentence[i].code == 0x1486 {
            continue;
        }
        let (pj, prev_tone) = prev_non_pause(i, buf, sentence);
        if prev_tone == 5 {
            if pj == ac.saturating_sub(1) && i > 0 {
                sentence[pj].tone_class = 5;
            } else if ac > 0 && pj == ac - 1 {
                buf[ac - 1].tone_class = 5;
            } else {
                sentence[pj].tone_class = 5;
            }
            sentence[i].tone_class = sentence[i].tone_class.wrapping_add(0x1E);
        } else {
            sentence[i].tone_class = sentence[i].tone_class.wrapping_add(prev_tone * 10);
        }
    }

    if ac == 0 {
        sentence[0].tone_class = sentence[0].tone_class % 10 + 0x28;
    } else {
        if buf[ac - 1].marker == MARKER_SENTENCE_END {
            sentence[0].marker = MARKER_SPECIAL;
        }
        sentence[0].tone_class = (buf[ac - 1].tone_class % 10) * 10 + sentence[0].tone_class % 10;
    }

    buf.extend_from_slice(sentence);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_class_mapping() {
        assert_eq!(initial_tone_class(0), 0);
        assert_eq!(initial_tone_class(1), 1);
        assert_eq!(initial_tone_class(2), 3);
        assert_eq!(initial_tone_class(3), 2);
        assert_eq!(initial_tone_class(4), 0);
        assert_eq!(initial_tone_class(5), 3);
        assert_eq!(initial_tone_class(6), 5);
        assert_eq!(initial_tone_class(7), 4);
        assert_eq!(initial_tone_class(0x82), 3);
    }

    #[test]
    fn build_sentence_records() {
        let s = build_sentence(&[0x0100, 0x0101, 0x0102], &[0, 3, 0]);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].tone_class, 0);
        assert_eq!(s[0].marker, 0);
        assert_eq!(s[1].tone_class, 2);
        assert_eq!(s[2].tone_class, 1);
        assert_eq!(s[2].marker, MARKER_SENTENCE_END);
        let s = build_sentence(&[0x0100], &[0x83]);
        assert_eq!(s[0].flags, 1);
        assert_eq!(s[0].tone_class, 2);
    }

    #[test]
    #[should_panic]
    fn build_sentence_mismatched_lengths() {
        build_sentence(&[0x0100], &[0, 1]);
    }

    #[test]
    fn normalize_class_roundtrip() {
        for (i, &cls) in NORMALIZED_CLASSES.iter().enumerate() {
            assert_eq!(normalize_class(cls), Some(i as u8));
        }
        assert_eq!(normalize_class(0x28), Some(0));
        assert_eq!(normalize_class(0x1E), Some(2));
        assert_eq!(normalize_class(2), None);
        assert_eq!(normalize_class(0xFF), None);
        assert_eq!(normalize_class(5), None);
    }

    #[test]
    fn normalize_voiceinfo_class_remaps() {
        assert_eq!(normalize_voiceinfo_class(21), Some(8));
        assert_eq!(normalize_voiceinfo_class(12), Some(9));
        assert_eq!(normalize_voiceinfo_class(15), Some(7));
        assert_eq!(normalize_voiceinfo_class(1), Some(5));
        assert_eq!(normalize_voiceinfo_class(40), Some(0));
    }

    #[test]
    fn sandhi_first_sentence() {
        let mut buf: Vec<ProsodyRecord> = Vec::new();
        let mut s1 = build_sentence(&[0x0100, 0x0101], &[0, 0]);
        apply_sandhi(&mut buf, &mut s1);
        assert_eq!(buf[0].tone_class, 0x28);
        assert_eq!(buf[1].tone_class, 1);
        assert_eq!(buf[1].marker, MARKER_SENTENCE_END);
    }

    #[test]
    fn sandhi_prev5_sets_prev_class_to_5() {
        let mut buf: Vec<ProsodyRecord> = Vec::new();
        let mut s1 = build_sentence(&[0x0100, 0x0101], &[6, 6]);
        apply_sandhi(&mut buf, &mut s1);
        assert_eq!(buf[0].tone_class, 45);
        assert_eq!(buf[1].tone_class, 5 + 0x1E);

        let mut s2 = build_sentence(&[0x0200, 0x0201], &[1, 1]);
        apply_sandhi(&mut buf, &mut s2);
        assert_eq!(buf[1].tone_class, 5);
        assert_eq!(buf[2].tone_class, 51);
        assert_eq!(buf[3].tone_class, 11);
    }

    #[test]
    fn sandhi_boundary_marker_linking() {
        let mut buf: Vec<ProsodyRecord> = Vec::new();
        let mut s1 = build_sentence(&[0x0100, 0x0101], &[0, 0]);
        apply_sandhi(&mut buf, &mut s1);
        assert_eq!(buf[0].tone_class, 0x28);
        assert_eq!(buf[1].tone_class, 1);
        assert_eq!(buf[1].marker, MARKER_SENTENCE_END);

        let mut s2 = build_sentence(&[0x0200, 0x0201], &[2, 1]);
        apply_sandhi(&mut buf, &mut s2);
        assert_eq!(buf[2].tone_class, 13);
        assert_eq!(buf[2].marker, MARKER_SPECIAL);
        assert_eq!(buf[3].tone_class, 31);
    }

    #[test]
    fn sandhi_default_adds_prev_tone_times_10() {
        let mut buf: Vec<ProsodyRecord> = Vec::new();
        let mut s1 = build_sentence(&[0x0100, 0x0101, 0x0102], &[0, 3, 1]);
        apply_sandhi(&mut buf, &mut s1);
        assert_eq!(buf[0].tone_class, 0x28);
        assert_eq!(buf[1].tone_class, 2);
        assert_eq!(buf[2].tone_class, 21);
    }

    #[test]
    fn sandhi_single_record_sentence() {
        let mut buf: Vec<ProsodyRecord> = Vec::new();
        let mut s1 = build_sentence(&[0x0100], &[0]);
        apply_sandhi(&mut buf, &mut s1);
        assert_eq!(buf[0].tone_class, 1 % 10 + 0x28);
        assert_eq!(buf[0].marker, MARKER_SENTENCE_END);
        let mut s2 = build_sentence(&[0x0200], &[0]);
        apply_sandhi(&mut buf, &mut s2);
        assert_eq!(buf[1].marker, MARKER_SPECIAL);
        assert_eq!(buf[1].tone_class, 11);
    }

    #[test]
    fn sandhi_empty_sentence_noop() {
        let mut buf: Vec<ProsodyRecord> = Vec::new();
        let mut s: Vec<ProsodyRecord> = Vec::new();
        apply_sandhi(&mut buf, &mut s);
        assert!(buf.is_empty());
    }

    #[test]
    fn sandhi_first_record_no_previous_adjustment() {
        let mut buf: Vec<ProsodyRecord> = Vec::new();
        let mut s1 = build_sentence(&[0x0100], &[7]);
        apply_sandhi(&mut buf, &mut s1);
        assert_eq!(buf[0].tone_class, 4 % 10 + 0x28);
    }
}
