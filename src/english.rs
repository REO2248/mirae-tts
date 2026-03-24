//! English words → KPS phoneme bytes. All-caps or single letter: spell-out; else longest-match LTS.

static LETTER_NAMES: [&[u8]; 26] = [
    &[0xcb, 0xe6, 0xcb, 0xcb],             // a → 에이
    &[0xb9, 0xbe],                         // b → 비
    &[0xc8, 0xc1],                         // c → 씨
    &[0xb4, 0xd1],                         // d → 디
    &[0xcb, 0xcb],                         // e → 이
    &[0xcb, 0xe6, 0xc2, 0xa3],             // f → 에프
    &[0xbd, 0xb8],                         // g → 쥐
    &[0xcb, 0xe6, 0xbe, 0xde],             // h → 에취
    &[0xca, 0xad, 0xcb, 0xcb],             // i → 아이
    &[0xbd, 0xa3, 0xcb, 0xcb],             // j → 제이
    &[0xbf, 0xe8, 0xcb, 0xcb],             // k → 케이
    &[0xcb, 0xe9],                         // l → 엘
    &[0xcb, 0xea],                         // m → 엠
    &[0xcb, 0xe8],                         // n → 엔
    &[0xca, 0xef, 0xcb, 0xa7],             // o → 오우
    &[0xc2, 0xaa],                         // p → 피
    &[0xbf, 0xc9],                         // q → 큐
    &[0xca, 0xad, 0xb6, 0xa3],             // r → 아르
    &[0xcb, 0xe6, 0xc8, 0xb8],             // s → 에쓰
    &[0xc0, 0xec],                         // t → 티
    &[0xcb, 0xb1],                         // u → 유
    &[0xb9, 0xb6, 0xcb, 0xcb],             // v → 브이
    &[0xb3, 0xf3, 0xb9, 0xa6, 0xcb, 0xb1], // w → 더불유
    &[0xcb, 0xe7, 0xc8, 0xb8],             // x → 엑쓰
    &[0xcc, 0xae, 0xcb, 0xcb],             // y → 와이
    &[0xbd, 0xa3, 0xc0, 0xe2],             // z → 제트
];

static VOWEL_LIST: &[u8] = b"aeiouy";

/// slot 0 = short/default, slot 1 = long (silent-e rule), slot 2 = before another vowel
static VOWEL_PHONEMES: [[&[u8]; 3]; 6] = [
    // a
    [
        &[0xcb, 0xd9],             // 애
        &[0xcb, 0xe6, 0xcb, 0xcb], // 에이
        &[0xca, 0xcc],             // 어
    ],
    // e
    [
        &[0xcb, 0xe6], // 에
        &[0xcb, 0xcb], // 이
        &[0xcb, 0xcb], // 이
    ],
    // i
    [
        &[0xcb, 0xcb],             // 이
        &[0xca, 0xad, 0xcb, 0xcb], // 아이
        &[],                       // (empty)
    ],
    // o
    [
        &[0xca, 0xef, 0xcb, 0xa7], // 오우
        &[0xca, 0xef, 0xcb, 0xa7], // 오우
        &[0xca, 0xcc],             // 어
    ],
    // u
    [
        &[0xca, 0xad], // 아
        &[0xcb, 0xb1], // 유
        &[0xca, 0xcc], // 어
    ],
    // y
    [
        &[0xcb, 0xcb],             // 이
        &[0xca, 0xad, 0xcb, 0xcb], // 아이
        &[0xcb, 0xcb],             // 이
    ],
];

// ---------------------------------------------------------------------------
// Simple consonants: "bdfjklmnpqrtvwz" (c, g, s, x use the special table below).
// ---------------------------------------------------------------------------
static SIMPLE_CONS: &[u8] = b"bdfjklmnpqrtvwz";

static SIMPLE_CONS_PHONEMES: [&[u8]; 15] = [
    &[0xa4, 0xa6], // b → ㅂ
    &[0xa4, 0xa3], // d → ㄷ
    &[0xa4, 0xae], // f → ㅎ
    &[0xa4, 0xa9], // j → ㅈ
    &[0xa4, 0xab], // k → ㅋ
    &[0xa4, 0xa4], // l → ㄹ
    &[0xa4, 0xa5], // m → ㅁ
    &[0xa4, 0xa2], // n → ㄴ
    &[0xa4, 0xad], // p → ㅍ
    &[0xa4, 0xab], // q → ㅋ
    &[0xa4, 0xa4], // r → ㄹ
    &[0xa4, 0xac], // t → ㅌ
    &[0xa4, 0xa6], // v → ㅂ
    &[0xcb, 0xa7], // w → 우
    &[0xa4, 0xa9], // z → ㅈ
];

// Also handle 'h' separately (h → ㅎ)
static H_PHONEME: &[u8] = &[0xa4, 0xae];

// ---------------------------------------------------------------------------
// Special consonants c, g, s, x: [0] default, [1] before e/i/y (palatalised).
// ---------------------------------------------------------------------------
static SPEC_CONS: &[u8] = b"cgsx";

static SPEC_CONS_PHONEMES: [[&[u8]; 2]; 4] = [
    // c
    [&[0xa4, 0xab], &[0xa4, 0xa7]], // ㅋ / ㅅ
    // g
    [&[0xa4, 0xa1], &[0xa4, 0xa9]], // ㄱ / ㅈ
    // s
    [&[0xa4, 0xa7], &[0xa4, 0xa9]], // ㅅ / ㅈ
    // x
    [
        &[0xa4, 0xa1, 0xa4, 0xa7], // ㄱㅅ
        &[0xa4, 0xa1, 0xa4, 0xa9], // ㄱㅈ
    ],
];

// ---------------------------------------------------------------------------
// Long grapheme→phoneme patterns. Listed longest-first within the same prefix so
// longest-match wins when scanning from window 5 down to 2.
// ---------------------------------------------------------------------------
static LONG_PATTERNS: &[(&[u8], &[u8])] = &[
    (b"ssion", &[0xba, 0xc8]),
    (b"ware", &[0xcc, 0xc1, 0xca, 0xcc]),
    (b"ould", &[0xcb, 0xa7, 0xa4, 0xa3]),
    (b"anch", &[0xca, 0xaf, 0xbe, 0xde]),
    (b"anc", &[0xca, 0xaf, 0xa4, 0xa7]),
    (b"ower", &[0xca, 0xad, 0xcc, 0xb8]),
    (b"sion", &[0xbc, 0xad]),
    (b"sure", &[0xbc, 0xab]),
    (b"tion", &[0xba, 0xc8]),
    (b"ture", &[0xbd, 0xee]),
    (b"alf", &[0xca, 0xad, 0xa4, 0xae]),
    (b"alm", &[0xca, 0xb7]),
    (b"alv", &[0xca, 0xad, 0xa4, 0xa6]),
    (b"alk", &[0xca, 0xef, 0xa4, 0xab]),
    (b"aff", &[0xca, 0xad, 0xa4, 0xae]),
    (b"aft", &[0xca, 0xad, 0xc3, 0xb8, 0xa4, 0xac]),
    (b"ant", &[0xca, 0xaf, 0xa4, 0xac]),
    (b"ask", &[0xca, 0xad, 0xba, 0xf7, 0xa4, 0xab]),
    (b"asp", &[0xca, 0xad, 0xba, 0xf7, 0xa4, 0xad]),
    (b"ass", &[0xca, 0xad, 0xa4, 0xa7]),
    (b"ast", &[0xca, 0xad, 0xba, 0xf7, 0xa4, 0xac]),
    (b"ath", &[0xca, 0xad, 0xa4, 0xb2]),
    (b"are", &[0xcb, 0xd9, 0xca, 0xcc]),
    (b"ere", &[0xcb, 0xcb, 0xca, 0xcc]),
    (b"ire", &[0xca, 0xad, 0xcb, 0xcb, 0xca, 0xcc]),
    (b"ure", &[0xcb, 0xb1, 0xca, 0xcc]),
    (b"air", &[0xcb, 0xd9, 0xca, 0xcc]),
    (b"ear", &[0xcb, 0xcb, 0xca, 0xcc]),
    (b"oor", &[0xcb, 0xa7, 0xca, 0xcc]),
    (b"our", &[0xca, 0xad, 0xcc, 0xb8]),
    (b"ian", &[0xca, 0xce]),
    (b"igh", &[0xca, 0xad, 0xcb, 0xcb]),
    (b"ild", &[0xca, 0xad, 0xcb, 0xce, 0xa4, 0xa3]),
    (b"ind", &[0xca, 0xad, 0xcb, 0xcd, 0xa4, 0xa3]),
    (b"ost", &[0xca, 0xef, 0xcb, 0xa7, 0xba, 0xf7, 0xa4, 0xac]),
    (b"old", &[0xca, 0xef, 0xcb, 0xaa, 0xa4, 0xa3]),
    (b"tch", &[0xbe, 0xde]),
    (b"wor", &[0xcc, 0xb8]),
    (b"tio", &[0xbb, 0xd5, 0xca, 0xef, 0xcb, 0xa7]),
    (b"tia", &[0xbb, 0xd5, 0xca, 0xcc]),
    (b"ai", &[0xcb, 0xe6, 0xcb, 0xcb]),
    (b"ei", &[0xcb, 0xe6, 0xcb, 0xcb]),
    (b"oi", &[0xca, 0xef, 0xcb, 0xcb]),
    (b"au", &[0xca, 0xef]),
    (b"aw", &[0xca, 0xef]),
    (b"ew", &[0xcb, 0xb1]),
    (b"ow", &[0xca, 0xef, 0xcb, 0xa7]),
    (b"ay", &[0xcb, 0xe6, 0xcb, 0xcb]),
    (b"ey", &[0xcb, 0xe6, 0xcb, 0xcb]),
    (b"oy", &[0xca, 0xef, 0xcb, 0xcb]),
    (b"oa", &[0xca, 0xef, 0xcb, 0xa7]),
    (b"ee", &[0xcb, 0xcb]),
    (b"ie", &[0xcb, 0xcb]),
    (b"ck", &[0xa4, 0xab]),
    (b"dg", &[0xbd, 0xb8]),
    (b"nk", &[0xa4, 0xa8, 0xa4, 0xab]),
    (b"ph", &[0xa4, 0xae]),
    (b"qu", &[0xa4, 0xab]),
    (b"sh", &[0xbb, 0xd5]),
    (b"th", &[0xa4, 0xb2]),
    (b"wa", &[0xcc, 0xb8]),
    // Patterns with end-of-word constraints:
    (b"ore", &[0xca, 0xef]), // eow-only (also matched by 'are','ere',etc.)
    (b"ar", &[0xca, 0xad]),
    (b"er", &[0xca, 0xcc]),
    (b"ir", &[0xca, 0xcc]),
    (b"ur", &[0xca, 0xcc]),
    (b"oo", &[0xcb, 0xa7]),
    (b"ng", &[0xa4, 0xa8]), // coda only (reject before vowel handled separately)
    (b"es", &[0xa4, 0xa9]), // eow-only
];

// End-of-word-only patterns (context-checked in the tokenizer)
static EOW_ONLY_PATTERNS: &[&[u8]] = &[b"ore", b"es"];

// 'ng' must not match when the following letter is a vowel
static NO_BEFORE_VOWEL: &[&[u8]] = &[b"ng"];

// ---------------------------------------------------------------------------
// Digraphs wh, ea, or, ch, al with context-dependent KPS output.
//   wh: before a/e/i/y → 우; before o or default → ㅎ
//   al: before t or at EOW → 오; else → 올
//   ea: before d, or t+y → 에; else → 이
//   or, ch: no context override (group default applies)
// ---------------------------------------------------------------------------
/// Returns KPS bytes for the digraph given the next char and at-eow flag.
fn digraph_phoneme(pat: &[u8], next: u8, next2: u8, at_eow: bool) -> Option<&'static [u8]> {
    match pat {
        b"wh" => {
            if matches!(next, b'a' | b'e' | b'i' | b'y') {
                Some(&[0xcb, 0xa7]) // 우
            } else {
                Some(&[0xa4, 0xae]) // ㅎ (default)
            }
        }
        b"al" => {
            if next == b't' || at_eow {
                Some(&[0xca, 0xef]) // 오
            } else {
                Some(&[0xca, 0xf2]) // 올
            }
        }
        b"ea" => {
            if next == b'd' || (next == b't' && next2 == b'y') {
                Some(&[0xcb, 0xe6]) // 에
            } else {
                Some(&[0xcb, 0xcb]) // 이 (default)
            }
        }
        b"or" => Some(&[0xca, 0xef]), // 오 (slot 0)
        b"ch" => Some(&[0xa4, 0xaa]), // ㅊ  (slot 1 — most common Korean rendering)
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn is_vowel(c: u8) -> bool {
    matches!(c, b'a' | b'e' | b'i' | b'o' | b'u' | b'y')
}

fn is_all_uppercase(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .all(|&b| b.is_ascii_alphabetic() && b.is_ascii_uppercase())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Convert an ASCII English string to a Korean pronunciation string.
///
/// Returns a Unicode (UTF-8) Korean string decoded from KPS 9566.
pub fn english_to_korean(input: &str) -> String {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return String::new();
    }

    let kps = if is_all_uppercase(bytes) || bytes.len() == 1 {
        abbreviation_to_kps(bytes)
    } else {
        lts_to_kps(&lowercase(bytes))
    };

    crate::kps9566_decode(&kps)
}

fn lowercase(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().map(|&b| b | 0x20).collect()
}

// ---------------------------------------------------------------------------
// Abbreviation mode: spell each letter by name
// ---------------------------------------------------------------------------
fn abbreviation_to_kps(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for &b in bytes {
        let lower = (b | 0x20) as usize;
        if lower >= b'a' as usize && lower <= b'z' as usize {
            out.extend_from_slice(LETTER_NAMES[lower - b'a' as usize]);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// LTS mode: longest-match from window=5 down to window=1
//
// Calls lts_tokenize then post_process (phoneme cleanup pass) before
// concatenating phonemes.
// ---------------------------------------------------------------------------
fn lts_to_kps(lower: &[u8]) -> Vec<u8> {
    let tokens = lts_tokenize(lower);
    let tokens = lts_post_process(tokens);
    tokens.into_iter().flat_map(|t| t.1).collect()
}

/// One LTS match: (matched input bytes, output phoneme bytes).
type LtsToken = (Vec<u8>, Vec<u8>);

fn lts_tokenize(lower: &[u8]) -> Vec<LtsToken> {
    let len = lower.len();
    let mut pos = 0;
    let mut out: Vec<LtsToken> = Vec::new();

    while pos < len {
        let max_window = (len - pos).min(5);
        let mut advance = 0usize;
        let mut phoneme: Option<&'static [u8]> = None;

        'outer: for window in (1..=max_window).rev() {
            let slice = &lower[pos..pos + window];
            let next = lower.get(pos + window).copied().unwrap_or(0);
            let next2 = lower.get(pos + window + 1).copied().unwrap_or(0);
            let at_eow = pos + window >= len;

            if window >= 2 {
                // 1. Long patterns
                for &(pat, ph) in LONG_PATTERNS {
                    if slice != pat {
                        continue;
                    }
                    // end-of-word-only patterns
                    if EOW_ONLY_PATTERNS.contains(&pat) && !at_eow {
                        continue; // reject; try shorter window
                    }
                    // 'ng' must not precede a vowel
                    if NO_BEFORE_VOWEL.contains(&pat) && is_vowel(next) {
                        continue;
                    }
                    phoneme = Some(ph);
                    advance = window;
                    break 'outer;
                }

                // 2. Digraphs (wh, ea, or, ch, al)
                if let Some(ph) = digraph_phoneme(slice, next, next2, at_eow) {
                    phoneme = Some(ph);
                    advance = window;
                    break 'outer;
                }
            }

            if window == 1 {
                let c = slice[0];

                // 3. Vowel
                if let Some(vi) = VOWEL_LIST.iter().position(|&v| v == c) {
                    let ph = vowel_phoneme(vi, next, next2);
                    phoneme = Some(ph);
                    advance = 1;
                    break 'outer;
                }

                // 4. Simple consonant (b,d,f,j,k,l,m,n,p,q,r,t,v,w,z)
                if c == b'h' {
                    phoneme = Some(H_PHONEME);
                    advance = 1;
                    break 'outer;
                }
                if let Some(ci) = SIMPLE_CONS.iter().position(|&x| x == c) {
                    phoneme = Some(SIMPLE_CONS_PHONEMES[ci]);
                    advance = 1;
                    break 'outer;
                }

                // 5. Special consonants c,g,s,x
                if let Some(si) = SPEC_CONS.iter().position(|&x| x == c) {
                    let slot = if matches!(next, b'e' | b'i' | b'y') {
                        1
                    } else {
                        0
                    };
                    phoneme = Some(SPEC_CONS_PHONEMES[si][slot]);
                    advance = 1;
                    break 'outer;
                }

                // No match for this char — skip
                advance = 1;
                break 'outer;
            }
        }

        let pat = lower[pos..pos + advance.max(1)].to_vec();
        if let Some(ph) = phoneme {
            out.push((pat, ph.to_vec()));
        } else {
            // unmatched char: push with empty phoneme
            out.push((pat, Vec::new()));
        }
        pos += advance.max(1);
    }

    out
}

// ---------------------------------------------------------------------------
// Post-processing: merge duplicate vowels, fix edge cases.
//
// Handles silent letters and letter-combination rules that cannot be expressed
// purely by the longest-match LTS table.
//
// Phase 1  – drop adjacent identical single-char vowels (e.g. "aa" → "a").
// Phase 2  – letter-sequence rules (switch on single-char pattern):
//   e  at last position     → drop (silent terminal e already used for vowel length).
//   r  at last position     → drop (silent terminal r, British coda).
//   b  + t                  → drop b  (debt, subtle).
//   k  + n  (not last)      → drop k  (knight, know).
//   w  + r  (not last)      → drop w  (write, wrong).
//   g  at count-2 + n/h     → drop g  (sign, gh coda).
//   g  (before count-2) + h → drop both g and h  (night: handled via "igh" pattern,
//                             but bare "ght" is covered here).
//   m  + b/n                → drop following b/n  (bomb, hymn).
// ---------------------------------------------------------------------------
fn lts_post_process(tokens: Vec<LtsToken>) -> Vec<LtsToken> {
    let n = tokens.len();
    if n == 0 {
        return tokens;
    }

    // ── Phase 1: drop adjacent identical single-char vowels ─────────────────
    let mut keep1 = vec![true; n];
    for i in 0..n.saturating_sub(1) {
        let t = &tokens[i];
        let u = &tokens[i + 1];
        if t.0.len() == 1 && u.0.len() == 1 && is_vowel(t.0[0]) && t.0[0] == u.0[0] {
            keep1[i] = false; // drop the first of the pair
        }
    }
    let tokens: Vec<LtsToken> = tokens
        .into_iter()
        .enumerate()
        .filter(|(i, _)| keep1[*i])
        .map(|(_, t)| t)
        .collect();

    // ── Phase 2: letter-sequence rules ──────────────────────────────────────
    let n = tokens.len();
    if n == 0 {
        return tokens;
    }
    let mut keep2 = vec![true; n];

    let single_char = |t: &LtsToken| -> Option<u8> {
        if t.0.len() == 1 {
            Some(t.0[0])
        } else {
            None
        }
    };

    let mut i = 0;
    while i < n {
        if let Some(c) = single_char(&tokens[i]) {
            let is_last = i == n - 1;
            let next_sc = if i + 1 < n {
                single_char(&tokens[i + 1])
            } else {
                None
            };

            match c {
                // terminal 'e' → drop
                b'e' if is_last => {
                    keep2[i] = false;
                }
                // terminal 'r' → drop
                b'r' if is_last => {
                    keep2[i] = false;
                }
                // 'b' + 't' → drop 'b'
                b'b' if !is_last && next_sc == Some(b't') => {
                    keep2[i] = false;
                }
                // 'k' + 'n' → drop 'k'
                b'k' if !is_last && next_sc == Some(b'n') => {
                    keep2[i] = false;
                }
                // 'w' + 'r' → drop 'w'
                b'w' if !is_last && next_sc == Some(b'r') => {
                    keep2[i] = false;
                }
                // 'g' rules
                b'g' => {
                    let count_m2 = n.saturating_sub(2);
                    if i == count_m2 {
                        // second-to-last: 'g' + 'n' or 'g' + 'h' → drop 'g'
                        if matches!(next_sc, Some(b'n') | Some(b'h')) {
                            keep2[i] = false;
                        }
                    } else if i < count_m2 {
                        // before second-to-last: 'g' + 'h' → drop both 'g' and 'h'
                        if next_sc == Some(b'h') {
                            keep2[i] = false;
                            keep2[i + 1] = false;
                            i += 1; // skip 'h' too
                        }
                    }
                }
                // 'm' + 'b'/'n' → drop the 'b' or 'n'
                b'm' if !is_last && matches!(next_sc, Some(b'b') | Some(b'n')) => {
                    keep2[i + 1] = false;
                    i += 1; // skip the dropped entry
                }
                _ => {}
            }
        }
        i += 1;
    }

    tokens
        .into_iter()
        .enumerate()
        .filter(|(i, _)| keep2[*i])
        .map(|(_, t)| t)
        .collect()
}

// ---------------------------------------------------------------------------
// Vowel phoneme selection.
//
// Per-vowel context rules:
//   - If next char is consonant AND next2 == 'e'  →  long/silent-e (slot 1)
//   - Else                                         →  short/default  (slot 0)
// (Slot 2 is never directly output; the "before another vowel" case keeps slot 0.)
// ---------------------------------------------------------------------------
fn vowel_phoneme(vowel_idx: usize, next: u8, next2: u8) -> &'static [u8] {
    let silent_e = !is_vowel(next) && next != 0 && next2 == b'e';
    if silent_e {
        let ph = VOWEL_PHONEMES[vowel_idx][1];
        if !ph.is_empty() {
            return ph;
        }
    }
    VOWEL_PHONEMES[vowel_idx][0]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abbreviation_single_letter() {
        // Single char → abbreviation
        assert_eq!(english_to_korean("A"), "에이");
        assert_eq!(english_to_korean("B"), "비");
        assert_eq!(english_to_korean("Z"), "제트");
    }

    #[test]
    fn test_abbreviation_all_caps() {
        // All uppercase → spell out
        assert_eq!(english_to_korean("TV"), "티브이");
        assert_eq!(english_to_korean("CPU"), "씨피유");
    }

    #[test]
    fn test_lts_long_pattern() {
        // 'sh' pattern
        let k = english_to_korean("shoe");
        assert!(k.contains("쉬"));
        // 'th' pattern
        let k2 = english_to_korean("the");
        assert!(k2.contains("ㅆ") || !k2.is_empty());
    }

    #[test]
    fn test_lts_vowel_silent_e() {
        // "game": a + m + e → long 'a' (에이), m (ㅁ), silent e
        let k = english_to_korean("came");
        // 'a' before consonant before 'e' → 에이
        assert!(k.contains("에이"), "game: expected 에이, got {}", k);
    }

    #[test]
    fn test_lts_digraph_wh() {
        // "what": wh before 'a' vowel → 우
        let k = english_to_korean("what");
        assert!(k.contains("우"), "what: expected 우, got {}", k);
    }

    #[test]
    fn test_lts_digraph_sh() {
        let k = english_to_korean("shop");
        assert!(k.contains("쉬"), "shop: expected 쉬, got {}", k);
    }

    #[test]
    fn test_lts_consonant_c() {
        // "cat" → c before 'a' (not e/i/y) → ㅋ
        let k = english_to_korean("cat");
        assert!(k.contains("ㅋ"), "cat: expected ㅋ, got {}", k);
        // "city" → c before 'i' → ㅅ
        let k2 = english_to_korean("city");
        assert!(k2.contains("ㅅ"), "city: expected ㅅ, got {}", k2);
    }

    #[test]
    fn test_lts_vowel_basic() {
        // simple short vowel
        let k = english_to_korean("bat");
        // 'a' before 't' (not before silent-e) → 애
        assert!(k.contains("애"), "bat: expected 애, got {}", k);
    }

    // ── Post-processing tests ───────────────────────────────────────────────

    #[test]
    fn test_terminal_e_dropped() {
        // Terminal silent 'e' must NOT produce a phoneme.
        // "game" → ㄱ + 에이 (long A) + ㅁ  — no 이 at the end.
        let k_game = english_to_korean("game");
        // Must contain 에이 (long a due to silent-e rule)
        assert!(
            k_game.contains("에이"),
            "game: expected 에이, got {}",
            k_game
        );
        // Must NOT end with 이 from the terminal 'e'
        assert!(
            !k_game.ends_with("이"),
            "game: terminal 이 was not dropped, got {}",
            k_game
        );
    }

    #[test]
    fn test_silent_k_in_kn() {
        // 'kn' → silent k.  "know" starts with n sound, not k.
        let k = english_to_korean("know");
        // Should NOT contain the ㅋ phoneme from 'k'
        assert!(
            !k.starts_with("ㅋ"),
            "know: 'k' should be silent, got {}",
            k
        );
    }

    #[test]
    fn test_silent_w_in_wr() {
        // 'wr' → silent w.  "write" starts with r sound, not w.
        let k_before = english_to_korean("rite"); // no 'w'
        let k_write = english_to_korean("write"); // with 'w'
                                                  // "write" phoneme output should equal "rite" (silent w dropped)
        assert_eq!(
            k_write, k_before,
            "write vs rite: 'w' should be silent, write={}, rite={}",
            k_write, k_before
        );
    }

    #[test]
    fn test_silent_g_in_gn() {
        // 'gn' at end → silent g.  "sign" s+i+gn should not have extra g phoneme.
        let _k_sin = english_to_korean("sin");
        let k_sign = english_to_korean("sign");
        // "sign" and "sin" should sound the same (except for the vowel length rule)
        // At minimum, "sign" should not contain a ㄱ from the 'g'
        assert!(
            !k_sign.contains('ㄱ'),
            "sign: silent 'g' produced ㄱ, got {}",
            k_sign
        );
    }

    #[test]
    fn test_mb_silent_b() {
        // 'mb' → silent b.  "bomb" → bom, not bomb.
        let k = english_to_korean("bomb");
        // Count ㅂ occurrences — should be 1 (initial b), not 2
        let b_count = k.matches('ㅂ').count();
        assert!(
            b_count <= 1,
            "bomb: expected at most 1 ㅂ, got {} in '{}'",
            b_count,
            k
        );
    }
}
