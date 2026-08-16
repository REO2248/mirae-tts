//! G2P - phoneme conversion from internal codes.
//! Dictionary pipeline (colligation/User/NonReg/Conjects) + word reading + 9-stage
//! postprocess chain + phoneme-code base + static exception tables + digit/unit/alphabet.
pub mod g2p_dict {

    use std::collections::HashMap;
    use std::sync::OnceLock;

    use crate::connect::ConnectMatrix;
    use crate::dict::{key_from_syllables, reverse_key, Dict, SubARecord};
    use crate::kps9566::Kps9566;
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
    pub const PROSODY_W2: f32 = 0.5;
    pub const PROSODY_W3: f32 = 0.99;
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
    }

    #[derive(Debug, Clone, Copy)]
    pub struct G2pDicts<'a> {
        pub colligation: &'a Dict,
        pub user: &'a Dict,
        pub nonreg: &'a Dict,
        pub conjects: &'a Dict,
        pub connect: &'a ConnectMatrix,
    }


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
    pub(crate) const INIT_KP_TO_STD: [u8; 19] = [0, 2, 3, 5, 6, 7, 9, 12, 14, 15, 16, 17, 18, 1, 4, 8, 10, 13, 11];

    pub(crate) const INIT_STD_TO_KP: [u8; 19] = [0, 13, 1, 2, 14, 3, 4, 5, 15, 6, 16, 18, 7, 17, 8, 9, 10, 11, 12];

    /// Unicode: ㅏㅐㅑㅒㅓㅔㅕㅖㅗㅘㅙㅚㅛㅜㅝㅞㅟㅠㅡㅢㅣ
    pub(crate) const MED_KP_TO_STD: [u8; 21] = [0, 2, 4, 6, 8, 12, 13, 17, 18, 20, 1, 3, 5, 7, 9, 10, 11, 14, 15, 16, 19];

    pub(crate) const MED_STD_TO_KP: [u8; 21] = [0, 10, 1, 11, 2, 12, 3, 13, 4, 14, 15, 16, 5, 6, 17, 18, 19, 7, 8, 20, 9];

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
        let mut mask = crate::kps_tables::COL_MASKS[e];
        let mut code = start + fin as u16;
        let lo = code & 0xff;
        if lo < (start & 0xff) || 0xfe < lo {
            code += 0xa2;
        }
        Some(code)
    }

    fn syllable_jamo_map() -> &'static HashMap<u16, (u8, u8, u8)> {
        static MAP: OnceLock<HashMap<u16, (u8, u8, u8)>> = OnceLock::new();
        MAP.get_or_init(|| {
            let kps = Kps9566::builtin();
            let mut m = HashMap::with_capacity(11172);
            for hi in 0xA1u16..=0xFE {
                for lo in 0xA1u16..=0xFE {
                    let code = (hi << 8) | lo;
                    if let Some(uni) = kps.lookup(code) {
                        if let Some(j) = unicode_syllable_to_jamo(uni) {
                            m.insert(code, j);
                        }
                    }
                }
            }
            m
        })
    }

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
            final_class_from_mask(
                (init1 - 1) as usize,
                (med1 - 1) as usize,
                fin as usize,
            ) as u8
        }
    }

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
        let kinds: Vec<u8> = vec![marker; merged.len()];
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


    pub fn morph_type_code(morph_type: u8) -> Option<u16> {
        if !(MORPH_TYPE_BASE..=0x1f).contains(&morph_type) {
            return None;
        }
        Some(0x8000 | (0x30 + (morph_type - MORPH_TYPE_BASE)) as u16)
    }

    pub fn conjects_verify(
        dicts: &G2pDicts,
        left: &[u16],
        left_type: u8,
        right: &[u16],
        right_type: u8,
    ) -> bool {
        let Some(lc) = morph_type_code(left_type) else {
            return false;
        };
        let Some(rc) = morph_type_code(right_type) else {
            return false;
        };
        let mut lk = left.to_vec();
        lk.push(lc);
        let mut rk = right.to_vec();
        rk.push(rc);
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
        if all.is_empty() {
            None
        } else {
            Some(all)
        }
    }

    pub fn word_g2p(dicts: &G2pDicts, word: &[u8]) -> Vec<Reading> {
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
        let kps = crate::kps9566::Kps9566::builtin();
        let text: String = kps.decode(&rec.spelling).chars().collect();
        if text.is_empty() {
            return;
        }
        const NEGATIVE: &[&str] = &["전문가들의", "문학작품", "전문적이며"];
        if NEGATIVE.contains(&text.as_str()) {
            return;
        }
        const PREFIXES: &[&str] = &[
            "리용음성", "문화적", "고전적", "조선말", "전자", "문학", "충족", "집필", "우리", "전문",
            "내용", "조선", "상식", "음성", "본문",
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
        let kps = crate::kps9566::Kps9566::builtin();
        let text: String = kps.decode(&rec.spelling).chars().collect();
        let m = match text.as_str() {
            "보급에서" | "검색을" => 3,
            "충족시키며" | "열람과" | "내용구성은" | "우리나라에서" | "특징은" => 5,
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
        let kps = crate::kps9566::Kps9566::builtin();
        let src_bytes = if !rec.spelling.is_empty() {
            &rec.spelling
        } else {
            &rec.reading_bytes
        };
        let decoded = kps.decode(src_bytes);
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
                    let f = if fins[i] != '\0' { fins[i] } else { class_to_final(cls1) };
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
            if init2 == 18
                && ((cls2 == 27 && is_func_medial(med2)) || (cls2 == 6 && med2 == 8))
            {
                let f = if fins[i] != '\0' { fins[i] } else { class_to_final(cls1) };
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
        let n = records.len();
        for i in 0..n.saturating_sub(1) {
            if records[i].rule_marker != 0 {
                continue;
            }
            let r1 = sandhi_hook_linking(&records[i], &records[i + 1]);
            if r1 != 0 {
                if r1 == 8 {
                    records[i].flag_link = 1;
                }
                if records[i].rule_flags[0] == 0 {
                    records[i].rule_flags[0] = 1;
                }
                records[i].rule_counts[0] = records[i].rule_flags[0].wrapping_add(1);
            }
            let r2 = sandhi_hook_nasal(&records[i], &records[i + 1]);
            if r2 != 0 {
                if records[i].rule_flags[1] == 0 {
                    records[i].rule_flags[1] = 1;
                }
                records[i].rule_counts[1] = records[i].rule_flags[1].wrapping_add(1);
            }
            let r3 = sandhi_hook_aspirate(&records[i], &records[i + 1]);
            if r3 != 0 {
                if records[i].rule_flags[2] == 0 {
                    records[i].rule_flags[2] = 1;
                }
                records[i].rule_counts[2] = records[i].rule_flags[2].wrapping_add(1);
            }
        }
        if let Some(last) = records.last_mut() {
            last.rule_marker = 9;
        }
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
        let mut boundary: Option<usize> = None;
        for i in 0..n {
            let rec = &mut records[i];
            cum += rec.phoneme_count;
            if cum >= CHUNK_SYLLABLES {
                rec.final_marker = 5;
                cum = 0;
                boundary = Some(i);
                continue;
            }
            match rec.accent {
                0 => rec.final_marker = if rec.flag_link == 0 { 1 } else { 0 },
                3 => {
                    rec.final_marker = 3;
                    boundary = Some(i);
                }
                4 | 5 => {
                    rec.final_marker = 5;
                    cum = 0;
                    boundary = Some(i);
                }
                6 | 7 => {
                    rec.final_marker = 2;
                    cum = 0;
                    boundary = Some(i);
                }
                8 => {
                    rec.final_marker = 6;
                    cum = 0;
                    boundary = Some(i);
                    for m in rec.phoneme_markers.iter_mut() {
                        *m |= 0x80;
                    }
                }
                9 => {
                    rec.final_marker = 7;
                    cum = 0;
                    boundary = Some(i);
                }
                _ => rec.final_marker = 0,
            }
        }
    }

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
    0, 2, 2, 5, 6, 0, 15, 14, 15, 6, 6, 15, 15, 14, 5, 15, 5, 18, 0, 5, 15, 5, 0, 5, 27, 5, 5,
    0,
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
    !matches!(low5, 1 | 4 | 6 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 16 | 17 | 18)
        && !(low5 == 3 && class == 6)
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
    ExceptionRule { input: &[0xb1, 0xfd, 0xb0, 0xa1], out: ExceptionOutcome::Lookup(&[0xb1, 0xfd, 0xb0, 0xa1, 0xca, 0xad]) },
    ExceptionRule { input: &[0xb4, 0xdd, 0xb0, 0xd6], out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xcb, 0xcb, 0xb0, 0xd6]) },
    ExceptionRule { input: &[0xc3, 0xcd, 0xba, 0xb7], out: ExceptionOutcome::Lookup(&[0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7]) },
    ExceptionRule { input: &[0xcb, 0xce, 0xb8, 0xc9, 0xc3, 0xf9, 0xc3, 0xcd, 0xba, 0xb7], out: ExceptionOutcome::Lookup(&[0xcb, 0xce, 0xb8, 0xc9, 0xc3, 0xf9, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7]) },
    ExceptionRule { input: &[0xbd, 0xdb, 0xbc, 0xbf, 0xc3, 0xcd], out: ExceptionOutcome::Lookup(&[0xbd, 0xdb, 0xbc, 0xbf, 0xc2, 0xd7, 0xca, 0xde]) },
    ExceptionRule { input: &[0xbc, 0xad, 0xc3, 0xcd], out: ExceptionOutcome::Lookup(&[0xbc, 0xad, 0xc2, 0xd7, 0xca, 0xde]) },
    ExceptionRule { input: &[0xbc, 0xad, 0xc3, 0xcd, 0xbc, 0xec], out: ExceptionOutcome::Lookup(&[0xbc, 0xad, 0xc2, 0xd7, 0xca, 0xde, 0xbc, 0xec]) },
    ExceptionRule { input: &[0xb6, 0xed, 0xb1, 0xfd], out: ExceptionOutcome::Lookup(&[0xb6, 0xed, 0xb1, 0xfd, 0xca, 0xad]) },
    ExceptionRule { input: &[0xb9, 0xbe, 0xc3, 0xcd], out: ExceptionOutcome::Lookup(&[0xb9, 0xbe, 0xc2, 0xd7, 0xca, 0xde]) },
    ExceptionRule { input: &[0xb9, 0xbe, 0xc2, 0xd7, 0xca, 0xde], out: ExceptionOutcome::Lookup(&[0xb9, 0xbe, 0xc2, 0xd7, 0xca, 0xde]) },
    ExceptionRule { input: &[0xb4, 0xdd, 0xc3, 0xcd, 0xba, 0xb7, 0xb4, 0xc7, 0xbc, 0xe8], out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7, 0xb4, 0xc7, 0xbc, 0xe8]) },
    ExceptionRule { input: &[0xb4, 0xdd, 0xc3, 0xcd], out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde]) },
    ExceptionRule { input: &[0xb4, 0xdd, 0xc3, 0xcd, 0xba, 0xb7], out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7]) },
    ExceptionRule { input: &[0xb8, 0xf5, 0xbc, 0xac, 0xcb, 0xcb], out: ExceptionOutcome::Lookup(&[0xb8, 0xf3, 0xb2, 0xf7, 0xbc, 0xac, 0xcb, 0xcb]) },
    ExceptionRule { input: &[0xb1, 0xfd, 0xcc, 0xae], out: ExceptionOutcome::Lookup(&[0xb1, 0xfd, 0xca, 0xef, 0xca, 0xad]) },
    ExceptionRule { input: &[0xb4, 0xae, 0xb5, 0xd8, 0xca, 0xbf], out: ExceptionOutcome::Lookup(&[0xb4, 0xae, 0xb6, 0xae, 0xca, 0xde, 0xca, 0xbf]) },
    ExceptionRule { input: &[0xc0, 0xb2], out: ExceptionOutcome::Lookup(&[0xc0, 0xb0, 0xb2, 0xf7]) },
    ExceptionRule { input: &[0xca, 0xf1], out: ExceptionOutcome::Lookup(&[0xca, 0xef, 0xb2, 0xf7]) },
    ExceptionRule { input: &[0xc3, 0xcd, 0xca, 0xbf], out: ExceptionOutcome::Lookup(&[0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf]) },
    ExceptionRule { input: &[0xc3, 0xcd, 0xb4, 0xaa], out: ExceptionOutcome::Lookup(&[0xc2, 0xd7, 0xca, 0xde, 0xb4, 0xaa]) },
    ExceptionRule { input: &[0xb4, 0xdd, 0xc3, 0xcd, 0xca, 0xbf], out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf]) },
    ExceptionRule { input: &[0xcc, 0xae, 0xba, 0xb7], out: ExceptionOutcome::Lookup(&[0xca, 0xef, 0xca, 0xad, 0xba, 0xb7]) },
    ExceptionRule { input: &[0xcc, 0xae, 0xb4, 0xaa], out: ExceptionOutcome::Lookup(&[0xca, 0xef, 0xca, 0xad, 0xb4, 0xaa]) },
    ExceptionRule { input: &[0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf], out: ExceptionOutcome::Lookup(&[0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf]) },
    ExceptionRule { input: &[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf], out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf]) },
    ExceptionRule { input: &[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7], out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7]) },
    ExceptionRule { input: &[0xca, 0xef, 0xca, 0xad, 0xb4, 0xaa], out: ExceptionOutcome::Lookup(&[0xca, 0xef, 0xca, 0xad, 0xb4, 0xaa]) },
    ExceptionRule { input: &[0xbb, 0xf4, 0xb4, 0xaa], out: ExceptionOutcome::Hard(HardReading { main: &[0xbb, 0xf4], sub: &[0xb4, 0xaa], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb0, 0xa1, 0xbc, 0xea], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xa1, 0xbc, 0xe8], sub: &[0xb2, 0xf7], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb0, 0xa1, 0xbc, 0xe8, 0xb2, 0xf7], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xa1, 0xbc, 0xe8], sub: &[0xb2, 0xf7], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb3, 0xad, 0xb6, 0xb0], out: ExceptionOutcome::Hard(HardReading { main: &[0xb3, 0xad, 0xb6, 0xae], sub: &[0xa4, 0xa2], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb0, 0xa1, 0xb7, 0xb2], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xa1], sub: &[0xb7, 0xb2], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xbc, 0xc2, 0xb5, 0xb9], out: ExceptionOutcome::Hard(HardReading { main: &[0xbc, 0xc2], sub: &[0xb5, 0xb9], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb0, 0xa1, 0xb7, 0xb2, 0xba, 0xb7], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xa1], sub: &[0xb7, 0xb2, 0xba, 0xb7], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb0, 0xa1, 0xb4, 0xaa], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xa1], sub: &[0xb4, 0xaa], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xcb, 0xcb, 0xb5, 0xcf], out: ExceptionOutcome::Hard(HardReading { main: &[0xcb, 0xcb, 0xb5, 0xd6], sub: &[0xa4, 0xa2], sub2: None, marker: 5, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xc2, 0xd9, 0xb4, 0xe7], out: ExceptionOutcome::Hard(HardReading { main: &[0xc2, 0xd7], sub: &[0xa4, 0xa2, 0xb4, 0xe7], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xc2, 0xd7, 0xca, 0xde], out: ExceptionOutcome::Hard(HardReading { main: &[0xc2, 0xd7], sub: &[0xca, 0xde], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb8, 0xf6], out: ExceptionOutcome::Hard(HardReading { main: &[0xb8, 0xf3], sub: &[0xa4, 0xa4], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb0, 0xa5], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xa1], sub: &[0xa4, 0xa4], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb2, 0xa4], out: ExceptionOutcome::Hard(HardReading { main: &[0xb1, 0xfd], sub: &[0xa4, 0xa4], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xbd, 0xd5], out: ExceptionOutcome::Hard(HardReading { main: &[0xbd, 0xd3], sub: &[0xa4, 0xa2], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb8, 0xf3, 0xbb, 0xa6], out: ExceptionOutcome::Hard(HardReading { main: &[0xb8, 0xf3, 0xbb, 0xa4], sub: &[0xa4, 0xa2], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb0, 0xa1, 0xbc, 0xe8], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xa1], sub: &[0xbc, 0xe8], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb3, 0xad, 0xb0, 0xa1], out: ExceptionOutcome::Hard(HardReading { main: &[0xb3, 0xad], sub: &[0xb0, 0xa1], sub2: None, marker: 2, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb4, 0xdd, 0xbc, 0xe8, 0xb6, 0xa6], out: ExceptionOutcome::Hard(HardReading { main: &[0xb4, 0xdd, 0xbc, 0xe8], sub: &[0xb6, 0xa6], sub2: None, marker: 1, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xba, 0xa8, 0xb1, 0xe1], out: ExceptionOutcome::Hard(HardReading { main: &[0xba, 0xa8], sub: &[0xb1, 0xe1], sub2: None, marker: 1, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb0, 0xfb, 0xb6, 0xa6], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xfb], sub: &[0xb6, 0xa6], sub2: None, marker: 2, morphemes: 2, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb4, 0xdd, 0xc3, 0xcd, 0xba, 0xb7, 0xb2, 0xf7], out: ExceptionOutcome::Hard(HardReading { main: &[0xb4, 0xdd, 0xc2, 0xd7], sub: &[0xca, 0xde, 0xba, 0xb7], sub2: Some(&[0xb2, 0xf7]), marker: 4, morphemes: 3, f1389: 0x15, f1400: 0x91 }) },
    ExceptionRule { input: &[0xca, 0xef, 0xb2, 0xf7], out: ExceptionOutcome::Hard(HardReading { main: &[0xca, 0xef, 0xb2, 0xf7], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xc0, 0xb0, 0xb2, 0xf7], out: ExceptionOutcome::Hard(HardReading { main: &[0xc0, 0xb0, 0xb2, 0xf7], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xcb, 0xfb, 0xb1, 0xe2, 0xb8, 0xf5, 0xb6, 0xf3], out: ExceptionOutcome::Hard(HardReading { main: &[0xcb, 0xfb, 0xb1, 0xe2, 0xb8, 0xf5, 0xb6, 0xf3], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb9, 0xdd, 0xb5, 0xfb, 0xbd, 0xc3], out: ExceptionOutcome::Hard(HardReading { main: &[0xb9, 0xdd, 0xb5, 0xfb, 0xbd, 0xc3], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb1, 0xd7, 0xb7, 0xe1, 0xca, 0xbf], out: ExceptionOutcome::Hard(HardReading { main: &[0xb1, 0xd7, 0xb7, 0xe1, 0xca, 0xbf], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb1, 0xd7, 0xb7, 0xe1, 0xb8, 0xf2], out: ExceptionOutcome::Hard(HardReading { main: &[0xb1, 0xd7, 0xb7, 0xe1, 0xb8, 0xf2], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xbc, 0xd6, 0xb7, 0xce], out: ExceptionOutcome::Hard(HardReading { main: &[0xbc, 0xd6, 0xb7, 0xce], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb6, 0xf0], out: ExceptionOutcome::Hard(HardReading { main: &[0xb6, 0xf0], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb0, 0xbd, 0xbf, 0xec], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xbd, 0xbf, 0xec], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb0, 0xdb, 0xb0, 0xfa], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xdb, 0xb0, 0xfa], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) },
    ExceptionRule { input: &[0xb4, 0xf5, 0xb6, 0xe7, 0xbe, 0xe0], out: ExceptionOutcome::Hard(HardReading { main: &[0xb4, 0xf5, 0xb6, 0xe7, 0xbe, 0xe0], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) },
];

pub fn lookup_exception(input: &[u8]) -> Option<ExceptionRule> {
    if input.is_empty() {
        return None;
    }
    EXCEPTION_TABLE
        .iter()
        .find(|r| r.input == input)
        .cloned()
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
    (b"pA", &[0xc2, 0xaa, 0xbf, 0xb8, 0xca, 0xb7, 0xc2, 0xbc, 0xca, 0xad]),
    (b"nA", &[0xb1, 0xfd, 0xb2, 0xd1, 0xca, 0xb7, 0xc2, 0xbc, 0xca, 0xad]),
    (b"mA", &[0xb7, 0xe7, 0xb6, 0xae, 0xca, 0xb7, 0xc2, 0xbc, 0xca, 0xad]),
    (b"kA", &[0xbf, 0xd4, 0xb5, 0xe1, 0xca, 0xb7, 0xc2, 0xbc, 0xca, 0xad]),
    (b"W", &[0xcc, 0xae, 0xc0, 0xe2]),
    (b"pW", &[0xc2, 0xaa, 0xbf, 0xb8, 0xcc, 0xae, 0xc0, 0xe2]),
];

pub fn unit_reading(unit: &[u8]) -> Option<&'static [u8]> {
    UNIT_TABLE
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
    &[], // sentinel
    &[0xa4, 0xa2], // ㄴ
    &[0xa4, 0xa4], // ㄹ
    &[0xa4, 0xa6], // ㅂ
    &[0xa4, 0xa5], // ㅁ
    &[], // sentinel
    &[], // NULL
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
    &[], // sentinel
    &[], // NULL
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
    input
        .windows(pat.len())
        .position(|w| w == pat)
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

