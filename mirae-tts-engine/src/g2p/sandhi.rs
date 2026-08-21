//! Phoneme sandhi rules and the stage7 prosody / stage8 final-marker passes
//! over [`WordRecord`]s (byte-exact port; stage6 is a no-op in the original).
use super::*;

pub const S_FIN_CHARS: [char; 28] = [
    '\0', 'ㄱ', 'ㄲ', 'ㄳ', 'ㄴ', 'ㄵ', 'ㄶ', 'ㄷ', 'ㄹ', 'ㄺ', 'ㄻ', 'ㄼ', 'ㄽ', 'ㄾ', 'ㄿ', 'ㅀ',
    'ㅁ', 'ㅂ', 'ㅄ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ',
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
    let decoded = crate::pipeline::kps_decode(src_bytes);
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
        // Original FUN_00440470 computes both smoothings in x87 80-bit
        // registers with a single rounding to f32 at each store (fstp).
        // Reproduce with f64 intermediates; m_prev/m_next enter via fild
        // (integer load) at full precision.
        let m_prev = if i > 0 {
            records[i - 1].rule_marker as f64
        } else {
            0.0
        };
        let m_next = if i + 1 < n {
            records[i + 1].rule_marker as f64
        } else {
            0.0
        };
        let w1 = PROSODY_W1 as f64;
        let w3 = PROSODY_W3 as f64;
        let p0 = records[i].prosody[0] as f64;
        let p2 = records[i].prosody[2] as f64;
        // s1 = W1*(m_prev+m_next) + (1-W1)*p0   [80-bit chain]
        let s1 = w1 * (m_prev + m_next) + (1.0 - w1) * p0;
        // s2 = W3*s1 + (1-W3)*p2               [80-bit chain, s1 unrounded]
        let s2 = w3 * s1 + (1.0 - w3) * p2;
        // single rounding per stored field (fstp dword)
        let s1f = s1 as f32;
        let s2f = s2 as f32;
        records[i].prosody[0] = s1f;
        records[i].prosody[1] = s1f;
        records[i].prosody[2] = s2f;
        if records[i].rule_marker != 0 {
            if records[i].rule_marker < 4 {
                // accent compare uses the rounded stored s2 (fstp before fcomp)
                let (lo, hi) = ACCENT_RANGE;
                records[i].accent = if !(lo..=hi).contains(&s2f) { 3 } else { 0 };
            } else {
                records[i].accent = records[i].rule_marker;
            }
        }
    }
    let last = records.last_mut().unwrap();
    last.accent = last.rule_marker;
}

pub fn stage8_final_markers(records: &mut [WordRecord]) {
    let mut cum = 0usize;
    for rec in records.iter_mut() {
        cum += rec.phoneme_count;
        if cum >= CHUNK_SYLLABLES {
            rec.final_marker = 5;
            cum = 0;
            continue;
        }
        match rec.accent {
            0 => rec.final_marker = if rec.flag_link == 0 { 1 } else { 0 },
            3 => {
                rec.final_marker = 3;
            }
            4 | 5 => {
                rec.final_marker = 5;
                cum = 0;
            }
            6 | 7 => {
                rec.final_marker = 2;
                cum = 0;
            }
            8 => {
                rec.final_marker = 6;
                cum = 0;
                for m in rec.phoneme_markers.iter_mut() {
                    *m |= 0x80;
                }
            }
            9 => {
                rec.final_marker = 7;
                cum = 0;
            }
            _ => rec.final_marker = 0,
        }
    }
}
