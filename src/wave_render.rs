//! Concatenate VoiceData.pkg slices + pause silence (+ liaison). Mono s16le.

use std::io::{self, Cursor};

use crate::phoneme::PhonemeUnit;
use crate::unit_select::SelectedUnit;
use crate::voice_info::VoiceDataReader;

pub const DEFAULT_SAMPLE_RATE: u32 = 22050;

pub fn render_to_pcm(
    reader: &VoiceDataReader,
    phonemes: &[PhonemeUnit],
    units: &[Option<SelectedUnit>],
    pause_between_sentences: i16,
) -> io::Result<Vec<i16>> {
    let estimated_size: usize = units
        .iter()
        .enumerate()
        .map(|(idx, u)| match u {
            Some(unit) => {
                let samples = unit.entry.wave_samples() as usize;
                let pause = unit.pause_samples.max(0) as usize;
                let liaison = unit
                    .liaison_entry
                    .as_ref()
                    .map(|e| e.wave_samples() as usize)
                    .unwrap_or(0);
                samples + pause + liaison
            }
            None => phonemes
                .get(idx)
                .filter(|p| p.pause.is_some())
                .map(|_| pause_between_sentences.max(0) as usize)
                .unwrap_or(0),
        })
        .sum();

    let mut output = Vec::with_capacity(estimated_size + 1024);

    for (idx, unit_opt) in units.iter().enumerate() {
        match unit_opt {
            Some(unit) => {
                let samples = reader.read_samples(&unit.entry)?;
                output.extend_from_slice(&samples);

                // liaison pitch 0 = invalid slot in VoiceInfo
                if let Some(ref liaison) = unit.liaison_entry {
                    if liaison.pitch() != 0 {
                        let liaison_samples = reader.read_samples(liaison)?;
                        output.extend_from_slice(&liaison_samples);
                    }
                }

                if unit.pause_samples > 0 {
                    output.resize(output.len() + unit.pause_samples as usize, 0i16);
                }
            }
            None => {
                // Pause phoneme → sentence_pause; other None slots (absorbed syllables) stay silent.
                let is_pause_slot = phonemes
                    .get(idx)
                    .map(|p| p.pause.is_some())
                    .unwrap_or(false);
                if is_pause_slot && pause_between_sentences > 0 {
                    output.resize(output.len() + pause_between_sentences as usize, 0i16);
                }
            }
        }
    }

    Ok(output)
}

/// Mono PCM as little-endian `i16` bytes (no WAV header). Same layout as HTTP `application/octet-stream` / `audio/l16` body.
pub fn pcm_i16le_to_bytes(pcm: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(pcm.len() * 2);
    for &s in pcm {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    bytes
}

pub fn encode_wav_vec(samples: &[i16], sample_rate: u32) -> io::Result<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut buf = Vec::new();
    {
        let mut w = hound::WavWriter::new(Cursor::new(&mut buf), spec).map_err(wav_err)?;
        for &s in samples {
            w.write_sample(s).map_err(wav_err)?;
        }
        w.finalize().map_err(wav_err)?;
    }
    Ok(buf)
}

fn wav_err(e: hound::Error) -> io::Error {
    io::Error::other(e)
}
