//! Text-to-speech library: [`TtsEngine`], [`TtsConfig`], plus [`encode_wav_vec`] / [`pcm_i16le_to_bytes`].
//!
//! Engine modules are private; the stable API is the crate root and [`prelude`].
#![allow(dead_code)]

mod colligation;
mod english;
mod g2p;
mod korean;
mod kps_class;
mod number;
mod phoneme;
mod segmenter;
mod speech;
mod synthesizer;
mod unit_select;
mod voice_info;
mod wave_render;

pub use synthesizer::{TtsConfig, TtsEngine};
pub use wave_render::{encode_wav_vec, pcm_i16le_to_bytes, DEFAULT_SAMPLE_RATE};

/// Same as the crate root: [`TtsEngine`], [`TtsConfig`], WAV/PCM helpers.
pub mod prelude {
    pub use super::{
        encode_wav_vec, pcm_i16le_to_bytes, TtsConfig, TtsEngine, DEFAULT_SAMPLE_RATE,
    };
}

/// KPS 9566 → UTF-8 with U+FFFD replacement (streaming decoder, one shot).
pub(crate) fn kps9566_decode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len());
    let mut d = kps9566::kps9566::Decoder::new();
    d.decode_to_string(bytes, &mut s, true);
    s
}

/// UTF-8 → KPS 9566; unmappable characters become `?` (0x3F).
pub(crate) fn kps9566_encode(text: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(text.len() * 2);
    kps9566::kps9566::Encoder.encode_to_vec(text, &mut v);
    v
}

#[cfg(test)]
mod kps9566_roundtrip_tests {
    use super::{kps9566_decode, kps9566_encode};

    #[test]
    fn ascii_roundtrip() {
        let text = "Hello, World!";
        let enc = kps9566_encode(text);
        assert_eq!(kps9566_decode(&enc), text);
    }

    #[test]
    fn korean_roundtrip() {
        let text = "안녕하십니까";
        let enc = kps9566_encode(text);
        assert_eq!(enc.len(), text.chars().count() * 2);
        assert_eq!(kps9566_decode(&enc), text);
    }

    #[test]
    fn mixed_roundtrip() {
        let text = "Hello 세계";
        let enc = kps9566_encode(text);
        assert_eq!(kps9566_decode(&enc), text);
    }
}
