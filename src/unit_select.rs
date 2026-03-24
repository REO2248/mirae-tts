//! Full VoiceInfo scan; Mirae-style scoring via `PROSODY_CHAR_TABLE`, `PROSODY_SCORE_TABLE`, `PROSODY_RANGES`.

use crate::phoneme::PhonemeUnit;
use crate::voice_info::{VoiceInfo, VoiceInfoEntry};

const PROSODY_CHAR_TABLE: [u8; 256] = [
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

/// Coda (jong) significance flags (32 entries). Non-zero at indices: 2, 6, 14, 18.
const CODA_SIGNIFICANT: [bool; 32] = [
    false, false, true, false, false, false, true, false, // 0-7
    false, false, false, false, false, false, true, false, // 8-15
    false, false, true, false, false, false, false, false, // 16-23
    false, false, false, false, false, false, false, false, // 24-31
];

/// Onset (initial) significance flags (32 entries).
/// Non-zero at indices: 1, 3, 4, 20, 22, 24, 26, 28, 29, 30.
const INITIAL_SIGNIFICANT: [bool; 32] = [
    false, true, false, true, true, false, false, false, // 0-7
    false, false, false, false, false, false, false, false, // 8-15
    false, false, false, false, true, false, true, false, // 16-23
    true, false, true, false, true, true, true, false, // 24-31
];

const PROSODY_SCORE_TABLE: [i32; 256] = [
    600, 595, 450, 425, 400, 375, 350, 325, 200, 175, 150, 125, 100, 75, 50, 25, 600, 595, 350,
    325, 300, 275, 250, 225, 200, 175, 150, 125, 100, 75, 50, 25, 600, 595, 450, 425, 400, 375,
    350, 325, 200, 175, 150, 125, 100, 75, 50, 25, 600, 475, 450, 425, 400, 375, 350, 325, 200,
    175, 150, 125, 100, 75, 50, 25, 600, 520, 500, 380, 360, 340, 320, 300, 200, 175, 150, 125,
    100, 75, 50, 25, 600, 450, 425, 400, 375, 350, 325, 300, 200, 175, 150, 125, 100, 75, 50, 25,
    600, 595, 400, 425, 350, 325, 300, 275, 200, 175, 150, 125, 100, 75, 50, 25, 600, 595, 450,
    425, 400, 375, 350, 325, 200, 175, 150, 125, 100, 75, 50, 25, 600, 595, 450, 425, 400, 375,
    350, 325, 200, 175, 150, 125, 100, 75, 50, 25, 600, 525, 500, 475, 450, 425, 350, 325, 200,
    175, 150, 125, 100, 75, 50, 25, 600, 500, 350, 325, 300, 275, 250, 225, 200, 175, 150, 125,
    100, 75, 50, 25, 600, 595, 590, 475, 450, 425, 400, 300, 275, 250, 225, 200, 175, 150, 125,
    100, 600, 595, 590, 475, 450, 425, 400, 300, 275, 250, 225, 200, 175, 150, 125, 100, 600, 595,
    590, 475, 450, 425, 400, 300, 275, 250, 225, 200, 175, 150, 125, 100, 600, 595, 590, 585, 500,
    475, 450, 425, 400, 375, 350, 325, 300, 275, 250, 225, 600, 475, 450, 425, 300, 275, 250, 225,
    200, 175, 150, 125, 100, 75, 50, 25,
];

/// 16 rows × (pitch_min, max, dur_min, max, _). Filter by `PROSODY_CHAR_TABLE` row for the candidate prosody.
#[derive(Clone, Copy)]
struct ProsodyRange {
    pitch_min: i32,
    pitch_max: i32,
    dur_min: i32,
    dur_max: i32,
}

const PROSODY_RANGES: [ProsodyRange; 16] = [
    ProsodyRange {
        pitch_min: 78,
        pitch_max: 220,
        dur_min: 700,
        dur_max: 7000,
    }, // row 0
    ProsodyRange {
        pitch_min: 80,
        pitch_max: 220,
        dur_min: 700,
        dur_max: 12000,
    }, // row 1
    ProsodyRange {
        pitch_min: 78,
        pitch_max: 220,
        dur_min: 700,
        dur_max: 7000,
    }, // row 2
    ProsodyRange {
        pitch_min: 77,
        pitch_max: 220,
        dur_min: 700,
        dur_max: 7000,
    }, // row 3
    ProsodyRange {
        pitch_min: 78,
        pitch_max: 105,
        dur_min: 700,
        dur_max: 6000,
    }, // row 4
    ProsodyRange {
        pitch_min: 78,
        pitch_max: 105,
        dur_min: 700,
        dur_max: 6500,
    }, // row 5
    ProsodyRange {
        pitch_min: 78,
        pitch_max: 220,
        dur_min: 700,
        dur_max: 7000,
    }, // row 6
    ProsodyRange {
        pitch_min: 77,
        pitch_max: 220,
        dur_min: 700,
        dur_max: 12000,
    }, // row 7
    ProsodyRange {
        pitch_min: 78,
        pitch_max: 220,
        dur_min: 700,
        dur_max: 7000,
    }, // row 8
    ProsodyRange {
        pitch_min: 77,
        pitch_max: 220,
        dur_min: 700,
        dur_max: 7000,
    }, // row 9
    ProsodyRange {
        pitch_min: 78,
        pitch_max: 105,
        dur_min: 700,
        dur_max: 7000,
    }, // row 10
    ProsodyRange {
        pitch_min: 78,
        pitch_max: 220,
        dur_min: 700,
        dur_max: 12000,
    }, // row 11
    ProsodyRange {
        pitch_min: 78,
        pitch_max: 220,
        dur_min: 700,
        dur_max: 12000,
    }, // row 12
    ProsodyRange {
        pitch_min: 78,
        pitch_max: 220,
        dur_min: 700,
        dur_max: 12000,
    }, // row 13
    ProsodyRange {
        pitch_min: 78,
        pitch_max: 220,
        dur_min: 700,
        dur_max: 12000,
    }, // row 14
    ProsodyRange {
        pitch_min: 78,
        pitch_max: 105,
        dur_min: 700,
        dur_max: 6000,
    }, // row 15
];

/// Initial-consonant group A (32 flags). Nonzero at: 0–7, 16, 28–31.
const INITIAL_GROUP_A: [bool; 32] = [
    true, true, true, true, true, true, true, true, // 0-7
    false, false, false, false, false, false, false, false, // 8-15
    true, false, false, false, false, false, false, false, // 16-23
    false, false, false, false, true, true, true, true, // 24-31
];

/// Initial-consonant group B (32 flags). Nonzero at: 8–11, 13–15, 17, 21, 23, 24.
const INITIAL_GROUP_B: [bool; 32] = [
    false, false, false, false, false, false, false, false, // 0-7
    true, true, true, true, false, true, true, true, // 8-15
    false, true, false, false, false, true, false, true, // 16-23
    true, false, false, false, false, false, false, false, // 24-31
];

#[derive(Debug, Clone)]
pub struct SelectedUnit {
    pub entry_index: usize,
    pub entry: VoiceInfoEntry,
    pub pause_samples: i16,
    pub score: i32,
    pub is_liaison: bool,
    pub liaison_entry: Option<VoiceInfoEntry>,
}

/// Returns 1/2/3/5. prev_context>>10 = coda, target&0x1f = onset; tiers use ㅎ/ㅆ (0x1B/0x12) vs ㅆ/ㅇ (0x12/0x0C).
fn compute_prev_importance(target_phoneme: u16, prev_context: u16, prosody_byte: i8) -> i32 {
    let prev_coda = (prev_context >> 10) as usize & 0x1F;
    let curr_initial = (target_phoneme & 0x1F) as usize;
    let prosody_row = (prosody_byte as i32) / 10;

    if (prev_coda == 0x1B || prev_coda == 0x12) && curr_initial == 0x12 && prosody_row < 2 {
        return 5;
    }
    if (prev_coda == 0x1B || prev_coda == 0x12) && curr_initial == 0x0C && prosody_row < 1 {
        return 3;
    }
    if !CODA_SIGNIFICANT[prev_coda] || !INITIAL_SIGNIFICANT[curr_initial] {
        return 1;
    }
    2
}

/// target>>10 = coda, next&0x1f = following onset; thresholds use prosody column.
fn compute_next_importance(target_phoneme: u16, next_context: u16, prosody_byte: i8) -> i32 {
    let curr_coda = (target_phoneme >> 10) as usize & 0x1F;
    let next_initial = (next_context & 0x1F) as usize;
    let prosody_col = (prosody_byte as i32) % 10;

    if (curr_coda == 0x1B || curr_coda == 0x12) && next_initial == 0x12 && prosody_col < 2 {
        return 5;
    }
    if (curr_coda == 0x1B || curr_coda == 0x12) && next_initial == 0x0C && prosody_col < 1 {
        return 3;
    }
    if !CODA_SIGNIFICANT[curr_coda] || !INITIAL_SIGNIFICANT[next_initial] {
        return 1;
    }
    2
}

fn score_prev_context(
    importance: i32,
    query_prev: u16,
    candidate_prev: u16,
    candidate_phoneme: u16,
    prosody_row: i32,
    param4: bool,
) -> i32 {
    use crate::korean::VOWEL_SUBSTITUTE;
    let p4_bonus = if param4 { 1u32 } else { 0u32 };

    let cand_prev_coda = ((candidate_prev >> 10) & 0x1F) as usize;
    let candidate_initial = (candidate_phoneme & 0x1F) as usize;

    match importance {
        2 => {
            // Check -200 penalty: candidate prev has ㅎ/ㅆ coda, candidate phoneme has ㅆ/ㅇ onset, row<1
            if (cand_prev_coda == 0x1B || cand_prev_coda == 0x12)
                && (candidate_initial == 0x12 || candidate_initial == 0x0C)
                && prosody_row < 1
            {
                return -200;
            }

            if query_prev == candidate_prev {
                // Exact match
                return 0x14 + (p4_bonus.wrapping_neg() & 0x50) as i32;
            }
            if (candidate_prev ^ query_prev) & 0xFFE0 == 0 {
                // Same coda + medium but initial differs (bits[14:5] same, bits[4:0] differ)
                return 0x14 + (p4_bonus.wrapping_neg() & 0x46) as i32;
            }
            // Same coda: bits[14:10] of query_prev == bits[14:10] of candidate_prev
            if query_prev >> 10 == candidate_prev >> 10 {
                return 0x14 + (p4_bonus.wrapping_neg() & 0x28) as i32;
            }
            // Fallback: CODA_SIGNIFICANT[candidate_prev_coda] → 0x14, else 0
            if CODA_SIGNIFICANT[cand_prev_coda] {
                0x14
            } else {
                0
            }
        }
        3 | 5 => {
            // Check -200 penalty (same as importance==2)
            if (cand_prev_coda == 0x1B || cand_prev_coda == 0x12)
                && (candidate_initial == 0x12 || candidate_initial == 0x0C)
                && prosody_row < 1
            {
                return -200;
            }

            if query_prev == candidate_prev {
                return 0x3C + (p4_bonus.wrapping_neg() & 0x28) as i32;
            }
            if (candidate_prev ^ query_prev) & 0xFFE0 == 0 {
                return 0x3C + (p4_bonus.wrapping_neg() & 0x1E) as i32;
            }
            // Same coda between query_prev and candidate_prev?
            if query_prev >> 10 == candidate_prev >> 10 {
                // Check `VOWEL_SUBSTITUTE` match (byte-indexed by medium vowel)
                let q_jung = ((query_prev >> 5) & 0x1F) as usize;
                let c_jung = ((candidate_prev >> 5) & 0x1F) as usize;
                if q_jung < 21
                    && c_jung < 21
                    && VOWEL_SUBSTITUTE[q_jung] == VOWEL_SUBSTITUTE[c_jung]
                {
                    return 0x3C + (p4_bonus.wrapping_neg() & 0x1E) as i32;
                }
                return 0x32;
            }
            // ㅎ/ㅆ coda of candidate previous context → weak match
            if cand_prev_coda == 0x1B || cand_prev_coda == 0x12 {
                return 0x14;
            }
            0
        }
        _ => {
            // importance == 1 (weakest)
            // Check -200 penalty
            if (cand_prev_coda == 0x1B || cand_prev_coda == 0x12)
                && (candidate_initial == 0x12 || candidate_initial == 0x0C)
                && prosody_row < 1
            {
                return -200;
            }

            if query_prev == candidate_prev {
                return 0x14 + (p4_bonus.wrapping_neg() & 0x50) as i32;
            }
            if (candidate_prev ^ query_prev) & 0xFFE0 == 0 {
                return 0x14 + (p4_bonus.wrapping_neg() & 0x46) as i32;
            }
            if query_prev >> 10 == candidate_prev >> 10 {
                return 0x14 + (p4_bonus.wrapping_neg() & 0x32) as i32;
            }
            // INITIAL_GROUP table check: phoneme must exactly match (target == candidate),
            // so query and candidate share the same initial.
            // Condition: !INITIAL_GROUP_A[initial] && !INITIAL_GROUP_B[initial] → significance fallback
            //            INITIAL_GROUP_A[initial] || INITIAL_GROUP_B[initial]  → bonus
            if INITIAL_GROUP_A[candidate_initial] || INITIAL_GROUP_B[candidate_initial] {
                return 0x14 + (p4_bonus.wrapping_neg() & 0x14) as i32;
            }
            // Significance fallback: if CODA_SIGNIFICANT[candidate_prev_coda] AND INITIAL_SIGNIFICANT[candidate_initial]
            // both true → no bonus (0), otherwise 0x14
            if !CODA_SIGNIFICANT[cand_prev_coda] || !INITIAL_SIGNIFICANT[candidate_initial] {
                0x14
            } else {
                0
            }
        }
    }
}

fn score_next_context(
    importance: i32,
    query_next: u16,
    candidate_next: u16,
    candidate_phoneme: u16,
    prosody_col: i32,
    param4: bool,
) -> i32 {
    use crate::korean::VOWEL_FIRST_COMPONENT;
    let p4_bonus = if param4 { 1u32 } else { 0u32 };

    let candidate_coda = ((candidate_phoneme >> 10) & 0x1F) as usize;
    let cand_next_initial = (candidate_next & 0x1F) as usize;

    match importance {
        2 => {
            if (candidate_coda == 0x1B || candidate_coda == 0x12)
                && (cand_next_initial == 0x12 || cand_next_initial == 0x0C)
                && prosody_col < 1
            {
                return -200;
            }

            if query_next == candidate_next {
                return 0x14 + (p4_bonus.wrapping_neg() & 0x50) as i32;
            }
            if (candidate_next ^ query_next) & 0x3FF == 0 {
                return 0x14 + (p4_bonus.wrapping_neg() & 0x46) as i32;
            }
            if ((candidate_next as u8) ^ (query_next as u8)) & 0x1F == 0 {
                return 0x14 + (p4_bonus.wrapping_neg() & 0x28) as i32;
            }
            if INITIAL_SIGNIFICANT[cand_next_initial] {
                0x14
            } else {
                0
            }
        }
        3 => {
            if (candidate_coda == 0x1B || candidate_coda == 0x12)
                && (cand_next_initial == 0x12 || cand_next_initial == 0x0C)
                && prosody_col < 1
            {
                return -200;
            }

            if query_next == candidate_next {
                return 0x3C + (p4_bonus.wrapping_neg() & 0x28) as i32;
            }
            if (candidate_next ^ query_next) & 0x3FF == 0 {
                return 0x3C + (p4_bonus.wrapping_neg() & 0x1E) as i32;
            }
            let q_initial = (query_next & 0x1F) as usize;
            if cand_next_initial == q_initial {
                let q_jung = ((query_next >> 5) & 0x1F) as usize;
                let c_jung = ((candidate_next >> 5) & 0x1F) as usize;
                if q_jung < 21
                    && c_jung < 21
                    && VOWEL_FIRST_COMPONENT[q_jung] == VOWEL_FIRST_COMPONENT[c_jung]
                {
                    return 0x3C + (p4_bonus.wrapping_neg() & 0x1E) as i32;
                }
                return 0x32;
            }
            if cand_next_initial == 0x0C {
                0x14
            } else {
                0
            }
        }
        5 => {
            if (candidate_coda == 0x1B || candidate_coda == 0x12)
                && (cand_next_initial == 0x12 || cand_next_initial == 0x0C)
                && prosody_col < 1
            {
                return -200;
            }

            if query_next == candidate_next {
                return 0x3C + (p4_bonus.wrapping_neg() & 0x28) as i32;
            }
            if (candidate_next ^ query_next) & 0x3FF == 0 {
                return 0x3C + (p4_bonus.wrapping_neg() & 0x1E) as i32;
            }
            let q_initial = (query_next & 0x1F) as usize;
            if cand_next_initial == q_initial {
                let q_jung = ((query_next >> 5) & 0x1F) as usize;
                let c_jung = ((candidate_next >> 5) & 0x1F) as usize;
                if q_jung < 21
                    && c_jung < 21
                    && VOWEL_FIRST_COMPONENT[q_jung] == VOWEL_FIRST_COMPONENT[c_jung]
                {
                    return 0x3C + (p4_bonus.wrapping_neg() & 0x1E) as i32;
                }
                return 0x32;
            }
            if cand_next_initial == 0x12 {
                0x14
            } else {
                0
            }
        }
        _ => {
            if (candidate_coda == 0x1B || candidate_coda == 0x12)
                && (cand_next_initial == 0x12 || cand_next_initial == 0x0C)
                && prosody_col < 1
            {
                return -200;
            }

            if query_next == candidate_next {
                return 0x14 + (p4_bonus.wrapping_neg() & 0x50) as i32;
            }
            if (candidate_next ^ query_next) & 0x3FF == 0 {
                return 0x14 + (p4_bonus.wrapping_neg() & 0x46) as i32;
            }
            if ((candidate_next as u8) ^ (query_next as u8)) & 0x1F == 0 {
                return 0x14 + (p4_bonus.wrapping_neg() & 0x32) as i32;
            }
            let q_next_initial = (query_next & 0x1F) as usize;
            let cond_a = !INITIAL_GROUP_A[q_next_initial] || !INITIAL_GROUP_A[cand_next_initial];
            let cond_b = !INITIAL_GROUP_B[q_next_initial] || !INITIAL_GROUP_B[cand_next_initial];
            if cond_a && cond_b {
                if !CODA_SIGNIFICANT[candidate_coda] || !INITIAL_SIGNIFICANT[cand_next_initial] {
                    0x14
                } else {
                    0
                }
            } else {
                0x14 + (p4_bonus.wrapping_neg() & 0x14) as i32
            }
        }
    }
}

/// Mirae-style coarticulation pause: +1000 for coda 0/5/F (skip 5 if next==0x10); +1000 if next onset 8–11; +1500 if 13–15 or 17; else +1500 when (0|5|F) and row>0 and col>0.
fn compute_coarticulation_pause(coda: u16, next_initial: u16, prosody: i8) -> i16 {
    let mut delta: i32 = 0;

    let skip_block1 = coda == 5 && next_initial == 0x10;
    if !skip_block1 && (coda == 0 || coda == 5 || coda == 0xF) {
        delta += 1000;
    }

    if matches!(next_initial, 8..=11) {
        delta += 1000;
    } else if matches!(next_initial, 13 | 14 | 15 | 17) {
        delta += 1500;
    } else {
        let row = (prosody as i32) / 10;
        let col = (prosody as i32) % 10;
        if (coda == 0 || coda == 5 || coda == 0xF) && row > 0 && col > 0 {
            delta += 1500;
        }
    }

    delta as i16
}

/// Liaison insert: false if initial in {1,4,6,8..18} (subset) or (initial,coda)==(3,6).
pub fn is_liaison(coda: i16, initial: i16) -> bool {
    let no_liaison = matches!(
        initial,
        1 | 4 | 6 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 16 | 17 | 18
    );
    let special_no_liaison = initial == 3 && coda == 6;
    !no_liaison && !special_no_liaison
}

/// Liaison-only entries: raw[22] as i8 < 0. Pick minimum |pitch − target|.
fn find_liaison_entry(voice_info: &VoiceInfo, target_pitch: i16) -> Option<VoiceInfoEntry> {
    let mut best_dist: i32 = 200;
    let mut best: Option<VoiceInfoEntry> = None;
    for entry in &voice_info.entries {
        if (entry.raw[22] as i8) >= 0 {
            continue;
        }
        let dist = (entry.pitch() as i32 - target_pitch as i32).abs();
        if dist < best_dist {
            best_dist = dist;
            best = Some(*entry);
        }
    }
    best
}

pub fn select_unit(
    voice_info: &VoiceInfo,
    phoneme: &PhonemeUnit,
    target_pitch: i16,
) -> Option<SelectedUnit> {
    let target_id = phoneme.syllable_id;
    if target_id == 0xFFFF {
        return None;
    }

    // colligation_variant: force col=0 (phrase-initial units)
    let prosody = if phoneme.colligation_variant {
        (phoneme.prosody / 10) * 10
    } else {
        phoneme.prosody
    };

    let prosody_row = (prosody as i32) / 10;
    let prosody_col = (prosody as i32) % 10;

    let prev_imp = compute_prev_importance(target_id, phoneme.prev_context, prosody);
    let next_imp = compute_next_importance(target_id, phoneme.next_context, prosody);
    let total_imp = prev_imp + next_imp;

    let query_prosody_row = prosody_row;

    let mut best_idx: Option<usize> = None;
    let mut best_score: i32 = i32::MIN;
    let mut best_pitch_dist: i32 = i32::MAX;
    let mut best_entry = VoiceInfoEntry::default();

    for (i, entry) in voice_info.entries.iter().enumerate() {
        if !entry.is_valid() {
            continue;
        }

        if entry.phoneme_id() != target_id {
            continue;
        }

        let cand_norm = normalize_candidate_prosody(entry.prosody_byte());
        let cand_row = find_prosody_row(cand_norm);
        if !passes_range_filter(entry, phoneme.emphasis, query_prosody_row, cand_row) {
            continue;
        }

        let prev_score = score_prev_context(
            prev_imp,
            phoneme.prev_context,
            entry.prev_context(),
            entry.phoneme_id(),
            prosody_row,
            phoneme.emphasis,
        );

        let next_score = score_next_context(
            next_imp,
            phoneme.next_context,
            entry.next_context(),
            entry.phoneme_id(),
            prosody_col,
            phoneme.emphasis,
        );

        let context_score = if total_imp > 0 {
            (prev_imp * prev_score + next_imp * next_score) / total_imp
        } else {
            0
        };

        let prosody_bonus = compute_prosody_bonus(prosody, entry.prosody_byte());

        let total_score = 100 + context_score + prosody_bonus;

        let pitch_dist = ((entry.pitch() as i32) - (target_pitch as i32)).abs();

        if total_score > best_score || (total_score == best_score && pitch_dist < best_pitch_dist) {
            best_score = total_score;
            best_idx = Some(i);
            best_pitch_dist = pitch_dist;
            best_entry = *entry;
        }
    }

    best_idx.map(|idx| SelectedUnit {
        entry_index: idx,
        entry: best_entry,
        pause_samples: 0,
        score: best_score,
        is_liaison: false,
        liaison_entry: None,
    })
}

fn compute_prosody_bonus(query_prosody: i8, candidate_prosody: i8) -> i32 {
    #[inline]
    fn normalize_query(mut p: i8) -> i8 {
        if p / 10 == 2 {
            p = (p % 10) + 30;
        }
        if p % 10 == 2 {
            p = (p / 10) * 10 + 3;
        } else if p % 10 == 5 {
            p = (p / 10) * 10 + 4;
        }
        p
    }

    #[inline]
    fn normalize_candidate(mut p: i8) -> i8 {
        if p / 10 == 2 {
            p = (p % 10) + 30;
        }
        if p % 10 == 2 {
            p = (p / 10) * 10 + 3;
        }
        p
    }

    let q = normalize_query(query_prosody) as u8;
    let c = normalize_candidate(candidate_prosody) as u8;

    let mut row = 0usize;
    for i in 0..16 {
        if PROSODY_CHAR_TABLE[i * 16] == q {
            row = i;
            break;
        }
    }

    let mut col = 15usize;
    for j in 0..16 {
        if PROSODY_CHAR_TABLE[row * 16 + j] == c {
            col = j;
            break;
        }
    }

    PROSODY_SCORE_TABLE[row * 16 + col]
}

fn normalize_candidate_prosody(p: i8) -> u8 {
    let mut r = p;
    if r / 10 == 2 {
        r = (r % 10) + 30;
    }
    if r % 10 == 2 {
        r = (r / 10) * 10 + 1;
    }
    r as u8
}

fn find_prosody_row(normalized_prosody: u8) -> usize {
    for i in 0..16 {
        if PROSODY_CHAR_TABLE[i * 16] == normalized_prosody {
            return i;
        }
    }
    0
}

/// With emphasis and query_row≤1: only pitch lower bound (no upper cap).
fn passes_range_filter(
    entry: &VoiceInfoEntry,
    query_emphasis: bool,
    query_prosody_row: i32,
    cand_row: usize,
) -> bool {
    let range = &PROSODY_RANGES[cand_row];
    let pitch = entry.pitch() as i32;
    let dur = entry.wave_samples() as i32;

    let pitch_ok = if !query_emphasis || query_prosody_row > 1 {
        pitch >= range.pitch_min && pitch <= range.pitch_max
    } else {
        pitch >= range.pitch_min
    };

    let dur_ok = dur >= range.dur_min && dur <= range.dur_max;

    pitch_ok && dur_ok
}

/// Per phoneme stream: direct match → vowel substitute → C+V split. Pauses → silence.
pub fn select_units_for_sequence(
    voice_info: &VoiceInfo,
    phonemes: &[PhonemeUnit],
    type1_matches: &[crate::colligation::Type1Match],
) -> Vec<Option<SelectedUnit>> {
    use crate::korean::{
        CODA_NO_CODA, IEUNG_INITIAL, VOWEL_FIRST_COMPONENT, VOWEL_FLAGS, VOWEL_SUBSTITUTE,
    };

    // Build a lookup: syllable_idx → list of (hyp_sid, syllables_covered).
    // `syllable_idx` is the index in the compact non-pause syllable stream that
    // find_type1_matches operates on, *not* in the `phonemes` slice directly.
    let mut hyp_map: std::collections::HashMap<usize, Vec<(u16, usize)>> =
        std::collections::HashMap::new();
    for m in type1_matches {
        hyp_map
            .entry(m.syllable_idx)
            .or_default()
            .push((m.hyp_sid, m.syllables_covered));
    }

    // Pre-compute: for each syllable position in the compact stream, record
    // the corresponding index in the `phonemes` slice.  Pause entries and
    // 0xFFFF sentinels are excluded from the compact stream.
    let syl_to_phoneme: Vec<usize> = phonemes
        .iter()
        .enumerate()
        .filter(|(_, p)| p.pause.is_none() && p.syllable_id != 0xFFFF)
        .map(|(i, _)| i)
        .collect();

    let mut results: Vec<Option<SelectedUnit>> = Vec::with_capacity(phonemes.len());
    // Phoneme-slice indices < skip_until have been absorbed by a Type-1 match.
    let mut skip_until = 0usize;
    // Syllable counter: how many real (non-pause, non-sentinel) phonemes have
    // been processed so far.  Used to index into hyp_map.
    let mut syl_counter = 0usize;

    // Adaptive pitch target (tracks last matched unit pitch).
    // Row < 2 → use previous matched entry's pitch; row ≥ 2 → reset to 90.
    // Default constructor pitch seed (90).
    let mut prev_match_pitch: i16 = 90; // default starting pitch (Hz-ish units)

    for (phoneme_idx, phoneme) in phonemes.iter().enumerate() {
        // Absorbed by a previous Type-1 hypothesis?
        if phoneme_idx < skip_until {
            results.push(None);
            // Advance syl_counter for real (non-pause, non-sentinel) syllables
            // so that subsequent entries remain indexed correctly.
            if phoneme.pause.is_none() && phoneme.syllable_id != 0xFFFF {
                syl_counter += 1;
            }
            continue;
        }
        if phoneme.pause.is_some() {
            results.push(None);
            continue;
        }

        let target_id = phoneme.syllable_id;
        if target_id == 0xFFFF {
            results.push(None);
            continue;
        }

        // Advance the syllable counter BEFORE we use it for lookup.
        // syl_counter matches the index into the compact stream passed to
        // find_type1_matches.
        let cur_syl = syl_counter;
        syl_counter += 1;

        // Emphasis flag on for normal syllables (drives pitch-range filter)
        // unit selection calls.  Only the pitch-smoothing re-selection passes
        // param4=0.  We mirror this here by forcing emphasis=true so that the
        // context scoring uses the full bonus (0x50 instead of 0x00).
        let mut q = *phoneme;
        q.emphasis = true;

        // Adaptive pitch target:
        //   if (prev_col < 2) pitch_target = prev_match.pitch;
        //   else              pitch_target = default seed (90)
        let prosody_row = (phoneme.prosody as i32) / 10;
        let pitch_target = if prosody_row < 2 {
            prev_match_pitch
        } else {
            90
        };

        // Step 1: Try direct lookup
        let mut selected = select_unit(voice_info, &q, pitch_target);

        if selected.is_some() {
            // ── Step 1 hit ─────────────────────────────
            // Pause = prosody-col pause (overwrite) + coarticulation (add)
            if let Some(ref mut unit) = selected {
                let pcol = (phoneme.prosody as i32) % 10;
                unit.pause_samples = prosody_col_to_pause(pcol);

                // Coarticulation pause adjustment
                let coda = (target_id >> 10) & 0x1F;
                let next_initial = phoneme.next_context & 0x1F;
                let adj = compute_coarticulation_pause(coda, next_initial, phoneme.prosody);
                unit.pause_samples = unit.pause_samples.saturating_add(adj);

                // Liaison: lookup pitch, then refine by scanning candidates
                // Insert a coarticulation unit after this syllable if coda +
                // next_initial combination triggers liaison AND prosody_col < 2.
                let prosody_col = (unit.entry.prosody_byte() as i32) % 10;
                if is_liaison(coda as i16, next_initial as i16) && prosody_col < 2 {
                    unit.liaison_entry = find_liaison_entry(voice_info, unit.entry.pitch());
                }
                // Update adaptive pitch for next unit
                prev_match_pitch = unit.entry.pitch();
            }
            results.push(selected);
            continue;
        }

        // Type-1 colligation hypothesis syllable IDs
        // Try each hypothesis alternative before falling back to vowel
        // substitution and the consonant+vowel split.
        if let Some(hyp_list) = hyp_map.get(&cur_syl) {
            let mut hyp_used = false;
            'hyp_loop: for &(hyp_sid, covered) in hyp_list {
                let mut hyp_phoneme = q;
                hyp_phoneme.syllable_id = hyp_sid;
                let hyp_result = select_unit(voice_info, &hyp_phoneme, pitch_target);
                if let Some(mut unit) = hyp_result {
                    let pcol = (phoneme.prosody as i32) % 10;
                    unit.pause_samples = prosody_col_to_pause(pcol);

                    let coda_bits = (hyp_sid >> 10) & 0x1F;
                    let next_initial = phoneme.next_context & 0x1F;
                    let adj =
                        compute_coarticulation_pause(coda_bits, next_initial, phoneme.prosody);
                    unit.pause_samples = unit.pause_samples.saturating_add(adj);

                    let prosody_col = (unit.entry.prosody_byte() as i32) % 10;
                    if is_liaison(coda_bits as i16, next_initial as i16) && prosody_col < 2 {
                        unit.liaison_entry = find_liaison_entry(voice_info, unit.entry.pitch());
                    }
                    prev_match_pitch = unit.entry.pitch();

                    results.push(Some(unit));
                    // Mark the next (covered-1) syllables as absorbed.
                    // Use syl_to_phoneme to translate syllable indices back to
                    // phoneme-slice indices, correctly skipping any pause entries.
                    if covered > 1 {
                        let last_absorbed_syl = cur_syl + covered - 1;
                        if let Some(&last_pidx) = syl_to_phoneme.get(last_absorbed_syl) {
                            skip_until = last_pidx + 1;
                        }
                    }
                    hyp_used = true;
                    break 'hyp_loop;
                }
            }
            if hyp_used {
                continue;
            }
        }

        // Step 2: Vowel substitution
        // Extract components from syllable_id using correct bit layout:
        //   bits 0-4: initial (onset, KPS 9566), bits 5-9: medium, bits 10-14: final group
        let initial = (target_id & 0x1F) as usize;
        let medium = ((target_id >> 5) & 0x1F) as usize;
        let final_c = ((target_id >> 10) & 0x1F) as usize;

        if medium < 21 && VOWEL_FLAGS[medium] == 0 {
            // Compound vowel: try substituting with a simpler one
            let sub_medium = VOWEL_SUBSTITUTE[medium] as u16;
            let sub_id = ((final_c as u16) << 10) | (sub_medium << 5) | (initial as u16);

            let mut sub_phoneme = q;
            sub_phoneme.syllable_id = sub_id;
            let sub_selected = select_unit(voice_info, &sub_phoneme, pitch_target);

            if sub_selected.is_some() {
                let mut unit = sub_selected;
                if let Some(ref mut u) = unit {
                    let pcol = (phoneme.prosody as i32) % 10;
                    u.pause_samples = prosody_col_to_pause(pcol);

                    // Coarticulation pause adjustment
                    let coda = (target_id >> 10) & 0x1F;
                    let next_initial = phoneme.next_context & 0x1F;
                    let adj = compute_coarticulation_pause(coda, next_initial, phoneme.prosody);
                    u.pause_samples = u.pause_samples.saturating_add(adj);

                    // Liaison check for vowel-substituted unit
                    let prosody_col = (u.entry.prosody_byte() as i32) % 10;
                    if is_liaison(coda as i16, next_initial as i16) && prosody_col < 2 {
                        u.liaison_entry = find_liaison_entry(voice_info, u.entry.pitch());
                    }
                    prev_match_pitch = u.entry.pitch();
                }
                results.push(unit);
                continue;
            }
        }

        // Step 3: Consonant + Vowel split
        // Hypothesis loop:
        //   Vowel unit:     coda=27(no coda), medium=VOWEL_FIRST_COMPONENT[orig_medium], initial=original_initial
        //   Consonant unit: coda=original_final, medium=VOWEL_SUBSTITUTE[orig_medium], initial=18(ㅇ marker)
        let first_medium = if medium < 21 {
            VOWEL_FIRST_COMPONENT[medium]
        } else {
            medium as u8
        };
        let vowel_id =
            ((CODA_NO_CODA as u16) << 10) | ((first_medium as u16) << 5) | (initial as u16);

        let mut vowel_phoneme = q;
        vowel_phoneme.syllable_id = vowel_id;
        let vowel_unit = select_unit(voice_info, &vowel_phoneme, pitch_target);

        if let Some(mut vu) = vowel_unit {
            vu.pause_samples = 0; // Consonant unit will handle the pause
            results.push(Some(vu));
        } else {
            // Even if vowel unit wasn't found, try with coda=27 and the same jung
            let vowel_no_coda =
                ((CODA_NO_CODA as u16) << 10) | ((first_medium as u16) << 5) | (initial as u16);
            let mut vp2 = q;
            vp2.syllable_id = vowel_no_coda;
            let vu2 = select_unit(voice_info, &vp2, pitch_target);
            if let Some(mut vu) = vu2 {
                vu.pause_samples = 0;
                results.push(Some(vu));
            } else {
                results.push(None);
            }
        }

        // Create consonant onset unit: initial=18(ㅇ marker), substitute medium, original coda
        // Skip if initial is already ㅇ(18) since it's a null consonant (silent onset)
        if initial != IEUNG_INITIAL as usize {
            let cons_medium = if medium < 21 {
                VOWEL_SUBSTITUTE[medium] as u16
            } else {
                0
            };
            // Consonant unit: coda=original_final, medium=substitute, initial=18(ㅇ)
            let cons_id = ((final_c as u16) << 10) | (cons_medium << 5) | (IEUNG_INITIAL as u16);
            let mut cons_phoneme = q;
            cons_phoneme.syllable_id = cons_id;
            cons_phoneme.break_level = 0; // No extra pause on consonant unit

            let cons_unit = select_unit(voice_info, &cons_phoneme, pitch_target);
            if let Some(mut cu) = cons_unit {
                let pcol = (phoneme.prosody as i32) % 10;
                cu.pause_samples = prosody_col_to_pause(pcol);

                // Coarticulation pause and liaison on the consonant unit (carries the real coda)
                let cons_coda = (final_c as u16) & 0x1F;
                let next_initial = phoneme.next_context & 0x1F;
                let adj = compute_coarticulation_pause(cons_coda, next_initial, phoneme.prosody);
                cu.pause_samples = cu.pause_samples.saturating_add(adj);

                let prosody_col = (cu.entry.prosody_byte() as i32) % 10;
                if is_liaison(cons_coda as i16, next_initial as i16) && prosody_col < 2 {
                    cu.liaison_entry = find_liaison_entry(voice_info, cu.entry.pitch());
                }
                prev_match_pitch = cu.entry.pitch();
                results.push(Some(cu));
            }
        }
    }

    results
}

/// If pitch is >threshold away from both neighbours and is a peak/valley, re-`select_unit` at neighbour average.
pub fn smooth_pitch_pass(
    voice_info: &VoiceInfo,
    phonemes: &[PhonemeUnit],
    units: &mut [Option<SelectedUnit>],
    pitch_threshold: i16,
) {
    let n = units.len();
    if n <= 2 {
        return;
    }

    // We need to align `phonemes` with `units`.  Both are indexed identically:
    // select_units_for_sequence produces exactly one Option<SelectedUnit> per
    // PhonemeUnit (pause phonemes → None).
    let phoneme_count = phonemes.len().min(n);

    for i in 1..phoneme_count.saturating_sub(1) {
        // Read pitch values directly via index access.  Each .map() returns
        // Option<i16> (Copy), so the immutable borrow on `units` is released
        // immediately — no intermediate Vec needed.
        let (pp, cp, np) = match (
            units[i - 1].as_ref().map(|u| u.entry.pitch()),
            units[i].as_ref().map(|u| u.entry.pitch()),
            units[i + 1].as_ref().map(|u| u.entry.pitch()),
        ) {
            (Some(a), Some(b), Some(c)) => (a, b, c),
            _ => continue,
        };

        if phonemes[i].emphasis {
            continue;
        }

        let row = (phonemes[i].prosody as i32) / 10;
        let col = (phonemes[i].prosody as i32) % 10;
        if row >= 2 || col >= 2 {
            continue;
        }

        let thr = pitch_threshold as i32;
        let diff_prev = ((cp as i32) - (pp as i32)).unsigned_abs() as i32;
        let diff_next = ((cp as i32) - (np as i32)).unsigned_abs() as i32;

        if diff_prev < thr || diff_next < thr {
            continue;
        }

        let double_diff = ((cp as i32) * 2 - (np as i32) - (pp as i32)).unsigned_abs() as i32;
        if double_diff < thr {
            continue;
        }

        let avg_pitch = ((pp as i32 + np as i32) / 2) as i16;
        let mut smoothed = phonemes[i];
        smoothed.emphasis = false;

        if let Some(new_unit) = select_unit(voice_info, &smoothed, avg_pitch) {
            if let Some(ref mut old) = units[i] {
                new_unit.clone_into(old);
            }
        }
    }
}

/// Prosody column → silence samples (synthesis overwrites; coarticulation delta added after).
fn prosody_col_to_pause(prosody_col: i32) -> i16 {
    match prosody_col {
        0 => 0,
        1 => 0,        // disabled (enable flag at 0xb8 == 0)
        2 => 3000,     // ~136 ms at 22 050 Hz
        3 | 5 => 5000, // ~227 ms
        4 => 20000,    // ~907 ms (sentence-final)
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_liaison() {
        // No liaison when the following initial is in
        // {1, 4, 6, 8, 9, 10, 11, 12, 13, 14, 16, 17, 18}.
        // We verify with an arbitrary coda=0.
        for initial in [1i16, 4, 6, 8, 9, 10, 11, 12, 13, 14, 16, 17, 18] {
            assert!(
                !is_liaison(0, initial),
                "initial={} should be non-liaison",
                initial
            );
        }
        // Liaison when initial NOT in exclusion set
        assert!(is_liaison(0, 0)); // initial=0 → liaison
        assert!(is_liaison(0, 2)); // initial=2 → liaison
        assert!(is_liaison(0, 3)); // initial=3 → liaison (unless coda==6)
        assert!(is_liaison(0, 5)); // initial=5 → liaison
        assert!(is_liaison(0, 7)); // initial=7 → liaison
        assert!(is_liaison(0, 15)); // initial=15 → liaison
        assert!(is_liaison(0, 19)); // initial=19 → liaison
                                    // Special case: initial==3 AND coda==6 → no liaison
        assert!(!is_liaison(6, 3));
        // But initial==3 with other codas → liaison
        assert!(is_liaison(5, 3));
        assert!(is_liaison(0, 3));
    }

    #[test]
    fn test_prosody_col_to_pause() {
        assert_eq!(prosody_col_to_pause(0), 0);
        assert_eq!(prosody_col_to_pause(1), 0); // disabled by default
        assert_eq!(prosody_col_to_pause(2), 3000);
        assert_eq!(prosody_col_to_pause(3), 5000);
        assert_eq!(prosody_col_to_pause(4), 20000);
        assert_eq!(prosody_col_to_pause(5), 5000); // same as col 3
    }

    #[test]
    fn test_prev_importance() {
        // Simple case: both non-zero → level 2
        assert_eq!(compute_prev_importance(0x3024, 0x08F5, 0), 2);
        // Zero context → level 1
        assert_eq!(compute_prev_importance(0x3024, 0, 0), 1);
    }

    #[test]
    fn test_prosody_bonus() {
        assert_eq!(compute_prosody_bonus(0, 0), 600); // exact table match
        assert!(compute_prosody_bonus(0, 10) < 600); // different row
        assert!(compute_prosody_bonus(0, 1) < 600); // different col
    }
}
