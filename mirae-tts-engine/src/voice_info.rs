//! VoiceInfo.pkg — unit index parser (28-byte entries).
//!
//! File layout (verified against real data, see T1_voiceinfo.md §3):
//! `[u32 count][count × 28B entry]`
//!
//! 28B entry (all little-endian):
//! ```text
//! +0x00 u16  phone_cur    current (right) phoneme code — match key; duplicates +0x06
//! +0x02 u16  phone_prev   previous (left) phoneme code — left-context score
//! +0x04 u16  phone_next   next phoneme code — right-context score (diphone link)
//! +0x06 u16  phone_cur2   copy of phone_cur (u32 pair storage)
//! +0x08 u32  woff         sample offset in VoiceData.pkg (byte offset = woff×2)
//! +0x0c u32  wlen         unit length in samples (byte length = wlen×2)
//! +0x10 u16  unk          build-time metadata; never read at runtime
//! +0x12 u16  pitch        pitch feature ≈ F0/3; filter range [78,220]
//! +0x14 u16  classcode    phoneme class code (byte+0x14 = code, byte+0x15 = low digit + bit7)
//! +0x16 u16  flags        byte+0x16: bit7 = special unit (excluded from normal selection)
//! +0x18 i16  pause        pause/silence samples (mostly 0; overwritten at runtime)
//! +0x1a u16  woff_lo      low 16 bits of woff (redundant)
//! ```
//!
//! Verified facts (70,150 entries):
//! - woff chain: woff(N+1) == woff(N) + wlen(N) for all 70,149 links
//! - sum(wlen) = 285,520,544 samples = VoiceData.pkg size
//! - u16[0] == u16[3] for all entries
//! - phoneme code: 16-bit `[bit15-10: high6][bit9-5: mid5][bit4-0: low5]`

use std::io;
use std::path::Path;

/// VoiceInfo 28-byte entry (14 × u16, little-endian).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VoiceInfoEntry {
    /// +0x00 current (right) phoneme code — selection match key.
    pub phone_cur: u16,
    /// +0x02 previous (left) phoneme code — left-context score.
    pub phone_prev: u16,
    /// +0x04 next phoneme code — right-context score.
    pub phone_next: u16,
    /// +0x06 copy of phone_cur (always equal in real data).
    pub phone_cur2: u16,
    /// +0x08 sample offset in VoiceData.pkg (byte offset = woff×2).
    pub woff: u32,
    /// +0x0c unit length in samples (byte length = wlen×2).
    pub wlen: u32,
    /// +0x10 build-time metadata; never read at runtime.
    pub unk_0x10: u16,
    /// +0x12 pitch feature (≈ F0/3); filter [78,220], tie-break key.
    pub pitch: u16,
    /// +0x14 phoneme class code (byte+0x14 = code, byte+0x15 = low digit + bit7 flag).
    pub classcode: u16,
    /// +0x16 flags: byte+0x16 bit7 = special unit (excluded from normal selection).
    pub flags: u16,
    /// +0x18 pause/silence samples (signed; mostly 0, overwritten at runtime).
    pub pause: i16,
    /// +0x1a low 16 bits of woff (redundant).
    pub woff_lo: u16,
}

impl VoiceInfoEntry {
    /// Parse one 28-byte entry (little-endian).
    pub fn from_bytes(b: &[u8; 28]) -> Self {
        let u16at = |off: usize| u16::from_le_bytes([b[off], b[off + 1]]);
        let u32at = |off: usize| u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]);
        VoiceInfoEntry {
            phone_cur: u16at(0x00),
            phone_prev: u16at(0x02),
            phone_next: u16at(0x04),
            phone_cur2: u16at(0x06),
            woff: u32at(0x08),
            wlen: u32at(0x0c),
            unk_0x10: u16at(0x10),
            pitch: u16at(0x12),
            classcode: u16at(0x14),
            flags: u16at(0x16),
            pause: i16::from_le_bytes([b[0x18], b[0x19]]),
            woff_lo: u16at(0x1a),
        }
    }

    /// Normal (non-special) unit: byte+0x16 bit7 clear (`-1 < (char)u16@+0x16`).
    pub fn is_normal(&self) -> bool {
        (self.flags as u8 as i8) >= 0
    }

    /// Special unit: byte+0x16 bit7 set (FUN_0044b220 target set).
    pub fn is_special(&self) -> bool {
        !self.is_normal()
    }

    /// Class code byte (byte+0x14).
    pub fn class_byte(&self) -> u8 {
        (self.classcode & 0xff) as u8
    }

    /// Class high byte (byte+0x15): low digit + bit7 flag.
    pub fn class_hi_byte(&self) -> u8 {
        (self.classcode >> 8) as u8
    }

    /// Pitch as signed value (original compares as `(short)`).
    pub fn pitch_signed(&self) -> i32 {
        self.pitch as i16 as i32
    }

    /// Serialize back to 28 bytes (for round-trip checks).
    pub fn to_bytes(&self) -> [u8; 28] {
        let mut b = [0u8; 28];
        let put_u16 = |b: &mut [u8; 28], off: usize, v: u16| {
            b[off] = (v & 0xff) as u8;
            b[off + 1] = (v >> 8) as u8;
        };
        put_u16(&mut b, 0x00, self.phone_cur);
        put_u16(&mut b, 0x02, self.phone_prev);
        put_u16(&mut b, 0x04, self.phone_next);
        put_u16(&mut b, 0x06, self.phone_cur2);
        b[0x08..0x0c].copy_from_slice(&self.woff.to_le_bytes());
        b[0x0c..0x10].copy_from_slice(&self.wlen.to_le_bytes());
        put_u16(&mut b, 0x10, self.unk_0x10);
        put_u16(&mut b, 0x12, self.pitch);
        put_u16(&mut b, 0x14, self.classcode);
        put_u16(&mut b, 0x16, self.flags);
        b[0x18..0x1a].copy_from_slice(&self.pause.to_le_bytes());
        put_u16(&mut b, 0x1a, self.woff_lo);
        b
    }
}

/// VoiceInfo.pkg contents.
#[derive(Clone, Debug, Default)]
pub struct VoiceInfo {
    /// All entries, in file order (linear scan target).
    pub entries: Vec<VoiceInfoEntry>,
}

impl VoiceInfo {
    /// Parse from raw file bytes (`[u32 count][count × 28B]`).
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < 4 {
            return Err(format!("VoiceInfo: file too short ({})", data.len()));
        }
        let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let need = 4usize
            .checked_add(count.checked_mul(28).ok_or("VoiceInfo: count overflow")?)
            .ok_or("VoiceInfo: size overflow")?;
        if data.len() != need {
            return Err(format!(
                "VoiceInfo: size mismatch: {} != 4 + {}×28 = {}",
                data.len(),
                count,
                need
            ));
        }
        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let b: &[u8; 28] = data[4 + i * 28..4 + (i + 1) * 28]
                .try_into()
                .map_err(|_| "VoiceInfo: entry slice")?;
            entries.push(VoiceInfoEntry::from_bytes(b));
        }
        Ok(VoiceInfo { entries })
    }

    /// Load from a file path.
    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        let data = std::fs::read(path)?;
        Self::from_bytes(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// woff chain integrity: woff(N+1) == woff(N) + wlen(N) for all links.
    pub fn woff_chain_ok(&self) -> bool {
        self.entries
            .windows(2)
            .all(|w| w[1].woff == w[0].woff.wrapping_add(w[0].wlen))
    }

    /// u16[0] == u16[3] duplicate field for all entries.
    pub fn cur_dup_ok(&self) -> bool {
        self.entries.iter().all(|e| e.phone_cur == e.phone_cur2)
    }

    /// Sum of all unit lengths (samples).
    pub fn total_samples(&self) -> u64 {
        self.entries.iter().map(|e| e.wlen as u64).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry0_matches_t1_hexdump() {
        // T1 §3.2 entry0 (verified against real data):
        // 86 6d b3 6e 80 6d 86 6d 00 00 00 00 1a 16 00 00 50 14 56 00 28 00 01 ff f9 ff 00 00
        let raw: [u8; 28] = [
            0x86, 0x6d, 0xb3, 0x6e, 0x80, 0x6d, 0x86, 0x6d, 0x00, 0x00, 0x00, 0x00, 0x1a, 0x16,
            0x00, 0x00, 0x50, 0x14, 0x56, 0x00, 0x28, 0x00, 0x01, 0xff, 0xf9, 0xff, 0x00, 0x00,
        ];
        let e = VoiceInfoEntry::from_bytes(&raw);
        assert_eq!(e.phone_cur, 0x6d86);
        assert_eq!(e.phone_prev, 0x6eb3);
        assert_eq!(e.phone_next, 0x6d80);
        assert_eq!(e.phone_cur2, 0x6d86);
        assert_eq!(e.woff, 0);
        assert_eq!(e.wlen, 5658);
        assert_eq!(e.pitch, 0x56); // 86
        assert_eq!(e.classcode, 0x0028); // 40
        assert_eq!(e.flags, 0xff01);
        assert_eq!(e.pause, -7); // 0xfff9
        assert_eq!(e.woff_lo, 0);
        assert!(e.is_normal());
        assert!(!e.is_special());
        // round trip
        assert_eq!(VoiceInfoEntry::from_bytes(&e.to_bytes()), e);
    }

    #[test]
    fn parse_rejects_truncated() {
        assert!(VoiceInfo::from_bytes(&[0u8; 3]).is_err());
        // count claims 1 entry but no data
        assert!(VoiceInfo::from_bytes(&[1, 0, 0, 0]).is_err());
    }
}
