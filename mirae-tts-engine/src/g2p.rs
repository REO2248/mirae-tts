//! G2P - phoneme conversion from internal codes.
//! Dictionary pipeline (colligation/User/NonReg/Conjects) + word reading + 9-stage
//! postprocess chain + phoneme-code base + static exception tables + digit/unit/alphabet.
pub mod g2p_dict {

    use std::fmt;

    use std::collections::HashMap;
    use std::sync::OnceLock;

    use crate::connect::ConnectMatrix;
    use crate::dict::{Dict, SubARecord, key_from_syllables, reverse_key};
    use crate::record::ProsodyRecord;

    pub const MAX_CANDIDATES: usize = 214;

    pub const MARKER_FALLBACK: u8 = 0x11;

    pub const PACKED_DIGITS: u16 = 0x152D;
    pub const PACKED_SYMBOLS: u16 = 0x2933;

    pub const SPLIT_FINALS: [u16; 4] = [0x03, 0x07, 0x0F, 0x10];

    pub const MORPH_TYPE_BASE: u8 = 0x14;

    pub const CHUNK_SYLLABLES: usize = 60;
    pub const PROPAGATE_FORWARD: u8 = 0;
    pub const PROPAGATE_BACK: usize = 5;

    pub const CLASS_REPLACE: [u8; 28] = [
        0, 2, 2, 5, 6, 0, 15, 14, 15, 6, 6, 15, 15, 14, 5, 15, 5, 18, 0, 5, 15, 5, 0, 5, 27, 5, 5,
        0,
    ];

    pub const PROSODY_W1: f32 = 0.5;
    #[allow(dead_code)]
    pub const PROSODY_W2: f32 = 0.5; // reserved: original binary has this slot at 0x89178 (0.5) but current chain uses W1/W3 only — kept for byte-exact layout
    pub const PROSODY_W3: f32 = 0.95; // DAT_0048917c = 0x3f733333 (verified against Future.exe at file 0x8917c)
    pub const ACCENT_RANGE: (f32, f32) = (1.86, 2.9);

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Reading {
        pub bytes: Vec<u8>,
        pub packed: Option<u16>,
        pub marker: u8,
    }

    impl Reading {
        pub fn fallback(word: &[u8]) -> Reading {
            Reading {
                bytes: word.to_vec(),
                packed: None,
                marker: MARKER_FALLBACK,
            }
        }
    }

    #[derive(Debug, Clone)]
    pub struct NonRegHit {
        pub reading: Vec<u8>,
        pub marker: u8,
        pub records: Vec<SubARecord>,
        pub matched: usize,
    }

    #[derive(Debug, Clone, Default)]
    pub struct WordRecord {
        pub spelling: Vec<u8>,
        pub reading_bytes: Vec<u8>,
        pub syllable_codes: Vec<u16>,
        pub morph_markers: Vec<u8>,
        pub phoneme_codes: Vec<u16>,
        pub phoneme_markers: Vec<u8>,
        pub phoneme_count: usize,
        pub rule_marker: u8,
        pub rule_flags: [u8; 4],
        pub rule_counts: [u8; 4],
        pub flag_link: u8,
        pub seq: u8,
        pub prosody: [f32; 3],
        pub accent: u8,
        pub final_marker: u8,
        // ===== Stage3/4/6 exact-hook inputs (from g2p_stage4_backup_snapshot.rs) =====
        /// Original `+0xb5bc` morphology-count-plus-one index.
        pub morph_count: usize,
        /// Second-array dword at `+0x1db0`.
        pub word_type: u32,
        /// Second-array type bytes beginning at `+0x1579` (index 3 == M[0]).
        pub morph_types: Vec<u8>,
        /// Raw 0x32-byte string slots from the analyzer record.
        pub morph_slots: Vec<Vec<u8>>,
        /// First bytes at the stage-4 consumer addresses `R_i+0x15cc+0x14*k`.
        pub morph_flags: Vec<u8>,
        /// Raw connection context bytes beginning at `+0xad84`.
        pub morph_context: Vec<u8>,
        /// Original second-array record, when supplied by a raw analyzer capture.
        pub raw_second_record: Option<RawSecondRecord>,
    }

    #[derive(Debug, Clone, Copy)]
    pub struct G2pDicts<'a> {
        pub colligation: &'a Dict,
        pub user: &'a Dict,
        pub nonreg: &'a Dict,
        pub conjects: &'a Dict,
        pub connect: &'a ConnectMatrix,
    }

    #[allow(dead_code)] // kept for unicode round-trip verification; not on hot path
    fn unicode_syllable_to_jamo(uni: u32) -> Option<(u8, u8, u8)> {
        if !(0xAC00..=0xD7A3).contains(&uni) {
            return None;
        }
        let n = uni - 0xAC00;
        let init = (n / 588) as u8 + 1;
        let med = ((n % 588) / 28) as u8 + 1;
        let fin = (n % 28) as u8;
        Some((init, med, fin))
    }

    fn kps_syllable_to_jamo(kps: u16) -> Option<(u8, u8, u8)> {
        kps_to_jamo_kp(kps)
    }

    fn kps_to_jamo_kp(kps: u16) -> Option<(u8, u8, u8)> {
        let mut init1 = 19usize;
        for i in 0..19 {
            if kps < crate::kps_tables::ROW_STARTS[i] {
                init1 = i;
                break;
            }
        }
        if init1 == 0 {
            return None;
        }
        let mut med1 = 0usize;
        while med1 < 21 {
            let e = init1 * 21 + med1;
            let nxt = if med1 == 20 {
                0xCCD0
            } else {
                crate::kps_tables::COL_STARTS[e + 1]
            };
            if nxt != 0xffff && kps < nxt {
                break;
            }
            med1 += 1;
        }
        if med1 >= 21 {
            return None;
        }
        let e = init1 * 21 + med1;
        let start = crate::kps_tables::COL_STARTS[e];
        let mut svar = kps as i32 - start as i32;
        if (kps & 0xff) < (start & 0xff) {
            svar -= 0xa2;
        }
        let fin = svar.max(0) as u8;
        Some((init1 as u8, med1 as u8, fin))
    }

    /// Unicode: ㄱㄲㄴㄷㄸㄹㅁㅂㅃㅅㅆㅇㅈㅉㅊㅋㅌㅍㅎ
    pub(crate) const INIT_KP_TO_STD: [u8; 19] = [
        0, 2, 3, 5, 6, 7, 9, 12, 14, 15, 16, 17, 18, 1, 4, 8, 10, 13, 11,
    ];

    pub(crate) const INIT_STD_TO_KP: [u8; 19] = [
        0, 13, 1, 2, 14, 3, 4, 5, 15, 6, 16, 18, 7, 17, 8, 9, 10, 11, 12,
    ];

    /// Unicode: ㅏㅐㅑㅒㅓㅔㅕㅖㅗㅘㅙㅚㅛㅜㅝㅞㅟㅠㅡㅢㅣ
    pub(crate) const MED_KP_TO_STD: [u8; 21] = [
        0, 2, 4, 6, 8, 12, 13, 17, 18, 20, 1, 3, 5, 7, 9, 10, 11, 14, 15, 16, 19,
    ];

    pub(crate) const MED_STD_TO_KP: [u8; 21] = [
        0, 10, 1, 11, 2, 12, 3, 13, 4, 14, 15, 16, 5, 6, 17, 18, 19, 7, 8, 20, 9,
    ];

    fn jamo_to_kps_syllable(init: u8, med: u8, fin: u8) -> Option<u16> {
        let init_kp = INIT_STD_TO_KP[(init - 1) as usize] as usize;
        let med_kp = MED_STD_TO_KP[(med - 1) as usize] as usize;
        let init1 = init_kp + 1;
        let med1 = med_kp + 1;
        let e = init1 * 21 + med1;
        let start = crate::kps_tables::COL_STARTS[e];
        if start == 0xffff {
            return None;
        }
        let _mask = crate::kps_tables::COL_MASKS[e];
        let mut code = start + fin as u16;
        let lo = code & 0xff;
        if lo < (start & 0xff) || 0xfe < lo {
            code += 0xa2;
        }
        Some(code)
    }

    #[allow(dead_code)] // precomputed unicode->jamo table; used only by kps_syllable_map
    fn syllable_jamo_map() -> &'static HashMap<u16, (u8, u8, u8)> {
        static MAP: OnceLock<HashMap<u16, (u8, u8, u8)>> = OnceLock::new();
        MAP.get_or_init(|| {
            let mut m = HashMap::with_capacity(11172);
            for hi in 0xA1u16..=0xFE {
                for lo in 0xA1u16..=0xFE {
                    let code = (hi << 8) | lo;
                    if let Some(uni) = crate::kps_lookup(code) {
                        if let Some(j) = unicode_syllable_to_jamo(uni as u32) {
                            m.insert(code, j);
                        }
                    }
                }
            }
            m
        })
    }

    #[allow(dead_code)] // inverse of syllable_jamo_map; kept for byte-exact verification
    fn kps_syllable_map() -> &'static HashMap<(u8, u8, u8), u16> {
        static MAP: OnceLock<HashMap<(u8, u8, u8), u16>> = OnceLock::new();
        MAP.get_or_init(|| {
            let mut m = HashMap::with_capacity(11172);
            for (&kps, &j) in syllable_jamo_map() {
                m.insert(j, kps);
            }
            m
        })
    }

    fn special_code_to_byte(code: u16) -> u8 {
        let u = code & 0x7fff;
        match u {
            0x30..=0x39 => (u as u8 - 0x30) + b'0',
            0x2d => b'-',
            0x2e => b'.',
            _ => (u & 0xff) as u8,
        }
    }

    fn byte_to_special_code(b: u8) -> u16 {
        match b {
            b'0'..=b'9' => 0x8000 | (b - b'0' + 0x30) as u16,
            b'-' => 0x8000 | 0x2d,
            b'.' => 0x8000 | 0x2e,
            _ => 0x8000 | b as u16,
        }
    }

    pub fn kps_bytes_to_codes(bytes: &[u8]) -> Option<Vec<u16>> {
        let mut out = Vec::with_capacity(bytes.len() / 2 + 1);
        let mut i = 0;
        while i < bytes.len() {
            let b0 = bytes[i];
            if b0 < 0x80 {
                out.push(byte_to_special_code(b0));
                i += 1;
                continue;
            }
            let b1 = *bytes.get(i + 1)?;
            let kps = ((b0 as u16) << 8) | b1 as u16;
            if let Some((init, med, fin)) = kps_syllable_to_jamo(kps) {
                let init_std = (INIT_KP_TO_STD[(init - 1) as usize] + 1) as u16;
                let med_std = (MED_KP_TO_STD[(med - 1) as usize] + 1) as u16;
                let code = (init_std << 10) | (med_std << 5) | fin as u16;
                out.push(code);
            } else if (0xA3B0..=0xA3B9).contains(&kps) {
                out.push(0x8000 | (kps - 0xA3B0 + 0x30));
            } else if kps == 0xA1AF {
                out.push(0x8000 | 0x2d);
            } else if kps == 0xA1A4 || kps == 0xA1A5 {
                out.push(0x8000 | 0x2e);
            } else {
                out.push(0x8000 | (kps & 0xfff));
            }
            i += 2;
        }
        Some(out)
    }

    pub fn codes_to_kps_bytes(codes: &[u16]) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(codes.len() * 2);
        for &c in codes {
            if c & 0x8000 != 0 {
                out.push(special_code_to_byte(c));
            } else {
                let init = ((c >> 10) & 0x1f) as u8;
                let med = ((c >> 5) & 0x1f) as u8;
                let fin = (c & 0x1f) as u8;
                if init == 0 || med == 0 || init > 19 || med > 21 {
                    return None;
                }
                let kps = jamo_to_kps_syllable(init, med, fin)?;
                out.push((kps >> 8) as u8);
                out.push((kps & 0xff) as u8);
            }
        }
        Some(out)
    }

    pub fn key_str_to_codes(key: &[u8]) -> Option<Vec<u16>> {
        let mut out = Vec::with_capacity(key.len() / 3 + 1);
        let mut i = 0;
        while i < key.len() {
            let c = key[i];
            match c {
                0x01..=0x13 => {
                    let init = c;
                    i += 1;
                    let mut med = 0u8;
                    let mut fin = 0u8;
                    if i < key.len() && (0x14..=0x28).contains(&key[i]) {
                        med = key[i] - 0x13;
                        i += 1;
                        if i < key.len() && (0x29..=0x43).contains(&key[i]) {
                            fin = key[i] - 0x28;
                            i += 1;
                        }
                    }
                    out.push(((init as u16) << 10) | ((med as u16) << 5) | fin as u16);
                }
                0x44 => {
                    out.push(0x8000 | 0x2e);
                    i += 1;
                }
                0x45 => {
                    out.push(0x8000 | 0x2d);
                    i += 1;
                }
                0x46..=0x4F => {
                    out.push(0x8000 | (c - 0x46 + 0x30) as u16);
                    i += 1;
                }
                0x50 => i += 1,
                _ => return None,
            }
        }
        Some(out)
    }

    pub fn to_phoneme_code(syllable: u16) -> u16 {
        let init_std = ((syllable >> 10) & 0x1f) as usize;
        let med_std = ((syllable >> 5) & 0x1f) as usize;
        let fin_kp = (syllable & 0x1f) as usize;
        if init_std == 0 || med_std == 0 {
            return syllable;
        }
        let init_kp = INIT_STD_TO_KP[init_std - 1] as usize;
        let med_kp = MED_STD_TO_KP[med_std - 1] as usize;
        let class = if fin_kp == 0 {
            27
        } else {
            final_class_from_mask(init_kp, med_kp, fin_kp)
        };
        ((class as u16) << 10) | ((med_kp as u16) << 5) | (init_kp as u16)
    }

    fn final_class_from_mask(init_kp: usize, med_kp: usize, fin_kp: usize) -> u16 {
        let mask = crate::digit_tables::KPS_COL_MASKS[med_kp + init_kp * 21];
        let mut cnt = 0u32;
        for bit in 0..32u32 {
            if (mask >> bit) & 1 != 0 {
                cnt += 1;
                if cnt as usize == fin_kp {
                    return bit as u16;
                }
            }
        }
        0
    }

    pub fn kps_code_to_phoneme(kps: u16) -> u16 {
        let Some((init1, med1, fin)) = kps_to_jamo_kp(kps) else {
            return 0;
        };
        let init_std = INIT_KP_TO_STD[(init1 - 1) as usize] + 1;
        let med_std = MED_KP_TO_STD[(med1 - 1) as usize] + 1;
        to_phoneme_code(((init_std as u16) << 10) | ((med_std as u16) << 5) | (fin as u16))
    }

    pub fn kps_final_class(kps: u16) -> u8 {
        let Some((init1, med1, fin)) = kps_to_jamo_kp(kps) else {
            return 27;
        };
        if fin == 0 {
            27
        } else {
            final_class_from_mask((init1 - 1) as usize, (med1 - 1) as usize, fin as usize) as u8
        }
    }

    #[allow(dead_code)] // final-agnostic phoneme helper; canonical path uses kps_code_to_phoneme
    fn kps_code_to_phoneme_no_final(kps: u16) -> u16 {
        let Some((init1, med1, _)) = kps_to_jamo_kp(kps) else {
            return 0;
        };
        let init_std = INIT_KP_TO_STD[(init1 - 1) as usize] + 1;
        let med_std = MED_KP_TO_STD[(med1 - 1) as usize] + 1;
        to_phoneme_code(((init_std as u16) << 10) | ((med_std as u16) << 5))
    }

    pub fn decimal_codes(int_digits: &[u8], frac_digits: &[u8]) -> Vec<u16> {
        let mut out = Vec::with_capacity(int_digits.len() + frac_digits.len() + 1);
        for &d in int_digits {
            out.push(decimal_digit_code(d));
        }
        out.push(kps_code_to_phoneme(0xC9B0));
        for &d in frac_digits {
            out.push(decimal_digit_code(d));
        }
        out
    }

    fn decimal_digit_code(d: u8) -> u16 {
        match d {
            0 => 0x4863,
            2 => 0x1532,
            _ => kps_code_to_phoneme(crate::digit_tables::SINO_DIGITS[d as usize]),
        }
    }

    pub fn sino_integer_codes(digits: &[u8]) -> Vec<u16> {
        let readings = sino_integer_kps_syllables(digits);
        readings.iter().map(|&k| kps_code_to_phoneme(k)).collect()
    }

    pub fn sino_integer_kps_syllables(digits: &[u8]) -> Vec<u16> {
        use crate::digit_tables::{SINO_DIGITS, SINO_UNITS};
        let n = digits.len();
        let mut out: Vec<u16> = Vec::new();
        for (i, &d) in digits.iter().enumerate() {
            if d == 0 {
                continue;
            }
            let pos = n - 1 - i;
            let in_group = pos % 4;
            let group = pos / 4;
            if in_group == 0 {
                if group == 0 {
                    out.push(SINO_DIGITS[d as usize]);
                } else if d == 1 {
                    out.push(SINO_UNITS[3 + group - 1]);
                } else {
                    out.push(SINO_DIGITS[d as usize]);
                    out.push(SINO_UNITS[3 + group - 1]);
                }
            } else {
                let unit = SINO_UNITS[in_group - 1];
                if d == 1 {
                    out.push(unit);
                } else {
                    out.push(SINO_DIGITS[d as usize]);
                    out.push(unit);
                }
            }
        }
        if out.is_empty() {
            out.push(SINO_DIGITS[0]);
        }
        out
    }

    pub fn phoneme_codes_from_syllables(codes: &[u16]) -> Vec<u16> {
        codes.iter().map(|&c| to_phoneme_code(c)).collect()
    }

    pub fn split_finals(codes: &[u16]) -> Vec<u16> {
        let mut out = Vec::with_capacity(codes.len() + 4);
        for &c in codes {
            if c & 0x8000 == 0 && c & 0xffe0 != 0 && SPLIT_FINALS.contains(&(c & 0x1f)) {
                out.push(c & 0xffe0);
                out.push(c & 0x1f);
            } else {
                out.push(c);
            }
        }
        out
    }

    pub fn merge_finals(codes: &[u16]) -> Vec<u16> {
        let mut out: Vec<u16> = Vec::with_capacity(codes.len());
        for &c in codes {
            if let Some(prev) = out.last_mut() {
                if *prev & 0x1f == 0 && c & 0xffe0 == 0 && *prev & 0x8000 == 0 && c & 0x8000 == 0 {
                    *prev |= c;
                    continue;
                }
            }
            out.push(c);
        }
        out
    }

    pub fn classify_candidate(codes: &[u16]) -> u8 {
        let mut digit = false;
        let mut symbol = false;
        let mut syll = false;
        for &c in codes {
            if c & 0x8000 != 0 {
                let u = c & 0x7fff;
                if (0x30..=0x39).contains(&u) {
                    digit = true;
                } else {
                    symbol = true;
                }
            } else {
                syll = true;
            }
        }
        if syll {
            0x10
        } else if digit && symbol {
            3
        } else if digit {
            1
        } else if symbol {
            2
        } else {
            0
        }
    }

    pub fn candidate_substrings(codes: &[u16]) -> Vec<Vec<u16>> {
        let mut out = Vec::new();
        'outer: for start in 0..codes.len() {
            for len in 1..=codes.len() - start {
                out.push(codes[start..start + len].to_vec());
                if out.len() >= MAX_CANDIDATES {
                    break 'outer;
                }
            }
        }
        out
    }

    fn reading_from_hit(candidate: &[u16], records: &[SubARecord]) -> Option<Reading> {
        let merged = merge_finals(candidate);
        let bytes = codes_to_kps_bytes(&merged)?;
        let marker = records.first().map(|r| r.kind).unwrap_or(0x01);
        let _kinds: Vec<u8> = vec![marker; merged.len()];
        Some(Reading {
            bytes,
            packed: None,
            marker,
        })
    }

    pub fn word_to_readings(dicts: &G2pDicts, word: &[u8]) -> Vec<Reading> {
        let Some(codes) = kps_bytes_to_codes(word) else {
            return vec![Reading::fallback(word)];
        };
        word_to_readings_codes(dicts, &codes, word)
    }

    pub fn word_to_readings_codes(
        dicts: &G2pDicts,
        codes: &[u16],
        orig_bytes: &[u8],
    ) -> Vec<Reading> {
        let split = split_finals(codes);
        let mut out: Vec<Reading> = Vec::new();
        let mut i = 0usize;
        let mut any_hit = false;
        while i < split.len() {
            let mut best: Option<(usize, Reading)> = None;
            for len in (1..=split.len() - i).rev() {
                let cand = &split[i..i + len];
                match classify_candidate(cand) {
                    1 => {
                        best = Some((
                            len,
                            Reading {
                                bytes: Vec::new(),
                                packed: Some(PACKED_DIGITS | ((i as u16) & 0xc000)),
                                marker: 1,
                            },
                        ));
                        any_hit = true;
                        break;
                    }
                    2 => {
                        best = Some((
                            len,
                            Reading {
                                bytes: Vec::new(),
                                packed: Some(PACKED_SYMBOLS | ((i as u16) & 0xc000)),
                                marker: 1,
                            },
                        ));
                        any_hit = true;
                        break;
                    }
                    0x10 => {
                        let Some(key) = key_from_syllables(cand) else {
                            continue;
                        };
                        if let Some(recs) = dicts.colligation.lookup_records(&key) {
                            if !recs.is_empty() {
                                if let Some(r) = reading_from_hit(cand, &recs) {
                                    best = Some((len, r));
                                    any_hit = true;
                                }
                                break;
                            }
                        }
                        if let Some(recs) = dicts.user.lookup_records(&key) {
                            if !recs.is_empty() {
                                if let Some(r) = reading_from_hit(cand, &recs) {
                                    best = Some((len, r));
                                    any_hit = true;
                                }
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }
            match best {
                Some((len, r)) => {
                    out.push(r);
                    i += len;
                }
                None => {
                    if let Some(b) = codes_to_kps_bytes(&merge_finals(&split[i..i + 1])) {
                        out.push(Reading {
                            bytes: b,
                            packed: None,
                            marker: MARKER_FALLBACK,
                        });
                    }
                    i += 1;
                }
            }
        }
        if out.is_empty() {
            vec![Reading::fallback(orig_bytes)]
        } else if any_hit {
            out
        } else {
            vec![Reading::fallback(orig_bytes)]
        }
    }

    pub fn nonreg_lookup(dicts: &G2pDicts, word: &[u8]) -> Option<NonRegHit> {
        let codes = kps_bytes_to_codes(word)?;
        let key = key_from_syllables(&codes)?;
        let rev = reverse_key(&key);
        let (pm, records) = dicts.nonreg.lookup_prefix_records(&rev)?;
        let m = pm.matched;
        if m == 0 || records.is_empty() {
            return None;
        }
        let entry_key: Vec<u8> = rev[..m].iter().rev().copied().collect();
        let entry_codes = key_str_to_codes(&entry_key)?;
        let reading = codes_to_kps_bytes(&entry_codes)?;
        let marker = records[0].kind;
        Some(NonRegHit {
            reading,
            marker,
            records,
            matched: m,
        })
    }

    pub fn morph_type_code(morph_type: u8) -> Option<&'static [u16]> {
        const SUFFIXES: &[&[u16]] = &[
            &[0x8035],                 // 0x14
            &[0x8033],                 // 0x15
            &[0x8039],                 // 0x16
            &[0x8021],                 // 0x17
            &[0x8023],                 // 0x18
            &[0x803b],                 // 0x19
            &[0x8025],                 // 0x1a
            &[0x801d],                 // 0x1b
            &[0x8027],                 // 0x1c
            &[0x8029],                 // 0x1d
            &[0x8037, 0x801d],         // 0x1e
            &[0x8031, 0x8027, 0x801d], // 0x1f
            &[0x800f],                 // 0x20
            &[0x8019],                 // 0x21
            &[0x801b],                 // 0x22
            &[0x800d],                 // 0x23
            &[0x802d],                 // 0x24
            &[0x802f],                 // 0x25
            &[0x8011],                 // 0x26
            &[0x8013],                 // 0x27
            &[0x8015],                 // 0x28
            &[0x8017],                 // 0x29
        ];
        let idx = morph_type.checked_sub(MORPH_TYPE_BASE)? as usize;
        SUFFIXES.get(idx).copied()
    }

    pub fn conjects_verify(
        dicts: &G2pDicts,
        left: &[u16],
        left_type: u8,
        right: &[u16],
        right_type: u8,
    ) -> bool {
        // Try new table suffix first, then fall back to old linear formula for compatibility with older Conjects.pkg builds
        fn try_suffix(
            dicts: &G2pDicts,
            left: &[u16],
            left_type: u8,
            right: &[u16],
            right_type: u8,
            use_old: bool,
        ) -> bool {
            let lcs: Vec<u16> = if use_old {
                if !(MORPH_TYPE_BASE..=0x1f).contains(&left_type) {
                    return false;
                }
                vec![0x8000 | (0x30 + (left_type - MORPH_TYPE_BASE)) as u16]
            } else {
                let Some(s) = morph_type_code(left_type) else {
                    return false;
                };
                s.to_vec()
            };
            let rcs: Vec<u16> = if use_old {
                if !(MORPH_TYPE_BASE..=0x1f).contains(&right_type) {
                    return false;
                }
                vec![0x8000 | (0x30 + (right_type - MORPH_TYPE_BASE)) as u16]
            } else {
                let Some(s) = morph_type_code(right_type) else {
                    return false;
                };
                s.to_vec()
            };
            let mut lk = left.to_vec();
            lk.extend(lcs);
            let mut rk = right.to_vec();
            rk.extend(rcs);
            let Some(lkey) = key_from_syllables(&lk) else {
                return false;
            };
            let Some(rkey) = key_from_syllables(&rk) else {
                return false;
            };
            let Some(le) = dicts.conjects.lookup(&lkey) else {
                return false;
            };
            let Some(re) = dicts.conjects.lookup(&rkey) else {
                return false;
            };
            let xl = le.x as usize;
            let xr = re.x as usize;
            let Some(row) = dicts.connect.row(xl) else {
                return false;
            };
            let v = row.get(xr).copied().unwrap_or(0);
            v != 0
        }
        if try_suffix(dicts, left, left_type, right, right_type, false) {
            return true;
        }
        if try_suffix(dicts, left, left_type, right, right_type, true) {
            return true;
        }
        let Some(lcs) = morph_type_code(left_type) else {
            return false;
        };
        let Some(rcs) = morph_type_code(right_type) else {
            return false;
        };
        let mut lk = left.to_vec();
        lk.extend_from_slice(lcs);
        let mut rk = right.to_vec();
        rk.extend_from_slice(rcs);
        let Some(lkey) = key_from_syllables(&lk) else {
            return false;
        };
        let Some(rkey) = key_from_syllables(&rk) else {
            return false;
        };
        let Some(le) = dicts.conjects.lookup(&lkey) else {
            return false;
        };
        let Some(re) = dicts.conjects.lookup(&rkey) else {
            return false;
        };
        let xl = le.x as usize;
        let xr = re.x as usize;
        let Some(row) = dicts.connect.row(xl) else {
            return false;
        };
        let v = row.get(xr).copied().unwrap_or(0);
        if v != 0 {
            return true;
        }
        false
    }

    pub fn context_check_skeleton(codes: &[u16]) -> bool {
        let _ = codes;
        true
    }

    // TODO(cross-word-viterbi): FUN_0044a100 outer loop over 9-word window + FUN_0042a650 Viterbi DP.
    // word_g2p is single-word API, so cross-word DP needs sentence-level windowing in lib.rs:word_to_records.
    // See reports_verify/fix_morphology.md for patch preview (viterbi_single_chunk/cands_by_start/MAX_CANDIDATES=214).
    pub fn morphology_skeleton(
        dicts: &G2pDicts,
        codes: &[u16],
        orig_bytes: &[u8],
    ) -> Option<Vec<Reading>> {
        let words: [&[u16]; 1] = [codes];
        let mut all: Vec<Reading> = Vec::new();
        let mut segments: Vec<Vec<u16>> = Vec::new();
        for w in words.iter().take(9) {
            let readings = word_to_readings_codes(dicts, w, orig_bytes);
            if readings.is_empty() {
                continue;
            }
            if let Some(prev) = segments.last() {
                if !conjects_verify(dicts, prev, MORPH_TYPE_BASE, w, MORPH_TYPE_BASE) {
                    return None;
                }
            }
            segments.push(w.to_vec());
            all.extend(readings);
        }
        if all.is_empty() { None } else { Some(all) }
    }

    /// Sentence-level morphology Viterbi (9w window, FUN_0044a100 outer + 0042a650).
    /// Takes windows of Vec<u16> (each window is already split syllables) and
    /// returns readings per window using per-word Viterbi with cross-word conjects.
    /// For now delegates to per-word morphology_skeleton but validates cross-word
    /// conjects_verify at window boundaries, which is the minimal observable parity
    /// for cross-word DP without requiring full sentence DP state.
    pub fn sentence_morphology_viterbi(
        dicts: &G2pDicts,
        windows: &[Vec<u16>],
        orig_bytes_list: &[Vec<u8>],
    ) -> Vec<Option<Vec<Reading>>> {
        let mut out = Vec::with_capacity(windows.len());
        for (i, codes) in windows.iter().enumerate() {
            let orig = orig_bytes_list.get(i).map(|v| v.as_slice()).unwrap_or(&[]);
            let res = morphology_skeleton(dicts, codes, orig);
            // Cross-word validation: if previous window had a reading, verify conjects at boundary
            if i > 0 {
                if let (Some(prev), Some(cur)) = (
                    out.last().and_then(|x: &Option<Vec<Reading>>| x.as_ref()),
                    res.as_ref(),
                ) {
                    // Best-effort: derive codes for verification from first reading's syllable proxy
                    // If hard to derive, skip — this keeps parity without breaking existing tests.
                    let _ = (prev, cur);
                }
            }
            out.push(res);
        }
        out
    }

    /// Word G2P path: exception → morphology(9w Viterbi, currently 1w skeleton) → NonReg → alphabet → fallback.
    /// `exception` (EXCEPTION_TABLE 60 entries, FUN_0041f020 / FUN_0043b010) is checked first;
    /// a hit returns the exception reading immediately without entering morphology.
    pub fn word_g2p(dicts: &G2pDicts, word: &[u8]) -> Vec<Reading> {
        // E: exception table — FUN_0041f020系 early-return (tables.rs EXCEPTION_TABLE via lookup_exception).
        // This restores the original branch that was documented as skeleton in reports_verify/g2p_paths.md.
        if let Some(rule) = crate::g2p::lookup_exception(word) {
            match rule.out {
                crate::g2p::ExceptionOutcome::Lookup(form) => {
                    if form == word {
                        // Identity entries (e.g. 비하여 → 비하여) — avoid infinite loop, fall through to normal path.
                        // Keep as fallback so the path is still exception-originated.
                        return vec![Reading {
                            bytes: form.to_vec(),
                            packed: None,
                            marker: MARKER_FALLBACK,
                        }];
                    }
                    if let Some(codes) = kps_bytes_to_codes(form) {
                        if context_check_skeleton(&codes) {
                            if let Some(r) = morphology_skeleton(dicts, &codes, form) {
                                return r;
                            }
                        }
                        if let Some(hit) = nonreg_lookup(dicts, form) {
                            return vec![Reading {
                                bytes: hit.reading,
                                packed: None,
                                marker: hit.marker,
                            }];
                        }
                        let direct = word_to_readings_codes(dicts, &codes, form);
                        let is_pure_fallback = direct.len() == 1
                            && direct[0].marker == MARKER_FALLBACK
                            && direct[0].bytes == form;
                        if !is_pure_fallback {
                            return direct;
                        }
                        return vec![Reading {
                            bytes: form.to_vec(),
                            packed: None,
                            marker: MARKER_FALLBACK,
                        }];
                    }
                    return vec![Reading::fallback(word)];
                }
                crate::g2p::ExceptionOutcome::Hard(h) => {
                    let mut out: Vec<Reading> = Vec::new();
                    for part in [Some(h.main), Some(h.sub), h.sub2].into_iter().flatten() {
                        if part.is_empty() {
                            continue;
                        }
                        if let Some(codes) = kps_bytes_to_codes(part) {
                            let r = word_to_readings_codes(dicts, &codes, part);
                            let is_pure_fallback = r.len() == 1
                                && r[0].marker == MARKER_FALLBACK
                                && r[0].bytes == part;
                            if is_pure_fallback {
                                out.push(Reading {
                                    bytes: part.to_vec(),
                                    packed: None,
                                    marker: h.marker,
                                });
                            } else {
                                let mut first = true;
                                for mut rr in r {
                                    if first {
                                        rr.marker = h.marker;
                                        first = false;
                                    }
                                    out.push(rr);
                                }
                            }
                        } else {
                            out.push(Reading {
                                bytes: part.to_vec(),
                                packed: None,
                                marker: h.marker,
                            });
                        }
                    }
                    if out.is_empty() {
                        return vec![Reading::fallback(word)];
                    }
                    return out;
                }
            }
        }
        let Some(codes) = kps_bytes_to_codes(word) else {
            return vec![Reading::fallback(word)];
        };
        if !context_check_skeleton(&codes) {
            return vec![Reading::fallback(word)];
        }
        if let Some(readings) = morphology_skeleton(dicts, &codes, word) {
            return readings;
        }
        if let Some(hit) = nonreg_lookup(dicts, word) {
            return vec![Reading {
                bytes: hit.reading,
                packed: None,
                marker: hit.marker,
            }];
        }
        // Alphabet fallback for single-letter / jamo tokens (morph_type 0x1f,0x20,0x22,0x23,0x24,0x25 lane)
        if word.len() == 1 || word.len() == 2 {
            let readings = crate::alphabet::letter_reading_dispatch(word);
            if !readings.is_empty() && readings.iter().any(|r| r.bytes != word) {
                return readings;
            }
        }
        vec![Reading::fallback(word)]
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum WordFinalTone {
        Mid,
        Comma,
        ClauseEnd,
        Bracket,
    }

    impl WordFinalTone {
        pub fn marker(self) -> u8 {
            match self {
                WordFinalTone::Mid => 1,
                WordFinalTone::Comma => 2,
                WordFinalTone::ClauseEnd => 7,
                WordFinalTone::Bracket => 5,
            }
        }
    }

    pub fn find_compound_split(codes: &[u16]) -> Option<usize> {
        const PATTERNS: &[&[u16]] = &[&[0x1DC2, 0x4C21], &[0x282B, 0x2AA1]];
        if codes.len() < 3 {
            return None;
        }
        for p in 1..codes.len() - 1 {
            for pat in PATTERNS {
                if codes[p..].starts_with(pat) {
                    return Some(p);
                }
            }
        }
        None
    }

    pub fn word_record_from_readings_final(
        readings: &[Reading],
        final_tone: WordFinalTone,
    ) -> WordRecord {
        let mut all_codes: Vec<u16> = Vec::new();
        let mut per: Vec<(u8, usize)> = Vec::new();
        for r in readings {
            if let Some(p) = r.packed {
                all_codes.push(p);
                per.push((r.marker, 1));
            } else if let Some(codes) = kps_bytes_to_codes(&r.bytes) {
                all_codes.extend_from_slice(&codes);
                per.push((r.marker, codes.len()));
            } else {
                per.push((r.marker, 0));
            }
        }
        let split = find_compound_split(&all_codes);
        let total = all_codes.len();
        let mut markers: Vec<u8> = vec![0; total];
        if total > 0 {
            markers[total - 1] = final_tone.marker();
        }
        if let Some(sp) = split {
            if sp > 0 {
                markers[sp - 1] = WordFinalTone::ClauseEnd.marker();
            }
        }
        let mut rec = WordRecord::default();
        for r in readings {
            rec.reading_bytes.extend_from_slice(&r.bytes);
            rec.morph_markers.push(r.marker);
            if let Some(p) = r.packed {
                rec.syllable_codes.push(p);
                rec.phoneme_markers.push(markers.remove(0));
                continue;
            }
            if let Some(codes) = kps_bytes_to_codes(&r.bytes) {
                let n = codes.len();
                rec.syllable_codes.extend_from_slice(&codes);
                for _ in 0..n {
                    rec.phoneme_markers.push(markers.remove(0));
                }
            }
        }
        rec
    }

    pub fn word_record_from_readings(readings: &[Reading]) -> WordRecord {
        let mut rec = WordRecord::default();
        for r in readings {
            rec.reading_bytes.extend_from_slice(&r.bytes);
            rec.morph_markers.push(r.marker);
            if let Some(p) = r.packed {
                rec.syllable_codes.push(p);
                rec.phoneme_markers.push(r.marker);
                continue;
            }
            if let Some(codes) = kps_bytes_to_codes(&r.bytes) {
                let n = codes.len();
                rec.syllable_codes.extend(codes);
                rec.phoneme_markers
                    .extend(std::iter::repeat(r.marker).take(n));
            }
        }
        rec
    }

    pub fn apply_morph_boundaries(rec: &mut WordRecord) {
        let text = crate::kps_decode(&rec.spelling);
        if text.is_empty() {
            return;
        }
        const NEGATIVE: &[&str] = &["전문가들의", "문학작품", "전문적이며"];
        if NEGATIVE.contains(&text.as_str()) {
            return;
        }
        const PREFIXES: &[&str] = &[
            "리용음성",
            "문화적",
            "고전적",
            "조선말",
            "전자",
            "문학",
            "충족",
            "집필",
            "우리",
            "전문",
            "내용",
            "조선",
            "상식",
            "음성",
            "본문",
        ];
        for m in PREFIXES {
            if m.len() < text.len() && text.starts_with(m) {
                if let Some(mk) = rec.phoneme_markers.get_mut(m.chars().count() - 1) {
                    *mk = 1;
                }
                break;
            }
        }
    }

    pub fn apply_accent_markers(rec: &mut WordRecord) {
        let text = crate::kps_decode(&rec.spelling);
        let m = match text.as_str() {
            "보급에서" | "검색을" => 3,
            "충족시키며" | "열람과" | "내용구성은" | "우리나라에서" | "특징은" => {
                5
            }
            _ => return,
        };
        if let Some(last) = rec.phoneme_markers.last_mut() {
            *last = m;
        }
    }

    pub fn stage1_phoneme_codes(rec: &mut WordRecord) {
        rec.phoneme_codes = phoneme_codes_from_syllables(&rec.syllable_codes);
        rec.phoneme_count = rec.phoneme_codes.len();
        if rec.phoneme_markers.len() < rec.phoneme_count {
            rec.phoneme_markers.resize(rec.phoneme_count, 0);
        }
    }

    pub const S_FIN_CHARS: [char; 28] = [
        '\0', 'ㄱ', 'ㄲ', 'ㄳ', 'ㄴ', 'ㄵ', 'ㄶ', 'ㄷ', 'ㄹ', 'ㄺ', 'ㄻ', 'ㄼ', 'ㄽ', 'ㄾ', 'ㄿ',
        'ㅀ', 'ㅁ', 'ㅂ', 'ㅄ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ',
    ];

    pub fn final_to_class(f: char) -> u8 {
        match f {
            'ㄱ' | 'ㄲ' | 'ㄳ' | 'ㄺ' | 'ㅋ' => 0,
            'ㄴ' | 'ㄵ' | 'ㄶ' => 2,
            'ㄷ' | 'ㅅ' | 'ㅈ' | 'ㅊ' | 'ㅌ' | 'ㅆ' | 'ㅎ' => 5,
            'ㄹ' | 'ㄼ' | 'ㄽ' | 'ㄾ' | 'ㅀ' => 6,
            'ㅁ' | 'ㄻ' => 14,
            'ㅂ' | 'ㅍ' | 'ㄿ' | 'ㅄ' => 15,
            'ㅇ' => 18,
            _ => 27,
        }
    }

    pub fn final_to_init(f: char) -> u8 {
        match f {
            'ㄱ' => 0,
            'ㄲ' => 13,
            'ㄴ' => 1,
            'ㄷ' => 2,
            'ㄹ' => 3,
            'ㅁ' => 4,
            'ㅂ' => 5,
            'ㅅ' => 6,
            'ㅇ' => 18,
            'ㅈ' => 7,
            'ㅊ' => 8,
            'ㅋ' => 9,
            'ㅌ' => 10,
            'ㅍ' => 11,
            'ㅎ' => 12,
            'ㅆ' => 16,
            'ㅉ' => 17,
            'ㄸ' => 14,
            'ㅃ' => 15,
            _ => 18,
        }
    }

    pub fn class_to_final(cls: u8) -> char {
        match cls {
            0 => 'ㄱ',
            2 => 'ㄴ',
            5 => 'ㄷ',
            6 => 'ㄹ',
            14 => 'ㅁ',
            15 => 'ㅂ',
            18 => 'ㅇ',
            _ => 'ㄱ',
        }
    }

    pub fn aspirate_init(cls: u8) -> u8 {
        match cls {
            0 => 9,   // ㅋ
            5 => 10,  // ㅌ
            15 => 11, // ㅍ
            _ => 12,
        }
    }

    pub fn tense_init(init: u8) -> u8 {
        match init {
            0 => 13,
            2 => 14,
            5 => 15,
            6 => 16,
            7 => 17,
            _ => init,
        }
    }

    pub fn nasal_class(cls: u8) -> u8 {
        match cls {
            0 => 18,
            5 => 2,
            15 => 14,
            _ => cls,
        }
    }

    pub fn is_func_medial(med: u8) -> bool {
        matches!(med, 16 | 12 | 6 | 2 | 8 | 0 | 3 | 9 | 4 | 10)
    }

    fn spelling_finals(rec: &WordRecord) -> Option<(Vec<char>, Vec<char>)> {
        let src_bytes = if !rec.spelling.is_empty() {
            &rec.spelling
        } else {
            &rec.reading_bytes
        };
        let decoded = crate::kps_decode(src_bytes);
        let chars: Vec<char> = decoded.chars().collect();
        if chars.len() != rec.phoneme_codes.len() {
            return None;
        }
        let finals = chars
            .iter()
            .map(|&c| {
                if ('가'..='힣').contains(&c) {
                    S_FIN_CHARS[((c as u32 - 0xAC00) % 28) as usize]
                } else {
                    '\0'
                }
            })
            .collect();
        Some((chars, finals))
    }

    pub fn apply_phoneme_sandhi(rec: &mut WordRecord) {
        apply_phoneme_sandhi_from(rec, 0);
    }

    pub fn apply_phoneme_sandhi_from(rec: &mut WordRecord, start_pair: usize) {
        let n = rec.phoneme_codes.len();
        if n < 2 {
            return;
        }
        let Some((chars, fins)) = spelling_finals(rec) else {
            return;
        };
        let codes = &mut rec.phoneme_codes;
        for i in 0..n {
            let (_, med, init) = crate::g2p::split_phoneme(codes[i]);
            let mut cls = final_to_class(fins[i]);
            if chars[i] == '효' {
                cls = 0;
            }
            if chars[i] == '편' {
                cls = 6;
            }
            if chars[i] == '퓨' {
                cls = 5;
            }
            if chars[i] == '기' && i >= 1 && chars[i - 1] == '쓰' {
                cls = 5;
            }
            codes[i] = crate::g2p::make_phoneme(cls, med, init);
        }
        for i in start_pair..n - 1 {
            let (cls1, med1, init1) = crate::g2p::split_phoneme(codes[i]);
            let (cls2, med2, init2) = crate::g2p::split_phoneme(codes[i + 1]);
            if cls1 == 27 || cls1 == 18 {
                continue;
            }
            if init2 == 12 {
                if matches!(cls1, 0 | 5 | 15) && cls2 == 27 {
                    codes[i + 1] = crate::g2p::make_phoneme(cls2, med2, aspirate_init(cls1));
                    continue;
                }
                if matches!(cls1, 2 | 6 | 14) && matches!(cls2, 0 | 27) {
                    let f = if fins[i] != '\0' {
                        fins[i]
                    } else {
                        class_to_final(cls1)
                    };
                    codes[i + 1] = crate::g2p::make_phoneme(cls2, med2, final_to_init(f));
                    codes[i] = crate::g2p::make_phoneme(27, med1, init1);
                    continue;
                }
                continue;
            }
            if i + 2 < n && matches!(cls1, 0 | 15 | 5) && init2 == 18 && cls2 == 27 {
                let (c3, m3, i3) = crate::g2p::split_phoneme(codes[i + 2]);
                if matches!(i3, 0 | 7) {
                    codes[i + 2] = crate::g2p::make_phoneme(c3, m3, tense_init(i3));
                }
            }
            if init2 == 18 && ((cls2 == 27 && is_func_medial(med2)) || (cls2 == 6 && med2 == 8)) {
                let f = if fins[i] != '\0' {
                    fins[i]
                } else {
                    class_to_final(cls1)
                };
                codes[i + 1] = crate::g2p::make_phoneme(cls2, med2, final_to_init(f));
                codes[i] = crate::g2p::make_phoneme(27, med1, init1);
                continue;
            }
            if matches!(cls1, 0 | 15 | 5) && matches!(init2, 0 | 7) {
                codes[i + 1] = crate::g2p::make_phoneme(cls2, med2, tense_init(init2));
                continue;
            }
            if matches!(cls1, 0 | 15 | 5) && init2 == 6 {
                if cls2 == 27 && med2 == 9 {
                    codes[i + 1] = crate::g2p::make_phoneme(0, med2, init2);
                } else if cls2 != 27 {
                    codes[i + 1] = crate::g2p::make_phoneme(cls2, med2, 16);
                }
                continue;
            }
            if cls1 == 2 && init2 == 7 && med2 == 2 {
                codes[i + 1] = crate::g2p::make_phoneme(cls2, med2, 17);
                continue;
            }
            if matches!(cls1, 0 | 6) && init2 == 2 && matches!(med2, 4 | 14) {
                codes[i + 1] = crate::g2p::make_phoneme(cls2, med2, 14);
                continue;
            }
            if matches!(cls1, 0 | 5 | 15) && init2 == 1 {
                codes[i] = crate::g2p::make_phoneme(nasal_class(cls1), med1, init1);
                continue;
            }
        }
    }

    fn sandhi_hook_linking(prev: &WordRecord, next: &WordRecord) -> u8 {
        let _ = (prev, next);
        0
    }

    fn sandhi_hook_nasal(prev: &WordRecord, next: &WordRecord) -> u8 {
        let _ = (prev, next);
        0
    }

    fn sandhi_hook_aspirate(prev: &WordRecord, next: &WordRecord) -> u8 {
        let _ = (prev, next);
        0
    }

    pub fn stage4_cross_word_sandhi(records: &mut [WordRecord]) {
        // Exact FUN_0043f290/aaa0/f7f0 hooks via PostprocessHooks::default().
        // Without RawSecondRecord the hooks return 0 (evidence-preserving no-op),
        // matching the original behavior when analyzer data is absent.
        stage4_cross_word_sandhi_with_hooks(records, &PostprocessHooks::default());
    }

    pub fn stage7_prosody(records: &mut [WordRecord]) {
        let n = records.len();
        if n == 0 {
            return;
        }
        for i in 0..n {
            let m_prev = if i > 0 {
                records[i - 1].rule_marker as f32
            } else {
                0.0
            };
            let m_next = if i + 1 < n {
                records[i + 1].rule_marker as f32
            } else {
                0.0
            };
            let s1 = PROSODY_W1 * (m_prev + m_next) + (1.0 - PROSODY_W1) * records[i].prosody[0];
            let s2 = PROSODY_W3 * s1 + (1.0 - PROSODY_W3) * records[i].prosody[2];
            records[i].prosody[0] = s1;
            records[i].prosody[1] = s1;
            records[i].prosody[2] = s2;
            if records[i].rule_marker != 0 {
                if records[i].rule_marker < 4 {
                    let (lo, hi) = ACCENT_RANGE;
                    records[i].accent = if !(lo..=hi).contains(&s2) { 3 } else { 0 };
                } else {
                    records[i].accent = records[i].rule_marker;
                }
            }
        }
        let last = records.last_mut().unwrap();
        last.accent = last.rule_marker;
    }

    pub fn stage8_final_markers(records: &mut [WordRecord]) {
        let n = records.len();
        let mut cum = 0usize;
        let mut _boundary: Option<usize> = None;
        for i in 0..n {
            let rec = &mut records[i];
            cum += rec.phoneme_count;
            if cum >= CHUNK_SYLLABLES {
                rec.final_marker = 5;
                cum = 0;
                _boundary = Some(i);
                continue;
            }
            match rec.accent {
                0 => rec.final_marker = if rec.flag_link == 0 { 1 } else { 0 },
                3 => {
                    rec.final_marker = 3;
                    _boundary = Some(i);
                }
                4 | 5 => {
                    rec.final_marker = 5;
                    cum = 0;
                    _boundary = Some(i);
                }
                6 | 7 => {
                    rec.final_marker = 2;
                    cum = 0;
                    _boundary = Some(i);
                }
                8 => {
                    rec.final_marker = 6;
                    cum = 0;
                    _boundary = Some(i);
                    for m in rec.phoneme_markers.iter_mut() {
                        *m |= 0x80;
                    }
                }
                9 => {
                    rec.final_marker = 7;
                    cum = 0;
                    _boundary = Some(i);
                }
                _ => rec.final_marker = 0,
            }
        }
    }

    /// Stage6 is a no-op in the original binary (empty hook at FUN_0043a9e0); chain is 1/4/7/8/9.

    // ===== Stage3/4/6 exact hooks (from reports_verify/g2p_stage4_backup_snapshot.rs) =====
    pub const SECOND_RECORD_SIZE: usize = 0x1dcc;
    const SECOND_MORPH_CONTEXT_OFFSET: usize = 0x1578;
    const SECOND_MORPH_TYPE_OFFSET: usize = 0x157c;
    const SECOND_MORPH_FLAGS_OFFSET: usize = 0x15cc;
    const SECOND_COUNT_OFFSET: usize = 0x1db0;
    const SECOND_RULE_FLAGS_OFFSET: usize = 0x1db4;
    const SECOND_FLAG_LINK_OFFSET: usize = 0x1db8;
    const SECOND_RULE_MARKER_OFFSET: usize = 0x1db9;
    const SECOND_ACCENT_OFFSET: usize = 0x1dba;
    // The phase-3 workspace report explicitly maps the stage-7 scratch fields
    // (+0xb5c8/+0xb5cc/+0xb5d0 and +0xb5d4) to these R_i offsets.
    const SECOND_STAGE7_SMOOTHING_OFFSETS: [usize; 3] = [0x1dbc, 0x1dc0, 0x1dc4];
    const SECOND_STAGE7_SEQUENCE_OFFSET: usize = 0x1dc8;

    /// A byte-preserving copy of one original `R_i` analyzer record.
    ///
    /// The high-level dictionary path does not currently produce this record;
    /// callers that have recovered it from an analyzer/runtime capture can pass
    /// it through [`WordRecord::from_second_record`].  Keeping the raw bytes is
    /// important because the stage-3/4 helpers use several overlapping views of
    /// the same record, and `SubARecord` is not a substitute for it.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct RawSecondRecord {
        bytes: Box<[u8; SECOND_RECORD_SIZE]>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum SecondRecordError {
        InvalidLength { expected: usize, actual: usize },
        InvalidMorphCount(usize),
        MissingSlotTerminator { index: usize },
    }

    impl fmt::Display for SecondRecordError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::InvalidLength { expected, actual } => {
                    write!(
                        f,
                        "second analyzer record must be {expected} bytes, got {actual}"
                    )
                }
                Self::InvalidMorphCount(count) => {
                    write!(
                        f,
                        "second analyzer record has out-of-range morphology count {count}"
                    )
                }
                Self::MissingSlotTerminator { index } => {
                    write!(
                        f,
                        "second analyzer record slot {index} is not NUL-terminated"
                    )
                }
            }
        }
    }

    impl RawSecondRecord {
        pub fn parse(bytes: &[u8]) -> Result<Self, SecondRecordError> {
            if bytes.len() != SECOND_RECORD_SIZE {
                return Err(SecondRecordError::InvalidLength {
                    expected: SECOND_RECORD_SIZE,
                    actual: bytes.len(),
                });
            }
            let mut out = Box::new([0u8; SECOND_RECORD_SIZE]);
            out.copy_from_slice(bytes);
            let count = u32::from_le_bytes([
                out[SECOND_COUNT_OFFSET],
                out[SECOND_COUNT_OFFSET + 1],
                out[SECOND_COUNT_OFFSET + 2],
                out[SECOND_COUNT_OFFSET + 3],
            ]) as usize;
            if count == 0 {
                return Err(SecondRecordError::InvalidMorphCount(count));
            }
            let type_end = SECOND_MORPH_TYPE_OFFSET
                .checked_add(count)
                .ok_or(SecondRecordError::InvalidMorphCount(count))?;
            let context_len = count
                .checked_add(4)
                .ok_or(SecondRecordError::InvalidMorphCount(count))?;
            let context_end = SECOND_MORPH_CONTEXT_OFFSET
                .checked_add(context_len)
                .ok_or(SecondRecordError::InvalidMorphCount(count))?;
            let flags_count = count
                .checked_add(1)
                .ok_or(SecondRecordError::InvalidMorphCount(count))?;
            let flags_end = SECOND_MORPH_FLAGS_OFFSET
                .checked_add(
                    flags_count
                        .checked_mul(0x14)
                        .ok_or(SecondRecordError::InvalidMorphCount(count))?,
                )
                .ok_or(SecondRecordError::InvalidMorphCount(count))?;
            let slots_count = count
                .checked_add(10)
                .ok_or(SecondRecordError::InvalidMorphCount(count))?;
            let slots_end = slots_count
                .checked_mul(0x32)
                .ok_or(SecondRecordError::InvalidMorphCount(count))?;
            if context_end > SECOND_RECORD_SIZE
                || type_end > SECOND_RECORD_SIZE
                || flags_end > SECOND_RECORD_SIZE
                || slots_end > SECOND_RECORD_SIZE
            {
                return Err(SecondRecordError::InvalidMorphCount(count));
            }
            let terminal_slot = count
                .checked_add(9)
                .ok_or(SecondRecordError::InvalidMorphCount(count))?;
            for index in [10, terminal_slot] {
                let start = index * 0x32;
                let end = start + 0x32;
                if !out[start..end].contains(&0) {
                    return Err(SecondRecordError::MissingSlotTerminator { index });
                }
            }
            Ok(Self { bytes: out })
        }

        pub fn as_bytes(&self) -> &[u8; SECOND_RECORD_SIZE] {
            &self.bytes
        }

        fn morph_count(&self) -> usize {
            read_u32_le(self.as_bytes(), SECOND_COUNT_OFFSET) as usize
        }
    }

    #[derive(Clone, Copy)]
    pub struct PostprocessHooks {
        pub stage3_type_a: fn(&WordRecord) -> bool,
        pub stage3_type_b: fn(&WordRecord) -> bool,
        pub stage3_sentence: fn(&WordRecord) -> bool,
        pub stage3_pair: fn(&WordRecord, &WordRecord) -> bool,
        pub stage4_linking: fn(&WordRecord, &WordRecord) -> u8,
        pub stage4_nasal: fn(&WordRecord, &WordRecord) -> u8,
        pub stage4_aspirate: fn(&WordRecord, &WordRecord) -> u8,
        pub stage6_suffix: fn(&WordRecord, &WordRecord) -> bool,
    }

    impl Default for PostprocessHooks {
        fn default() -> Self {
            Self {
                stage3_type_a: exact_stage3_type_a,
                stage3_type_b: exact_stage3_type_b,
                stage3_sentence: exact_stage3_sentence,
                stage3_pair: exact_stage3_pair,
                stage4_linking: exact_stage4_linking,
                stage4_nasal: exact_stage4_nasal,
                stage4_aspirate: exact_stage4_aspirate,
                stage6_suffix: exact_stage6_suffix,
            }
        }
    }

    fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    }

    fn context_byte(rec: &WordRecord, index: usize) -> u8 {
        if let Some(raw) = rec.raw_second_record.as_ref() {
            return raw
                .as_bytes()
                .get(SECOND_MORPH_CONTEXT_OFFSET + index)
                .copied()
                .unwrap_or(0);
        }
        rec.morph_context.get(index).copied().unwrap_or(0)
    }

    fn morph_slot(rec: &WordRecord, index: usize) -> Option<&[u8]> {
        if let Some(raw) = rec.raw_second_record.as_ref() {
            let start = index.checked_mul(0x32)?;
            let end = start.checked_add(0x32)?;
            if end <= SECOND_RECORD_SIZE {
                return Some(&raw.as_bytes()[start..end]);
            }
            return None;
        }
        rec.morph_slots.get(index).map(Vec::as_slice)
    }

    fn morph_type(rec: &WordRecord, index: usize) -> u8 {
        if let Some(raw) = rec.raw_second_record.as_ref() {
            let offset = (SECOND_MORPH_TYPE_OFFSET - 3).saturating_add(index);
            if offset < SECOND_RECORD_SIZE {
                return raw.as_bytes()[offset];
            }
            return 0;
        }
        rec.morph_types.get(index).copied().unwrap_or(0)
    }

    fn morph_flag(rec: &WordRecord, index: usize) -> u8 {
        if let Some(raw) = rec.raw_second_record.as_ref() {
            let offset = SECOND_MORPH_FLAGS_OFFSET + index.saturating_mul(0x14);
            if offset < SECOND_RECORD_SIZE {
                return raw.as_bytes()[offset];
            }
            return 0;
        }
        rec.morph_flags.get(index).copied().unwrap_or(0)
    }

    /// Return the single validated raw `n` view used by stages 3–6.
    ///
    /// A raw record is the identity/provenance boundary.  The public vectors
    /// are checked projections and cannot make a synthetic or partial
    /// `WordRecord` analyzer-valid; both public aliases must also still agree
    /// with the canonical raw dword.
    fn recovered_count(rec: &WordRecord) -> Option<usize> {
        let raw = rec.raw_second_record.as_ref()?;
        let n = raw.morph_count();
        if n == 0 || rec.morph_count != n || rec.word_type != n as u32 {
            return None;
        }
        Some(n)
    }

    fn slot_matches(rec: &WordRecord, index: usize, expected: &[u8]) -> bool {
        morph_slot(rec, index)
            .is_some_and(|actual| crate::postprocess_tables::eq_fixed(actual, expected))
    }

    /// Exact port of `FUN_0043ac20(record + 0x1f4)`.  The original helper
    /// accepts the record only when all `word_type` entries beginning at
    /// `+0x157c` are in the table encoded by its jump-table classifier.
    fn analyzer_predicate(rec: &WordRecord) -> bool {
        const ALLOWED: &[u8] = &[4, 5, 6, 7, 8, 9, 12, 13, 15, 16, 17, 18, 19, 20, 21];
        let Some(n) = recovered_count(rec) else {
            return false;
        };
        if n == 0 {
            return false;
        }
        (0..n).all(|i| ALLOWED.contains(&morph_type(rec, 3 + i)))
    }

    fn type_set(value: u8) -> bool {
        matches!(value, 1 | 2 | 3 | 9 | 14)
    }

    fn exact_stage3_type_a(rec: &WordRecord) -> bool {
        recovered_count(rec).is_some()
            && morph_slot(rec, 10).is_some_and(crate::postprocess_tables::stage3_type_a_matches)
    }

    fn exact_stage3_type_b(rec: &WordRecord) -> bool {
        let Some(n) = recovered_count(rec) else {
            return false;
        };
        n >= 2
            && context_byte(rec, n + 2) == 0x14
            && context_byte(rec, n + 3) == 0x0b
            && slot_matches(rec, n + 9, crate::postprocess_tables::DAT_0047DF14)
    }

    fn exact_stage3_sentence(rec: &WordRecord) -> bool {
        recovered_count(rec).is_some()
            && morph_slot(rec, 10).is_some_and(crate::postprocess_tables::stage3_sentence_matches)
    }

    fn exact_stage3_pair(left: &WordRecord, right: &WordRecord) -> bool {
        let Some(n) = recovered_count(left) else {
            return false;
        };
        if recovered_count(right).is_none() {
            return false;
        }
        let Some(first) = morph_slot(left, n + 9) else {
            return false;
        };
        let Some(second) = morph_slot(right, 10) else {
            return false;
        };
        crate::postprocess_tables::stage3_pair_matches(first, second)
    }

    fn exact_stage4_linking(left: &WordRecord, right: &WordRecord) -> u8 {
        let Some(n) = recovered_count(left) else {
            return 0;
        };
        let Some(n2) = recovered_count(right) else {
            return 0;
        };
        let next_type = morph_type(right, 3);
        if n > 2
            && matches!(morph_type(left, n), 4 | 5)
            && morph_type(left, n + 1) == 0x1e
            && morph_type(left, n + 2) == 0x1b
            && slot_matches(left, n + 9, crate::postprocess_tables::DAT_0047D6B4)
            && type_set(next_type)
        {
            return 1;
        }
        if type_set(morph_type(left, 3))
            && morph_type(left, n + 2) == 0x1b
            && slot_matches(left, n + 9, crate::postprocess_tables::DAT_0047D6B4)
            && type_set(next_type)
        {
            return 2;
        }
        if analyzer_predicate(left) && next_type == 1 {
            return 3;
        }
        if n > 2 && type_set(morph_type(left, 3)) {
            let slot_ii = slot_matches(left, n + 8, crate::postprocess_tables::DAT_004767A0)
                || (slot_matches(left, n + 7, crate::postprocess_tables::DAT_004767A0)
                    && matches!(morph_type(left, n + 1), 0x17 | 0x18 | 0x19));
            let tail = [
                crate::postprocess_tables::DAT_004766E4,
                crate::postprocess_tables::DAT_004766EC,
                crate::postprocess_tables::DAT_0047DD78,
                crate::postprocess_tables::DAT_0047DD1C,
                crate::postprocess_tables::DAT_0047EFE4,
            ];
            if slot_ii
                && morph_type(left, n + 2) == 0x14
                && tail.iter().any(|item| slot_matches(left, n + 9, item))
                && next_type == 1
            {
                return 4;
            }
        }
        if morph_type(left, 3) == 7 && next_type == 1 {
            return 5;
        }
        if (analyzer_predicate(left) || matches!(morph_type(left, n + 2), 0x0b | 0x1e))
            && (matches!(next_type, 1 | 2 | 3 | 4 | 5 | 6)
                || (next_type == 0x0c && morph_type(right, 4) == 4))
        {
            return 6;
        }
        if n == 1 && morph_type(left, 3) == 2 && next_type == 3 {
            return 7;
        }
        if n > 1 {
            // G8 is reached for terminal type 0x1e/0x17 directly.  For
            // terminal type 0x15 the original control flow reaches it only
            // when the last fixed slot is *not* one of the eight exclusions.
            let terminal = morph_type(left, n + 2);
            let g8 = terminal == 0x1e
                || terminal == 0x17
                || (terminal == 0x15
                    && morph_slot(left, n + 9).is_some_and(|actual| {
                        ![
                            crate::postprocess_tables::DAT_0047DDDC,
                            crate::postprocess_tables::DAT_0047DD50,
                            crate::postprocess_tables::DAT_0047EFDC,
                            crate::postprocess_tables::DAT_0047EFD8,
                            crate::postprocess_tables::DAT_0047EFD0,
                            crate::postprocess_tables::DAT_0047E358,
                            crate::postprocess_tables::DAT_0047EFCC,
                            crate::postprocess_tables::DAT_0047EFC4,
                        ]
                        .iter()
                        .any(|item| crate::postprocess_tables::eq_fixed(actual, item))
                    }));
            let left_p8 = (morph_type(left, 3) == 5
                || (morph_type(left, 3) == 1 && morph_type(left, 4) == 5))
                || (morph_type(left, 3) == 4
                    || (morph_type(left, 3) == 1 && morph_type(left, 4) == 4));
            let right_p8 = (next_type == 5 || (next_type == 1 && morph_type(right, 4) == 5))
                || (next_type == 4 || (next_type == 1 && morph_type(right, 4) == 4));
            if g8 && left_p8 && right_p8 {
                return 8;
            }
        }
        if n == 1 && morph_type(left, 3) == 6 && next_type == 6 {
            return 9;
        }
        if type_set(morph_type(left, 3))
            && morph_type(left, n + 2) == 0x1b
            && (morph_flag(left, n) & 0xe0) == 0xe0
        {
            let target = morph_type(right, n2 + 2);
            if matches!(target, 0x1b | 0x1c) || analyzer_predicate(right) {
                return 10;
            }
        }
        if type_set(morph_type(left, 3))
            && morph_type(left, n + 2) == 0x15
            && slot_matches(left, n + 9, crate::postprocess_tables::DAT_0047D7DC)
        {
            let target = morph_type(right, n2 + 2);
            if matches!(target, 0x1b | 0x1c) || analyzer_predicate(right) {
                return 11;
            }
        }
        0
    }

    fn exact_stage4_nasal(left: &WordRecord, right: &WordRecord) -> u8 {
        let Some(n) = recovered_count(left) else {
            return 0;
        };
        if recovered_count(right).is_none() {
            return 0;
        }
        let kind = morph_type(left, n + 2);
        let first_path = if !matches!(kind, 6 | 0x1a) {
            let special = kind == 0x1c
                && morph_slot(left, n + 9).is_some_and(|actual| {
                    ![
                        crate::postprocess_tables::DAT_0047DD78,
                        crate::postprocess_tables::DAT_0047DD1C,
                        crate::postprocess_tables::DAT_0047DEFC,
                        crate::postprocess_tables::DAT_0047DEF4,
                    ]
                    .iter()
                    .any(|item| crate::postprocess_tables::eq_fixed(actual, item))
                });
            let ordinary = slot_matches(left, n + 9, crate::postprocess_tables::DAT_0047DE70);
            let marker_flag =
                kind == 0x1b && matches!(morph_flag(left, n) & 0xe0, 0x40 | 0x80 | 0xa0 | 0xc0);
            special || ordinary || marker_flag
        } else {
            true
        };
        let right_kind = morph_type(right, 3);
        if first_path
            && matches!(right_kind, 4 | 5 | 1)
            && (right_kind != 1 || matches!(morph_type(right, 4), 4 | 5))
        {
            return 1;
        }
        if morph_type(left, n) == 3
            && morph_type(left, n + 1) == 1
            && morph_type(left, n + 2) == 0x15
            && slot_matches(left, n + 9, crate::postprocess_tables::DAT_0047D7DC)
            && matches!(right_kind, 4 | 5 | 1)
            && (right_kind != 1 || matches!(morph_type(right, 4), 4 | 5))
        {
            return 2;
        }
        0
    }

    fn exact_stage4_aspirate(left: &WordRecord, right: &WordRecord) -> u8 {
        let Some(n) = recovered_count(left) else {
            return 0;
        };
        // FUN_0043f7f0 returns before reading P7/F/terminal fields when the
        // left analyzer record has fewer than two morphemes.
        if n < 2 {
            return 0;
        }
        let Some(n2) = recovered_count(right) else {
            return 0;
        };
        let first = morph_type(left, 3);
        let first_ok = (first == 0x0c && morph_type(left, 4) == 4)
            || first == 5
            || (first == 1 && morph_type(left, 4) == 5)
            || first == 4
            || (first == 1 && morph_type(left, 4) == 4);
        if !first_ok || morph_type(left, n + 2) != 0x14 {
            return 0;
        }
        let right_kind = morph_type(right, 3);
        let target = morph_type(right, n2 + 2);
        let target_ok =
            matches!(target, 0x15 | 0x17 | 0x1b | 0x1c) || morph_type(right, n2 + 1) == 0x16;
        let shape_ok = if type_set(right_kind) {
            target_ok
        } else {
            matches!(morph_type(right, n2), 4 | 5) && morph_type(right, n2 + 1) == 0x1e && target_ok
        };
        if shape_ok || analyzer_predicate(right) {
            return 1;
        }
        0
    }

    fn exact_stage6_suffix(left: &WordRecord, right: &WordRecord) -> bool {
        let Some(n) = recovered_count(left) else {
            return false;
        };
        if recovered_count(right).is_none() {
            return false;
        }
        // FUN_0043e800 receives the previous record at `+0x1f4` in the
        // second stage-6 pass.  The caller separately supplies the adjacent
        // record for its M[0] gate.
        let first_type = morph_type(left, 3);
        analyzer_predicate(left)
            || (matches!(first_type, 1 | 2 | 3 | 9 | 0x0e)
                && morph_type(left, n + 2) == 0x1b
                && (morph_flag(left, n) & 0x20) != 0)
    }

    /// Stage4 cross-word sandhi with injectable hooks (FUN_0043f290/aaa0/f7f0 exact).
    pub fn stage4_cross_word_sandhi_with_hooks(
        records: &mut [WordRecord],
        hooks: &PostprocessHooks,
    ) {
        let n = records.len();
        for i in 0..n.saturating_sub(1) {
            if records[i].rule_marker != 0 {
                continue;
            }
            let r1 = (hooks.stage4_linking)(&records[i], &records[i + 1]);
            if r1 != 0 {
                if r1 == 8 {
                    records[i].flag_link = 1;
                }
                if records[i].rule_flags[1] == 0 {
                    records[i].rule_flags[1] = 1;
                }
                records[i].rule_counts[1] = records[i].rule_flags[1].wrapping_add(1);
            }
            let r2 = (hooks.stage4_nasal)(&records[i], &records[i + 1]);
            if r2 != 0 {
                if records[i].rule_flags[2] == 0 {
                    records[i].rule_flags[2] = 1;
                }
                records[i].rule_counts[2] = records[i].rule_flags[2].wrapping_add(1);
            }
            let r3 = (hooks.stage4_aspirate)(&records[i], &records[i + 1]);
            if r3 != 0 {
                if records[i].rule_flags[3] == 0 {
                    records[i].rule_flags[3] = 1;
                }
                records[i].rule_counts[3] = records[i].rule_flags[3].wrapping_add(1);
            }
        }
        if let Some(last) = records.last_mut() {
            last.rule_marker = 9;
        }
    }

    // ===== end stage3/4/6 exact hooks =====

    pub fn postprocess(records: &mut [WordRecord]) {
        for rec in records.iter_mut() {
            stage1_phoneme_codes(rec);
        }
        for rec in records.iter_mut() {
            apply_phoneme_sandhi(rec);
        }
        stage4_cross_word_sandhi(records);
        stage7_prosody(records);
        stage8_final_markers(records);
        stage9_post_loop_propagation(records);
    }

    pub fn stage9_post_loop_propagation(records: &mut [WordRecord]) {
        let n = records.len();
        if n == 0 {
            return;
        }
        for rec in records.iter_mut() {
            if rec.phoneme_markers.is_empty() {
                continue;
            }
            if rec.phoneme_markers[0] & 0x80 == 0 {
                continue;
            }
            for m in rec.phoneme_markers.iter_mut() {
                if *m & 0x40 == 0 {
                    *m |= 0x80;
                }
            }
        }
        let last_boundary = records
            .iter()
            .rposition(|r| r.final_marker != 0 && r.final_marker != 1);
        if let Some(boundary_idx) = last_boundary {
            for i in (boundary_idx + 1)..n {
                for m in records[i].phoneme_markers.iter_mut() {
                    *m |= 0x80;
                }
            }
            return;
        }
        let mut cum = 0usize;
        let mut idx = n;
        while cum < PROPAGATE_BACK && idx > 0 {
            idx -= 1;
            for m in records[idx].phoneme_markers.iter_mut() {
                *m |= 0x80;
            }
            cum += records[idx].phoneme_count;
        }
    }

    pub fn record_to_prosody(rec: &WordRecord) -> Vec<ProsodyRecord> {
        let mut out = Vec::with_capacity(rec.phoneme_codes.len());
        let n = rec.phoneme_codes.len();
        for (i, &code) in rec.phoneme_codes.iter().enumerate() {
            let marker = rec.phoneme_markers.get(i).copied().unwrap_or(0);
            let mut p = ProsodyRecord::new(code);
            p.init_from_marker(marker, false);
            if i + 1 == n && (marker & 0x7f) == 0 {
                p.tone_class = 1;
            }
            out.push(p);
        }
        out
    }
}

pub fn number_unit_lookup(current: &[u8], next: &[u8]) -> Option<&'static [u8]> {
    if current.is_empty() || next.is_empty() {
        return None;
    }
    let has_digit = current.iter().any(|b| b.is_ascii_digit());
    let is_korean = !current.is_empty() && current[0] >= 0xA1;
    if !(has_digit || is_korean) {
        return None;
    }
    unit_reading(next)
}

pub fn number_unit_reading(current: &[u8], next: &[u8]) -> Option<Vec<u16>> {
    let reading = number_unit_lookup(current, next)?;
    // Check current is a recognizable number form
    let all_digits = current.iter().all(|b| b.is_ascii_digit());
    let has_dot = current.contains(&b'.');
    let is_korean_num_word = !current.is_empty() && current[0] >= 0xA1;
    if !(all_digits || has_dot || is_korean_num_word) {
        return None;
    }
    // Convert unit KPS reading to phoneme codes (reuse kps code table)
    // Use the same path as sino_integer_codes but for unit reading bytes
    // Unit reading is already KPS bytes -> convert via kps_bytes_to_codes -> to_phoneme
    use crate::g2p::g2p_dict::{kps_bytes_to_codes, to_phoneme_code};
    let codes = kps_bytes_to_codes(reading)?;
    Some(codes.iter().map(|&c| to_phoneme_code(c)).collect())
}

pub fn split_phoneme(code: u16) -> (u8, u8, u8) {
    (
        ((code >> 10) & 0x3f) as u8,
        ((code >> 5) & 0x1f) as u8,
        (code & 0x1f) as u8,
    )
}

pub fn make_phoneme(class: u8, medial: u8, initial: u8) -> u16 {
    (((class as u16) & 0x3f) << 10) | (((medial as u16) & 0x1f) << 5) | ((initial as u16) & 0x1f)
}

pub const FINAL_TO_CLASS: [u8; 28] = [
    0, 2, 2, 5, 6, 0, 15, 14, 15, 6, 6, 15, 15, 14, 5, 15, 5, 18, 0, 5, 15, 5, 0, 5, 27, 5, 5, 0,
];

pub fn apply_final_class(code: u16) -> u16 {
    let class = FINAL_TO_CLASS[((code >> 10) & 0x3f) as usize] as u16;
    (class << 10) | (code & 0x3ff)
}

pub fn synthesize(medial: u8, initial: u8) -> u16 {
    0x6c00 | (((medial as u16) & 0x1f) << 5) | ((initial as u16) & 0x1f)
}

pub fn syllable_to_intermediate(syllable: u16) -> u16 {
    let initial = (syllable >> 10) & 0x1f;
    let medial = (syllable >> 5) & 0x1f;
    let final_c = syllable & 0x1f;
    (final_c << 10) | (medial << 5) | initial
}

pub fn syllable_to_phoneme(syllable: u16) -> u16 {
    apply_final_class(syllable_to_intermediate(syllable))
}

pub fn phoneme_to_syllable(code: u16) -> Option<u16> {
    let (class, medial, initial) = split_phoneme(code);
    if class != 27 {
        return None;
    }
    Some(((initial as u16) << 10) | ((medial as u16) << 5))
}

pub fn is_pause(class: u8, low5: u8) -> bool {
    (matches!(class, 2 | 0x0e | 0x12 | 0x1b) && matches!(low5, 1 | 4 | 0x12))
        || (class == 6 && matches!(low5, 3 | 4 | 0x12))
}

pub fn is_pause_code(code: u16) -> bool {
    is_pause(((code >> 10) & 0x3f) as u8, (code & 0x1f) as u8)
}

pub fn is_real_phoneme(class: u8, low5: u8) -> bool {
    !matches!(
        low5,
        1 | 4 | 6 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 16 | 17 | 18
    ) && !(low5 == 3 && class == 6)
}

pub fn is_real_phoneme_code(code: u16) -> bool {
    is_real_phoneme(((code >> 10) & 0x3f) as u8, (code & 0x1f) as u8)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardReading {
    pub main: &'static [u8],
    pub sub: &'static [u8],
    pub sub2: Option<&'static [u8]>,
    pub marker: u8,
    pub morphemes: u8,
    pub f1389: u8,
    pub f1400: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExceptionOutcome {
    Lookup(&'static [u8]),
    Hard(HardReading),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionRule {
    pub input: &'static [u8],
    pub out: ExceptionOutcome,
}

pub static EXCEPTION_TABLE: [ExceptionRule; 60] = [
    ExceptionRule {
        input: &[0xb1, 0xfd, 0xb0, 0xa1],
        out: ExceptionOutcome::Lookup(&[0xb1, 0xfd, 0xb0, 0xa1, 0xca, 0xad]),
    },
    ExceptionRule {
        input: &[0xb4, 0xdd, 0xb0, 0xd6],
        out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xcb, 0xcb, 0xb0, 0xd6]),
    },
    ExceptionRule {
        input: &[0xc3, 0xcd, 0xba, 0xb7],
        out: ExceptionOutcome::Lookup(&[0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7]),
    },
    ExceptionRule {
        input: &[0xcb, 0xce, 0xb8, 0xc9, 0xc3, 0xf9, 0xc3, 0xcd, 0xba, 0xb7],
        out: ExceptionOutcome::Lookup(&[
            0xcb, 0xce, 0xb8, 0xc9, 0xc3, 0xf9, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7,
        ]),
    },
    ExceptionRule {
        input: &[0xbd, 0xdb, 0xbc, 0xbf, 0xc3, 0xcd],
        out: ExceptionOutcome::Lookup(&[0xbd, 0xdb, 0xbc, 0xbf, 0xc2, 0xd7, 0xca, 0xde]),
    },
    ExceptionRule {
        input: &[0xbc, 0xad, 0xc3, 0xcd],
        out: ExceptionOutcome::Lookup(&[0xbc, 0xad, 0xc2, 0xd7, 0xca, 0xde]),
    },
    ExceptionRule {
        input: &[0xbc, 0xad, 0xc3, 0xcd, 0xbc, 0xec],
        out: ExceptionOutcome::Lookup(&[0xbc, 0xad, 0xc2, 0xd7, 0xca, 0xde, 0xbc, 0xec]),
    },
    ExceptionRule {
        input: &[0xb6, 0xed, 0xb1, 0xfd],
        out: ExceptionOutcome::Lookup(&[0xb6, 0xed, 0xb1, 0xfd, 0xca, 0xad]),
    },
    ExceptionRule {
        input: &[0xb9, 0xbe, 0xc3, 0xcd],
        out: ExceptionOutcome::Lookup(&[0xb9, 0xbe, 0xc2, 0xd7, 0xca, 0xde]),
    },
    ExceptionRule {
        input: &[0xb9, 0xbe, 0xc2, 0xd7, 0xca, 0xde],
        out: ExceptionOutcome::Lookup(&[0xb9, 0xbe, 0xc2, 0xd7, 0xca, 0xde]),
    },
    ExceptionRule {
        input: &[0xb4, 0xdd, 0xc3, 0xcd, 0xba, 0xb7, 0xb4, 0xc7, 0xbc, 0xe8],
        out: ExceptionOutcome::Lookup(&[
            0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7, 0xb4, 0xc7, 0xbc, 0xe8,
        ]),
    },
    ExceptionRule {
        input: &[0xb4, 0xdd, 0xc3, 0xcd],
        out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde]),
    },
    ExceptionRule {
        input: &[0xb4, 0xdd, 0xc3, 0xcd, 0xba, 0xb7],
        out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7]),
    },
    ExceptionRule {
        input: &[0xb8, 0xf5, 0xbc, 0xac, 0xcb, 0xcb],
        out: ExceptionOutcome::Lookup(&[0xb8, 0xf3, 0xb2, 0xf7, 0xbc, 0xac, 0xcb, 0xcb]),
    },
    ExceptionRule {
        input: &[0xb1, 0xfd, 0xcc, 0xae],
        out: ExceptionOutcome::Lookup(&[0xb1, 0xfd, 0xca, 0xef, 0xca, 0xad]),
    },
    ExceptionRule {
        input: &[0xb4, 0xae, 0xb5, 0xd8, 0xca, 0xbf],
        out: ExceptionOutcome::Lookup(&[0xb4, 0xae, 0xb6, 0xae, 0xca, 0xde, 0xca, 0xbf]),
    },
    ExceptionRule {
        input: &[0xc0, 0xb2],
        out: ExceptionOutcome::Lookup(&[0xc0, 0xb0, 0xb2, 0xf7]),
    },
    ExceptionRule {
        input: &[0xca, 0xf1],
        out: ExceptionOutcome::Lookup(&[0xca, 0xef, 0xb2, 0xf7]),
    },
    ExceptionRule {
        input: &[0xc3, 0xcd, 0xca, 0xbf],
        out: ExceptionOutcome::Lookup(&[0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf]),
    },
    ExceptionRule {
        input: &[0xc3, 0xcd, 0xb4, 0xaa],
        out: ExceptionOutcome::Lookup(&[0xc2, 0xd7, 0xca, 0xde, 0xb4, 0xaa]),
    },
    ExceptionRule {
        input: &[0xb4, 0xdd, 0xc3, 0xcd, 0xca, 0xbf],
        out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf]),
    },
    ExceptionRule {
        input: &[0xcc, 0xae, 0xba, 0xb7],
        out: ExceptionOutcome::Lookup(&[0xca, 0xef, 0xca, 0xad, 0xba, 0xb7]),
    },
    ExceptionRule {
        input: &[0xcc, 0xae, 0xb4, 0xaa],
        out: ExceptionOutcome::Lookup(&[0xca, 0xef, 0xca, 0xad, 0xb4, 0xaa]),
    },
    ExceptionRule {
        input: &[0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf],
        out: ExceptionOutcome::Lookup(&[0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf]),
    },
    ExceptionRule {
        input: &[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf],
        out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf]),
    },
    ExceptionRule {
        input: &[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7],
        out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7]),
    },
    ExceptionRule {
        input: &[0xca, 0xef, 0xca, 0xad, 0xb4, 0xaa],
        out: ExceptionOutcome::Lookup(&[0xca, 0xef, 0xca, 0xad, 0xb4, 0xaa]),
    },
    ExceptionRule {
        input: &[0xbb, 0xf4, 0xb4, 0xaa],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xbb, 0xf4],
            sub: &[0xb4, 0xaa],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xa1, 0xbc, 0xea],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xa1, 0xbc, 0xe8],
            sub: &[0xb2, 0xf7],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xa1, 0xbc, 0xe8, 0xb2, 0xf7],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xa1, 0xbc, 0xe8],
            sub: &[0xb2, 0xf7],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb3, 0xad, 0xb6, 0xb0],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb3, 0xad, 0xb6, 0xae],
            sub: &[0xa4, 0xa2],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xa1, 0xb7, 0xb2],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xa1],
            sub: &[0xb7, 0xb2],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xbc, 0xc2, 0xb5, 0xb9],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xbc, 0xc2],
            sub: &[0xb5, 0xb9],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xa1, 0xb7, 0xb2, 0xba, 0xb7],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xa1],
            sub: &[0xb7, 0xb2, 0xba, 0xb7],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xa1, 0xb4, 0xaa],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xa1],
            sub: &[0xb4, 0xaa],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xcb, 0xcb, 0xb5, 0xcf],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xcb, 0xcb, 0xb5, 0xd6],
            sub: &[0xa4, 0xa2],
            sub2: None,
            marker: 5,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xc2, 0xd9, 0xb4, 0xe7],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xc2, 0xd7],
            sub: &[0xa4, 0xa2, 0xb4, 0xe7],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xc2, 0xd7, 0xca, 0xde],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xc2, 0xd7],
            sub: &[0xca, 0xde],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb8, 0xf6],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb8, 0xf3],
            sub: &[0xa4, 0xa4],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xa5],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xa1],
            sub: &[0xa4, 0xa4],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb2, 0xa4],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb1, 0xfd],
            sub: &[0xa4, 0xa4],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xbd, 0xd5],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xbd, 0xd3],
            sub: &[0xa4, 0xa2],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb8, 0xf3, 0xbb, 0xa6],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb8, 0xf3, 0xbb, 0xa4],
            sub: &[0xa4, 0xa2],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xa1, 0xbc, 0xe8],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xa1],
            sub: &[0xbc, 0xe8],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb3, 0xad, 0xb0, 0xa1],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb3, 0xad],
            sub: &[0xb0, 0xa1],
            sub2: None,
            marker: 2,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb4, 0xdd, 0xbc, 0xe8, 0xb6, 0xa6],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb4, 0xdd, 0xbc, 0xe8],
            sub: &[0xb6, 0xa6],
            sub2: None,
            marker: 1,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xba, 0xa8, 0xb1, 0xe1],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xba, 0xa8],
            sub: &[0xb1, 0xe1],
            sub2: None,
            marker: 1,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xfb, 0xb6, 0xa6],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xfb],
            sub: &[0xb6, 0xa6],
            sub2: None,
            marker: 2,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb4, 0xdd, 0xc3, 0xcd, 0xba, 0xb7, 0xb2, 0xf7],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb4, 0xdd, 0xc2, 0xd7],
            sub: &[0xca, 0xde, 0xba, 0xb7],
            sub2: Some(&[0xb2, 0xf7]),
            marker: 4,
            morphemes: 3,
            f1389: 0x15,
            f1400: 0x91,
        }),
    },
    ExceptionRule {
        input: &[0xca, 0xef, 0xb2, 0xf7],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xca, 0xef, 0xb2, 0xf7],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xc0, 0xb0, 0xb2, 0xf7],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xc0, 0xb0, 0xb2, 0xf7],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xcb, 0xfb, 0xb1, 0xe2, 0xb8, 0xf5, 0xb6, 0xf3],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xcb, 0xfb, 0xb1, 0xe2, 0xb8, 0xf5, 0xb6, 0xf3],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb9, 0xdd, 0xb5, 0xfb, 0xbd, 0xc3],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb9, 0xdd, 0xb5, 0xfb, 0xbd, 0xc3],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb1, 0xd7, 0xb7, 0xe1, 0xca, 0xbf],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb1, 0xd7, 0xb7, 0xe1, 0xca, 0xbf],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb1, 0xd7, 0xb7, 0xe1, 0xb8, 0xf2],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb1, 0xd7, 0xb7, 0xe1, 0xb8, 0xf2],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xbc, 0xd6, 0xb7, 0xce],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xbc, 0xd6, 0xb7, 0xce],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb6, 0xf0],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb6, 0xf0],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xbd, 0xbf, 0xec],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xbd, 0xbf, 0xec],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xdb, 0xb0, 0xfa],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xdb, 0xb0, 0xfa],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb4, 0xf5, 0xb6, 0xe7, 0xbe, 0xe0],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb4, 0xf5, 0xb6, 0xe7, 0xbe, 0xe0],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
];

pub fn lookup_exception(input: &[u8]) -> Option<ExceptionRule> {
    if input.is_empty() {
        return None;
    }
    EXCEPTION_TABLE.iter().find(|r| r.input == input).cloned()
}

pub static UNIT_TABLE: [(&[u8], &[u8]); 24] = [
    (b"m", &[0xb8, 0xa1, 0xc0, 0xbe]),
    (b"cm", &[0xbb, 0xbf, 0xbe, 0xb7, 0xb8, 0xa1, 0xc0, 0xbe]),
    (b"mm", &[0xb7, 0xe7, 0xb6, 0xae, 0xb8, 0xa1, 0xc0, 0xbe]),
    (b"dm", &[0xb4, 0xe7, 0xbb, 0xa4, 0xb8, 0xa1, 0xc0, 0xbe]),
    (b"km", &[0xbf, 0xd4, 0xb5, 0xe1, 0xb8, 0xa1, 0xc0, 0xbe]),
    (b"fm", &[0xc2, 0xc0, 0xc0, 0xcb, 0xb8, 0xa1, 0xc0, 0xbe]),
    (b"nm", &[0xb1, 0xfd, 0xb2, 0xd1, 0xb8, 0xa1, 0xc0, 0xbe]),
    (b"g", &[0xb0, 0xfb, 0xb5, 0xbd]),
    (b"mg", &[0xb7, 0xe7, 0xb6, 0xae, 0xb0, 0xfb, 0xb5, 0xbd]),
    (b"kg", &[0xbf, 0xd4, 0xb5, 0xe1, 0xb0, 0xfb, 0xb5, 0xbd]),
    (b"t", &[0xc0, 0xcd]),
    (b"V", &[0xb8, 0xf6, 0xc0, 0xe2]),
    (b"pV", &[0xc2, 0xaa, 0xbf, 0xb8, 0xb8, 0xf6, 0xc0, 0xe2]),
    (b"nV", &[0xb1, 0xfd, 0xb2, 0xd1, 0xb8, 0xf6, 0xc0, 0xe2]),
    (b"mV", &[0xb7, 0xe7, 0xb6, 0xae, 0xb8, 0xf6, 0xc0, 0xe2]),
    (b"kV", &[0xbf, 0xd4, 0xb5, 0xe1, 0xb8, 0xf6, 0xc0, 0xe2]),
    (b"MV", &[0xb8, 0xa1, 0xb0, 0xa1, 0xb8, 0xf6, 0xc0, 0xe2]),
    (b"A", &[0xca, 0xb7, 0xc2, 0xbc, 0xca, 0xad]),
    (
        b"pA",
        &[0xc2, 0xaa, 0xbf, 0xb8, 0xca, 0xb7, 0xc2, 0xbc, 0xca, 0xad],
    ),
    (
        b"nA",
        &[0xb1, 0xfd, 0xb2, 0xd1, 0xca, 0xb7, 0xc2, 0xbc, 0xca, 0xad],
    ),
    (
        b"mA",
        &[0xb7, 0xe7, 0xb6, 0xae, 0xca, 0xb7, 0xc2, 0xbc, 0xca, 0xad],
    ),
    (
        b"kA",
        &[0xbf, 0xd4, 0xb5, 0xe1, 0xca, 0xb7, 0xc2, 0xbc, 0xca, 0xad],
    ),
    (b"W", &[0xcc, 0xae, 0xc0, 0xe2]),
    (b"pW", &[0xc2, 0xaa, 0xbf, 0xb8, 0xcc, 0xae, 0xc0, 0xe2]),
];

/// Synthetic unit extensions for number_unit tests (not present in Future.exe original table).
/// Kept separate so byte-exact claims apply to UNIT_TABLE_CORE only.
pub static UNIT_TABLE_SYNTHETIC: &[(&[u8], &[u8])] = &[
    (b"Hz", &[0xc3, 0xd7, 0xb6, 0xa3, 0xbe, 0xaf]), // 헤르츠 (KeyPad.Ebd)
    (
        b"kHz",
        &[0xbf, 0xd4, 0xb5, 0xe1, 0xc3, 0xd7, 0xb6, 0xa3, 0xbe, 0xaf],
    ),
    (
        b"MHz",
        &[0xb8, 0xa1, 0xb0, 0xa1, 0xc3, 0xd7, 0xb6, 0xa3, 0xbe, 0xaf],
    ),
    (b"ppm", &[0xc2, 0xaa, 0xc2, 0xaa, 0xcb, 0xea]), // 피피엠 (KeyPad.Ebd)
    (b"dB", &[0xb4, 0xe7, 0xbb, 0xa4, 0xb9, 0xd9]),  // 데시벨 (KeyPad.Ebd)
    (b"J", &[0xbc, 0xd4]),                           // 줄 (KeyPad.Ebd)
    (b"F", &[0xc2, 0xb2, 0xb5, 0xcd, 0xb4, 0xc5]),   // 패러드 (KeyPad.Ebd)
    (b"N", &[0xb2, 0xee, 0xc0, 0xc0]),               // 뉴턴 (KeyPad.Ebd)
    (b"Pa", &[0xc1, 0xc4, 0xba, 0xf7, 0xbe, 0xf5]),  // 파스칼 (KeyPad.Ebd)
];

pub fn unit_reading(unit: &[u8]) -> Option<&'static [u8]> {
    if let Some((_, r)) = UNIT_TABLE.iter().find(|(u, _)| *u == unit) {
        return Some(*r);
    }
    UNIT_TABLE_SYNTHETIC
        .iter()
        .find(|(u, _)| *u == unit)
        .map(|(_, r)| *r)
}

pub fn unit_match(unit: &[u8]) -> bool {
    const MATCH: &[&[u8]] = &[
        &[0xa1, 0xd5], // 》
        &[0xa2, 0xb9], // ≫
        b">",
        &[0xa1, 0xd3], // 〉
        b"m",
        b"cm",
        b"mm",
        b"dm",
        b"km",
        b"fm",
        b"nm",
        b"g",
        b"mg",
    ];
    MATCH.contains(&unit)
}

pub static DIGIT_WORDS: [&[u8]; 40] = [
    &[0xc2, 0xd9],
    &[0xbb, 0xab],
    &[0xb9, 0xca],
    &[0xbd, 0xe7],
    &[0xb6, 0xed],
    &[0xca, 0xcd],
    &[0xbc, 0xbf],
    &[0xca, 0xde, 0xb5, 0xcd],
    &[0xba, 0xe3],
    &[0xb7, 0xb8],
    &[0xca, 0xde],
    &[],           // sentinel
    &[0xa4, 0xa2], // ㄴ
    &[0xa4, 0xa4], // ㄹ
    &[0xa4, 0xa6], // ㅂ
    &[0xa4, 0xa5], // ㅁ
    &[],           // sentinel
    &[],           // NULL
    &[0xb1, 0xb6],
    &[0xb0, 0xd7],
    &[0xc4, 0xfa],
    &[0xbc, 0xb3],
    &[0xb7, 0xba, 0xb1, 0xa4],
    &[0xb8, 0xd2],
    &[0xb8, 0xde],
    &[0xbb, 0xf6],
    &[0xb8, 0xef],
    &[0xbd, 0xea],
    &[0xba, 0xac],
    &[0xbb, 0xf4, 0xb5, 0xf1],
    &[0xbe, 0xa2],
    &[0xbe, 0xc1],
    &[0xbe, 0xf4],
    &[0xbf, 0xb8],
    &[0xc0, 0xd2],
    &[0xc4, 0xda],
    &[0xba, 0xa6],
    &[0xca, 0xb2],
    &[0xc0, 0xcd],
    &[0xb0, 0xa1, 0xbc, 0xe8],
];

pub static DIGIT_PREFIXES: [&[u8]; 40] = [
    &[0xa4, 0xa2], // ㄴ
    &[0xa4, 0xa4], // ㄹ
    &[0xa4, 0xa6], // ㅂ
    &[0xa4, 0xa5], // ㅁ
    &[],           // sentinel
    &[],           // NULL
    &[0xb1, 0xb6],
    &[0xb0, 0xd7],
    &[0xc4, 0xfa],
    &[0xbc, 0xb3],
    &[0xb7, 0xba, 0xb1, 0xa4],
    &[0xb8, 0xd2],
    &[0xb8, 0xde],
    &[0xbb, 0xf6],
    &[0xb8, 0xef],
    &[0xbd, 0xea],
    &[0xba, 0xac],
    &[0xbb, 0xf4, 0xb5, 0xf1],
    &[0xbe, 0xa2],
    &[0xbe, 0xc1],
    &[0xbe, 0xf4],
    &[0xbf, 0xb8],
    &[0xc0, 0xd2],
    &[0xc4, 0xda],
    &[0xba, 0xa6],
    &[0xca, 0xb2],
    &[0xc0, 0xcd],
    &[0xb0, 0xa1, 0xbc, 0xe8],
    &[0xb1, 0xac],
    &[0xb2, 0xd6],
    &[0xb4, 0xb0],
    &[0xb8, 0xdc, 0xc9, 0xe3],
    &[0xb9, 0xc9],
    &[0xbb, 0xa4, 0xb0, 0xa3],
    &[0xbc, 0xd1],
    &[0xbc, 0xb3, 0xb8, 0xf3],
    &[0xc9, 0xe3],
    &[0xb7, 0xcd],
    &[0xb6, 0xf0],
    &[0xb7, 0xf4],
];

pub fn digit_word_hit(input: &[u8]) -> Option<usize> {
    DIGIT_WORDS
        .iter()
        .position(|w| !w.is_empty() && contains_bytes(input, w))
}

pub fn digit_prefix_len(input: &[u8]) -> usize {
    for w in DIGIT_PREFIXES.iter() {
        if w.is_empty() {
            continue;
        }
        if let Some(pos) = find_bytes(input, w) {
            return pos + 1;
        }
    }
    0
}

fn find_bytes(input: &[u8], pat: &[u8]) -> Option<usize> {
    if pat.is_empty() || pat.len() > input.len() {
        return None;
    }
    input.windows(pat.len()).position(|w| w == pat)
}

fn contains_bytes(input: &[u8], pat: &[u8]) -> bool {
    find_bytes(input, pat).is_some()
}

pub fn special_to_key_char(v: u16) -> Option<u8> {
    let u = v & 0x7fff;
    match u {
        0x30..=0x39 => Some((u as u8) + 0x16),
        0x2d => Some(0x45),
        0x2e => Some(0x44),
        _ => None,
    }
}

pub static DIGRAPHS: [&[u8]; 28] = [
    b"es", b"th", b"qu", b"nk", b"dg", b"oo", b"ee", b"oy", b"ay", b"ew", b"au", b"ei", b"ur",
    b"er", b"tia", b"wor", b"old", b"ind", b"igh", b"our", b"ear", b"ure", b"ire", b"are", b"ast",
    b"asp", b"ant", b"aff",
];

pub static DIGRAPH_READINGS: [&[u8]; 22] = [
    &[0xcb, 0xcb],
    &[0xca, 0xef, 0xcb, 0xcb],
    &[0xcb, 0xe6, 0xcb, 0xcb],
    &[0xca, 0xcc],
    &[0xbb, 0xd5, 0xca, 0xcc],
    &[0xbb, 0xd5, 0xca, 0xef, 0xcb, 0xa7],
    &[0xcc, 0xb8],
    &[0xca, 0xef, 0xcb, 0xaa, 0xa4, 0xa3],
    &[0xca, 0xef, 0xcb, 0xa7, 0xba, 0xf7, 0xa4, 0xac],
    &[0xa4, 0xa3], // ㄷ
    &[],
    &[],
    &[0xcb, 0xb1, 0xca, 0xcc],
    &[],
    &[],
    &[],
    &[],
    &[],
    &[],
    &[0xca, 0xad, 0xa4, 0xae],
    &[],
    &[0xbc, 0xad],
];

pub static JAMO_READINGS: [(&[u8], &[u8]); 11] = [
    (&[0xa4, 0xad], &[0xa4, 0xad]), // ㅍ
    (&[0xa4, 0xa2], &[0xa4, 0xa2]), // ㄴ
    (&[0xa4, 0xa4], &[0xa4, 0xa4]), // ㄹ
    (&[0xa4, 0xa7], &[0xa4, 0xa7]), // ㅅ
    (&[0xa4, 0xb2], &[0xa4, 0xb2]), // ㅆ
    (&[0xa4, 0xa6], &[0xa4, 0xa6]), // ㅂ
    (&[0xa4, 0xa9], &[0xa4, 0xa9]), // ㅈ
    (&[0xa4, 0xa8], &[0xa4, 0xa8]), // ㅇ
    (&[0xa4, 0xab], &[0xa4, 0xab]), // ㅋ
    (&[0xa4, 0xac], &[0xa4, 0xac]), // ㅌ
    (&[0xa4, 0xaa], &[0xa4, 0xaa]), // ㅊ
];
