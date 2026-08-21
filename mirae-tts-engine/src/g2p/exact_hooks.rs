//! Stage3/4/6 "exact hooks": byte-level accessors over a captured original
//! analyzer record (`RawSecondRecord`) plus the rule predicates wired into
//! [`super::PostprocessHooks`] for analyzer-parity runs.
use std::fmt;

use super::*;

pub const SECOND_RECORD_SIZE: usize = 0x1dcc;
const SECOND_MORPH_CONTEXT_OFFSET: usize = 0x1578;
const SECOND_MORPH_TYPE_OFFSET: usize = 0x157c;
const SECOND_MORPH_FLAGS_OFFSET: usize = 0x15cc;
const SECOND_COUNT_OFFSET: usize = 0x1db0;
// Offsets mapped in the phase-3 report but not yet consumed by the
// high-level path; kept as documentation of the original record layout.
#[allow(dead_code)]
const SECOND_RULE_FLAGS_OFFSET: usize = 0x1db4;
#[allow(dead_code)]
const SECOND_FLAG_LINK_OFFSET: usize = 0x1db8;
#[allow(dead_code)]
const SECOND_RULE_MARKER_OFFSET: usize = 0x1db9;
#[allow(dead_code)]
const SECOND_ACCENT_OFFSET: usize = 0x1dba;
// The phase-3 workspace report maps the stage-7 scratch fields
// (+0xb5c8/+0xb5cc/+0xb5d0 and +0xb5d4) to these R_i offsets.
#[allow(dead_code)]
const SECOND_STAGE7_SMOOTHING_OFFSETS: [usize; 3] = [0x1dbc, 0x1dc0, 0x1dc4];
#[allow(dead_code)]
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
                && matches!(morph_type(left, n + 1), 0x17..=0x19));
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
        && (matches!(next_type, 1..=6) || (next_type == 0x0c && morph_type(right, 4) == 4))
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
            || (morph_type(left, 3) == 4 || (morph_type(left, 3) == 1 && morph_type(left, 4) == 4));
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
pub fn stage4_cross_word_sandhi_with_hooks(records: &mut [WordRecord], hooks: &PostprocessHooks) {
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
