//! VoiceInfo.pkg - unit index parser (28-byte entries: [u32 count][count x 28B]).
//! Entry: +0 cur u16, +2 prev u16, +4 next u16, +6 cur2 u16, +8 woff u32, +0xc wlen u32,
//! +0x10 unk u16, +0x12 pitch u16, +0x14 class u16, +0x16 flags u16, +0x18 pause i16, +0x1a woff_lo.
use std::io;
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VoiceInfoEntry {
    /// +0x00 current (right) phoneme code — selection match key.
    pub phone_cur: u16,
    pub phone_prev: u16,
    pub phone_next: u16,
    pub phone_cur2: u16,
    /// +0x08 sample offset in VoiceData.pkg (byte offset = woff×2).
    pub woff: u32,
    /// +0x0c unit length in samples (byte length = wlen×2).
    pub wlen: u32,
    pub unk_0x10: u16,
    /// +0x12 pitch feature (≈ F0/3); filter [78,220], tie-break key.
    pub pitch: u16,
    /// +0x14 phoneme class code (byte+0x14 = code, byte+0x15 = low digit + bit7 flag).
    pub classcode: u16,
    /// +0x16 flags: byte+0x16 bit7 = special unit (excluded from normal selection).
    pub flags: u16,
    pub pause: i16,
    pub woff_lo: u16,
}

impl VoiceInfoEntry {
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

    pub fn is_normal(&self) -> bool {
        (self.flags as u8 as i8) >= 0
    }

    pub fn is_special(&self) -> bool {
        !self.is_normal()
    }

    pub fn class_byte(&self) -> u8 {
        (self.classcode & 0xff) as u8
    }

    pub fn class_hi_byte(&self) -> u8 {
        (self.classcode >> 8) as u8
    }

    pub fn pitch_signed(&self) -> i32 {
        self.pitch as i16 as i32
    }

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

#[derive(Clone, Debug, Default)]
pub struct VoiceInfo {
    pub entries: Vec<VoiceInfoEntry>,
}

impl VoiceInfo {
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

    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        let data = std::fs::read(path)?;
        Self::from_bytes(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn count(&self) -> usize {
        self.entries.len()
    }

    pub fn woff_chain_ok(&self) -> bool {
        self.entries
            .windows(2)
            .all(|w| w[1].woff == w[0].woff.wrapping_add(w[0].wlen))
    }

    pub fn cur_dup_ok(&self) -> bool {
        self.entries.iter().all(|e| e.phone_cur == e.phone_cur2)
    }

    pub fn total_samples(&self) -> u64 {
        self.entries.iter().map(|e| e.wlen as u64).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry0_matches_t1_hexdump() {
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
        assert_eq!(e.pitch, 0x56);
        assert_eq!(e.classcode, 0x0028);
        assert_eq!(e.flags, 0xff01);
        assert_eq!(e.pause, -7);
        assert_eq!(e.woff_lo, 0);
        assert!(e.is_normal());
        assert!(!e.is_special());
        assert_eq!(VoiceInfoEntry::from_bytes(&e.to_bytes()), e);
    }

    #[test]
    fn parse_rejects_truncated() {
        assert!(VoiceInfo::from_bytes(&[0u8; 3]).is_err());
        assert!(VoiceInfo::from_bytes(&[1, 0, 0, 0]).is_err());
    }
}
