//! Phoneme stream for VoiceInfo. Legacy prosody = row*10+col; `NO_CONTEXT` = 0xFFFF.

use crate::g2p;
use crate::korean::{DecomposedChar, PauseType, decompose_text, pack_syllable};
use crate::number::apply_number_conversion;
use crate::segmenter::BreakType;

#[derive(Debug, Clone, Copy)]
pub struct PhonemeUnit {
    pub syllable_id: u16,
    pub prev_context: u16,
    pub next_context: u16,
    pub prosody: i8,
    pub emphasis: bool,
    pub pause: Option<PauseType>,
    pub break_level: u8,
    pub colligation_variant: bool,
}

/// 0x0000 is a valid syllable id, so “no context” uses 0xFFFF.
const NO_CONTEXT: u16 = 0xFFFF;

/// `more_segments_follow`: more text in the same utterance → do not force sentence-final col=4 on the last syllable (avoids bad pauses next to digits/English).
pub fn text_to_phonemes_with_context(
    text: &str,
    prev_col: i8,
    trailing_break: Option<BreakType>,
    trailing_word_gap: bool,
    more_segments_follow: bool,
    out_units: &mut Vec<PhonemeUnit>,
) -> i8 {
    let normalized = apply_number_conversion(text, true);
    let mut decomposed = decompose_text(&normalized);

    if trailing_word_gap {
        decomposed.push(DecomposedChar::Space);
    }
    if let Some(bt) = trailing_break {
        let pt = match bt {
            BreakType::Clause => PauseType::Comma,
            BreakType::Sentence => PauseType::Period,
            BreakType::None => PauseType::Break,
        };
        decomposed.push(DecomposedChar::Pause(pt));
    }

    let mut syllable_ids: Vec<u16> = Vec::new();
    let mut decomp_types: Vec<DecomposedChar> = Vec::new();

    for dc in &decomposed {
        match dc {
            DecomposedChar::KoreanSyllable(jamo) => {
                syllable_ids.push(pack_syllable(jamo));
                decomp_types.push(*dc);
            }
            DecomposedChar::Space => {
                syllable_ids.push(NO_CONTEXT);
                decomp_types.push(*dc);
            }
            DecomposedChar::Pause(_pt) => {
                syllable_ids.push(NO_CONTEXT);
                decomp_types.push(*dc);
            }
            DecomposedChar::Other(code) => {
                syllable_ids.push(*code);
                decomp_types.push(*dc);
            }
        }
    }

    if syllable_ids.is_empty() {
        return prev_col;
    }

    g2p::apply_g2p(&mut syllable_ids);
    g2p::neutralize_codas(&mut syllable_ids);

    let total = syllable_ids.len();
    let mut current_prev_col: i8 = prev_col;

    for i in 0..total {
        let dc = &decomp_types[i];
        let sid = syllable_ids[i];

        match dc {
            DecomposedChar::KoreanSyllable(_) => {
                let prev_ctx = if i > 0 {
                    match &decomp_types[i - 1] {
                        DecomposedChar::KoreanSyllable(_) => syllable_ids[i - 1],
                        _ => NO_CONTEXT,
                    }
                } else {
                    NO_CONTEXT
                };

                let next_ctx = if i + 1 < total {
                    match &decomp_types[i + 1] {
                        DecomposedChar::KoreanSyllable(_) => syllable_ids[i + 1],
                        _ => NO_CONTEXT,
                    }
                } else {
                    NO_CONTEXT
                };

                let col = compute_col(i, total, &decomp_types, more_segments_follow);
                let row = current_prev_col;
                let prosody = row * 10 + col;
                current_prev_col = col;

                out_units.push(PhonemeUnit {
                    syllable_id: sid,
                    prev_context: prev_ctx,
                    next_context: next_ctx,
                    prosody,
                    emphasis: false,
                    pause: None,
                    break_level: 0,
                    colligation_variant: false,
                });
            }
            DecomposedChar::Pause(pt) => {
                let break_level = match pt {
                    PauseType::Comma => 2,
                    PauseType::Period => 4,
                    PauseType::Question => 4,
                    PauseType::Exclamation => 4,
                    PauseType::Break => 3,
                };

                out_units.push(PhonemeUnit {
                    syllable_id: NO_CONTEXT,
                    prev_context: if i > 0 {
                        syllable_ids[i - 1]
                    } else {
                        NO_CONTEXT
                    },
                    next_context: if i + 1 < total {
                        syllable_ids[i + 1]
                    } else {
                        NO_CONTEXT
                    },
                    prosody: 0,
                    emphasis: false,
                    pause: Some(*pt),
                    break_level,
                    colligation_variant: false,
                });
            }
            DecomposedChar::Space => {
                if let Some(last) = out_units.last_mut()
                    && last.break_level < 1
                {
                    last.break_level = 1;
                }
            }
            DecomposedChar::Other(_) => {}
        }
    }

    current_prev_col
}

fn compute_col(
    pos: usize,
    total: usize,
    decomp_types: &[DecomposedChar],
    more_segments_follow: bool,
) -> i8 {
    if let Some(decomp) = decomp_types.iter().take(total).nth(pos + 1) {
        return match decomp {
            DecomposedChar::KoreanSyllable(_) | DecomposedChar::Other(_) => 0,
            DecomposedChar::Space => 1,
            DecomposedChar::Pause(PauseType::Comma) | DecomposedChar::Pause(PauseType::Break) => 3,
            DecomposedChar::Pause(PauseType::Period)
            | DecomposedChar::Pause(PauseType::Question)
            | DecomposedChar::Pause(PauseType::Exclamation) => 4,
        };
    }
    if more_segments_follow { 0 } else { 4 }
}

/// seg_type / flags: Mirae segment encoding (1=Korean, 10=number, …).
pub fn compute_emphasis(seg_type: u8, flags: u8) -> bool {
    match seg_type {
        1 | 10 => (flags & 0x0c) == 8,
        3 | 0x0e => (flags & 0x1c) == 8,
        4 | 5 => (flags & 0x03) == 2,
        _ => false,
    }
}

pub fn raw_break_type_to_level(raw: u8, is_last: bool) -> u8 {
    match raw {
        0 => {
            if is_last {
                1
            } else {
                0
            }
        }
        1 => 1,
        2 | 5 => 3,
        3 => 2,
        6 => 5,
        7 => 4,
        _ => 0,
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PitchConfig {
    pub mode1: Option<u16>,
    pub mode2: Option<u16>,
    pub mode3: Option<u16>,
    pub mode4: Option<u16>,
}

pub fn get_boundary_pitch(mode: u8, config: &PitchConfig) -> u16 {
    match mode {
        1 => config.mode1.unwrap_or(0),
        2 => config.mode2.unwrap_or(0),
        3 | 5 => config.mode3.unwrap_or(0),
        4 => config.mode4.unwrap_or(0),
        _ => 0,
    }
}

/// Get the phoneme byte sequence for a syllable ID (for debugging/display).
pub fn syllable_id_to_phoneme_string(sid: u16) -> String {
    if sid == 0xFFFF {
        return "SIL".to_string();
    }
    if sid & 0x8000 != 0 {
        return format!("NONK_{:04X}", sid);
    }

    // Correct bit layout: coda(10-14), medium(5-9), initial(0-4)
    let initial = (sid & 0x1F) as u8;
    let medium = ((sid >> 5) & 0x1F) as u8;
    let coda = ((sid >> 10) & 0x1F) as u8;

    let mut s = String::new();
    s.push_str(&format!("O{:02X}", initial));
    s.push_str(&format!("V{:02X}", medium));
    s.push_str(&format!("C{:02X}", coda));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_to_phonemes_single_segment(text: &str) -> Vec<PhonemeUnit> {
        let mut units = Vec::new();
        text_to_phonemes_with_context(text, 4, None, false, false, &mut units);
        units
    }

    #[test]
    fn test_compute_emphasis() {
        // seg_type=1, flags=0x08: (0x08 & 0x0c)==8 → true
        assert!(compute_emphasis(1, 0x08));
        // seg_type=1, flags=0x04: (0x04 & 0x0c)==4 ≠ 8 → false
        assert!(!compute_emphasis(1, 0x04));
        // seg_type=10, flags=0x0c: (0x0c & 0x0c)==0x0c ≠ 8 → false
        assert!(!compute_emphasis(10, 0x0c));
        // seg_type=3, flags=0x08: (0x08 & 0x1c)==8 → true
        assert!(compute_emphasis(3, 0x08));
        // seg_type=0x0e, flags=0x18: (0x18 & 0x1c)==0x18 ≠ 8 → false
        assert!(!compute_emphasis(0x0e, 0x18));
        // seg_type=4, flags=0x02: (0x02 & 0x03)==2 → true
        assert!(compute_emphasis(4, 0x02));
        // seg_type=5, flags=0x03: (0x03 & 0x03)==3 ≠ 2 → false
        assert!(!compute_emphasis(5, 0x03));
        // seg_type=7, flags=0x08: default → false
        assert!(!compute_emphasis(7, 0x08));
    }

    #[test]
    fn test_raw_break_type_to_level() {
        assert_eq!(raw_break_type_to_level(0, false), 0);
        assert_eq!(raw_break_type_to_level(0, true), 1);
        assert_eq!(raw_break_type_to_level(1, false), 1);
        assert_eq!(raw_break_type_to_level(2, false), 3);
        assert_eq!(raw_break_type_to_level(5, false), 3);
        assert_eq!(raw_break_type_to_level(3, false), 2);
        assert_eq!(raw_break_type_to_level(6, false), 5);
        assert_eq!(raw_break_type_to_level(7, false), 4);
    }

    #[test]
    fn test_prosody_inter_segment_continuation() {
        let mut units = Vec::new();
        text_to_phonemes_with_context("백이십삼", 4, None, false, true, &mut units);
        let syllables: Vec<_> = units.iter().filter(|u| u.pause.is_none()).collect();
        assert!(!syllables.is_empty());
        assert_eq!(
            syllables.last().unwrap().prosody % 10,
            0,
            "with more_segments_follow, last syllable must not use sentence-final col"
        );

        let mut units2 = Vec::new();
        text_to_phonemes_with_context("백이십삼", 4, None, false, false, &mut units2);
        let syllables2: Vec<_> = units2.iter().filter(|u| u.pause.is_none()).collect();
        assert_eq!(syllables2.last().unwrap().prosody % 10, 4);
    }

    #[test]
    fn test_basic_text_to_phonemes() {
        let units = text_to_phonemes_single_segment("동지");
        assert_eq!(units.len(), 2);
        // 동: initial=ㄷ(KPS 2), medium=ㅗ(KPS 4), coda=ㅇ group(18)
        // packed = (18 << 10) | (4 << 5) | 2 = 0x4882
        assert_eq!(units[0].syllable_id, 0x4882);
        // 지: initial=ㅈ(KPS 7), medium=ㅣ(KPS 9), coda=none(27)
        // packed = (27 << 10) | (9 << 5) | 7 = 0x6D27
        assert_eq!(units[1].syllable_id, 0x6D27);
    }

    #[test]
    fn test_text_with_punctuation() {
        let units = text_to_phonemes_single_segment("동지.");
        assert!(units.len() >= 2);
        // Should have the last unit be a pause
        let last = units.last().unwrap();
        assert!(last.pause.is_some());
    }

    #[test]
    fn test_get_boundary_pitch() {
        let config = PitchConfig {
            mode1: Some(120),
            mode2: Some(180),
            mode3: Some(100),
            mode4: Some(90),
        };
        assert_eq!(get_boundary_pitch(1, &config), 120);
        assert_eq!(get_boundary_pitch(2, &config), 180);
        assert_eq!(get_boundary_pitch(3, &config), 100);
        assert_eq!(get_boundary_pitch(5, &config), 100); // mode 5 → same as mode 3
        assert_eq!(get_boundary_pitch(4, &config), 90);
        assert_eq!(get_boundary_pitch(0, &config), 0); // unknown mode → 0
        assert_eq!(get_boundary_pitch(6, &config), 0);
    }

    #[test]
    fn test_prosody_phrase_start() {
        // First phoneme of any text → row=4 (phrase start), col depends on following boundary
        // "가" alone (end of text) → col=4, row=4, prosody=44
        let units = text_to_phonemes_single_segment("가");
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].prosody, 44); // row=4 (phrase-start) + col=4 (sentence-final)
    }

    #[test]
    fn test_prosody_word_boundary() {
        // "가 나" → two syllables with space:
        //   가: row=4(initial), col=1(space follows), prosody=41
        //   나: row=1(prev col), col=4(end-of-text), prosody=14
        let units = text_to_phonemes_single_segment("가 나");
        assert_eq!(units.len(), 2);
        assert_eq!(units[0].prosody, 41); // row=4, col=1
        assert_eq!(units[1].prosody, 14); // row=1, col=4
    }

    #[test]
    fn test_prosody_two_syllables_no_break() {
        // "가나" → no space, no punctuation
        //   가: row=4(initial), col=0(no break follows), prosody=40
        //   나: row=0(prev col), col=4(end-of-text), prosody=4
        let units = text_to_phonemes_single_segment("가나");
        assert_eq!(units.len(), 2);
        assert_eq!(units[0].prosody, 40); // row=4, col=0
        assert_eq!(units[1].prosody, 4); // row=0, col=4
    }

    #[test]
    fn test_prosody_comma() {
        // "가, 나" → comma between syllables:
        //   가: row=4, col=3 (comma follows), prosody=43
        //   나: row=3 (from comma col), col=4 (end-of-text), prosody=34
        let units = text_to_phonemes_single_segment("가, 나");
        // First unit is 가, ignoring the pause unit
        let syllables: Vec<_> = units.iter().filter(|u| u.pause.is_none()).collect();
        assert!(syllables.len() >= 2);
        assert_eq!(syllables[0].prosody, 43); // row=4, col=3
        assert_eq!(syllables[1].prosody, 34); // row=3, col=4
    }

    #[test]
    fn test_prosody_period_reset() {
        // "가. 나" → period resets next phrase to row=4
        //   가: row=4, col=4 (period follows), prosody=44
        //   나: row=4 (from preceding period col), col=4 (end), prosody=44
        let units = text_to_phonemes_single_segment("가. 나");
        let syllables: Vec<_> = units.iter().filter(|u| u.pause.is_none()).collect();
        assert!(syllables.len() >= 2);
        assert_eq!(syllables[0].prosody, 44); // row=4, col=4
        assert_eq!(syllables[1].prosody, 44); // row=4, col=4
    }

    #[test]
    fn test_prosody_sentence_annyeonghashimnikka() {
        // "안녕하십니까." (6 syllables + period)
        //   안: row=4, col=0, prosody=40
        //   녕: row=0, col=0, prosody=0
        //   하: row=0, col=0, prosody=0
        //   십: row=0, col=0, prosody=0
        //   니: row=0, col=0, prosody=0
        //   까: row=0, col=4 (period), prosody=4
        let units = text_to_phonemes_single_segment("안녕하십니까.");
        let syllables: Vec<_> = units.iter().filter(|u| u.pause.is_none()).collect();
        assert_eq!(syllables.len(), 6);
        assert_eq!(syllables[0].prosody, 40);
        assert_eq!(syllables[1].prosody, 0);
        assert_eq!(syllables[2].prosody, 0);
        assert_eq!(syllables[3].prosody, 0);
        assert_eq!(syllables[4].prosody, 0);
        assert_eq!(syllables[5].prosody, 4);
    }

    #[test]
    fn test_prosody_multi_word() {
        // "조국을 지키자" → 조국을 (공백) 지키자
        //   조: row=4, col=0, prosody=40
        //   국: row=0, col=0, prosody=0
        //   을: row=0, col=1 (space), prosody=1
        //   지: row=1, col=0, prosody=10
        //   키: row=0, col=0, prosody=0
        //   자: row=0, col=4 (end), prosody=4
        let units = text_to_phonemes_single_segment("조국을 지키자");
        let syllables: Vec<_> = units.iter().filter(|u| u.pause.is_none()).collect();
        assert_eq!(syllables.len(), 6);
        assert_eq!(syllables[0].prosody, 40); // 조: phrase start
        assert_eq!(syllables[1].prosody, 0); // 국
        assert_eq!(syllables[2].prosody, 1); // 을: before space
        assert_eq!(syllables[3].prosody, 10); // 지: after word break
        assert_eq!(syllables[4].prosody, 0); // 키
        assert_eq!(syllables[5].prosody, 4); // 자: sentence-final
    }

    #[test]
    fn test_get_boundary_pitch_disabled() {
        let config = PitchConfig::default();
        // All modes disabled → always 0.
        for m in 0u8..8 {
            assert_eq!(get_boundary_pitch(m, &config), 0);
        }
    }
}
