//! KPS bytes → segments. `break_after` uses engine codes 7=clause / 8=sentence (see `punct_boundary` 7 vs 9).

use crate::kps_class::{classify_next_char, KpsCharClass};

static LIST_9: &[&[u8]] = &[
    b".",        // ASCII period
    b"\xa1\xa5", // KPS fullwidth period
    b"!",        // ASCII exclamation
    b"\xa1\xaa", // KPS fullwidth exclamation
];

static LIST_7: &[&[u8]] = &[
    b",",        // ASCII comma
    b"\xa1\xa4", // KPS fullwidth comma
    b":",        // ASCII colon
    b"\xa1\xa7", // KPS fullwidth colon
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BreakType {
    None = 0,
    Clause = 7,
    Sentence = 8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegKind {
    Korean,
    Number,
    Latin,
    Symbol,
}

#[derive(Debug, Clone)]
pub struct Segment<'a> {
    pub bytes: &'a [u8],
    pub kind: SegKind,
    pub break_after: BreakType,
    /// Feeds `trailing_word_gap` in phoneme pass (space vs `\n` flush).
    pub after_whitespace: bool,
}

/// Returns 9 = sentence punct, 7 = clause punct, 0 = none (KPS or ASCII bytes).
fn punct_boundary(norm_bytes: &[u8]) -> u8 {
    let nb: &[u8] = {
        let end = norm_bytes
            .iter()
            .rposition(|&b| b != 0)
            .map(|i| i + 1)
            .unwrap_or(0);
        &norm_bytes[..end]
    };
    if nb.is_empty() {
        return 0;
    }

    for entry in LIST_9 {
        if *entry == nb {
            return 9;
        }
    }
    for entry in LIST_7 {
        if *entry == nb {
            return 7;
        }
    }
    0
}

fn class_to_kind(cls: KpsCharClass) -> SegKind {
    match cls {
        KpsCharClass::KoreanSyllable => SegKind::Korean,
        KpsCharClass::FullwidthDigit => SegKind::Number,
        KpsCharClass::FullwidthLetter => SegKind::Latin,
        _ => SegKind::Symbol,
    }
}

fn class_code(cls: KpsCharClass) -> u8 {
    match cls {
        KpsCharClass::KoreanSyllable => 1,
        KpsCharClass::Symbol => 2,
        KpsCharClass::FullwidthDigit => 3,
        KpsCharClass::FullwidthLetter => 4,
        KpsCharClass::MiscSymbol => 5,
        KpsCharClass::ExtSymbol => 6,
        KpsCharClass::KoreanJamo => 7,
        KpsCharClass::Unknown => 0,
    }
}

fn classify_next(input: &[u8]) -> (KpsCharClass, usize) {
    if input.is_empty() {
        return (KpsCharClass::Unknown, 0);
    }
    let b = input[0];
    if b.is_ascii_alphabetic() {
        (KpsCharClass::FullwidthLetter, 1)
    } else if b.is_ascii_digit() {
        (KpsCharClass::FullwidthDigit, 1)
    } else {
        classify_next_char(input)
    }
}

fn is_whitespace_at(input: &[u8], pos: usize) -> Option<usize> {
    let b = *input.get(pos)?;
    match b {
        b'\t' | b' ' | b'\r' | b'\n' => Some(1),
        0xa1 if input.get(pos + 1) == Some(&0xa2) => Some(2),
        _ => None,
    }
}

fn punct_break_type(prev_class: u8, next_code: u8, pb: u8) -> Option<BreakType> {
    if pb == 9 {
        // Sentence-final punctuation (. !)
        if prev_class == 1 {
            // Korean preceded → Sentence (8)
            Some(BreakType::Sentence)
        } else if (prev_class == 7 && next_code == 7) || (prev_class == 3 && next_code == 3) {
            // Both Jamo (no sentence break), or decimal separator (digit.digit)
            None
        } else if next_code == 2 {
            // Something + period + KPS Symbol → Sentence (8)
            Some(BreakType::Sentence)
        } else {
            // Default: clause-level break
            Some(BreakType::Clause)
        }
    } else {
        // pb == 7: clause punctuation (, : etc.)
        // Only exception: digit + comma/colon + digit = thousands/time separator
        if prev_class == 3 && next_code == 3 {
            None
        } else {
            Some(BreakType::Clause)
        }
    }
}

/// Kind-change + punct uses `prev_class` before the change (Mirae-style); `3.14`-style absorbs `.`.
pub fn segment<'a>(input: &'a [u8]) -> Vec<Segment<'a>> {
    let mut result: Vec<Segment<'a>> = Vec::new();

    let mut seg_start: usize = 0;
    let mut seg_end: usize = 0;
    let mut cur_kind: Option<SegKind> = None;
    let mut prev_class: u8 = 0;

    let len = input.len();
    let mut i = 0usize;

    macro_rules! flush {
        ($break_type:expr, $after_ws:expr) => {{
            if seg_end > seg_start {
                let kind = cur_kind.unwrap_or(SegKind::Symbol);
                result.push(Segment {
                    bytes: &input[seg_start..seg_end],
                    kind,
                    break_after: $break_type,
                    after_whitespace: $after_ws,
                });
            }
        }};
    }

    while i < len {
        if let Some(ws_len) = is_whitespace_at(input, i) {
            let mut j = i + ws_len;
            while j < len {
                if let Some(ws2) = is_whitespace_at(input, j) {
                    j += ws2;
                } else {
                    break;
                }
            }
            let has_line_break = input[i..j].iter().any(|&b| b == b'\n' || b == b'\r');
            let (break_after, after_ws) = if has_line_break {
                (BreakType::Sentence, false)
            } else {
                (BreakType::None, true)
            };
            flush!(break_after, after_ws);
            cur_kind = None;
            prev_class = 0;
            i = j;
            seg_start = i;
            seg_end = i;
            continue;
        }

        let (cls, char_len) = {
            let b = input[i];
            if b.is_ascii_alphabetic() {
                (KpsCharClass::FullwidthLetter, 1usize)
            } else if b.is_ascii_digit() {
                (KpsCharClass::FullwidthDigit, 1usize)
            } else {
                classify_next_char(&input[i..])
            }
        };
        if char_len == 0 {
            i += 1;
            continue;
        }

        let char_bytes = &input[i..i + char_len];
        let kind = class_to_kind(cls);
        let cur_code = class_code(cls);

        if let Some(existing_kind) = cur_kind {
            if existing_kind != kind {
                let pb_ahead = punct_boundary(char_bytes);
                if pb_ahead != 0 {
                    let next_code_look = if i + char_len < len {
                        let (nc, _) = classify_next(&input[i + char_len..]);
                        class_code(nc)
                    } else {
                        0
                    };
                    if let Some(bt) = punct_break_type(prev_class, next_code_look, pb_ahead) {
                        seg_end = i;
                        flush!(bt, false);
                        cur_kind = None;
                        prev_class = 0;
                        i += char_len;
                        seg_start = i;
                        seg_end = i;
                        continue;
                    } else {
                        seg_end = i + char_len;
                        prev_class = cur_code;
                        i += char_len;
                        continue;
                    }
                }
                seg_end = i;
                flush!(BreakType::None, false);
                seg_start = i;
            }
        }
        seg_end = i + char_len;
        cur_kind = Some(kind);

        let pb = punct_boundary(char_bytes);

        if pb != 0 {
            let next_code = if i + char_len < len {
                let (ncls, _) = classify_next(&input[i + char_len..]);
                class_code(ncls)
            } else {
                0
            };

            if let Some(bt) = punct_break_type(prev_class, next_code, pb) {
                flush!(bt, false);
                cur_kind = None;
                prev_class = 0;
                i += char_len;
                seg_start = i;
                seg_end = i;
                continue;
            }
        }

        if seg_end - seg_start > 0x1f0 {
            // 496-byte segment cap (original buffer limit)
            flush!(BreakType::None, false);
            cur_kind = None;
            seg_start = seg_end;
            prev_class = cur_code;
            i += char_len;
            continue;
        }

        prev_class = cur_code;
        i += char_len;
    }

    seg_end = i;
    flush!(BreakType::None, false);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_punct_boundary_sentence() {
        assert_eq!(punct_boundary(b"."), 9);
        assert_eq!(punct_boundary(b"!"), 9);
        assert_eq!(punct_boundary(b"\xa1\xa5"), 9); // KPS fullwidth period
    }

    #[test]
    fn test_punct_boundary_clause() {
        assert_eq!(punct_boundary(b","), 7);
        assert_eq!(punct_boundary(b":"), 7);
        assert_eq!(punct_boundary(b"\xa1\xa4"), 7); // KPS fullwidth comma
    }

    #[test]
    fn test_punct_boundary_none() {
        assert_eq!(punct_boundary(b"a"), 0);
        assert_eq!(punct_boundary(b"\xb9\xce"), 0); // KPS Korean syllable 민
    }

    #[test]
    fn test_segment_ascii_words() {
        // "hello world" — two Latin segments separated by a space
        let input = b"hello world";
        let segs = segment(input);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].bytes, b"hello");
        assert_eq!(segs[0].kind, SegKind::Latin);
        assert_eq!(segs[1].bytes, b"world");
    }

    #[test]
    fn test_segment_sentences() {
        // "abc. def" — sentence break after period
        let input = b"abc. def";
        let segs = segment(input);
        // "abc" then "." then " " then "def"
        // The period is flushed as symbol segment with Sentence break,
        // then space is consumed, then "def".
        let kinds: Vec<_> = segs.iter().map(|s| s.kind).collect();
        assert!(kinds.contains(&SegKind::Latin));
    }

    #[test]
    fn test_period_after_latin_gives_clause() {
        // With the kind-change punctuation-lookahead fix, "Hello." is now ONE
        // segment: '.' triggers the flush of "Hello" with Clause break (and is
        // consumed itself, not added to a separate segment).
        // Latin + period at end of input → Clause (short pause), not Sentence.
        let input = b"Hello.";
        let segs = segment(input);
        // Only one segment: "Hello" (Latin, Clause break)
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].kind, SegKind::Latin);
        assert_eq!(
            segs[0].break_after as u8,
            BreakType::Clause as u8,
            "period after Latin text → Clause (short pause), not Sentence"
        );
    }

    #[test]
    fn test_period_after_korean_gives_sentence() {
        // KPS Korean syllable 가 = 0xb0a1, followed by ASCII period.
        // With the kind-change fix, '.' flushes "가" with Sentence (prev_class==1=Korean)
        // and is consumed → Sentence break after Korean + period.
        let input: &[u8] = &[0xb0, 0xa1, b'.'];
        let segs = segment(input);
        // One segment: Korean "가" with Sentence break
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].kind, SegKind::Korean);
        assert_eq!(
            segs[0].break_after as u8,
            BreakType::Sentence as u8,
            "period after Korean text → Sentence (long pause)"
        );
    }

    #[test]
    fn test_decimal_point_absorbed() {
        // "3.14" — digit + period + digit → decimal separator.
        // With the kind-change punctuation-lookahead fix, '.' is absorbed into the
        // Number segment (punct_break_type returns None for prev=digit, next=digit).
        // Result: ONE Number segment "3.14", correctly passed to apply_number_conversion.
        let input = b"3.14";
        let segs = segment(input);
        assert_eq!(segs.len(), 1, "decimal point absorbed into Number segment");
        assert_eq!(segs[0].kind, SegKind::Number);
        assert_eq!(segs[0].bytes, b"3.14");
        assert_eq!(segs[0].break_after as u8, BreakType::None as u8);
    }

    #[test]
    fn test_segment_whitespace_only() {
        let segs = segment(b"   \t\r\n");
        assert!(segs.is_empty());
    }

    #[test]
    fn newline_flush_matches_sentence_final_prosody() {
        let segs = segment(b"A\nB");
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].bytes, b"A");
        assert_eq!(segs[0].break_after, BreakType::Sentence);
        assert!(!segs[0].after_whitespace);
        assert_eq!(segs[1].bytes, b"B");
    }

    #[test]
    fn ascii_space_only_keeps_word_gap_flag() {
        let segs = segment(b"A B");
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].break_after, BreakType::None);
        assert!(segs[0].after_whitespace);
    }

    #[test]
    fn test_segment_kps_fullwidth_space() {
        // KPS full-width space 0xa1a2 followed by KPS text then ASCII
        let input: &[u8] = &[0xa1, 0xa2, b'A', b'B'];
        let segs = segment(input);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].bytes, b"AB");
    }

    #[test]
    fn test_break_type_values() {
        assert_eq!(BreakType::None as u8, 0);
        assert_eq!(BreakType::Clause as u8, 7);
        assert_eq!(BreakType::Sentence as u8, 8);
    }
}
