//! 韻律レコード — 12-byte prosody record (SPEC §2.4 / T4 §2.3).
//!
//! The G2P stage emits up to 1,000 records per sentence; the engine pump
//! (`FUN_0044ca50`) copies them into a 12-byte-per-record buffer
//! (engine +0xa8, 150,000 records) and applies tone sandhi
//! (see [`crate::tone`]).
//!
//! Layout (offsets within the 12 bytes):
//! ```text
//! +0  u16 前ユニットコード (prev unit code — logical; the original leaves
//!          it unused in the engine buffer and reads the previous record's
//!          +2 directly, FUN_0044b880)
//! +2  u16 現ユニットコード (current unit code)
//! +4  u8  マーカ (1 = 文末, 2 = 特殊/文頭継続 — set by the pump)
//! +5  u8  フラグ (pump stores 0/1 from the G2P marker bit7; bit7 → 選択時 0x80)
//! +6  u8  声調クラス = レベル×10 + 調値 (0..4 × 0..9)
//! +7..11  未使用 (padding)
//! ```

use crate::tone;

/// Byte size of one prosody record.
pub const RECORD_SIZE: usize = 12;

/// Marker value: sentence end (`+4 == 1`).
pub const MARKER_SENTENCE_END: u8 = 1;
/// Marker value: special / sentence-head continuation (`+4 == 2`).
pub const MARKER_SPECIAL: u8 = 2;

/// 12-byte prosody record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProsodyRecord {
    /// +0: previous unit code (logical prev; see module docs).
    pub prev_code: u16,
    /// +2: current unit code.
    pub code: u16,
    /// +4: marker (0, [`MARKER_SENTENCE_END`], [`MARKER_SPECIAL`]).
    pub marker: u8,
    /// +5: flags (0/1; bit7 semantics applied at selection time).
    pub flags: u8,
    /// +6: tone class = level×10 + tone (0..4 × 0..9).
    pub tone_class: u8,
}

impl ProsodyRecord {
    /// New record with the given unit code; everything else zeroed.
    #[inline]
    pub const fn new(code: u16) -> Self {
        ProsodyRecord {
            prev_code: 0,
            code,
            marker: 0,
            flags: 0,
            tone_class: 0,
        }
    }

    /// Tone level (`cls / 10`, 0..4).
    #[inline]
    pub const fn tone_level(&self) -> u8 {
        self.tone_class / 10
    }

    /// Tone value (`cls % 10`, 0..9).
    #[inline]
    pub const fn tone_value(&self) -> u8 {
        self.tone_class % 10
    }

    /// Serialize to the 12-byte layout (little-endian u16s, like the
    /// original x86 in-memory representation).
    pub fn to_bytes(&self) -> [u8; RECORD_SIZE] {
        let mut b = [0u8; RECORD_SIZE];
        b[0..2].copy_from_slice(&self.prev_code.to_le_bytes());
        b[2..4].copy_from_slice(&self.code.to_le_bytes());
        b[4] = self.marker;
        b[5] = self.flags;
        b[6] = self.tone_class;
        b
    }

    /// Parse from the 12-byte layout. Returns `None` for short buffers.
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < RECORD_SIZE {
            return None;
        }
        Some(ProsodyRecord {
            prev_code: u16::from_le_bytes([b[0], b[1]]),
            code: u16::from_le_bytes([b[2], b[3]]),
            marker: b[4],
            flags: b[5],
            tone_class: b[6],
        })
    }

    /// Apply the pump's initial class mapping for this record
    /// (`FUN_0044ca50` switch on the G2P marker byte `& 0x7f`), and set the
    /// sentence-end marker when `sentence_final`. Used by
    /// [`crate::tone::apply_sandhi`].
    #[inline]
    pub(crate) fn init_from_marker(&mut self, marker_byte: u8, sentence_final: bool) {
        self.flags = (marker_byte >> 7) & 1;
        let m = marker_byte & 0x7F;
        self.tone_class = tone::initial_tone_class(m);
        if m == 0 && sentence_final {
            self.marker = MARKER_SENTENCE_END;
            self.tone_class = 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_bytes() {
        let r = ProsodyRecord {
            prev_code: 0x1234,
            code: 0x6D86,
            marker: 2,
            flags: 0x80,
            tone_class: 0x28,
        };
        let b = r.to_bytes();
        assert_eq!(b.len(), 12);
        assert_eq!(&b[0..2], &[0x34, 0x12]);
        assert_eq!(&b[2..4], &[0x86, 0x6D]);
        assert_eq!(b[4], 2);
        assert_eq!(b[5], 0x80);
        assert_eq!(b[6], 0x28);
        assert_eq!(ProsodyRecord::from_bytes(&b), Some(r));
        assert_eq!(ProsodyRecord::from_bytes(&b[..10]), None);
    }

    #[test]
    fn default_and_new() {
        let r = ProsodyRecord::new(0x0100);
        assert_eq!(r.code, 0x0100);
        assert_eq!(r.tone_class, 0);
        assert_eq!(ProsodyRecord::default().code, 0);
    }

    #[test]
    fn level_and_tone() {
        // 0x35 = 53 = level 5 × 10 + tone 3
        let r = ProsodyRecord {
            tone_class: 0x35,
            ..ProsodyRecord::new(0)
        };
        assert_eq!(r.tone_level(), 5);
        assert_eq!(r.tone_value(), 3);
        // 0x23 = 35 = level 3 × 10 + tone 5
        let r = ProsodyRecord {
            tone_class: 0x23,
            ..ProsodyRecord::new(0)
        };
        assert_eq!(r.tone_level(), 3);
        assert_eq!(r.tone_value(), 5);
    }

    #[test]
    fn marker_init_mapping() {
        // sentence-final marker-0 record → marker 1, class 1
        let mut r = ProsodyRecord::new(0);
        r.init_from_marker(0, true);
        assert_eq!(r.marker, MARKER_SENTENCE_END);
        assert_eq!(r.tone_class, 1);
        assert_eq!(r.flags, 0);
        // non-final marker 0 → class 0
        let mut r = ProsodyRecord::new(0);
        r.init_from_marker(0, false);
        assert_eq!(r.marker, 0);
        assert_eq!(r.tone_class, 0);
        // bit7 flag preserved: marker 0x80|2 → flags 1, class 3
        let mut r = ProsodyRecord::new(0);
        r.init_from_marker(0x82, false);
        assert_eq!(r.flags, 1);
        assert_eq!(r.tone_class, 3);
    }
}
