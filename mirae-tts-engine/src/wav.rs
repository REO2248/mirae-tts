//! WAV output (FUN_0042b630): 46-byte header + PCM.
//! Header: "RIFF" | u32(data+0x30) | "WAVE" | "fmt " | u32(0x12) | u16(1) | u16(1) |
//! u32(22050) | u32(44100) | u16(2) | u16(16) | u16(0) | "data" | u32(data_size).
use std::fs::File;
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const WAV_HEADER_SIZE: usize = 46;

pub const SPLIT_THRESHOLD_BYTES: u64 = 26_000_000;

pub const SAMPLE_RATE: u32 = 22050;

/// Byte-exact 46-byte WAV header replica (46B fmt chunk = WAVEFORMATEX,
/// RIFF size quirk +0x30). Single source shared by [`write_wav_header`] and
/// [`crate::wave_render::encode_wav_vec`].
pub fn wav_header_bytes(data_size: u32, sample_rate: u32) -> [u8; WAV_HEADER_SIZE] {
    let mut h = [0u8; WAV_HEADER_SIZE];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(data_size + 0x30).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&0x12u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&1u16.to_le_bytes());
    h[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    h[28..32].copy_from_slice(&(sample_rate * 2).to_le_bytes());
    h[32..34].copy_from_slice(&2u16.to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..38].copy_from_slice(&0u16.to_le_bytes()); // WAVEFORMATEX cbSize = 0
    h[38..42].copy_from_slice(b"data");
    h[42..46].copy_from_slice(&data_size.to_le_bytes());
    h
}

pub fn write_wav_header<W: Write + Seek>(w: &mut W, data_size: u32) -> io::Result<()> {
    w.seek(SeekFrom::Start(0))?;
    w.write_all(&wav_header_bytes(data_size, SAMPLE_RATE))
}

pub fn write_wav_file(path: &Path, pcm: &[u8]) -> io::Result<u64> {
    let mut w = WavWriter::create(path)?;
    w.append(pcm)?;
    w.finish()
}

pub struct WavWriter {
    file: Option<File>,
    path: PathBuf,
    data_size: u64,
    split_index: usize,
    split_threshold: u64,
}

impl WavWriter {
    pub fn create(path: &Path) -> io::Result<Self> {
        Self::create_with_threshold(path, SPLIT_THRESHOLD_BYTES)
    }

    pub fn create_with_threshold(path: &Path, split_threshold: u64) -> io::Result<Self> {
        let mut file = File::create(path)?;
        file.seek(SeekFrom::Start(WAV_HEADER_SIZE as u64))?;
        Ok(WavWriter {
            file: Some(file),
            path: path.to_path_buf(),
            data_size: 0,
            split_index: 0,
            split_threshold,
        })
    }

    pub fn data_size(&self) -> u64 {
        self.data_size
    }

    pub fn append(&mut self, pcm: &[u8]) -> io::Result<()> {
        let f = self.file.as_mut().expect("WavWriter already finished");
        f.write_all(pcm)?;
        self.data_size += pcm.len() as u64;
        if self.data_size > self.split_threshold {
            self.split()?;
        }
        Ok(())
    }

    fn split(&mut self) -> io::Result<()> {
        let f = self.file.as_mut().expect("WavWriter already finished");
        write_wav_header(f, self.data_size as u32)?;
        self.file = None;
        let stem = self
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        let ext = self
            .path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("wav");
        let parent = self.path.parent().unwrap_or(std::path::Path::new("."));
        let next = parent.join(format!("{}_{:03}.{}", stem, self.split_index + 1, ext));
        let mut file = File::create(&next)?;
        file.seek(SeekFrom::Start(WAV_HEADER_SIZE as u64))?;
        self.split_index += 1;
        self.file = Some(file);
        self.data_size = 0;
        Ok(())
    }

    pub fn finish(mut self) -> io::Result<u64> {
        let f = self.file.as_mut().expect("WavWriter already finished");
        write_wav_header(f, self.data_size as u32)?;
        self.file = None;
        Ok(self.data_size)
    }
}
