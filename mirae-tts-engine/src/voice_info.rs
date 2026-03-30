//! VoiceInfo.pkg: count N then N×28-byte entries. VoiceData.pkg: s16le mono; `wave_offset` is samples (bytes ×2).
//! 28B layout: 0 phoneme_id, 2 prev, 4 next, 8 wave_off i32, 12 wave_len i32, 16 quality i16, 18 pitch i16, 20 prosody i8 (/10=row,%10=col), 22 flags (bit7=emphasis), 24 pause i16.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct VoiceInfoEntry {
    pub raw: [u8; 28],
}

impl VoiceInfoEntry {
    pub fn phoneme_id(&self) -> u16 {
        u16::from_le_bytes([self.raw[0], self.raw[1]])
    }

    pub fn prev_context(&self) -> u16 {
        u16::from_le_bytes([self.raw[2], self.raw[3]])
    }

    pub fn next_context(&self) -> u16 {
        u16::from_le_bytes([self.raw[4], self.raw[5]])
    }

    pub fn wave_offset(&self) -> u32 {
        u32::from_le_bytes([self.raw[8], self.raw[9], self.raw[10], self.raw[11]])
    }

    pub fn wave_samples(&self) -> u32 {
        u32::from_le_bytes([self.raw[12], self.raw[13], self.raw[14], self.raw[15]])
    }

    /// Mirae: `% 10000 == 0` may select an alternate take.
    pub fn quality_marker(&self) -> i16 {
        i16::from_le_bytes([self.raw[16], self.raw[17]])
    }

    pub fn entry_pause_samples(&self) -> i16 {
        i16::from_le_bytes([self.raw[24], self.raw[25]])
    }

    pub fn pitch(&self) -> i16 {
        i16::from_le_bytes([self.raw[18], self.raw[19]])
    }

    pub fn prosody_byte(&self) -> i8 {
        self.raw[20] as i8
    }

    pub fn prosody_row(&self) -> i32 {
        (self.prosody_byte() as i32) / 10
    }

    pub fn prosody_col(&self) -> i32 {
        (self.prosody_byte() as i32) % 10
    }

    /// bit7 = emphasis
    pub fn flags_byte(&self) -> u8 {
        self.raw[22]
    }

    pub fn is_emphasis(&self) -> bool {
        (self.raw[22] & 0x80) != 0
    }

    /// flags byte as signed: negative ⇒ invalid entry (skip)
    pub fn is_valid(&self) -> bool {
        (self.raw[22] as i8) >= 0
    }
}

#[derive(Debug)]
pub struct VoiceInfo {
    pub entries: Vec<VoiceInfoEntry>,
}

impl VoiceInfo {
    pub fn load<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mut f = File::open(path.as_ref())?;

        let mut buf4 = [0u8; 4];
        f.read_exact(&mut buf4)?;
        let count = u32::from_le_bytes(buf4) as usize;

        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let mut entry = VoiceInfoEntry::default();
            f.read_exact(&mut entry.raw)?;
            entries.push(entry);
        }

        Ok(VoiceInfo { entries })
    }

    pub fn find_best_unit(
        &self,
        target_phoneme: u16,
        prev_ctx: u16,
        next_ctx: u16,
        prosody: i8,
        target_pitch: i16,
    ) -> Option<(usize, i32)> {
        let mut best_idx: Option<usize> = None;
        let mut best_score: i32 = i32::MIN;
        let mut best_pitch_dist: i32 = i32::MAX;

        for (i, entry) in self.entries.iter().enumerate() {
            if !entry.is_valid() {
                continue;
            }

            if entry.phoneme_id() != target_phoneme {
                continue;
            }

            let mut score: i32 = 100;

            let prev_score = Self::context_score(prev_ctx, entry.prev_context());
            score += prev_score;

            let next_score = Self::context_score_next(next_ctx, entry.next_context());
            score += next_score;

            let entry_prosody = entry.prosody_byte();
            if entry_prosody == prosody {
                score += 50;
            } else if entry_prosody / 10 == prosody / 10 {
                score += 20;
            }

            let pitch_dist = ((entry.pitch() as i32) - (target_pitch as i32)).abs();

            if score > best_score || (score == best_score && pitch_dist < best_pitch_dist) {
                best_score = score;
                best_idx = Some(i);
                best_pitch_dist = pitch_dist;
            }
        }

        best_idx.map(|idx| (idx, best_score))
    }

    fn context_score(target: u16, candidate: u16) -> i32 {
        if target == candidate {
            80
        } else if (target ^ candidate) & 0xFFE0 == 0 {
            // onset class (upper bits match)
            70
        } else if target >> 10 == candidate >> 10 {
            40
        } else {
            20
        }
    }

    fn context_score_next(target: u16, candidate: u16) -> i32 {
        if target == candidate {
            80
        } else if (target ^ candidate) & 0x3FF == 0 {
            // vowel side: lower 10 bits
            70
        } else if (target & 0x1F) == (candidate & 0x1F) {
            // coda class: low 5 bits
            40
        } else {
            20
        }
    }
}

pub struct VoiceDataReader {
    file: File,
    len: usize,
}

fn read_exact_at(file: &File, buf: &mut [u8], mut offset: u64) -> io::Result<()> {
    let mut filled = 0usize;
    while filled < buf.len() {
        let n = read_at_os(file, &mut buf[filled..], offset)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "VoiceData.pkg: unexpected EOF while reading range",
            ));
        }
        filled += n;
        offset += n as u64;
    }
    Ok(())
}

#[cfg(unix)]
fn read_at_os(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.read_at(buf, offset)
}

#[cfg(windows)]
fn read_at_os(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_read(buf, offset)
}

#[cfg(not(any(unix, windows)))]
fn read_at_os(_file: &File, _buf: &mut [u8], _offset: u64) -> io::Result<usize> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "VoiceDataReader: only Unix and Windows are supported",
    ))
}

impl VoiceDataReader {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = File::open(path.as_ref())?;
        let len = file.metadata()?.len() as usize;
        Ok(VoiceDataReader { file, len })
    }

    pub fn read_samples(&self, entry: &VoiceInfoEntry) -> io::Result<Vec<i16>> {
        let offset = entry.wave_offset() as usize;
        let sample_count = entry.wave_samples() as usize;

        if sample_count == 0 {
            return Ok(Vec::new());
        }

        let byte_start = offset * 2;
        let byte_end = byte_start + sample_count * 2;

        if byte_end > self.len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "VoiceData.pkg: range {}..{} exceeds file size {}",
                    byte_start, byte_end, self.len
                ),
            ));
        }

        let mut raw = vec![0u8; sample_count * 2];
        read_exact_at(&self.file, &mut raw, byte_start as u64)?;

        let mut samples = Vec::with_capacity(sample_count);
        for chunk in raw.chunks_exact(2) {
            samples.push(i16::from_le_bytes([chunk[0], chunk[1]]));
        }

        Ok(samples)
    }

    pub fn read_raw(&self, byte_offset: u64, byte_count: usize) -> io::Result<Vec<u8>> {
        let start = byte_offset as usize;
        let end = start + byte_count;

        if end > self.len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "VoiceData.pkg: range {}..{} exceeds file size {}",
                    start, end, self.len
                ),
            ));
        }

        let mut buf = vec![0u8; byte_count];
        read_exact_at(&self.file, &mut buf, byte_offset)?;
        Ok(buf)
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}
