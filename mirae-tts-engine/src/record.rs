//! 12-byte prosody record (SPEC 2.4 / T4 2.3).
//! Layout: +0 prev u16, +2 cur u16, +4 marker u8, +5 flags u8, +6 tone class u8.
use crate::tone;

pub const RECORD_SIZE: usize = 12;

pub const MARKER_SENTENCE_END: u8 = 1;
pub const MARKER_SPECIAL: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProsodyRecord {
    /// +0: previous unit code (logical prev; see module docs).
    pub prev_code: u16,
    /// +2: current unit code.
    pub code: u16,
    /// +4: marker (0, [`MARKER_SENTENCE_END`], [`MARKER_SPECIAL`]).
    pub marker: u8,
    pub flags: u8,
    /// +6: tone class = level×10 + tone (0..4 × 0..9).
    pub tone_class: u8,
}

impl ProsodyRecord {
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

    #[inline]
    pub const fn tone_level(&self) -> u8 {
        self.tone_class / 10
    }

    #[inline]
    pub const fn tone_value(&self) -> u8 {
        self.tone_class % 10
    }

    pub fn to_bytes(&self) -> [u8; RECORD_SIZE] {
        let mut b = [0u8; RECORD_SIZE];
        b[0..2].copy_from_slice(&self.prev_code.to_le_bytes());
        b[2..4].copy_from_slice(&self.code.to_le_bytes());
        b[4] = self.marker;
        b[5] = self.flags;
        b[6] = self.tone_class;
        b
    }

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

/// Apply the pump's initial class mapping (FUN_0044ca50) and set the sentence-end marker.
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
        let r = ProsodyRecord {
            tone_class: 0x35,
            ..ProsodyRecord::new(0)
        };
        assert_eq!(r.tone_level(), 5);
        assert_eq!(r.tone_value(), 3);
        let r = ProsodyRecord {
            tone_class: 0x23,
            ..ProsodyRecord::new(0)
        };
        assert_eq!(r.tone_level(), 3);
        assert_eq!(r.tone_value(), 5);
    }

    #[test]
    fn marker_init_mapping() {
        let mut r = ProsodyRecord::new(0);
        r.init_from_marker(0, true);
        assert_eq!(r.marker, MARKER_SENTENCE_END);
        assert_eq!(r.tone_class, 1);
        assert_eq!(r.flags, 0);
        let mut r = ProsodyRecord::new(0);
        r.init_from_marker(0, false);
        assert_eq!(r.marker, 0);
        assert_eq!(r.tone_class, 0);
        let mut r = ProsodyRecord::new(0);
        r.init_from_marker(0x82, false);
        assert_eq!(r.flags, 1);
        assert_eq!(r.tone_class, 3);
    }
}

