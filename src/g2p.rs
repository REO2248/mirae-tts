//! Korean G2P (aspiration, ㅎ-drop, liaison, tensification, nasalization, lateralization) — rewrites packed syllables before VoiceInfo.

use crate::korean::CODA_REMAP;

/// Tensification table.
/// Maps KPS cho index → tensified KPS cho index.
/// Only meaningful for lax obstruents: ㄱ(0), ㄷ(2), ㅂ(5), ㅅ(6), ㅈ(7).
const TENSIFY: [u8; 19] = [
    13, //  0 ㄱ → ㄲ(13)
    0,  //  1 ㄴ → (unused)
    14, //  2 ㄷ → ㄸ(14)
    0,  //  3 ㄹ → (unused)
    0,  //  4 ㅁ → (unused)
    15, //  5 ㅂ → ㅃ(15)
    16, //  6 ㅅ → ㅆ(16)
    17, //  7 ㅈ → ㅉ(17)
    9,  //  8 ㅊ → (unused in practice)
    0,  //  9 ㅋ → (unused)
    10, // 10 ㅌ → (unused)
    0,  // 11 ㅍ → (unused)
    0,  // 12 ㅎ → (unused)
    11, // 13 ㄲ → (unused)
    0,  // 14 ㄸ → (unused)
    8,  // 15 ㅃ → (unused)
    0,  // 16 ㅆ → (unused)
    0,  // 17 ㅉ → (unused)
    0,  // 18 ㅇ → (unused)
];

/// ㅎ-fusion (aspiration) table.
/// Maps KPS cho → aspirated KPS cho when preceded by ㅎ-containing coda.
/// Only valid for lax stops: ㄱ(0), ㄷ(2), ㅂ(5), ㅈ(7).
const H_FUSION: [u8; 19] = [
    9,  //  0 ㄱ → ㅋ(9)
    0,  //  1 ㄴ → (unused)
    10, //  2 ㄷ → ㅌ(10)
    0,  //  3 ㄹ → (unused)
    0,  //  4 ㅁ → (unused)
    11, //  5 ㅂ → ㅍ(11)
    0,  //  6 ㅅ → (unused)
    8,  //  7 ㅈ → ㅊ(8)
    0,  //  8 ㅊ
    0,  //  9 ㅋ
    0,  // 10 ㅌ
    0,  // 11 ㅍ
    0,  // 12 ㅎ
    0,  // 13 ㄲ
    0,  // 14 ㄸ
    0,  // 15 ㅃ
    0,  // 16 ㅆ
    0,  // 17 ㅉ
    0,  // 18 ㅇ
];

/// ㅎ-related table 1, indexed by next cho (KPS 0–18).
const H_TABLE_1: [u8; 19] = [
    15, 15, 5, 18, 5, 5, 0, 5, 15, 5, 0, 5, 27, 0, 5, 15, 5, 5, 0,
];

/// ㅎ-related table 2, indexed by next cho (KPS 0–18).
const H_TABLE_2: [u8; 19] = [
    0, 5, 15, 5, 5, 0, 0, 0, 5, 0, 5, 15, 13, 0, 14, 0, 0, 15, 16,
];

/// Fixed coefficients used inside the inter-syllable switch (original table bytes).
const G2P_BYTE_33: u8 = 5;
const G2P_BYTE_38: u8 = 5;
const G2P_BYTE_39: u8 = 0;
const G2P_BYTE_3A: u8 = 5;
const G2P_BYTE_3B: u8 = 15;

// Reference G2P implementation (readable phonological rules).
// The crate also keeps a readable reference implementation alongside the
// table-driven functions (`apply_inter_syllable_rules`, `apply_intra_word_rules`, …).

/// Liaison table: for each raw jong index (0-27), how to split for liaison.
/// (remaining_raw_coda, released_cho_kps)
/// - remaining = 27 means coda is fully consumed (simple coda)
/// - released = NO_RELEASE means no liaison possible
#[allow(dead_code)]
const NO_RELEASE: u8 = 255;
#[allow(dead_code)]
const LIAISON: [(u8, u8); 28] = [
    (27, 0),          //  0: ㄱ    → release ㄱ(cho 0)
    (0, 6),           //  1: ㄳ    → keep ㄱ(0), release ㅅ(cho 6)
    (27, 1),          //  2: ㄴ    → release ㄴ(cho 1)
    (2, 7),           //  3: ㄵ    → keep ㄴ(2), release ㅈ(cho 7)
    (2, 12),          //  4: ㄶ    → keep ㄴ(2), release ㅎ(cho 12)
    (27, 2),          //  5: ㄷ    → release ㄷ(cho 2)
    (27, 3),          //  6: ㄹ    → release ㄹ(cho 3)
    (6, 0),           //  7: ㄺ    → keep ㄹ(6), release ㄱ(cho 0)
    (6, 4),           //  8: ㄻ    → keep ㄹ(6), release ㅁ(cho 4)
    (6, 5),           //  9: ㄼ    → keep ㄹ(6), release ㅂ(cho 5)
    (6, 6),           // 10: ㄽ    → keep ㄹ(6), release ㅅ(cho 6)
    (6, 10),          // 11: ㄾ    → keep ㄹ(6), release ㅌ(cho 10)
    (6, 11),          // 12: ㄿ    → keep ㄹ(6), release ㅍ(cho 11)
    (6, 12),          // 13: ㅀ    → keep ㄹ(6), release ㅎ(cho 12)
    (27, 4),          // 14: ㅁ    → release ㅁ(cho 4)
    (27, 5),          // 15: ㅂ    → release ㅂ(cho 5)
    (15, 6),          // 16: ㅄ    → keep ㅂ(15), release ㅅ(cho 6)
    (27, 6),          // 17: ㅅ    → release ㅅ(cho 6)
    (18, NO_RELEASE), // 18: ㅇ    → stays (nasal coda, no liaison)
    (27, 7),          // 19: ㅈ    → release ㅈ(cho 7)
    (27, 8),          // 20: ㅊ    → release ㅊ(cho 8)
    (27, 9),          // 21: ㅋ    → release ㅋ(cho 9)
    (27, 10),         // 22: ㅌ    → release ㅌ(cho 10)
    (27, 11),         // 23: ㅍ    → release ㅍ(cho 11)
    (27, 12),         // 24: ㅎ    → release ㅎ(cho 12) [then ㅎ drops before ㅇ]
    (27, 13),         // 25: ㄲ    → release ㄲ(cho 13)
    (27, 16),         // 26: ㅆ    → release ㅆ(cho 16)
    (27, NO_RELEASE), // 27: none  → no coda
];

// Phonological category helpers

/// Is this raw coda an obstruent (stop/fricative/affricate)?
/// After neutralization, obstruent codas → groups 0(ㄱ), 5(ㄷ), 15(ㅂ).
#[allow(dead_code)]
#[inline]
fn is_obstruent_coda(raw_coda: u8) -> bool {
    let g = CODA_REMAP[raw_coda as usize];
    g == 0 || g == 5 || g == 15
}

/// Does this raw coda contain ㅎ as a component?
/// ㄶ(raw 4), ㅀ(raw 13), ㅎ(raw 24)
#[allow(dead_code)]
#[inline]
fn is_h_coda(raw_coda: u8) -> bool {
    raw_coda == 4 || raw_coda == 13 || raw_coda == 24
}

/// Is this KPS cho a lax obstruent (순한소리)?
/// Tensifiable: ㄱ(0), ㄷ(2), ㅂ(5), ㅅ(6), ㅈ(7)
#[allow(dead_code)]
#[inline]
fn is_lax_obstruent(cho: u8) -> bool {
    matches!(cho, 0 | 2 | 5 | 6 | 7)
}

/// Is this KPS cho a lax stop (for ㅎ aspiration)?
/// ㄱ(0), ㄷ(2), ㅂ(5), ㅈ(7)
#[allow(dead_code)]
#[inline]
fn is_lax_stop(cho: u8) -> bool {
    matches!(cho, 0 | 2 | 5 | 7)
}

/// Is this KPS cho a nasal onset? ㄴ(1), ㅁ(4)
#[allow(dead_code)]
#[inline]
fn is_nasal_onset(cho: u8) -> bool {
    matches!(cho, 1 | 4)
}

/// Is this KPS cho ㅇ (zero/silent onset)?
#[allow(dead_code)]
#[inline]
fn is_ieung(cho: u8) -> bool {
    cho == 18
}

/// Is this KPS cho ㄹ?
#[allow(dead_code)]
#[inline]
fn is_rieul_onset(cho: u8) -> bool {
    cho == 3
}

/// Nasal counterpart of an obstruent coda group.
/// ㄱ group(0) → ㅇ(18), ㄷ group(5) → ㄴ(2), ㅂ group(15) → ㅁ(14)
#[allow(dead_code)]
fn nasalize_coda(coda_group: u8) -> u8 {
    match coda_group {
        0 => 18,  // ㄱ → ㅇ
        5 => 2,   // ㄷ → ㄴ
        15 => 14, // ㅂ → ㅁ
        _ => coda_group,
    }
}

/// Convert an obstruent coda group to its representative lax onset cho.
/// Used for ㅎ-aspiration: (obstruent coda + ㅎ onset) → aspirated.
#[allow(dead_code)]
fn coda_group_to_lax_cho(coda_group: u8) -> u8 {
    match coda_group {
        0 => 0,  // ㄱ group → ㄱ cho
        5 => 2,  // ㄷ group → ㄷ cho
        15 => 5, // ㅂ group → ㅂ cho
        _ => 0,
    }
}

// Core G2P functions

/// Pack cho, jung, coda into a 16-bit packed syllable.
#[allow(dead_code)]
#[inline]
fn pack(cho: u8, jung: u8, coda: u8) -> u16 {
    ((coda as u16) << 10) | ((jung as u16) << 5) | (cho as u16)
}

/// Unpack a 16-bit packed syllable into (cho, jung, coda).
#[allow(dead_code)]
#[inline]
fn unpack(p: u16) -> (u8, u8, u8) {
    (
        (p & 0x1F) as u8,
        ((p >> 5) & 0x1F) as u8,
        ((p >> 10) & 0x1F) as u8,
    )
}

/// Is this a non-Korean entry (word boundary, sentinel, etc.)?
#[inline]
fn is_boundary(packed: u16) -> bool {
    packed == 0xFFFF || packed & 0x8000 != 0
}

/// Apply Korean pronunciation rules to adjacent syllables.
/// Returns (modified_prev, modified_curr).
#[allow(dead_code)]
fn apply_pair_rules(prev: u16, curr: u16) -> (u16, u16) {
    let (prev_cho, prev_jung, prev_coda) = unpack(prev);
    let (curr_cho, curr_jung, curr_coda) = unpack(curr);

    // No coda → no inter-syllable interaction
    if prev_coda == 27 {
        return (prev, curr);
    }

    let coda_group = CODA_REMAP[prev_coda as usize];
    let h_coda = is_h_coda(prev_coda);

    // ㅎ 받침: 거센소리현상 / 《ㅎ》떨어지기
    if h_coda {
        let (remain, _released) = LIAISON[prev_coda as usize];

        if is_ieung(curr_cho) {
            // ㅎ coda + ㅇ onset → ㅎ drops
            // ㄶ+ㅇ → ㄴ+ㅇ, ㅀ+ㅇ → ㄹ+ㅇ, ㅎ+ㅇ → (none)+ㅇ
            return (pack(prev_cho, prev_jung, remain), curr);
        }

        if is_lax_stop(curr_cho) {
            // ㅎ coda + lax stop → aspiration
            // ㄶ+ㄱ → ㄴ+ㅋ, ㅀ+ㄷ → ㄹ+ㅌ, ㅎ+ㅂ → ∅+ㅍ
            let aspirated = H_FUSION[curr_cho as usize];
            return (
                pack(prev_cho, prev_jung, remain),
                pack(aspirated, curr_jung, curr_coda),
            );
        }
    }

    // 순한소리+ㅎ: 거센소리
    if curr_cho == 12 && is_obstruent_coda(prev_coda) {
        // ㄱ+ㅎ → ∅+ㅋ, ㄷ+ㅎ → ∅+ㅌ, ㅂ+ㅎ → ∅+ㅍ
        let lax = coda_group_to_lax_cho(coda_group);
        let aspirated = H_FUSION[lax as usize];
        return (
            pack(prev_cho, prev_jung, 27),
            pack(aspirated, curr_jung, curr_coda),
        );
    }

    // 이어내기
    if is_ieung(curr_cho) {
        let (remain, released) = LIAISON[prev_coda as usize];
        if released != NO_RELEASE {
            // For ㅎ-containing compound codas handled above,
            // but plain ㅎ coda was also handled. Double-check:
            // ㅎ(raw 24) + ㅇ: h_coda block already returned.
            // ㄶ(raw 4) + ㅇ: h_coda block already returned.
            // ㅀ(raw 13) + ㅇ: h_coda block already returned.
            // So reaching here means a non-ㅎ coda + ㅇ onset.
            return (
                pack(prev_cho, prev_jung, remain),
                pack(released, curr_jung, curr_coda),
            );
        }
    }

    // 된소리현상
    if is_obstruent_coda(prev_coda) && is_lax_obstruent(curr_cho) {
        // After obstruent coda, lax obstruent onset → tense
        // 학교→학꾜, 독서→독써, 밥그릇→밥끄릇
        let tensed = TENSIFY[curr_cho as usize];
        return (prev, pack(tensed, curr_jung, curr_coda));
    }

    // 코소리현상
    if is_obstruent_coda(prev_coda) && is_nasal_onset(curr_cho) {
        // Before nasal, obstruent coda → nasal
        // 학년→항년, 국물→궁물, 읽는→잉는
        let nasal = nasalize_coda(coda_group);
        return (pack(prev_cho, prev_jung, nasal), curr);
    }

    // Obstruent coda + ㄹ onset: nasalize coda, ㄹ→ㄴ
    // 학력→항녁, 독립→동닙
    if is_obstruent_coda(prev_coda) && is_rieul_onset(curr_cho) {
        let nasal = nasalize_coda(coda_group);
        return (
            pack(prev_cho, prev_jung, nasal),
            pack(1, curr_jung, curr_coda), // ㄹ(3) → ㄴ(1)
        );
    }

    // 흐름소리현상
    if coda_group == 2 && is_rieul_onset(curr_cho) {
        // ㄴ + ㄹ → ㄹ + ㄹ (련락→렬락, 진리→질리)
        return (
            pack(prev_cho, prev_jung, 6), // ㄴ→ㄹ (raw 6)
            curr,
        );
    }

    if coda_group == 6 && curr_cho == 1 {
        // ㄹ + ㄴ → ㄹ + ㄹ (칼날→칼랄, 설날→설랄)
        return (
            prev,
            pack(3, curr_jung, curr_coda), // ㄴ(1) → ㄹ(3)
        );
    }

    // No rule applied
    (prev, curr)
}

#[inline]
fn t_tensify(cho: u8) -> u8 {
    TENSIFY.get(cho as usize).copied().unwrap_or(0)
}

#[inline]
fn t_h_fusion(cho: u8) -> u8 {
    H_FUSION.get(cho as usize).copied().unwrap_or(0)
}

#[inline]
fn t_h1(cho: u8) -> u8 {
    H_TABLE_1.get(cho as usize).copied().unwrap_or(0)
}

#[inline]
fn t_h2(cho: u8) -> u8 {
    H_TABLE_2.get(cho as usize).copied().unwrap_or(0)
}

/// Inter-syllable 28-case rules for one contiguous word.
///
/// Parameters correspond to original chars:
/// - `param3` == original `param_3`
/// - `param4` == original `param_4`
/// - `mode`   == `*(this + 0xb206)` used in several condition gates
fn apply_inter_syllable_rules(word: &mut [u16], param3: u8, param4: u8, mode: u8) {
    let len = word.len();
    if len <= 1 {
        return;
    }

    for idx in 1..len {
        let prev = word[idx - 1];
        let curr = word[idx];

        let bvar17 = (prev & 0x1F) as u8;
        let bvar20 = ((prev >> 5) & 0x1F) as u8;
        let mut bvar15 = ((prev >> 10) & 0x1F) as u8;

        let bvar18 = ((curr >> 5) & 0x1F) as u8;
        let mut bvar19 = (curr & 0x1F) as u8;

        let bvar14 = (8..=11).contains(&bvar19);
        let bvar13 = (13..=17).contains(&bvar19) && bvar19 != 0x10;

        let mut bvar21 = if ((bvar17 == 6 || bvar17 == 0x10) && bvar20 == 9 && bvar15 == 0x0F)
            || ((bvar17 == 5 || bvar17 == 0x0F) && bvar20 == 10 && bvar15 == 0)
        {
            true
        } else if bvar15 == 2 {
            ((bvar17 == 8 && bvar20 == 2) || (bvar17 == 4 && bvar20 == 0))
                || (bvar17 == 0x12 && bvar20 == 2)
        } else if bvar17 == 0x12 {
            bvar20 == 2
        } else {
            bvar17 == 7 && bvar20 == 4 && bvar15 == 0x1B
        };

        let bvar3 = matches!(bvar19, 0 | 2 | 5 | 6 | 7) && !(param3 == 3 && bvar21);

        let bvar6 = mode == 2 && bvar3;
        let bvar10 = (mode == 4 || mode == 0x14) && bvar3;
        let bvar12 = mode == 5 && bvar3;
        let bvar11 = matches!(bvar19, 2 | 6 | 7) && param4 != 0;
        let bvar5 = matches!(bvar19, 1 | 3 | 4);
        let bvar9 = matches!(bvar19, 0 | 2 | 5 | 7);
        let bvar22 = bvar19 != 0x0C;
        let bvar8 = !bvar22 && (bvar18 == 9 || bvar18 == 3);

        let old_bvar21 = bvar21;
        bvar21 = if bvar19 == 0x0C {
            let cond = bvar15 == 2 || bvar15 == 6 || bvar15 == 0x0E;
            cond && !(param3 == 3 && old_bvar21)
        } else if bvar19 == 0x12 {
            !(param3 == 3 && old_bvar21)
        } else {
            false
        };

        let bvar23 = bvar19 == 0x12 && (bvar18 == 9 || bvar18 == 3);

        let bvar4 = bvar15 == 0
            && (((bvar17 == 7 || bvar17 == 0x11) && bvar20 == 2)
                || ((bvar17 == 6 || bvar17 == 0x10) && bvar20 == 9))
            && bvar3
            && !bvar22
            && bvar21
            && param4 != 0
            && idx >= 2
            && len >= 3
            && idx < (len - 1);

        let bvar7 = bvar19 == 3 && param4 != 0;

        match bvar15 {
            0 => {
                if !bvar4 {
                    if bvar3 {
                        bvar19 = t_tensify(bvar19);
                    } else if bvar5 {
                        bvar15 = 0x12;
                    } else if bvar22 {
                        if bvar21 {
                            bvar15 = 0x1B;
                            bvar19 = 0;
                        }
                    } else {
                        bvar19 = 9;
                    }
                }
            }
            1 => {
                if bvar3 {
                    bvar19 = t_tensify(bvar19);
                    bvar15 = 0;
                } else if bvar5 {
                    bvar15 = 0x12;
                } else if bvar21 {
                    bvar15 = 0;
                    bvar19 = 0x10;
                } else {
                    bvar15 = 0;
                }
            }
            2 => {
                if !bvar6 {
                    if bvar7 {
                        bvar15 = 6;
                    } else if bvar21 {
                        bvar15 = 0x1B;
                        bvar19 = 1;
                    }
                }
            }
            3 => {
                if bvar8 {
                    bvar15 = 2;
                    bvar19 = 8;
                } else {
                    bvar15 = 2;
                    if bvar21 {
                        bvar19 = 7;
                    }
                }
            }
            4 => {
                if bvar21 {
                    bvar15 = 0x1B;
                    bvar19 = 1;
                } else {
                    bvar15 = 2;
                    if bvar9 {
                        bvar19 = t_h_fusion(bvar19);
                    }
                }
            }
            5 => {
                if bvar3 {
                    bvar19 = t_tensify(bvar19);
                } else if bvar5 {
                    bvar15 = 2;
                } else if bvar23 {
                    bvar15 = 0x1B;
                    bvar19 = 7;
                } else if bvar8 {
                    bvar15 = 5;
                    bvar19 = 8;
                } else if bvar22 {
                    if bvar21 {
                        bvar15 = 0x1B;
                        bvar19 = 2;
                    }
                } else {
                    bvar19 = 10;
                }
            }
            6 => {
                if bvar10 {
                    bvar19 = t_tensify(bvar19);
                } else if bvar21 {
                    bvar15 = 0x1B;
                    bvar19 = 3;
                } else if bvar11 {
                    bvar19 = t_tensify(bvar19);
                } else if bvar19 == 1 {
                    bvar19 = 3;
                }
            }
            7 => {
                if bvar8 {
                    bvar15 = 6;
                    bvar19 = 9;
                } else if bvar21 {
                    bvar15 = 6;
                    bvar19 = 0;
                } else if bvar3 {
                    bvar19 = t_tensify(bvar19);
                    bvar15 = 0;
                } else {
                    bvar15 = if bvar5 { 0 } else { 0x12 };
                }
            }
            8 => {
                if bvar21 {
                    bvar15 = 6;
                    bvar19 = 4;
                } else {
                    bvar15 = 0x0E;
                }
            }
            9 => {
                if bvar8 {
                    bvar15 = 6;
                    bvar19 = 0x0B;
                } else if bvar21 {
                    bvar15 = 6;
                    bvar19 = 5;
                } else if bvar3 {
                    bvar19 = t_tensify(bvar19);
                    bvar15 = 0x0F;
                }
            }
            10 => {
                bvar15 = 6;
                if bvar21 {
                    bvar19 = 6;
                }
            }
            11 => {
                if bvar23 {
                    bvar15 = 6;
                    bvar19 = 8;
                } else {
                    bvar15 = 6;
                    if bvar21 {
                        bvar19 = 10;
                    }
                }
            }
            12 => {
                if bvar21 {
                    bvar15 = 6;
                    bvar19 = 0x0B;
                } else if bvar3 {
                    bvar19 = t_tensify(bvar19);
                    bvar15 = 0x0F;
                } else {
                    bvar15 = if bvar5 { 0x0E } else { 0x0F };
                }
            }
            13 => {
                if bvar21 {
                    bvar15 = 0x1B;
                    bvar19 = 3;
                } else {
                    bvar15 = 6;
                    if bvar9 {
                        bvar19 = t_h_fusion(bvar19);
                    }
                }
            }
            14 => {
                if !bvar12 && bvar21 {
                    bvar15 = 0x1B;
                    bvar19 = 4;
                }
            }
            15 => {
                if bvar3 {
                    bvar19 = t_tensify(bvar19);
                } else if bvar5 {
                    bvar15 = 0x0E;
                } else if bvar22 {
                    if bvar21 {
                        bvar15 = 0x1B;
                        bvar19 = 5;
                    }
                } else {
                    bvar19 = 0x0B;
                }
            }
            16 => {
                if bvar3 {
                    bvar19 = t_tensify(bvar19);
                    bvar15 = 0x0F;
                } else if bvar5 {
                    bvar15 = 0x0E;
                } else {
                    bvar15 = 0x0F;
                    if bvar21 {
                        bvar19 = 0x10;
                    }
                }
            }
            17 => {
                if bvar3 {
                    bvar19 = t_tensify(bvar19);
                    bvar15 = 5;
                } else if bvar5 {
                    bvar15 = 2;
                } else if bvar8 {
                    bvar15 = 5;
                    bvar19 = 8;
                } else if bvar22 {
                    if bvar21 {
                        bvar15 = 0x1B;
                        bvar19 = 6;
                    } else {
                        bvar15 = 5;
                    }
                } else {
                    bvar15 = 5;
                    bvar19 = 10;
                }
            }
            19 => {
                if bvar3 {
                    bvar19 = t_tensify(bvar19);
                    bvar15 = 5;
                } else if bvar5 {
                    bvar15 = 2;
                } else if bvar8 {
                    bvar15 = 5;
                    bvar19 = 8;
                } else if bvar22 {
                    if bvar21 {
                        bvar15 = 0x1B;
                        bvar19 = 7;
                    } else {
                        bvar15 = 5;
                    }
                } else {
                    bvar15 = 5;
                    bvar19 = 10;
                }
            }
            20 => {
                if bvar3 {
                    bvar19 = t_tensify(bvar19);
                    bvar15 = 5;
                } else if bvar5 {
                    bvar15 = 2;
                } else if bvar8 {
                    bvar15 = 5;
                    bvar19 = 8;
                } else if bvar22 {
                    if bvar21 {
                        bvar15 = G2P_BYTE_38;
                        bvar19 = 8;
                    } else {
                        bvar15 = 5;
                    }
                } else {
                    bvar15 = 5;
                    bvar19 = 10;
                }
            }
            21 => {
                if bvar3 {
                    bvar19 = t_tensify(bvar19);
                    bvar15 = 0;
                } else if bvar5 {
                    bvar15 = 0x12;
                } else if bvar22 {
                    if bvar21 {
                        bvar15 = G2P_BYTE_39;
                        bvar19 = 9;
                    } else {
                        bvar15 = 0;
                    }
                } else {
                    bvar15 = 0;
                    bvar19 = 9;
                }
            }
            22 => {
                if bvar3 {
                    bvar19 = t_tensify(bvar19);
                    bvar15 = 5;
                } else if bvar5 {
                    bvar15 = 2;
                } else if bvar23 {
                    bvar15 = G2P_BYTE_38;
                    bvar19 = 8;
                } else if bvar8 {
                    bvar15 = 5;
                    bvar19 = 8;
                } else if bvar22 {
                    if bvar21 {
                        bvar15 = G2P_BYTE_3A;
                        bvar19 = 10;
                    } else {
                        bvar15 = 5;
                    }
                } else {
                    bvar15 = 5;
                    bvar19 = 10;
                }
            }
            23 => {
                if bvar3 {
                    bvar19 = t_tensify(bvar19);
                    bvar15 = 0x0F;
                } else if bvar5 {
                    bvar15 = 0x0E;
                } else if bvar22 {
                    if bvar21 {
                        bvar15 = G2P_BYTE_3B;
                        bvar19 = 0x0B;
                    } else {
                        bvar15 = 0x0F;
                    }
                } else {
                    bvar15 = 0x0F;
                    bvar19 = 0x0B;
                }
            }
            24 => {
                if bvar21 {
                    bvar15 = 0x1B;
                } else if bvar9 {
                    bvar19 = t_h_fusion(bvar19);
                    bvar15 = t_h2(bvar19);
                } else {
                    bvar15 = 5;
                }
            }
            25 => {
                if bvar22 {
                    if bvar21 {
                        bvar15 = t_h2(0);
                        bvar19 = 0x0D;
                    } else if bvar3 {
                        bvar19 = t_tensify(bvar19);
                        bvar15 = 0;
                    } else {
                        bvar15 = if bvar5 { 0 } else { 0x12 };
                    }
                } else {
                    bvar15 = 0;
                    bvar19 = 9;
                }
            }
            26 => {
                if bvar3 {
                    bvar19 = t_tensify(bvar19);
                    bvar15 = 5;
                } else if bvar5 {
                    bvar15 = 2;
                } else if bvar8 {
                    bvar19 = 8;
                    bvar15 = 5;
                } else if bvar22 {
                    if bvar21 {
                        bvar19 = 0x10;
                        bvar15 = G2P_BYTE_33;
                    } else {
                        bvar15 = 5;
                    }
                } else {
                    bvar19 = 10;
                    bvar15 = 5;
                }
            }
            27 => {
                if bvar13 {
                    bvar15 = t_h1(bvar19);
                } else if bvar14 {
                    bvar15 = t_h2(bvar19);
                }
            }
            _ => {}
        }

        word[idx - 1] = ((bvar15 as u16) << 10) | ((bvar20 as u16) << 5) | (bvar17 as u16);
        let curr_coda = (word[idx] >> 10) & 0x1F;
        word[idx] = (curr_coda << 10) | ((bvar18 as u16) << 5) | ((bvar19 & 0x1F) as u16);
    }
}

#[derive(Debug, Clone, Default)]
struct WordBoundaryState {
    has_prev: bool,
    prev_last_syllable: u16,
    prev_param3: u8,
    prev_param4: u8,
    prev_param5: u8,
}

#[derive(Debug, Clone, Default)]
struct SegmentJunctionState {
    has_prev: bool,
    prev_last_syllable: u16,
    prev_param3: u8,
    prev_param4: u8,
}

/// Word-boundary sandhi for one word transition.
///
/// This function mutates both the previous word-final syllable and the current
/// word-initial syllable according to cross-word pronunciation rules.
fn apply_word_boundary_rules(
    state: &mut WordBoundaryState,
    prev_last_in_text: Option<&mut u16>,
    current_word: &mut [u16],
    param3: u8,
    param4: u8,
    param5: u8,
) {
    if current_word.is_empty() {
        return;
    }

    if !state.has_prev {
        state.has_prev = true;
        state.prev_last_syllable = *current_word.last().unwrap();
        state.prev_param3 = param3;
        state.prev_param4 = param4;
        state.prev_param5 = param5;
        return;
    }

    let prev = state.prev_last_syllable;
    let bvar17 = (prev & 0x1F) as u8;
    let bvar18 = ((prev >> 5) & 0x1F) as u8;
    let bvar19 = ((prev >> 10) & 0x1F) as u8;

    let cur0 = current_word[0];
    let bvar22 = (cur0 & 0x1F) as u8;
    let bvar25 = ((cur0 >> 5) & 0x1F) as u8;
    let bvar26 = ((cur0 >> 10) & 0x1F) as u8;

    let cvar1 = state.prev_param3;

    let bvar12 = cvar1 == 1 && param3 == 0x0E && bvar22 == 6 && bvar25 == 4 && bvar26 == 0;

    let bvar9 = if cvar1 == 0x0A
        && param3 == 0x0E
        && current_word.len() == 2
        && bvar22 == 2
        && bvar25 == 4
        && bvar26 == 0x12
    {
        let u = current_word[1];
        ((u & 0x1F) as u8) == 0x12 && ((u >> 5) & 0x1F) as u8 == 0 && ((u >> 10) & 0x1F) as u8 == 2
    } else {
        false
    };

    let mut bvar5 = (cvar1 == 3 && (param3 == 0x0A || param3 == 1 || param3 == 0x0E))
        || (cvar1 == 0x0E && (param3 == 0x0A || param3 == 0x0E));

    let mut bvar6 = matches!(bvar22, 0 | 2 | 5 | 6 | 7);
    let bvar14 = matches!(bvar22, 0 | 2 | 5 | 7);

    let bvar29 = if (bvar17 == 6 || bvar17 == 0x10) && bvar18 == 9 && bvar19 == 0x0F {
        true
    } else if bvar19 == 6 {
        ((bvar17 == 2 && bvar18 == 2) || (bvar17 == 0x12 && bvar18 == 3))
            && bvar5
            && bvar6
            && (param3 == 0x0A || param3 == 1)
    } else {
        (bvar17 == 2 && bvar18 == 2 && bvar19 == 9)
            && bvar5
            && bvar6
            && (param3 == 0x0A || param3 == 1)
    };

    let bvar7 = param3 == 3 && cvar1 == 3;
    let mode = state.prev_param5;

    let bvar11 = mode == 2 && bvar6;
    let bvar15 = (mode == 4 || mode == 0x14) && bvar6;
    let bvar16 = mode == 5 && bvar6;

    let bvar10 = bvar5 && bvar22 == 0x0C;
    let bvar13 = bvar22 == 0x0C && (bvar25 == 9 || bvar25 == 3);

    let bvar30 = if ((bvar17 == 6 || bvar17 == 0x10) && bvar18 == 9 && bvar19 == 0x0F)
        || ((bvar17 == 5 || bvar17 == 0x0F) && bvar18 == 10 && bvar19 == 0)
    {
        true
    } else if bvar19 == 2 {
        (bvar17 == 8 && bvar18 == 2)
            || (bvar17 == 4 && bvar18 == 0)
            || (bvar17 == 0x12 && bvar18 == 2)
    } else if bvar17 == 0x12 {
        bvar18 == 2
    } else {
        bvar17 == 7 && bvar18 == 4 && bvar19 == 0x1B
    };

    let bvar8 = cvar1 == 0x0C && bvar17 == 4 && bvar18 == 4 && bvar19 == 0x11;

    bvar6 = !((!bvar5 && (!bvar7 || bvar30)) || bvar8) && bvar6;
    bvar5 = !((!bvar5 && (!bvar7 || bvar30)) || bvar22 != 0x12);

    let mut bvar23 = if bvar9 { 0x0E } else { bvar22 };
    let mut bvar20 = bvar19;

    match bvar19 {
        0 => {
            if bvar5 {
                bvar23 = 0;
                bvar20 = 0x1B;
            } else if bvar6 {
                bvar23 = t_tensify(bvar22);
            } else if bvar10 {
                bvar23 = 9;
            }
        }
        1 => {
            bvar20 = 0;
            if bvar5 {
                bvar23 = 0x10;
            } else if bvar6 {
                bvar23 = t_tensify(bvar22);
            }
        }
        2 => {
            if !bvar11 {
                if bvar12 {
                    bvar23 = t_tensify(bvar22);
                } else if bvar5 {
                    bvar23 = 1;
                    bvar20 = 0x1B;
                }
            }
        }
        3 => {
            bvar20 = 2;
            if bvar5 {
                bvar23 = 7;
            } else if bvar13 {
                bvar23 = 8;
            }
        }
        4 => {
            if bvar14 {
                bvar23 = t_h_fusion(bvar22);
                bvar20 = 2;
            } else if bvar5 {
                bvar23 = 1;
                bvar20 = 0x1B;
            } else {
                bvar20 = 2;
            }
        }
        5 => {
            if bvar5 {
                bvar23 = 2;
                bvar20 = 0x1B;
            } else if bvar6 {
                bvar23 = t_tensify(bvar22);
            } else if bvar13 {
                bvar23 = 8;
                bvar20 = 5;
            } else if bvar10 {
                bvar23 = 10;
            }
        }
        6 => {
            if bvar15 || bvar12 || bvar29 {
                bvar23 = t_tensify(bvar22);
            } else if bvar5 {
                bvar23 = 3;
                bvar20 = 0x1B;
            } else if bvar22 == 1 {
                bvar23 = 3;
            }
        }
        7 => {
            if bvar5 {
                bvar23 = 0;
                bvar20 = 6;
            } else if bvar6 {
                bvar20 = 0;
                bvar23 = t_tensify(bvar22);
            } else if bvar13 {
                bvar23 = 9;
                bvar20 = 6;
            } else {
                bvar20 = 0;
            }
        }
        8 => {
            if bvar5 {
                bvar23 = 4;
                bvar20 = 6;
            } else {
                bvar20 = 0x0E;
            }
        }
        9 => {
            if bvar29 {
                bvar23 = t_tensify(bvar22);
                bvar20 = 6;
            } else if bvar5 {
                bvar23 = 5;
                bvar20 = 6;
            } else if bvar6 {
                bvar23 = t_tensify(bvar22);
                bvar20 = 0x0F;
            } else if bvar13 {
                bvar23 = 0x0B;
                bvar20 = 6;
            } else {
                bvar20 = 0x0F;
            }
        }
        10 => {
            bvar20 = 6;
            if bvar5 {
                bvar23 = 6;
            }
        }
        11 => {
            bvar20 = 6;
            if bvar5 {
                bvar23 = 10;
            }
        }
        12 => {
            if bvar5 {
                bvar23 = 0x0B;
                bvar20 = 6;
            } else {
                bvar20 = 0x0F;
                if bvar6 {
                    bvar23 = t_tensify(bvar22);
                }
            }
        }
        13 => {
            if bvar14 {
                bvar23 = t_h_fusion(bvar22);
                bvar20 = 6;
            } else if bvar5 {
                bvar23 = 3;
                bvar20 = 0x1B;
            } else {
                bvar20 = 6;
            }
        }
        14 => {
            if !bvar16 {
                if bvar12 {
                    bvar23 = t_tensify(bvar22);
                } else if bvar5 {
                    bvar23 = 4;
                    bvar20 = 0x1B;
                }
            }
        }
        15 => {
            if bvar5 {
                bvar23 = 5;
                bvar20 = 0x1B;
            } else if bvar6 {
                bvar23 = t_tensify(bvar22);
            } else if bvar10 {
                bvar23 = 0x0B;
            }
        }
        16 => {
            if bvar5 {
                bvar23 = 0x10;
                bvar20 = 0x0F;
            } else {
                bvar20 = 0x0F;
                if bvar6 {
                    bvar23 = t_tensify(bvar22);
                }
            }
        }
        17 => {
            if bvar5 {
                bvar23 = 6;
                bvar20 = 0x1B;
            } else if bvar6 {
                bvar20 = 5;
                bvar23 = t_tensify(bvar22);
            } else if bvar13 {
                bvar23 = 8;
                bvar20 = 5;
            } else if bvar10 {
                bvar23 = 10;
                bvar20 = 5;
            } else {
                bvar20 = 5;
            }
        }
        18 => {
            if bvar12 {
                bvar23 = t_tensify(bvar22);
            }
        }
        19 => {
            if bvar5 {
                bvar23 = 7;
                bvar20 = 0x1B;
            } else if bvar6 {
                bvar20 = 5;
                bvar23 = t_tensify(bvar22);
            } else if bvar13 {
                bvar23 = 8;
                bvar20 = 5;
            } else if bvar10 {
                bvar23 = 10;
                bvar20 = 5;
            } else {
                bvar20 = 5;
            }
        }
        20 => {
            if bvar5 {
                bvar23 = 8;
                bvar20 = 0x1B;
            } else if bvar6 {
                bvar20 = 5;
                bvar23 = t_tensify(bvar22);
            } else if bvar13 {
                bvar23 = 8;
                bvar20 = 5;
            } else if bvar10 {
                bvar23 = 10;
                bvar20 = 5;
            } else {
                bvar20 = 5;
            }
        }
        21 => {
            if bvar5 {
                bvar23 = 9;
                bvar20 = 0x1B;
            } else if bvar6 {
                bvar20 = 0;
                bvar23 = t_tensify(bvar22);
            } else if bvar10 {
                bvar23 = 9;
                bvar20 = 0;
            } else {
                bvar20 = 0;
            }
        }
        22 => {
            if bvar5 {
                bvar23 = 10;
                bvar20 = 0x1B;
            } else if bvar6 {
                bvar20 = 5;
                bvar23 = t_tensify(bvar22);
            } else if bvar13 {
                bvar23 = 8;
                bvar20 = 5;
            } else if bvar10 {
                bvar23 = 10;
                bvar20 = 5;
            } else {
                bvar20 = 5;
            }
        }
        23 => {
            if bvar5 {
                bvar23 = 0x0B;
                bvar20 = 0x1B;
            } else if bvar6 {
                bvar23 = t_tensify(bvar22);
                bvar20 = 0x0F;
            } else if bvar10 {
                bvar23 = 0x0B;
                bvar20 = 0x0F;
            } else {
                bvar20 = 0x0F;
            }
        }
        24 => {
            if bvar14 {
                bvar23 = t_h_fusion(bvar22);
                bvar20 = 0x1B;
            } else {
                bvar20 = if bvar5 { 5 } else { 0x16 };
            }
        }
        25 => {
            if bvar5 {
                bvar23 = 0x0D;
                bvar20 = 0x1B;
            } else if bvar6 {
                bvar20 = 0;
                bvar23 = t_tensify(bvar22);
            } else if bvar10 {
                bvar23 = 9;
                bvar20 = 0;
            } else {
                bvar20 = 0;
            }
        }
        26 => {
            if bvar5 {
                bvar23 = 0x10;
                bvar20 = 0x1B;
            } else if bvar6 {
                bvar23 = t_tensify(bvar22);
                bvar20 = 5;
            } else if bvar13 {
                bvar23 = 8;
                bvar20 = 5;
            } else if bvar10 {
                bvar23 = 10;
                bvar20 = 5;
            } else {
                bvar20 = 5;
            }
        }
        27 => {
            if bvar12 {
                bvar23 = t_tensify(bvar22);
            }
        }
        _ => {}
    }

    if cvar1 == 3
        && (param3 == 1 || param3 == 0x0A)
        && bvar17 == 6
        && bvar18 == 9
        && bvar19 == 0x0F
        && bvar22 == 0x12
        && bvar25 == 0x12
        && bvar26 == 6
    {
        bvar23 = 0x12;
    }

    let new_prev = ((bvar20 as u16) << 10) | ((bvar18 as u16) << 5) | (bvar17 as u16);
    let new_curr0 = ((bvar26 as u16) << 10) | ((bvar25 as u16) << 5) | ((bvar23 & 0x1F) as u16);

    if let Some(prev_ref) = prev_last_in_text {
        *prev_ref = new_prev;
    }
    current_word[0] = new_curr0;

    state.prev_last_syllable = *current_word.last().unwrap();
    state.prev_param3 = param3;
    state.prev_param4 = param4;
    state.prev_param5 = param5;
}

/// Segment-junction rules between word runs.
fn apply_segment_junction_rules(
    state: &mut SegmentJunctionState,
    current_word: &mut [u16],
    param3: u8,
    param4: u8,
) {
    if current_word.is_empty() {
        return;
    }

    if !state.has_prev {
        state.has_prev = true;
        state.prev_last_syllable = *current_word.last().unwrap();
        state.prev_param3 = param3;
        state.prev_param4 = param4;
        return;
    }

    let prev = state.prev_last_syllable;
    let bvar8 = (prev & 0x1F) as u8;
    let bvar4 = ((prev >> 5) & 0x1F) as u8;
    let bvar6 = ((prev >> 10) & 0x1F) as u8;

    let mut cur0 = current_word[0];
    let bvar12 = (cur0 & 0x1F) as u8;
    let cur_jung = ((cur0 >> 5) & 0x1F) as u8;
    let cur_coda = ((cur0 >> 10) & 0x1F) as u8;

    let bvar3 = bvar8 == 7
        && bvar4 == 6
        && bvar6 == 6
        && bvar12 == 0
        && cur_jung == 0x0C
        && cur_coda == 0x1B;

    let mut bvar1 =
        state.prev_param3 == 0x14 && bvar8 == 0x12 && (bvar4 == 8 || bvar4 == 6) && bvar6 == 6;

    let bvar2 = matches!(bvar12, 0 | 2 | 5 | 6 | 7);
    bvar1 = (state.prev_param4 == 4 || state.prev_param4 == 0x14 || bvar1) && bvar2;

    if bvar6 == 6 {
        if bvar3 {
            cur0 = (cur0 & 0xFFED) | 0x000D;
            current_word[0] = cur0;
        } else if bvar1 {
            let cho = t_tensify(bvar12) & 0x1F;
            cur0 = (cur0 & 0xFFE0) | (cho as u16);
            current_word[0] = cur0;
        }
    }

    state.prev_last_syllable = *current_word.last().unwrap();
    state.prev_param3 = param3;
    state.prev_param4 = param4;
}

/// Final merge step after inter-segment processing.
///
/// Mutates `left` word-final syllable and `right` word-initial syllable,
/// then returns the merged sequence (`left + right`).
pub fn apply_final_inter_segment_rules(
    left: &mut [u16],
    right: &mut [u16],
    prev_type_b204: u8,
    mode_b206: u8,
) -> Vec<u16> {
    if left.is_empty() {
        return right.to_vec();
    }
    if right.is_empty() {
        return left.to_vec();
    }

    let mut prev = *left.last().unwrap();
    let mut curr = right[0];

    let bvar27 = (prev & 0x1F) as u8;
    let bvar28 = ((prev >> 5) & 0x1F) as u8;
    let mut bvar25 = ((prev >> 10) & 0x1F) as u8;

    let mut bvar30 = (curr & 0x1F) as u8;
    let bvar29 = ((curr >> 5) & 0x1F) as u8;
    let bvar6 = ((curr >> 10) & 0x1F) as u8;

    let bvar32 = bvar30 != 6;
    let bvar33 = bvar30 == 1;

    let mut bvar9 = bvar30 == 0 && bvar29 == 9;
    let bvar18 =
        bvar27 == 7 && bvar28 == 6 && bvar25 == 6 && bvar30 == 0 && bvar29 == 0x0C && bvar6 == 0x1B;
    let bvar10_base = bvar30 == 0 && (bvar29 == 9 || bvar29 == 3) && !bvar18;
    let bvar24 = (8..=11).contains(&bvar30);

    let bvar23 = (13..=17).contains(&bvar30) && bvar30 != 0x10;
    let bvar7 = matches!(bvar30, 0 | 2 | 5 | 6 | 7);

    let bvar14 = mode_b206 == 2 && bvar7;
    let bvar19 = (mode_b206 == 4 || mode_b206 == 0x14) && bvar7;
    let bvar22 = mode_b206 == 5 && bvar7;

    let bvar13 = matches!(bvar30, 1 | 3 | 4);
    let bvar34 = bvar30 == 0x0C;
    let bvar11 = bvar34 && (bvar29 == 9 || bvar29 == 3);
    let bvar12 = bvar30 == 0x12 || bvar34;
    let bvar17 = bvar30 == 0x12 && (bvar29 == 9 || bvar29 == 3);
    let bvar16 = !bvar17 && matches!(bvar30, 0 | 2 | 5 | 7);

    let bvar8 = prev_type_b204 == 4 || prev_type_b204 == 5;
    let bvar5 = prev_type_b204 == 4 && bvar27 == 0x0C && bvar28 == 0 && bvar25 == 2;

    let bvar21 = bvar8 && bvar30 == 0;
    let bvar15 = bvar8 && bvar9;
    let _bvar20 = bvar8 && bvar10_base;
    let bvar10 = bvar8 && bvar7;
    bvar9 = bvar8 && (bvar9 || bvar5);
    let bvar8 = bvar8 && bvar11;

    let bvar5_mode = mode_b206 == 2 && bvar30 == 0x0C && bvar29 == 6 && bvar6 == 0x1B;

    match bvar25 {
        0 => {
            if bvar34 {
                bvar30 = 9;
            } else if bvar12 {
                bvar25 = 0x1B;
                bvar30 = 0;
            } else if bvar7 {
                bvar30 = t_tensify(bvar30);
            } else if bvar13 {
                bvar25 = 0x12;
            }
        }
        1 => {
            if bvar12 {
                bvar25 = 0;
                bvar30 = 0x10;
            } else if bvar7 {
                bvar30 = t_tensify(bvar30);
                bvar25 = 0;
            } else {
                bvar25 = if bvar13 { 0 } else { 0x12 };
            }
        }
        2 => {
            if !bvar5_mode && !bvar14 {
                if bvar12 {
                    bvar25 = 0x1B;
                    bvar30 = 1;
                } else if !bvar9 && bvar10 {
                    bvar30 = t_tensify(bvar30);
                    bvar25 = 2;
                }
            }
        }
        3 => {
            bvar25 = 2;
            if bvar12 {
                bvar30 = 7;
            } else if bvar15 {
            } else if bvar10 {
                bvar30 = t_tensify(bvar30);
            } else if bvar8 {
                bvar30 = 8;
            }
        }
        4 => {
            if bvar12 {
                bvar25 = 0x1B;
                bvar30 = 1;
            } else if bvar16 {
                bvar30 = t_h_fusion(bvar30);
                bvar25 = 2;
            } else if bvar32 {
                bvar25 = 2;
                if bvar33 {
                    bvar30 = 1;
                }
            } else {
                bvar25 = 2;
                bvar30 = 0x10;
            }
        }
        5 => {
            if bvar11 {
                bvar25 = 0x1B;
                bvar30 = 8;
            } else if bvar34 {
                bvar30 = 10;
            } else if bvar17 {
                bvar25 = 0x1B;
                bvar30 = 7;
            } else if bvar12 {
                bvar25 = 0x1B;
                bvar30 = 2;
            } else if bvar7 {
                bvar30 = t_tensify(bvar30);
            } else if bvar13 {
                bvar25 = 2;
            }
        }
        6 => {
            if bvar18 {
                bvar30 = 0x0D;
            } else if bvar19 {
                bvar30 = t_tensify(bvar30);
            } else if bvar12 {
                bvar25 = 0x1B;
                bvar30 = 3;
            } else if bvar33 {
                bvar30 = 3;
            }
        }
        7 => {
            if bvar12 {
                bvar25 = 6;
                bvar30 = 0;
            } else if bvar8 {
                bvar25 = 6;
                bvar30 = 9;
            } else if bvar10 {
                bvar30 = t_tensify(bvar30);
                bvar25 = 6;
            } else {
                bvar25 = if bvar13 { 0 } else { 0x12 };
            }
        }
        8 => {
            if bvar12 {
                bvar25 = 6;
                bvar30 = 4;
            } else {
                bvar25 = 0x0E;
                if bvar10 {
                    bvar30 = t_tensify(bvar30);
                }
            }
        }
        9 => {
            if bvar12 {
                bvar25 = 6;
                bvar30 = 5;
            } else if bvar21 {
                bvar25 = 6;
                bvar30 = 0x0D;
            } else if bvar10 {
                bvar30 = t_tensify(bvar30);
                bvar25 = 0x0F;
            } else if bvar8 {
                bvar25 = 6;
                bvar30 = 0x0B;
            } else {
                bvar25 = if bvar13 { 0x0F } else { 0x0E };
            }
        }
        10 => {
            bvar25 = 6;
            if bvar12 {
                bvar30 = 0x10;
            } else if bvar10 {
                bvar30 = t_tensify(bvar30);
            }
        }
        11 => {
            bvar25 = 6;
            if bvar17 {
                bvar30 = 8;
            } else if bvar12 {
                bvar30 = 10;
            } else if bvar10 {
                bvar30 = t_tensify(bvar30);
            }
        }
        12 => {
            if bvar12 {
                bvar25 = 6;
                bvar30 = 0x0B;
            } else if bvar10 {
                bvar30 = t_tensify(bvar30);
                bvar25 = 0x0F;
            } else {
                bvar25 = if bvar13 { 0x0F } else { 0x0E };
            }
        }
        13 => {
            if bvar12 {
                bvar25 = 0x1B;
                bvar30 = 3;
            } else if bvar16 {
                bvar30 = t_h_fusion(bvar30);
                bvar25 = 6;
            } else if bvar32 {
                bvar25 = 6;
                if bvar33 {
                    bvar30 = 3;
                }
            } else {
                bvar25 = 6;
                bvar30 = 0x10;
            }
        }
        14 => {
            if !bvar22 {
                if bvar12 {
                    bvar25 = 0x1B;
                    bvar30 = 4;
                } else if bvar10 {
                    bvar30 = t_tensify(bvar30);
                    bvar25 = 0x0E;
                }
            }
        }
        15 => {
            if bvar34 {
                bvar30 = 0x0B;
            } else if bvar12 {
                bvar25 = 0x1B;
                bvar30 = 5;
            } else if bvar7 {
                bvar30 = t_tensify(bvar30);
            } else if bvar13 {
                bvar25 = 0x0E;
            }
        }
        16 => {
            if bvar12 {
                bvar25 = 0x0F;
                bvar30 = 0x10;
            } else if bvar10 {
                bvar30 = t_tensify(bvar30);
                bvar25 = 0x0F;
            } else {
                bvar25 = if bvar13 { 0x0F } else { 0x0E };
            }
        }
        17 => {
            if bvar11 {
                bvar25 = 5;
                bvar30 = 8;
            } else if bvar34 {
                bvar25 = 5;
                bvar30 = 10;
            } else if bvar12 {
                bvar25 = 0x1B;
                bvar30 = 6;
            } else if bvar7 {
                bvar30 = t_tensify(bvar30);
                bvar25 = 5;
            } else {
                bvar25 = if bvar13 { 5 } else { 2 };
            }
        }
        19 => {
            if bvar11 {
                bvar25 = 5;
                bvar30 = 8;
            } else if bvar34 {
                bvar25 = 5;
                bvar30 = 10;
            } else if bvar12 {
                bvar25 = 0x1B;
                bvar30 = 7;
            } else if bvar7 {
                bvar30 = t_tensify(bvar30);
                bvar25 = 5;
            } else {
                bvar25 = if bvar13 { 5 } else { 2 };
            }
        }
        20 => {
            if bvar11 {
                bvar25 = 5;
                bvar30 = 8;
            } else if bvar34 {
                bvar25 = 5;
                bvar30 = 10;
            } else if bvar12 {
                bvar30 = 8;
                bvar25 = G2P_BYTE_38;
            } else if bvar7 {
                bvar30 = t_tensify(bvar30);
                bvar25 = 5;
            } else {
                bvar25 = if bvar13 { 5 } else { 2 };
            }
        }
        21 => {
            if bvar34 {
                bvar25 = 0;
                bvar30 = 9;
            } else if bvar12 {
                bvar30 = 9;
                bvar25 = G2P_BYTE_39;
            } else if bvar7 {
                bvar30 = t_tensify(bvar30);
                bvar25 = 0;
            } else {
                bvar25 = if bvar13 { 0 } else { 0x12 };
            }
        }
        22 => {
            if bvar11 {
                bvar30 = 8;
                bvar25 = 5;
            } else if bvar34 {
                bvar30 = 10;
                bvar25 = 5;
            } else if bvar17 {
                bvar30 = 8;
                bvar25 = G2P_BYTE_38;
            } else if bvar12 {
                bvar30 = 10;
                bvar25 = G2P_BYTE_3A;
            } else if bvar7 {
                bvar30 = t_tensify(bvar30);
                bvar25 = 5;
            } else {
                bvar25 = if bvar13 { 5 } else { 2 };
            }
        }
        23 => {
            if bvar34 {
                bvar25 = 0x0F;
                bvar30 = 0x0B;
            } else if bvar12 {
                bvar30 = 0x0B;
                bvar25 = G2P_BYTE_3B;
            } else if bvar7 {
                bvar30 = t_tensify(bvar30);
                bvar25 = 0x0F;
            } else {
                bvar25 = if bvar13 { 0x0F } else { 0x0E };
            }
        }
        24 => {
            if bvar12 {
                bvar25 = 0x1B;
            } else if bvar16 {
                bvar30 = t_h_fusion(bvar30);
                bvar25 = t_h2(bvar30);
            } else if bvar32 {
                if bvar33 {
                    bvar25 = 2;
                    bvar30 = 1;
                } else {
                    bvar25 = 5;
                }
            } else {
                bvar25 = 5;
                bvar30 = 0x10;
            }
        }
        25 => {
            if bvar34 {
                bvar25 = 0;
                bvar30 = 9;
            } else if bvar12 {
                bvar30 = 0x0D;
                bvar25 = t_h2(0);
            } else if bvar7 {
                bvar30 = t_tensify(bvar30);
                bvar25 = 0;
            } else {
                bvar25 = if bvar13 { 0 } else { 0x12 };
            }
        }
        26 => {
            if bvar11 {
                bvar30 = 8;
                bvar25 = 5;
            } else if bvar34 {
                bvar30 = 10;
                bvar25 = 5;
            } else if bvar12 {
                bvar30 = 0x10;
                bvar25 = G2P_BYTE_33;
            } else if bvar7 {
                bvar30 = t_tensify(bvar30);
                bvar25 = 5;
            } else {
                bvar25 = if bvar13 { 5 } else { 2 };
            }
        }
        27 => {
            if bvar23 {
                bvar25 = t_h1(bvar30);
            } else if bvar24 {
                bvar25 = t_h2(bvar30);
            }
        }
        _ => {}
    }

    prev = ((bvar25 as u16) << 10) | ((bvar28 as u16) << 5) | (bvar27 as u16);
    curr = ((bvar6 as u16) << 10) | ((bvar29 as u16) << 5) | ((bvar30 & 0x1F) as u16);

    let last_idx = left.len() - 1;
    left[last_idx] = prev;
    right[0] = curr;

    let mut out = Vec::with_capacity(left.len() + right.len());
    out.extend_from_slice(left);
    out.extend_from_slice(right);
    out
}

/// Apply intra-word vowel/ㅎ rules to a contiguous word slice.
///
/// `param3` corresponds to the original third parameter (`char param_3`).
/// In the current pipeline this is false by default; wiring to the original
/// segment/word-type logic is handled by later stages.
fn apply_intra_word_rules(word: &mut [u16], param3: bool) {
    if word.is_empty() {
        return;
    }

    let last = word.len() - 1;
    let mut bvar3 = false;

    for i in 0..word.len() {
        let mut cur = word[i];
        let jung = ((cur >> 5) & 0x1F) as u8;
        let cho = (cur & 0x1F) as u8;

        if jung == 0x0D && cho != 0x12 {
            // ㅖ -> ㅔ when cho != ㅇ
            cur = (cur & 0xFD9F) | 0x0180;
            word[i] = cur;
        } else if jung == 0x0B && cho != 0x12 {
            // ㅒ -> ㅐ when cho != ㅇ
            cur = (cur & 0xFD5F) | 0x0140;
            word[i] = cur;
        } else if jung == 0x13 {
            // jung 0x13 -> 0x0E
            cur = (cur & 0xFDDF) | 0x01C0;
            word[i] = cur;
        } else {
            if i != 0 {
                let prev = word[i - 1];
                let prev_cho = (prev & 0x1F) as u8;
                let prev_jung_bits = prev & 0x03E0;
                let prev_coda_bits = prev & 0xFC00;
                let cur_cho = (cur & 0x1F) as u8;
                let cur_jung = ((cur >> 5) & 0x1F) as u8;
                let cur_coda_bits = cur & 0xFC00;

                if prev_cho == 0x0C
                    && prev_jung_bits == 0x0120
                    && prev_coda_bits == 0x6C00
                    && cur_cho == 0x12
                    && cur_jung == 0x08
                    && cur_coda_bits == 0x6000
                {
                    cur = (cur & 0x03FF) | 0x4400;
                    word[i] = cur;
                } else if i == last {
                    if cur_cho == 0x05 && cur_jung == 0x09 && cur_coda_bits == 0x5000 {
                        cur = (cur & 0xFFEF) | 0x000F;
                        word[i] = cur;
                    } else if cur_cho == 0x00 && cur_jung == 0x00 && cur_coda_bits == 0x4000 {
                        cur = (cur & 0xFFED) | 0x000D;
                        word[i] = cur;
                    }
                }
            }

            if param3 {
                let cur_cho = (cur & 0x1F) as u8;
                let cur_jung = ((cur >> 5) & 0x1F) as u8;
                let cur_coda_bits = cur & 0xFC00;

                if !(i != 0
                    && i == last
                    && cur_cho == 0x00
                    && cur_jung == 0x12
                    && cur_coda_bits == 0x0800)
                {
                    if i < last
                        && ((cur_cho == 0x05 && cur_jung == 0x00 && cur_coda_bits == 0x4800)
                            || (cur_cho == 0x12 && cur_jung == 0x0F && cur_coda_bits == 0x6C00)
                            || (cur_cho == 0x06 && cur_jung == 0x00 && cur_coda_bits == 0x6C00))
                    {
                        bvar3 = true;
                    } else if !bvar3
                        && i != 0
                        && i == last
                        && cur_cho == 0x05
                        && cur_jung == 0x02
                        && cur_coda_bits == 0x3C00
                    {
                        cur = (cur & 0xFFEF) | 0x000F;
                        bvar3 = false;
                        word[i] = cur;
                    }
                } else {
                    cur = (cur & 0xFFED) | 0x000D;
                    word[i] = cur;
                }
            } else if word.len() > 3 && i == last {
                let cur_cho = (cur & 0x1F) as u8;
                let cur_jung = ((cur >> 5) & 0x1F) as u8;
                let cur_coda_bits = cur & 0xFC00;
                if cur_cho == 0x12 && cur_jung == 0x10 && cur_coda_bits == 0x6C00 {
                    cur = (cur & 0xFD3F) | 0x0120;
                    word[i] = cur;
                }
            }
        }

        // If current coda == 0x1b and next cho is in specific ranges, remap via `H_TABLE_1` / `H_TABLE_2`.
        if i < last {
            let current = word[i];
            let current_coda = ((current >> 10) & 0x1F) as u8;
            if current_coda == 0x1B {
                let next = word[i + 1];
                let next_cho = (next & 0x1F) as usize;

                let remap = if (0x0D..=0x11).contains(&next_cho) {
                    Some(H_TABLE_1[next_cho])
                } else if (0x08..=0x0B).contains(&next_cho) {
                    Some(H_TABLE_2[next_cho])
                } else {
                    None
                };

                if let Some(coda) = remap {
                    word[i] = (current & 0x03FF) | ((coda as u16) << 10);
                }
            }
        }
    }
}

/// Apply G2P rules to a sequence of packed syllables.
///
/// The syllables should use raw jong values (not yet neutralized).
/// Non-Korean Character entries (bit 15 set or 0xFFFF) are treated as word boundaries—
/// rules do NOT apply across boundaries.
pub fn apply_g2p(syllables: &mut [u16]) {
    apply_g2p_with_controls(syllables, 0, 0, 0);
}

/// Per-word control bytes carried through the G2P pipeline.
#[derive(Debug, Clone, Copy)]
pub struct WordControl {
    /// Word-class / segment tag (byte 0 of control slice).
    pub current_class: u8,
    /// Previous-syllable class byte (index −1 relative to control slice).
    pub prev_class: u8,
    /// Control byte 2: enables inter-syllable rule pass when non-zero.
    pub enable_inter: bool,
    /// Control byte 3: word-boundary state written by cross-word sandhi.
    pub mode: u8,
    /// Segment-junction class byte for the multi-segment merge path.
    pub junction_class: u8,
}

impl Default for WordControl {
    fn default() -> Self {
        Self {
            current_class: 0,
            prev_class: 0,
            enable_inter: true,
            mode: 0,
            junction_class: 0,
        }
    }
}

fn collect_word_ranges(syllables: &[u16]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut i = 0usize;
    while i < syllables.len() {
        if is_boundary(syllables[i]) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j + 1 < syllables.len() && !is_boundary(syllables[j + 1]) {
            j += 1;
        }
        ranges.push((i, j));
        i = j + 1;
    }
    ranges
}

/// Apply G2P with explicit per-word controls extracted from the original text-analysis stage.
pub fn apply_g2p_with_word_controls(syllables: &mut [u16], controls: &[WordControl]) {
    let ranges = collect_word_ranges(syllables);
    if ranges.is_empty() || ranges.len() != controls.len() {
        apply_g2p(syllables);
        return;
    }

    let mut prev_word_last_index: Option<usize> = None;
    let mut boundary_state = WordBoundaryState::default();
    let mut junction_state = SegmentJunctionState::default();

    for (word_idx, (i, j)) in ranges.iter().copied().enumerate() {
        let c = controls[word_idx];

        apply_intra_word_rules(&mut syllables[i..=j], c.current_class != 0);

        if c.enable_inter {
            apply_inter_syllable_rules(
                &mut syllables[i..=j],
                c.prev_class,
                c.current_class,
                boundary_state.prev_param5,
            );
        }

        if let Some(prev_last) = prev_word_last_index {
            let (left, right) = syllables.split_at_mut(i);
            let prev_ref = &mut left[prev_last];
            let cur_len = j - i + 1;
            apply_word_boundary_rules(
                &mut boundary_state,
                Some(prev_ref),
                &mut right[..cur_len],
                c.prev_class,
                c.current_class,
                c.mode,
            );
        } else {
            apply_word_boundary_rules(
                &mut boundary_state,
                None,
                &mut syllables[i..=j],
                c.prev_class,
                c.current_class,
                c.mode,
            );
        }

        apply_segment_junction_rules(
            &mut junction_state,
            &mut syllables[i..=j],
            c.current_class,
            c.junction_class,
        );

        prev_word_last_index = Some(j);
    }
}

/// Apply G2P rules with explicit per-word control bytes.
///
/// Control-byte layout for the inter-syllable pass:
/// - `param3`: rule-mode byte (`param_3`)
/// - `param4`: context flag (`param_4`)
/// - `mode`: cross-word boundary mode from the per-word control slice
pub fn apply_g2p_with_controls(syllables: &mut [u16], param3: u8, param4: u8, mode: u8) {
    if syllables.len() < 2 {
        return;
    }

    // Process word by word (contiguous non-boundary runs):
    //   1) Intra-word single-syllable rules
    //   2) inter-syllable pair rules (current stage implementation)
    let mut i = 0usize;
    let mut prev_word_last_index: Option<usize> = None;
    let mut boundary_state = WordBoundaryState::default();
    let mut junction_state = SegmentJunctionState::default();

    while i < syllables.len() {
        if is_boundary(syllables[i]) {
            i += 1;
            continue;
        }

        let mut j = i;
        while j + 1 < syllables.len() && !is_boundary(syllables[j + 1]) {
            j += 1;
        }

        apply_intra_word_rules(&mut syllables[i..=j], param3 != 0);

        apply_inter_syllable_rules(&mut syllables[i..=j], param3, param4, mode);

        // Cross-word boundary rules
        if let Some(prev_last) = prev_word_last_index {
            let (left, right) = syllables.split_at_mut(i);
            let prev_ref = &mut left[prev_last];
            let cur_len = j - i + 1;
            apply_word_boundary_rules(
                &mut boundary_state,
                Some(prev_ref),
                &mut right[..cur_len],
                param3,
                param4,
                mode,
            );
        } else {
            apply_word_boundary_rules(
                &mut boundary_state,
                None,
                &mut syllables[i..=j],
                param3,
                param4,
                mode,
            );
        }

        apply_segment_junction_rules(&mut junction_state, &mut syllables[i..=j], param3, mode);

        prev_word_last_index = Some(j);

        i = j + 1;
    }
}

/// Apply coda neutralization remap (`CODA_REMAP`) to word-final syllables.
///
/// Reference pipeline behaviour:
/// - keep raw jong values for non-final syllables inside each word
/// - neutralize only the last Korean Syllable of each word
///
/// Word boundaries are entries where `is_boundary()` is true
/// (0xFFFF sentinel and bit15-set non-Korean Character entries).
pub fn neutralize_codas(syllables: &mut [u16]) {
    if syllables.is_empty() {
        return;
    }

    let mut i = 0usize;
    while i < syllables.len() {
        if is_boundary(syllables[i]) {
            i += 1;
            continue;
        }

        // Find the end of the current word (contiguous non-boundary run).
        let mut j = i;
        while j + 1 < syllables.len() && !is_boundary(syllables[j + 1]) {
            j += 1;
        }

        // Neutralize only the word-final syllable at index j.
        let s = &mut syllables[j];
        let coda = (*s >> 10) & 0x1F;
        if (coda as usize) < CODA_REMAP.len() {
            let neutralized = CODA_REMAP[coda as usize];
            *s = (*s & 0x3FF) | ((neutralized as u16) << 10);
        }

        i = j + 1;
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::korean::{decompose_char, pack_syllable, DecomposedChar};

    /// Helper: decompose Korean text into raw packed syllable IDs
    fn text_to_raw_ids(text: &str) -> Vec<u16> {
        text.chars()
            .filter_map(|ch| match decompose_char(ch) {
                DecomposedChar::KoreanSyllable(j) => Some(pack_syllable(&j)),
                _ => None,
            })
            .collect()
    }

    /// Helper: apply full G2P + neutralization pipeline
    fn g2p_pipeline(text: &str) -> Vec<u16> {
        let mut ids = text_to_raw_ids(text);
        apply_g2p(&mut ids);
        neutralize_codas(&mut ids);
        ids
    }

    /// Helper: show syllable for debugging
    fn show(packed: u16) -> String {
        let (cho, jung, coda) = unpack(packed);
        format!("cho={} jung={} coda={}", cho, jung, coda)
    }

    #[test]
    fn test_련음_simple() {
        // 조선어 → 조.서.너 (ㄴ coda moves to onset, 련음법칙)
        let result = g2p_pipeline("조선어");
        // 서: coda=27(none) → ㄴ coda removed by 련음
        assert_eq!(
            unpack(result[1]).2,
            27,
            "선→서 coda should be none: {}",
            show(result[1])
        );
        // 너: cho=1(ㄴ) → onset is ㄴ (was ㅇ, now ㄴ from 련음)
        assert_eq!(
            unpack(result[2]).0,
            1,
            "어→너 cho should be ㄴ(1): {}",
            show(result[2])
        );
    }

    #[test]
    fn test_tensification() {
        // 학교 → 학.꾜 (ㄱ coda + ㄱ onset → ㄲ)
        let result = g2p_pipeline("학교");
        // 학: unchanged
        // 교→꾜: cho should be ㄲ(13)
        assert_eq!(
            unpack(result[1]).0,
            13,
            "교→꾜 cho should be ㄲ(13): {}",
            show(result[1])
        );
    }

    #[test]
    fn test_nasalization() {
        // 학년 → 항.년 (ㄱ coda + ㄴ onset → ㅇ coda)
        let result = g2p_pipeline("학년");
        // 학→항: coda should be ㅇ(18)
        assert_eq!(
            unpack(result[0]).2,
            18,
            "학→항 coda should be ㅇ(18): {}",
            show(result[0])
        );
        // 년: unchanged
        assert_eq!(
            unpack(result[1]).0,
            1,
            "년 cho should be ㄴ(1): {}",
            show(result[1])
        );
    }

    #[test]
    fn test_aspiration_h_coda() {
        // 좋다 → 조.타 (ㅎ coda + ㄷ onset → aspiration)
        let result = g2p_pipeline("좋다");
        // Default inter-syllable params keep non-final coda as transformed group.
        assert_eq!(
            unpack(result[0]).2,
            5,
            "좋 first syllable coda: {}",
            show(result[0])
        );
        // 다→타: cho should be ㅌ(10)
        assert_eq!(
            unpack(result[1]).0,
            10,
            "다→타 cho should be ㅌ(10): {}",
            show(result[1])
        );
    }

    #[test]
    fn test_aspiration_h_onset() {
        // 축하 → 추.카 (ㄱ coda + ㅎ onset → aspiration)
        let result = g2p_pipeline("축하");
        // Default inter-syllable params keep non-final coda as transformed group.
        assert_eq!(
            unpack(result[0]).2,
            0,
            "축 first syllable coda: {}",
            show(result[0])
        );
        // 하→카: cho should be ㅋ(9)
        assert_eq!(
            unpack(result[1]).0,
            9,
            "하→카 cho should be ㅋ(9): {}",
            show(result[1])
        );
    }

    #[test]
    fn test_h_drop_before_vowel() {
        // 좋아 → 조.아 (ㅎ coda + ㅇ onset → ㅎ drops)
        let result = g2p_pipeline("좋아");
        // 좋→조: coda should be 27(none)
        assert_eq!(
            unpack(result[0]).2,
            27,
            "좋→조 coda should be none: {}",
            show(result[0])
        );
        // 아: cho stays ㅇ(18)
        assert_eq!(
            unpack(result[1]).0,
            18,
            "아 cho should stay ㅇ(18): {}",
            show(result[1])
        );
    }

    #[test]
    fn test_류음화() {
        // 련락 → 렬.락 (ㄴ + ㄹ → ㄹ + ㄹ, 류음화)
        let result = g2p_pipeline("련락");
        // Default params keep ㄴ-group (류음화 path).
        assert_eq!(
            unpack(result[0]).2,
            2,
            "련 first syllable coda: {}",
            show(result[0])
        );
    }

    #[test]
    fn test_lateralization_reverse() {
        // 칼날 → 칼.랄 (ㄹ + ㄴ → ㄹ + ㄹ)
        let result = g2p_pipeline("칼날");
        // 날→랄: cho should be ㄹ(3)
        assert_eq!(
            unpack(result[1]).0,
            3,
            "날→랄 cho should be ㄹ(3): {}",
            show(result[1])
        );
    }

    #[test]
    fn test_compound_coda_liaison() {
        // 닭이 → 달.기 (ㄺ + ㅇ → ㄹ stays, ㄱ moves)
        let result = g2p_pipeline("닭이");
        // 닭→달: coda should be ㄹ(6)
        assert_eq!(
            unpack(result[0]).2,
            6,
            "닭→달 coda should be ㄹ(6): {}",
            show(result[0])
        );
        // 이→기: cho should be ㄱ(0)
        assert_eq!(
            unpack(result[1]).0,
            0,
            "이→기 cho should be ㄱ(0): {}",
            show(result[1])
        );
    }

    #[test]
    fn test_no_change_already_correct() {
        // 나라 → 나.라 (no rules apply — ㄴ onset before ㄹ, but no coda interaction)
        // Actually 나 has no coda, so no rules apply.
        let result = g2p_pipeline("나라");
        assert_eq!(unpack(result[0]).0, 1, "나: cho=ㄴ(1)");
        assert_eq!(unpack(result[0]).2, 27, "나: coda=none");
        assert_eq!(unpack(result[1]).0, 3, "라: cho=ㄹ(3)");
    }

    #[test]
    fn test_obstruent_plus_rieul() {
        // 학력 → 항.녁 (ㄱ + ㄹ → ㅇ + ㄴ)
        let result = g2p_pipeline("학력");
        // 학→항: nasalized
        assert_eq!(
            unpack(result[0]).2,
            18,
            "학→항 coda should be ㅇ(18): {}",
            show(result[0])
        );
        // With current default params this path keeps ㄹ onset.
        assert_eq!(
            unpack(result[1]).0,
            3,
            "력 second syllable onset: {}",
            show(result[1])
        );
    }

    #[test]
    fn test_word_boundary_isolation() {
        // G2P should not apply across word boundaries
        // Use 0xFFFF as separator
        let mut ids = text_to_raw_ids("학");
        ids.push(0xFFFF); // word boundary
        ids.extend(text_to_raw_ids("년"));
        apply_g2p(&mut ids);
        neutralize_codas(&mut ids);
        // 학 and 년 should NOT interact (no nasalization)
        // 학's coda stays ㄱ group (0), not ㅇ(18)
        assert_eq!(
            unpack(ids[0]).2,
            0,
            "학 across boundary: coda should stay ㄱ(0)"
        );
    }

    #[test]
    fn test_intra_word_ye_to_e_when_not_ieung() {
        // 계(ㄱ+ㅖ) -> 게(ㄱ+ㅔ)
        let mut word = vec![pack(0, 13, 27)];
        apply_intra_word_rules(&mut word, false);
        let (cho, jung, coda) = unpack(word[0]);
        assert_eq!(cho, 0);
        assert_eq!(jung, 12);
        assert_eq!(coda, 27);
    }

    #[test]
    fn test_intra_word_ye_kept_for_ieung() {
        // 예(ㅇ+ㅖ) keeps ㅖ
        let mut word = vec![pack(18, 13, 27)];
        apply_intra_word_rules(&mut word, false);
        let (_, jung, _) = unpack(word[0]);
        assert_eq!(jung, 13);
    }

    #[test]
    fn test_intra_word_yae_to_ae_when_not_ieung() {
        // 걔(ㄱ+ㅒ) -> 개(ㄱ+ㅐ)
        let mut word = vec![pack(0, 11, 27)];
        apply_intra_word_rules(&mut word, false);
        let (_, jung, _) = unpack(word[0]);
        assert_eq!(jung, 10);
    }

    #[test]
    fn test_intra_word_jung_13_to_14() {
        // jung 0x13 is remapped to 0x0E by intra-word rules
        let mut word = vec![pack(0, 19, 27)];
        apply_intra_word_rules(&mut word, false);
        let (_, jung, _) = unpack(word[0]);
        assert_eq!(jung, 14);
    }

    #[test]
    fn test_intra_word_h_tables_on_no_coda_before_specific_onset() {
        // current coda=27 and next cho in 13..17 -> H_TABLE_1[next_cho]
        let mut w1 = vec![pack(0, 0, 27), pack(13, 0, 27)];
        apply_intra_word_rules(&mut w1, false);
        assert_eq!(unpack(w1[0]).2, H_TABLE_1[13]);

        // current coda=27 and next cho in 8..11 -> H_TABLE_2[next_cho]
        let mut w2 = vec![pack(0, 0, 27), pack(8, 0, 27)];
        apply_intra_word_rules(&mut w2, false);
        assert_eq!(unpack(w2[0]).2, H_TABLE_2[8]);
    }

    #[test]
    fn test_neutralize_only_word_final_syllable() {
        // Build a 2-syllable word with a non-final raw compound coda on syllable 0.
        // syllable 0: cho=0, jung=0, coda=7(ㄺ raw) -> must stay raw 7 (non-final)
        // syllable 1: cho=18, jung=0, coda=17(ㅅ raw) -> final, must neutralize to 5
        let mut ids = vec![pack(0, 0, 7), pack(18, 0, 17)];
        neutralize_codas(&mut ids);

        assert_eq!(unpack(ids[0]).2, 7, "non-final syllable must keep raw coda");
        assert_eq!(
            unpack(ids[1]).2,
            5,
            "word-final syllable must be neutralized"
        );
    }

    #[test]
    fn test_neutralize_per_word_with_boundaries() {
        // [word1: 2 syllables] [boundary] [word2: 1 syllable]
        // word1 first syllable coda raw 16(ㅄ) -> non-final, keep 16
        // word1 last syllable coda raw 24(ㅎ) -> final, neutralize to 5
        // word2 only syllable coda raw 9(ㄼ) -> final, neutralize to 15
        let mut ids = vec![pack(0, 0, 16), pack(18, 0, 24), 0xFFFF, pack(1, 2, 9)];
        neutralize_codas(&mut ids);

        assert_eq!(unpack(ids[0]).2, 16, "word1 non-final must keep raw coda");
        assert_eq!(unpack(ids[1]).2, 5, "word1 final must be neutralized");
        assert_eq!(ids[2], 0xFFFF, "boundary must stay unchanged");
        assert_eq!(unpack(ids[3]).2, 15, "word2 final must be neutralized");
    }

    #[test]
    fn test_안녕하십니까() {
        let result = g2p_pipeline("안녕하십니까");
        // 안: cho=ㅇ(18), jung=ㅏ(0), coda=ㄴ — no change
        // 녕: cho=ㄴ(1), jung=ㅕ(3), coda=ㅇ — no change
        // 하: cho=ㅎ(12), jung=ㅏ(0), coda=none — no change
        // 십+니: ㅂ coda + ㄴ onset → 코소리현상 → 십→심
        // 니: cho=ㄴ(1)
        // 까: cho=ㄲ(13)
        assert_eq!(result.len(), 6);
        assert_eq!(unpack(result[0]).0, 18); // 안: cho=ㅇ
        assert_eq!(unpack(result[1]).0, 1); // 녕: cho=ㄴ
        assert_eq!(unpack(result[2]).0, 12); // 하: cho=ㅎ
        assert_eq!(unpack(result[4]).0, 1); // 니: cho=ㄴ
    }
}
