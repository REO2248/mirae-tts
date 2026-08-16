//! KeyPad.Ebd — UTF-16 → internal-code conversion table.
//!
//! Original loader/converter: `FUN_00421850` (see T4 §2.1, SPEC §2.1):
//! the file `Data\Dictionary\KeyPad.Ebd` is read whole (196,608 B) and
//! treated as a fixed table of **65,536 × 3-byte** entries, indexed by the
//! UTF-16 code unit:
//!
//! ```text
//! entry(code) = [ 1B len ][ payload of len bytes (max 2) ]
//!               table[code*3]        table[code*3+1 .. +1+len]
//! ```
//!
//! Conversion (`FUN_00421850`) concatenates the payload of every code unit
//! of the input text into a NUL-terminated "internal code" byte string
//! (each char → 1–2 bytes; the SPEC's "1〜3" accounts for the len byte
//! itself). Verified against the real file (2026-08):
//!
//! - all 65,536 entries have `len` 1 or 2 (never 0, never ≥ 3)
//! - ASCII 0x00–0x7F → 1-byte identity payload
//! - all 11,172 Hangul syllables → their exact KPS9566 2-byte code
//!   (가 → `b0 a1`, 조 → `bc bf`, 건 → `b0 bc`, …)
//! - all other code units (unassigned, surrogates, U+FFFD …) → 1 byte `0x3F` (`?`)
//! - KPS9566-mappable CJK/fullwidth/jamo chars → 2-byte KPS9566 code
//!
//! The original reads the length byte as a **signed char**; since all real
//! lengths are 1–2 this never matters, and we treat it as unsigned.

use std::io::{self, Read};
use std::path::Path;

/// Table size in bytes: 65,536 entries × 3 bytes.
pub const TABLE_BYTES: usize = 65536 * 3;

/// The original engine's path to KeyPad.Ebd (relative to the app CWD).
pub const DEFAULT_PATH: &str = "Data/Dictionary/KeyPad.Ebd";

/// KeyPad.Ebd conversion table.
#[derive(Debug, Clone)]
pub struct KeyPad {
    /// 196,608-byte table: `[len][payload ≤2B]` per UTF-16 code unit.
    table: Vec<u8>,
}

impl KeyPad {
    /// Parse a raw KeyPad.Ebd buffer. Returns `None` unless the buffer is
    /// exactly 65,536 × 3 bytes.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() != TABLE_BYTES {
            return None;
        }
        Some(KeyPad {
            table: data.to_vec(),
        })
    }

    /// Load and parse a KeyPad.Ebd file.
    pub fn load<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mut f = std::fs::File::open(path)?;
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        Self::parse(&data).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "KeyPad.Ebd: expected exactly 65536*3 bytes",
            )
        })
    }

    /// Look up one UTF-16 code unit → `(len, payload)`.
    ///
    /// The payload slice is `len` bytes (0–2); `len` is 1 or 2 for every
    /// entry in the real table.
    #[inline]
    pub fn entry(&self, code: u16) -> (u8, &[u8]) {
        let off = code as usize * 3;
        let len = (self.table[off] as usize).min(2);
        (len as u8, &self.table[off + 1..off + 1 + len])
    }

    /// Convert UTF-16 code units to the internal-code byte string
    /// (faithful port of `FUN_00421850`; the trailing NUL is not included).
    ///
    /// Every code unit maps to 1–2 bytes (KPS9566 2-byte codes for Korean
    /// and other KPS-mappable chars, identity for ASCII, `0x3F` otherwise).
    pub fn convert(&self, text: &[u16]) -> Vec<u8> {
        let mut out = Vec::with_capacity(text.len() * 2);
        for &code in text {
            let (len, payload) = self.entry(code);
            out.extend_from_slice(&payload[..len as usize]);
        }
        out
    }

    /// Convert a `&str` to the internal-code byte string.
    ///
    /// Surrogate pairs are passed through as two code units (each maps to
    /// `0x3F` in the real table), matching the original which operates on
    /// raw UTF-16 code units.
    pub fn convert_str(&self, text: &str) -> Vec<u8> {
        let units: Vec<u16> = text.encode_utf16().collect();
        self.convert(&units)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keypad() -> KeyPad {
        KeyPad::load("/home/user/reo_work/mirae2_re/extracted/미래2.0/Data/Dictionary/KeyPad.Ebd")
            .expect("real KeyPad.Ebd must load")
    }

    #[test]
    fn parses_real_table() {
        let kp = keypad();
        // spot-check known mappings
        assert_eq!(kp.entry('A' as u16), (1, &[0x41][..]));
        assert_eq!(kp.entry(' ' as u16), (1, &[0x20][..]));
        assert_eq!(kp.entry('가' as u16), (2, &[0xb0, 0xa1][..]));
        assert_eq!(kp.entry('조' as u16), (2, &[0xbc, 0xbf][..]));
        assert_eq!(kp.entry('건' as u16), (2, &[0xb0, 0xbc][..]));
        assert_eq!(kp.entry(0xFFFF), (1, &[0x3F][..]));
    }

    #[test]
    fn all_entries_have_len_1_or_2() {
        let kp = keypad();
        for code in 0u16..=0xFFFF {
            let (len, payload) = kp.entry(code);
            assert!(len == 1 || len == 2, "U+{code:04X} len={len}");
            assert_eq!(payload.len(), len as usize);
        }
    }

    #[test]
    fn converts_korean_text() {
        let kp = keypad();
        // 조건 → KPS9566 bc bf b0 bc (matches Speech.pkg storage)
        assert_eq!(kp.convert_str("조건"), &[0xbc, 0xbf, 0xb0, 0xbc]);
        // 안녕하세요
        assert_eq!(
            kp.convert_str("안녕하세요"),
            &[0xca, 0xaf, 0xb2, 0xce, 0xc2, 0xd7, 0xbb, 0xbd, 0xca, 0xfd]
        );
    }

    #[test]
    fn converts_ascii_and_unmapped() {
        let kp = keypad();
        assert_eq!(kp.convert_str("Hi!"), b"Hi!");
        // unmapped code units → '?'
        assert_eq!(kp.convert(&[0x1234]), &[0x3F]);
        assert_eq!(kp.convert(&[0xD800, 0xDC00]), &[0x3F, 0x3F]);
    }

    #[test]
    fn rejects_wrong_size() {
        assert!(KeyPad::parse(&[0u8; 10]).is_none());
        assert!(KeyPad::parse(&[0u8; TABLE_BYTES - 1]).is_none());
    }
}
