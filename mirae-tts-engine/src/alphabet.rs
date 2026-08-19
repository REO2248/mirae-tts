//! Alphabet/letter reading dispatch — 0x46598c / 0x466d34 tables.
//!
//! Implements letter-by-letter reading for ASCII letters and Korean jamo characters.
//! Activated when morph_type ∈ {0x1f, 0x20, 0x22, 0x23, 0x24, 0x25}.

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::g2p::g2p_dict::Reading;

// ASCII_LETTER_READINGS — 0x46598c equivalent (1-byte → letter name reading)
pub static ASCII_LETTER_READINGS: [&[u8]; 26] = [
    &[0xcbu8, 0xe6, 0xcb, 0xcb],             // a → 에이
    &[0xb9u8, 0xbe],                         // b → 비
    &[0xc8u8, 0xc1],                         // c → 씨
    &[0xb4u8, 0xd1],                         // d → 디
    &[0xcbu8, 0xcb],                         // e → 이
    &[0xcbu8, 0xe6, 0xc2, 0xa3],             // f → 에프
    &[0xbdu8, 0xb8],                         // g → 지
    &[0xcbu8, 0xe6, 0xbe, 0xde],             // h → 에이치
    &[0xcau8, 0xad, 0xcb, 0xcb],             // i → 아이
    &[0xbdu8, 0xa3, 0xcb, 0xcb],             // j → 제이
    &[0xbfu8, 0xe8, 0xcb, 0xcb],             // k → 케이
    &[0xcbu8, 0xe9],                         // l → 엘
    &[0xcbu8, 0xea],                         // m → 엠
    &[0xcbu8, 0xe8],                         // n → 엔
    &[0xcau8, 0xef, 0xcb, 0xa7],             // o → 오
    &[0xc2u8, 0xaa],                         // p → 피
    &[0xbfu8, 0xc9],                         // q → 큐
    &[0xcau8, 0xad, 0xb6, 0xa3],             // r → 알
    &[0xcbu8, 0xe6, 0xc8, 0xb8],             // s → 에스
    &[0xc0u8, 0xec],                         // t → 티
    &[0xcbu8, 0xb1],                         // u → 우
    &[0xb9u8, 0xb6, 0xcb, 0xcb],             // v → 브이
    &[0xb3u8, 0xf3, 0xb9, 0xa6, 0xcb, 0xb1], // w → 더블유
    &[0xcbu8, 0xe7, 0xc8, 0xb8],             // x → 엑스
    &[0xccu8, 0xae, 0xcb, 0xcb],             // y → 와이
    &[0xbdu8, 0xa3, 0xc0, 0xe2],             // z → 제트
];

pub fn ascii_letter_reading(code: u8) -> Option<&'static [u8]> {
    let lower = code | 0x20;
    if lower >= b'a' && lower <= b'z' {
        Some(ASCII_LETTER_READINGS[(lower - b'a') as usize])
    } else {
        None
    }
}

// TWO_BYTE_READINGS — 0x466d34 equivalent (2-byte KPS → letter reading)
pub static TWO_BYTE_READINGS: LazyLock<HashMap<u16, &'static [u8]>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(0xA4A1, &[0xBDu8, 0xBC, 0xC7, 0xEC] as &[u8]); // ㄱ → 기역
    m.insert(0xA4A2, &[0xC5u8, 0xE0, 0xC8, 0xC4]); // ㄴ → 니은
    m.insert(0xA4A3, &[0xB4u8, 0xD1, 0xB8, 0xF2]); // ㄷ → 디귿
    m.insert(0xA4A4, &[0xC0u8, 0xEC, 0xC8, 0xC4]); // ㄹ → 리을
    m.insert(0xA4A5, &[0xBBu8, 0xE8, 0xC8, 0xB0]); // ㅁ → 미음
    m.insert(0xA4A6, &[0xB9u8, 0xBE, 0xC8, 0xB8]); // ㅂ → 비읍
    m.insert(0xA4A7, &[0xC0u8, 0xAF, 0xC6, 0xF7]); // ㅅ → 시옷
    m.insert(0xA4A8, &[0xC8u8, 0xB0, 0xC8, 0xB0]); // ㅇ → 이응
    m.insert(0xA4A9, &[0xBDu8, 0xBC, 0xC8, 0xB0]); // ㅈ → 지읒
    m.insert(0xA4AA, &[0xC7u8, 0xCF, 0xC8, 0xB0]); // ㅊ → 치읓
    m.insert(0xA4AB, &[0xBFu8, 0xA4, 0xC8, 0xB0]); // ㅋ → 키읔
    m.insert(0xA4AC, &[0xC0u8, 0xEC, 0xC8, 0xB0]); // ㅌ → 티읕
    m.insert(0xA4AD, &[0xC2u8, 0xAA, 0xC8, 0xB8]); // ㅍ → 피읍
    m.insert(0xA4AE, &[0xC7u8, 0xE5, 0xC8, 0xB0]); // ㅎ → 히읗
    m.insert(0xA5A1, &[0xC5u8, 0xB8]); // ㅏ → 아
    m.insert(0xA5A2, &[0xC5u8, 0xB8]); // ㅑ → 야
    m.insert(0xA5A7, &[0xC5u8, 0xB4]); // ㅓ → 어
    m.insert(0xA5A8, &[0xC5u8, 0xB8]); // ㅕ → 여
    m.insert(0xA5A9, &[0xC6u8, 0xF7]); // ㅗ → 오
    m.insert(0xA5AA, &[0xC6u8, 0xF7]); // ㅛ → 요
    m.insert(0xA5B7, &[0xC6u8, 0xB4]); // ㅜ → 우
    m.insert(0xA5B8, &[0xC6u8, 0xB8]); // ㅠ → 유
    m.insert(0xA5B9, &[0xC8u8, 0xB0]); // ㅡ → 으
    m.insert(0xA5BA, &[0xC8u8, 0xB0]); // ㅣ → 이
    m
});

pub fn two_byte_reading(code: u16) -> Option<&'static [u8]> {
    TWO_BYTE_READINGS.get(&code).copied()
}

pub fn is_letter_reading_type(morph_type: u8) -> bool {
    matches!(morph_type, 0x1f | 0x20 | 0x22 | 0x23 | 0x24 | 0x25)
}

fn has_ascii_alpha(bytes: &[u8]) -> bool {
    bytes.iter().any(|&b| b.is_ascii_alphabetic())
}

fn ascii_letter_by_letter(word: &[u8]) -> Vec<Reading> {
    let mut readings = Vec::with_capacity(word.len());
    for &b in word {
        let lower = b | 0x20;
        if lower >= b'a' && lower <= b'z' {
            let reading = ASCII_LETTER_READINGS[(lower - b'a') as usize];
            readings.push(Reading {
                bytes: reading.to_vec(),
                packed: None,
                marker: 1,
            });
        } else {
            readings.push(Reading::fallback(&[b]));
        }
    }
    readings
}

pub fn letter_reading_dispatch(word: &[u8]) -> Vec<Reading> {
    let mut readings = Vec::with_capacity(word.len());
    let mut i = 0;
    while i < word.len() {
        let b = word[i];
        if b < 0x80 {
            let lower = b | 0x20;
            if lower >= b'a' && lower <= b'z' {
                let reading = ASCII_LETTER_READINGS[(lower - b'a') as usize];
                readings.push(Reading {
                    bytes: reading.to_vec(),
                    packed: None,
                    marker: 1,
                });
            } else {
                readings.push(Reading::fallback(&[b]));
            }
            i += 1;
        } else if b >= 0xA1 && i + 1 < word.len() {
            let code = ((b as u16) << 8) | (word[i + 1] as u16);
            if let Some(reading) = two_byte_reading(code) {
                readings.push(Reading {
                    bytes: reading.to_vec(),
                    packed: None,
                    marker: 1,
                });
            } else {
                readings.push(Reading::fallback(&[b, word[i + 1]]));
            }
            i += 2;
        } else {
            readings.push(Reading::fallback(&[b]));
            i += 1;
        }
    }
    readings
}
