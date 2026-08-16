//! 声調 (tone) — tone classes, sandhi rules, and 16-value normalization.
//!
//! A tone class is `レベル(level)×10 + 調値(tone)` with level 0..4 and
//! tone 0..9 (SPEC §2.4).
//!
//! ## Sandhi (FUN_0044ca50)
//!
//! When the engine pump appends a sentence's records to the record buffer,
//! it applies, per record (using the previous record's class):
//! ```text
//! prev = previous record's tone_class % 10
//! if prev == 5: previous class = 5; current class += 0x1e (30)
//! else:         current class += prev × 10
//! ```
//! plus, for the very first record of the whole text:
//! ```text
//! class = class % 10 + 0x28 (40)
//! ```
//! and for the first record of a non-first sentence (sentence-boundary
//! tone linking):
//! ```text
//! if previous sentence's last record marker == 1: marker = 2
//! class = (previous sentence's last class % 10) × 10 + class % 10
//! ```
//!
//! ## Normalization (DAT_0048bd40)
//!
//! `TONE_CLASS_MAP` is the 16×16 table dumped from the binary; row `i`'s
//! head entry `TONE_CLASS_MAP[i*16]` is the canonical class of normalized
//! index `i`. [`normalize_class`] finds the row whose head equals the class
//! (the same search FUN_0044a800 performs); the 16 canonical values are
//! {0,1,3,4,10,11,13,14,30,31,33,34,40,41,43,44}. `FUN_0044a800` first
//! adjusts VoiceInfo class codes (level 2 → tone+0x1e; tone 2 → 3; tone 5 →
//! 4) — see [`normalize_voiceinfo_class`].

use crate::record::{ProsodyRecord, MARKER_SENTENCE_END, MARKER_SPECIAL};

/// `DAT_0048bd40` — 声調クラス→調値 index (16×16) 変換表, dumped from
/// Future.exe (mirrors `crate::tables`, kept here as the public source for
/// the tone pipeline).
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

/// The 16 canonical tone classes — the **first column** (row heads) of
/// [`TONE_CLASS_MAP`], i.e. the classes that [`normalize_class`] maps to
/// row indices 0..15 (verified against the binary dump):
/// {0,1,3,4,10,11,13,14,30,31,33,34,40,41,43,44}.
pub const NORMALIZED_CLASSES: [u8; 16] =
    [40, 4, 30, 3, 10, 1, 41, 14, 31, 13, 11, 33, 34, 43, 44, 0];

/// Level of a tone class (`cls / 10`).
#[inline]
pub const fn level(cls: u8) -> u8 {
    cls / 10
}

/// Tone value of a tone class (`cls % 10`).
#[inline]
pub const fn tone(cls: u8) -> u8 {
    cls % 10
}

/// Initial tone class assigned by the pump (`FUN_0044ca50` switch on the
/// G2P marker byte `& 0x7f`): 0→0, 1→1, 2|5→3, 3→2, 6→5, 7→4 (t13 §3.1).
///
/// t19: marker 1→1 (最終マーカ 1 の音節は実測クラス 1)。T9 の「1→0」は
/// 辞書マーカと最終マーカの混同に基づく誤りで、音素別マーカ変換
/// (段階1/段階8) の実装に伴い撤去した。marker 4 はオリジナルの switch に
/// case がなく書込なし (= 初期値 0)。
#[inline]
pub const fn initial_tone_class(marker: u8) -> u8 {
    match marker & 0x7F {
        1 => 1,
        2 | 5 => 3,
        3 => 2,
        6 => 5,
        7 => 4,
        _ => 0, // 0, 4, >7 (case 4 = 書込なし)
    }
}

/// `case 4` の stale 値の検証用: marker 4 → 1 (仮説)。
#[inline]
pub const fn initial_tone_class_marker4_stale() -> u8 {
    1
}

/// Build one sentence's 12B records from the G2P output (the pump's copy
/// loop in `FUN_0044ca50`): per record a `(u16 code, u8 marker_byte)` pair.
/// The marker byte's bit7 becomes the record flag, its low 7 bits select
/// the initial tone class; the sentence-final record (marker 0) gets
/// `marker = 1` (文末) and class 1.
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

/// Normalize a tone class to its 16-value row index, searching the row
/// heads of [`TONE_CLASS_MAP`] (`DAT_0048bd40[i*16] == cls`, the same
/// search as FUN_0044a800). Returns `None` when the class is not a
/// canonical value.
pub fn normalize_class(cls: u8) -> Option<u8> {
    for i in 0..16u8 {
        if TONE_CLASS_MAP[i as usize * 16] == cls {
            return Some(i);
        }
    }
    None
}

/// Full FUN_0044a800 normalization of a VoiceInfo class code: level 2 is
/// remapped to tone+0x1e, tone 2 → 3, tone 5 → 4, then looked up in
/// [`TONE_CLASS_MAP`].
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

/// Append one sentence's records to the engine record buffer, applying the
/// tone sandhi and boundary linking of `FUN_0044ca50` (see module docs).
///
/// `buf` must already contain the sandhi'd records of all previous
/// sentences (the pump's `+0xa8` buffer). The records in `sentence` must be
/// freshly built (e.g. via [`build_sentence`]); they are modified in place
/// (final classes/markers) and appended to `buf` in order.
pub fn apply_sandhi(buf: &mut Vec<ProsodyRecord>, sentence: &mut [ProsodyRecord]) {
    let ac = buf.len(); // engine record count before this sentence
    let n = sentence.len();
    if n == 0 {
        return;
    }

    // Sandhi loop: prev = previous record's class % 10.
    // For the first sentence (ac == 0) record 0 is skipped (it gets the
    // +0x28 treatment below); otherwise record 0's prev is the last record
    // of the previous sentence.
    // T9 fix: the original pump's parse never emits records for pause
    // syllables (《》/． etc. — the pause unit 4114906 is inserted later by
    // FUN_0044b7a0 without a scan). The port's G2P emits them as code
    // 0x1486; they must not participate in the sandhi chain (their class
    // must not propagate a prev_tone and they must not receive one).
    let prev_non_pause = |i: usize, buf: &[ProsodyRecord], sentence: &[ProsodyRecord]| -> (usize, u8) {
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
            // prev record's class forced to 5; current class += 0x1e
            // (ac == 0 の先頭文ではバッファ前レコードは存在しない —
            // 文内 prev への書込に倒す。旧コードは ac-1 でアンダーフロー)
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
        // Very first record of the whole text: (class % 10) + 0x28
        sentence[0].tone_class = sentence[0].tone_class % 10 + 0x28;
    } else {
        // Sentence-boundary linking for a non-first sentence.
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
        // t13 §3.1: 最終マーカ 1 → tone 1 (T9 の 1→0 は辞書マーカ混同)
        assert_eq!(initial_tone_class(1), 1);
        assert_eq!(initial_tone_class(2), 3);
        assert_eq!(initial_tone_class(3), 2);
        assert_eq!(initial_tone_class(4), 0); // オリジナル switch に case 4 なし
        assert_eq!(initial_tone_class(5), 3);
        assert_eq!(initial_tone_class(6), 5);
        assert_eq!(initial_tone_class(7), 4);
        assert_eq!(initial_tone_class(0x82), 3); // bit7 ignored for class
    }

    #[test]
    fn build_sentence_records() {
        let s = build_sentence(&[0x0100, 0x0101, 0x0102], &[0, 3, 0]);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].tone_class, 0);
        assert_eq!(s[0].marker, 0);
        assert_eq!(s[1].tone_class, 2);
        assert_eq!(s[2].tone_class, 1); // sentence-final marker-0 → class 1
        assert_eq!(s[2].marker, MARKER_SENTENCE_END);
        // bit7 → flags
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
        // every canonical class (row head) normalizes to its row index
        for (i, &cls) in NORMALIZED_CLASSES.iter().enumerate() {
            assert_eq!(normalize_class(cls), Some(i as u8));
        }
        assert_eq!(normalize_class(0x28), Some(0)); // 40 → row 0
        assert_eq!(normalize_class(0x1E), Some(2)); // 30 → row 2
        assert_eq!(normalize_class(2), None); // non-canonical
        assert_eq!(normalize_class(0xFF), None);
        assert_eq!(normalize_class(5), None);
    }

    #[test]
    fn normalize_voiceinfo_class_remaps() {
        // level 2 → tone + 0x1e: 21 (2*10+1) → 1+30=31 → row 8
        assert_eq!(normalize_voiceinfo_class(21), Some(8));
        // tone 2 → 3: 12 → 13 → row 9
        assert_eq!(normalize_voiceinfo_class(12), Some(9));
        // tone 5 → 4: 15 → 14 → row 7
        assert_eq!(normalize_voiceinfo_class(15), Some(7));
        // canonical already: 1 → row 5
        assert_eq!(normalize_voiceinfo_class(1), Some(5));
        // 40 → row 0
        assert_eq!(normalize_voiceinfo_class(40), Some(0));
    }

    #[test]
    fn sandhi_first_sentence() {
        // T4 §2.3 example: first record of text → (class % 10) + 0x28
        let mut buf: Vec<ProsodyRecord> = Vec::new();
        let mut s1 = build_sentence(&[0x0100, 0x0101], &[0, 0]); // classes 0, 1
        apply_sandhi(&mut buf, &mut s1);
        assert_eq!(buf[0].tone_class, 0x28); // (0 % 10) + 40
                                             // second: prev = buf[0] % 10 = 0 → += 0 → class stays 1
        assert_eq!(buf[1].tone_class, 1);
        assert_eq!(buf[1].marker, MARKER_SENTENCE_END);
    }

    #[test]
    fn sandhi_prev5_sets_prev_class_to_5() {
        // prev class 5 (tone 5): prev class = 5, current += 0x1e
        let mut buf: Vec<ProsodyRecord> = Vec::new();
        let mut s1 = build_sentence(&[0x0100, 0x0101], &[6, 6]); // classes 5, 5
        apply_sandhi(&mut buf, &mut s1);
        // record 0: (5 % 10) + 0x28 = 45
        assert_eq!(buf[0].tone_class, 45);
        // record 1: prev tone 5 → prev class = 5, cur += 0x1e → 35
        assert_eq!(buf[1].tone_class, 5 + 0x1E);

        // second sentence: [marker 1 → class 1, marker 1 → class 1] (t13)
        let mut s2 = build_sentence(&[0x0200, 0x0201], &[1, 1]);
        apply_sandhi(&mut buf, &mut s2);
        // record 0's prev = buf[1] (35, tone 5) → buf[1] class = 5,
        // s2[0] += 0x1e → 31, then boundary link replaces it:
        // (5 % 10) * 10 + 31 % 10 = 51
        assert_eq!(buf[1].tone_class, 5);
        assert_eq!(buf[2].tone_class, 51);
        // record 1: prev = s2[0] (51, tone 1) → += 10 → 11
        assert_eq!(buf[3].tone_class, 11);
    }

    #[test]
    fn sandhi_boundary_marker_linking() {
        // previous sentence ended with marker 1 → next sentence's first
        // record gets marker 2 and its class is linked
        let mut buf: Vec<ProsodyRecord> = Vec::new();
        let mut s1 = build_sentence(&[0x0100, 0x0101], &[0, 0]); // [0, 1], markers [0, 1]
        apply_sandhi(&mut buf, &mut s1);
        assert_eq!(buf[0].tone_class, 0x28);
        assert_eq!(buf[1].tone_class, 1);
        assert_eq!(buf[1].marker, MARKER_SENTENCE_END);

        let mut s2 = build_sentence(&[0x0200, 0x0201], &[2, 1]); // [3, 1]
        apply_sandhi(&mut buf, &mut s2);
        // s2[0]: prev buf[1] (1, tone 1) → += 10 → 13; linked:
        //        (1 % 10) * 10 + 13 % 10 = 13; marker 1 → 2
        assert_eq!(buf[2].tone_class, 13);
        assert_eq!(buf[2].marker, MARKER_SPECIAL);
        // s2[1]: prev 13 (tone 3) → += 30 → 31 (marker 1 → class 1, t13)
        assert_eq!(buf[3].tone_class, 31);
    }

    #[test]
    fn sandhi_default_adds_prev_tone_times_10() {
        // prev tone 3 → current class += 30
        let mut buf: Vec<ProsodyRecord> = Vec::new();
        let mut s1 = build_sentence(&[0x0100, 0x0101, 0x0102], &[0, 3, 1]);
        apply_sandhi(&mut buf, &mut s1);
        assert_eq!(buf[0].tone_class, 0x28);
        // buf[1]: prev 40 % 10 = 0 → 2
        assert_eq!(buf[1].tone_class, 2);
        // buf[2]: prev 2 % 10 = 2 → 1 + 20 = 21 (marker 1 → class 1, t13)
        assert_eq!(buf[2].tone_class, 21);
    }

    #[test]
    fn sandhi_single_record_sentence() {
        let mut buf: Vec<ProsodyRecord> = Vec::new();
        let mut s1 = build_sentence(&[0x0100], &[0]); // class 1, marker 1 (sentence-final)
        apply_sandhi(&mut buf, &mut s1);
        assert_eq!(buf[0].tone_class, 1 % 10 + 0x28); // 41
        assert_eq!(buf[0].marker, MARKER_SENTENCE_END);
        // second single-record sentence: linking uses buf[0] class 41
        let mut s2 = build_sentence(&[0x0200], &[0]);
        apply_sandhi(&mut buf, &mut s2);
        // buf[0].marker == 1 → s2[0].marker = 2
        assert_eq!(buf[1].marker, MARKER_SPECIAL);
        // (41 % 10) * 10 + (1 + 10) % 10 = 10 + 1 = 11
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
        // record 0 of the whole text gets no sandhi adjustment, only +0x28
        let mut buf: Vec<ProsodyRecord> = Vec::new();
        let mut s1 = build_sentence(&[0x0100], &[7]); // class 4
        apply_sandhi(&mut buf, &mut s1);
        assert_eq!(buf[0].tone_class, 4 % 10 + 0x28); // 44
    }
}
