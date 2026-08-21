//! Tokenizer / sentence splitter (FUN_00402240) over internal-code bytes.
//! ASCII bytes -> 1-byte tokens; KPS9566 chars -> 2-byte tokens; boundary punctuation
//! (0xA1A5/0xA1A9/0xA1AA) + special '.' heuristics; 496-char forced break,
//! hard 49,996B buffer limit; crlf_breaks mode per original DAT_00489140.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sentence {
    pub text: Vec<u8>,
    pub start: usize,
}

/// Tokenizer buffer size of the original (FUN_00402240, `operator_new(50000)`).
pub const TOKEN_BUFFER_SIZE: usize = 50000;

/// Hard flush limit: the original flushes when pos > 0xC34C (49996).
pub const HARD_FLUSH_LIMIT: usize = 0xC34C;

/// Forced sentence break once the current sentence exceeds this many bytes (SPEC 2.2: 496).
pub const MAX_SENTENCE_BYTES: usize = 0x1F0;

pub const KPS_SPACE: u16 = 0xA1A1;
pub const KPS_FULL_STOP: u16 = 0xA1A5;
pub const KPS_QUESTION: u16 = 0xA1A9;
pub const KPS_EXCLAMATION: u16 = 0xA1AA;

/// `DAT_0048a308` - per-ASCII-byte mapping to the KPS9566 equivalent code (dumped).
pub const ASCII_TO_KPS: [u16; 256] = [
    0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0009, 0x0000, 0x0000,
    0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0xA1A1, 0xA1AA, 0xA1D4, 0xA2D7,
    0xA8A8, 0xA8AC, 0xA2D8, 0xA1BB, 0xA1CA, 0xA1CB, 0xA2D9, 0xA2A1, 0xA1A4, 0xA1AF, 0xA1A5, 0xA1B3,
    0xA3B0, 0xA3B1, 0xA3B2, 0xA3B3, 0xA3B4, 0xA3B5, 0xA3B6, 0xA3B7, 0xA3B8, 0xA3B9, 0xA1A7, 0xA1A8,
    0xA2A8, 0xA2A6, 0xA2A9, 0xA1A9, 0xA2DA, 0xA3C1, 0xA3C2, 0xA3C3, 0xA3C4, 0xA3C5, 0xA3C6, 0xA3C7,
    0xA3C8, 0xA3C9, 0xA3CA, 0xA3CB, 0xA3CC, 0xA3CD, 0xA3CE, 0xA3CF, 0xA3D0, 0xA3D1, 0xA3D2, 0xA3D3,
    0xA3D4, 0xA3D5, 0xA3D6, 0xA3D7, 0xA3D8, 0xA3D9, 0xA3DA, 0xA1CE, 0xA1B4, 0xA1CF, 0xA1BE, 0xA1B1,
    0xA1BC, 0xA3E1, 0xA3E2, 0xA3E3, 0xA3E4, 0xA3E5, 0xA3E6, 0xA3E7, 0xA3E8, 0xA3E9, 0xA3EA, 0xA3EB,
    0xA3EC, 0xA3ED, 0xA3EE, 0xA3EF, 0xA3F0, 0xA3F1, 0xA3F2, 0xA3F3, 0xA3F4, 0xA3F5, 0xA3F6, 0xA3F7,
    0xA3F8, 0xA3F9, 0xA3FA, 0xA1D0, 0xA1D0, 0xA1B5, 0xA1D1, 0x0000, 0x0200, 0x0202, 0x0403, 0x0704,
    0x0001, 0x0000, 0x0000, 0x0500, 0x0505, 0x0000, 0x0006, 0x0000, 0x0100, 0x0000, 0x7520, 0x0047,
    0x751C, 0x0047, 0x7518, 0x0047, 0x7514, 0x0047, 0x68C7, 0x0047, 0x7510, 0x0047, 0x750C, 0x0047,
    0x7508, 0x0047, 0x7504, 0x0047, 0x68C7, 0x0047, 0x7500, 0x0047, 0x74FC, 0x0047, 0x74F8, 0x0047,
    0x74F4, 0x0047, 0x74F0, 0x0047, 0x74EC, 0x0047, 0x74E8, 0x0047, 0x74E4, 0x0047, 0x74E0, 0x0047,
    0x74DC, 0x0047, 0x68C7, 0x0047, 0x74D8, 0x0047, 0x74D4, 0x0047, 0x74D0, 0x0047, 0x74CC, 0x0047,
    0x68C7, 0x0047, 0x0000, 0x0000, 0x6718, 0x0047, 0x74C8, 0x0047, 0x74C4, 0x0047, 0x74C0, 0x0047,
    0x74BC, 0x0047, 0x74B8, 0x0047, 0x74B4, 0x0047, 0x66D4, 0x0047, 0x74B0, 0x0047, 0x74AC, 0x0047,
    0x6704, 0x0047, 0x74A8, 0x0047, 0x74A4, 0x0047, 0x74A0, 0x0047, 0x749C, 0x0047, 0x7498, 0x0047,
    0x7494, 0x0047, 0x7490, 0x0047, 0x748C, 0x0047, 0x7488, 0x0047, 0x7484, 0x0047, 0x7480, 0x0047,
    0x747C, 0x0047, 0x7478, 0x0047, 0x7474, 0x0047, 0x7470, 0x0047, 0x746C, 0x0047, 0x7468, 0x0047,
    0x7464, 0x0047, 0x7460, 0x0047,
];

/// Character class of a 16-bit code - faithful port of `FUN_0040b240`.
#[inline]
pub fn char_class_16(ch: u16) -> u8 {
    if (0xA1A0 < ch) && (ch < 0xA1F4) {
        return 1;
    }
    if (0xA2A0 < ch) && (ch < 0xA2DD) {
        return 2;
    }
    if (0xA2DC < ch) && (ch < 0xA2FF) {
        return 3;
    }
    if (0xA3AF < ch) && (ch < 0xA3BA) {
        return 4;
    }
    if (0xA3C0 < ch) && (ch < 0xA3DB) {
        return 5;
    }
    if (0xA3E0 < ch) && (ch < 0xA3FB) {
        return 6;
    }
    if (0xA4A0 < ch) && (ch < 0xA4D4) {
        return 7;
    }
    if (0xA4E7 < ch) && (ch < 0xA4EE) {
        return 8;
    }
    if (0xA5A0 < ch) && (ch < 0xA5C2) {
        return 9;
    }
    if (0xA5D0 < ch) && (ch < 0xA5F2) {
        return 10;
    }
    if (0xA6A0 < ch) && (ch < 0xA6B9) {
        return 0xB;
    }
    if (0xA6C0 < ch) && (ch < 0xA6D9) {
        return 0xC;
    }
    if (0xA6E0 < ch) && (ch < 0xA6EB) {
        return 0xD;
    }
    if (0xA6F0 < ch) && (ch < 0xA6FB) {
        return 0xE;
    }
    if (0xA7A0 < ch) && (ch < 0xA7BF) {
        return 0xF;
    }
    if (0xA7C0 < ch) && (ch < 0xA7CF) {
        return 0x10;
    }
    if (0xA7D0 < ch) && (ch < 0xA7DF) {
        return 0x11;
    }
    if (0xA7DF < ch) && (ch < 0xA7EF) {
        return 0x12;
    }
    if (0xA7EF < ch) && (ch < 0xA7FF) {
        return 0x13;
    }
    if (0xA8A0 < ch) && (ch < 0xA8FF) {
        return 0x14;
    }
    if (0xA9A0 < ch) && (ch < 0xA9E5) {
        return 0x15;
    }
    if (0xAAA0 < ch) && (ch < 0xAAF4) {
        return 0x16;
    }
    if (0xABA0 < ch) && (ch < 0xABF7) {
        return 0x17;
    }
    if (0xACA0 < ch) && (ch < 0xACE1) {
        return 0x18;
    }
    if (0xB0A0 < ch) && (ch < 0xCCD0) && is_syllable_code(ch) {
        return 0x19;
    }
    if (0xCDA0 < ch) && (ch < 0xFED0) {
        return 0x1A;
    }
    0
}

#[inline]
pub fn char_class(byte: u8) -> u8 {
    char_class_16(ASCII_TO_KPS[byte as usize])
}

#[inline]
pub fn is_sentence_punct_ascii(b: u8) -> bool {
    b == b'!' || b == b'.' || b == b'?'
}

#[inline]
pub fn is_continue_ascii(b: u8) -> bool {
    matches!(b, 0x09 | 0x20 | 0x28 | 0x3C)
}

#[inline]
pub fn is_continue_kps(ch: u16) -> bool {
    matches!(ch, 0xA1A1 | 0xA1CA | 0xA1D2 | 0xA1D4 | 0xA2A8 | 0xA2B8)
}

#[inline]
pub fn is_sentence_punct_kps(ch: u16) -> bool {
    matches!(ch, 0xA1A5 | 0xA1A9 | 0xA1AA)
}

#[inline]
pub fn is_syllable_code(ch: u16) -> bool {
    (0xA0 < (ch >> 8)) && ((ch >> 8) < 0xCD) && (0x9F < (ch & 0xFF)) && ((ch & 0xFF) < 0xFF)
}

pub fn next_token_class(bytes: &[u8]) -> (u8, usize) {
    let Some(&b0) = bytes.first() else {
        return (0, 0);
    };
    let code = if b0 < 0x80 {
        ASCII_TO_KPS[b0 as usize]
    } else if b0 < 0xA1 {
        return (0, 0);
    } else {
        let b1 = bytes.get(1).copied().unwrap_or(0);
        ((b0 as u16) << 8) | b1 as u16
    };
    let class = char_class_16(code);
    if class != 0 {
        let len = if b0 < 0x80 { 1 } else { 2 };
        (class, len)
    } else {
        (0, 0)
    }
}

/// Tokenize the internal-code byte string into sentences (`FUN_00402240`).
pub fn tokenize(text: &[u8]) -> Vec<Sentence> {
    tokenize_with(text, false, MAX_SENTENCE_BYTES)
}

pub fn tokenize_crlf(text: &[u8]) -> Vec<Sentence> {
    tokenize_with(text, true, MAX_SENTENCE_BYTES)
}

/// Tokenize with explicit options (`max_sentence_bytes` override).
pub fn tokenize_with(text: &[u8], crlf_breaks: bool, max_sentence_bytes: usize) -> Vec<Sentence> {
    // strlen semantics of the original
    let text = &text[..text.iter().position(|&b| b == 0).unwrap_or(text.len())];
    let len = text.len();

    // Flush the current sentence, optionally appending a delimiter first (FUN_00402180).
    fn flush(
        sentences: &mut Vec<Sentence>,
        buf: &mut Vec<u8>,
        start: usize,
        prev_class: &mut u8,
        delim: Option<&[u8]>,
    ) {
        if buf.is_empty() {
            return;
        }
        if let Some(d) = delim {
            buf.extend_from_slice(d);
        }
        sentences.push(Sentence {
            text: std::mem::take(buf),
            start,
        });
        *prev_class = 0;
    }

    let mut sentences: Vec<Sentence> = Vec::new();
    let mut buf: Vec<u8> = Vec::with_capacity(1024); // current sentence (≤ 50000)
    let mut start: usize = 0; // byte offset where the current sentence began
    let mut prev_class: u8 = 0; // obj+0x3c
    // NOTE: the original stores the next-token class at obj+0x40 but the port re-derives it.

    let mut pos = 0usize;
    while pos < len {
        if crlf_breaks && (text[pos] == b'\r' || text[pos] == b'\n') {
            flush(&mut sentences, &mut buf, start, &mut prev_class, None);
            // skip run of \r \n \t ' ' and KPS space (0xA1A1)
            while let Some(&c) = text.get(pos + 1) {
                let kps_space =
                    pos + 2 < len && ((c as u16) << 8 | text[pos + 2] as u16) == KPS_SPACE;
                if c != b'\r' && c != b'\n' && c != b'\t' && c != b' ' && !kps_space {
                    break;
                }
                pos += 1;
                if c >= 0xA1 {
                    pos += 1;
                }
            }
            pos += 1;
            continue;
        }

        let b0 = text[pos];
        let b1 = text.get(pos + 1).copied().unwrap_or(0);
        let b2 = text.get(pos + 2).copied().unwrap_or(0);
        let b3 = text.get(pos + 3).copied().unwrap_or(0);

        // forced flush: hard buffer limit + SPEC §2.2 496-char limit
        if buf.len() > HARD_FLUSH_LIMIT || buf.len() > max_sentence_bytes {
            flush(&mut sentences, &mut buf, start, &mut prev_class, None);
        }

        if b0 < 0x80 {
            // ---- ASCII ----
            if !is_sentence_punct_ascii(b0) {
                if buf.is_empty() {
                    start = pos;
                }
                buf.push(b0);
                prev_class = char_class(b0);
            } else if buf.is_empty()
                || pos + 1 >= len
                || ((b1 > 0x7F || !is_continue_ascii(b1))
                    && (b1 < 0xA1 || !is_continue_kps(((b1 as u16) << 8) | b2 as u16)))
            {
                // boundary candidate: check the '.' special cases
                let (nc, _) = next_token_class(&text[pos + 1..]);
                if buf.is_empty()
                    || b0 != b'.'
                    || (prev_class == 0x19 && nc == 1)
                    || (prev_class == 4 && nc == 4)
                    || (prev_class == 7 && nc == 7)
                {
                    if buf.is_empty() {
                        start = pos;
                    }
                    buf.push(b0);
                    prev_class = char_class(b0);
                } else {
                    flush(&mut sentences, &mut buf, start, &mut prev_class, Some(b"."));
                }
            } else {
                flush(
                    &mut sentences,
                    &mut buf,
                    start,
                    &mut prev_class,
                    Some(&[b0]),
                );
            }
        } else if b0 > 0xA0 && b0 != 0xFF && b1 > 0xA0 {
            // ---- 2-byte KPS9566 char ----
            let ch = ((b0 as u16) << 8) | b1 as u16;
            if !is_sentence_punct_kps(ch) {
                // syllable token
                if buf.is_empty() {
                    start = pos;
                }
                buf.extend_from_slice(&[b0, b1]);
                prev_class = char_class_16(ch);
            } else if buf.is_empty()
                || pos + 2 >= len
                || ((b2 > 0x7F || !is_continue_ascii(b2))
                    && (b2 < 0xA1 || !is_continue_kps(((b2 as u16) << 8) | b3 as u16)))
            {
                // boundary candidate; only '．' (0xA1A5) has the '.' exceptions
                let (nc, _) = next_token_class(&text[pos + 2..]);
                if buf.is_empty()
                    || ch != KPS_FULL_STOP
                    || (prev_class == 0x19 && nc == 1)
                    || (prev_class == 4 && nc == 4)
                    || (prev_class == 7 && nc == 7)
                {
                    if buf.is_empty() {
                        start = pos;
                    }
                    buf.extend_from_slice(&[b0, b1]);
                    prev_class = char_class_16(ch);
                } else {
                    flush(
                        &mut sentences,
                        &mut buf,
                        start,
                        &mut prev_class,
                        Some(&[b0, b1]),
                    );
                }
            } else {
                flush(
                    &mut sentences,
                    &mut buf,
                    start,
                    &mut prev_class,
                    Some(&[b0, b1]),
                );
            }
            pos += 1; // 2-byte char: second increment below
        }
        // else: 0x80..0xA0 / 0xFF bytes are silently dropped
        pos += 1;
    }
    flush(&mut sentences, &mut buf, start, &mut prev_class, None);
    sentences
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(s: &[Sentence]) -> Vec<Vec<u8>> {
        s.iter().map(|x| x.text.clone()).collect()
    }

    #[test]
    fn plain_korean_one_sentence() {
        let t = tokenize(&[0xca, 0xaf, 0xb2, 0xce, 0xc2, 0xd7, 0xbb, 0xbd, 0xca, 0xfd]);
        assert_eq!(
            texts(&t),
            vec![vec![
                0xca, 0xaf, 0xb2, 0xce, 0xc2, 0xd7, 0xbb, 0xbd, 0xca, 0xfd
            ]]
        );
        assert_eq!(t[0].start, 0);
    }

    #[test]
    fn ascii_period_space_breaks() {
        let t = tokenize(b"Hello. World");
        assert_eq!(texts(&t), vec![b"Hello.".to_vec(), b" World".to_vec()]);
        assert_eq!(t[0].start, 0);
        assert_eq!(t[1].start, 6);
    }

    #[test]
    fn decimal_point_stays_inline() {
        let t = tokenize(b"3.14");
        assert_eq!(texts(&t), vec![b"3.14".to_vec()]);
        let t = tokenize(b"3.14 is pi");
        assert_eq!(texts(&t), vec![b"3.14 is pi".to_vec()]);
    }

    #[test]
    fn period_between_syllables_stays_inline() {
        let 가 = [0xb0, 0xa1];
        let 나 = [0xb1, 0xfd];
        let mut input = Vec::new();
        input.extend_from_slice(&가);
        input.push(b'.');
        input.extend_from_slice(&나);
        let t = tokenize(&input);
        assert_eq!(texts(&t), vec![vec![0xb0, 0xa1, b'.'], vec![0xb1, 0xfd]]);
    }

    #[test]
    fn kps_punctuation_breaks() {
        let 가 = [0xb0, 0xa1];
        let 나 = [0xb1, 0xfd];
        let mut input = Vec::new();
        input.extend_from_slice(&가);
        input.extend_from_slice(&[0xA1, 0xA5]);
        input.extend_from_slice(&나);
        let t = tokenize(&input);
        assert_eq!(
            texts(&t),
            vec![vec![0xb0, 0xa1, 0xA1, 0xA5], vec![0xb1, 0xfd]]
        );
    }

    #[test]
    fn kps_space_continues_sentence() {
        let 가 = [0xb0, 0xa1];
        let 나 = [0xb1, 0xfd];
        let mut input = Vec::new();
        input.extend_from_slice(&가);
        input.extend_from_slice(&[0xA1, 0xA5, 0xA1, 0xA1]);
        input.extend_from_slice(&나);
        let t = tokenize(&input);
        assert_eq!(
            texts(&t),
            vec![vec![0xb0, 0xa1, 0xA1, 0xA5], vec![0xA1, 0xA1, 0xb1, 0xfd]]
        );
        assert_eq!(t[1].start, 4);
    }

    #[test]
    fn dropped_bytes_skipped() {
        let t = tokenize(&[0x41, 0x80, 0x9F, 0xFF, 0x42]);
        assert_eq!(texts(&t), vec![b"AB".to_vec()]);
    }

    #[test]
    fn nul_terminates_input() {
        let t = tokenize(b"AB\0CD");
        assert_eq!(texts(&t), vec![b"AB".to_vec()]);
    }

    #[test]
    fn empty_input_no_sentences() {
        assert!(tokenize(b"").is_empty());
        assert!(tokenize(&[0x80, 0xff]).is_empty());
    }

    #[test]
    fn crlf_mode_breaks_on_newline() {
        let t = tokenize(b"AB\r\nCD");
        assert_eq!(texts(&t), vec![b"AB\r\nCD".to_vec()]);
        let t = tokenize_crlf(b"AB\r\n  CD");
        assert_eq!(texts(&t), vec![b"AB".to_vec(), b"CD".to_vec()]);
        assert_eq!(t[1].start, 6);
    }

    #[test]
    fn sentence_start_offsets() {
        let t = tokenize(b"Hi. Bye!");
        assert_eq!(t[0].start, 0);
        assert_eq!(texts(&t), vec![b"Hi.".to_vec(), b" Bye!".to_vec()]);
        assert_eq!(t[1].start, 3);
    }

    #[test]
    fn max_sentence_bytes_forced_break() {
        let input: Vec<u8> = (0..500u16).map(|i| b'a' + (i % 26) as u8).collect();
        let t = tokenize(&input);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].text.len(), MAX_SENTENCE_BYTES + 1);
        let t = tokenize_with(&input, false, usize::MAX);
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn char_class_checks() {
        assert_eq!(char_class(b'A'), 5);
        assert_eq!(char_class(b'a'), 6);
        assert_eq!(char_class(b'0'), 4);
        assert_eq!(char_class(b' '), 1);
        assert_eq!(char_class(b'.'), 1);
        assert_eq!(char_class(0x09), 0);
        assert_eq!(char_class_16(0xB0A1), 0x19);
        assert_eq!(char_class_16(0xA1A5), 1);
        assert_eq!(char_class_16(0xA3C1), 5);
        assert_eq!(char_class_16(0x1234), 0);
        assert!(is_sentence_punct_ascii(b'!'));
        assert!(is_sentence_punct_ascii(b'.'));
        assert!(is_sentence_punct_ascii(b'?'));
        assert!(!is_sentence_punct_ascii(b','));
        assert!(is_continue_ascii(b' '));
        assert!(is_continue_ascii(0x09));
        assert!(!is_continue_ascii(b'W'));
        assert!(is_continue_kps(0xA1A1));
        assert!(!is_continue_kps(0xA1A5));
        assert!(is_sentence_punct_kps(0xA1A5));
        assert!(is_sentence_punct_kps(0xA1A9));
        assert!(is_sentence_punct_kps(0xA1AA));
        assert!(!is_sentence_punct_kps(0xB0A1));
        assert!(is_syllable_code(0xB0A1));
        assert!(is_syllable_code(0xCCFE));
        assert!(is_syllable_code(0xA1A5));
        assert!(!is_syllable_code(0xA0A1));
        assert!(!is_syllable_code(0xCDA1));
        assert!(!is_syllable_code(0xB1FF));
    }

    #[test]
    fn next_token_class_checks() {
        assert_eq!(next_token_class(b"Ab"), (5, 1));
        assert_eq!(next_token_class(b"ab"), (6, 1));
        assert_eq!(next_token_class(b"1x"), (4, 1));
        assert_eq!(next_token_class(b"\t"), (0, 0));
        assert_eq!(next_token_class(&[0x80]), (0, 0));
        assert_eq!(next_token_class(&[0xB0, 0xA1]), (0x19, 2));
        assert_eq!(next_token_class(&[0xA1, 0xA5]), (1, 2));
        assert_eq!(next_token_class(&[0xFF]), (0, 0));
    }
}
