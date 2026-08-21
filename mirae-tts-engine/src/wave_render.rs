//! Public PCM / WAV output encoding: [`pcm_i16le_to_bytes`], [`encode_wav_vec`].
//! The 46-byte header is the byte-exact replica of the original Future.exe
//! output (see [`crate::wav::wav_header_bytes`] — single shared source).
use std::io;

use crate::wav::{WAV_HEADER_SIZE, wav_header_bytes};

/// Logical output sample rate (Hz), per original WAVEFORMATEX (speed 50 × 441).
pub const SAMPLE_RATE: u32 = 22050;

/// Default WAV sample rate used by [`encode_wav_vec`] / [`pcm_i16le_to_bytes`].
pub const DEFAULT_SAMPLE_RATE: u32 = SAMPLE_RATE;

/// Mono PCM as little-endian `i16` bytes (no WAV header).
pub fn pcm_i16le_to_bytes(pcm: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(pcm.len() * 2);
    for &s in pcm {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    bytes
}

/// Encode mono s16le PCM as a WAV file (byte-exact 46-byte header replica).
pub fn encode_wav_vec(pcm: &[i16], sample_rate: u32) -> io::Result<Vec<u8>> {
    let data = pcm_i16le_to_bytes(pcm);
    let mut out = Vec::with_capacity(WAV_HEADER_SIZE + data.len());
    out.extend_from_slice(&wav_header_bytes(data.len() as u32, sample_rate));
    out.extend_from_slice(&data);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_i16le_to_bytes_roundtrip() {
        let pcm = vec![0i16, -1, 1, 0x1234, -0x5678];
        let bytes = pcm_i16le_to_bytes(&pcm);
        assert_eq!(bytes.len(), pcm.len() * 2);
        assert_eq!(bytes, [0, 0, 0xff, 0xff, 1, 0, 0x34, 0x12, 0x88, 0xa9]);
    }

    #[test]
    fn encode_wav_vec_header_is_46_bytes() {
        let wav = encode_wav_vec(&[0i16; 100], 22050).unwrap();
        assert_eq!(wav.len(), 46 + 200);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[16..20], &0x12u32.to_le_bytes()); // fmt chunk = 18 (WAVEFORMATEX)
        assert_eq!(&wav[24..28], &22050u32.to_le_bytes());
        assert_eq!(&wav[28..32], &(22050u32 * 2).to_le_bytes());
        assert_eq!(&wav[38..42], b"data");
        assert_eq!(&wav[42..46], &200u32.to_le_bytes());
        assert_eq!(&wav[4..8], &(200u32 + 0x30).to_le_bytes()); // RIFF size quirk
    }
}
