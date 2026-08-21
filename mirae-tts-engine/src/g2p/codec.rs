//! KPS ↔ jamo / phoneme-code conversions and the lazy jamo lookup maps.
//! All tables come from the original Future.exe dumps (kps_tables / digit_tables).
use std::collections::HashMap;
use std::sync::OnceLock;

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

pub(super) fn kps_to_jamo_kp(kps: u16) -> Option<(u8, u8, u8)> {
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
                if let Some(uni) = crate::pipeline::kps_lookup(code)
                    && let Some(j) = unicode_syllable_to_jamo(uni as u32)
                {
                    m.insert(code, j);
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
