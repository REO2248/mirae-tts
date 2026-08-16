//! VoiceData.pkg - unit waveform readout (FUN_0044b700).
//! Original opens/seeks/reads per unit; this port keeps the handle open and uses
//! absolute seeks (sample x 2 = byte; 64,000B scratch, wlen_max 12,000).
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const SCRATCH_SIZE: usize = 64000;

pub struct VoiceData {
    file: File,
    path: PathBuf,
}

impl VoiceData {
    pub fn open(dir: &Path) -> io::Result<Self> {
        Self::open_path(&dir.join("VoiceData.pkg"))
    }

    pub fn open_path(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        Ok(VoiceData {
            file,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn len(&self) -> io::Result<u64> {
        Ok(self.file.metadata()?.len())
    }

    pub fn read_unit(&mut self, woff: u32, wlen: u32, out: &mut [u8]) -> io::Result<usize> {
        let byte_off = u64::from(woff) * 2;
        let byte_len = usize::try_from(u64::from(wlen) * 2).expect("wlen×2 overflow");
        assert!(
            byte_len <= SCRATCH_SIZE,
            "unit read {}B exceeds 64,000B scratch buffer (wlen={})",
            byte_len,
            wlen
        );
        assert!(
            out.len() >= byte_len,
            "output buffer {}B too small for {}B unit",
            out.len(),
            byte_len
        );
        self.file.seek(SeekFrom::Start(byte_off))?;
        self.file.read_exact(&mut out[..byte_len])?;
        Ok(byte_len)
    }

    pub fn read_unit_vec(&mut self, woff: u32, wlen: u32) -> io::Result<Vec<u8>> {
        let byte_len = usize::try_from(u64::from(wlen) * 2).expect("wlen×2 overflow");
        let mut buf = vec![0u8; byte_len];
        self.read_unit(woff, wlen, &mut buf)?;
        Ok(buf)
    }

    pub fn read_unit_samples(&mut self, woff: u32, wlen: u32) -> io::Result<Vec<i16>> {
        let bytes = self.read_unit_vec(woff, wlen)?;
        let mut samples = Vec::with_capacity(bytes.len() / 2);
        for chunk in bytes.chunks_exact(2) {
            samples.push(i16::from_le_bytes([chunk[0], chunk[1]]));
        }
        Ok(samples)
    }
}

