//! Text-to-speech library: [`TtsEngine`], [`TtsConfig`], [`encode_wav_vec`] / [`pcm_i16le_to_bytes`].
//! Rust port of the original Future.exe TTS pipeline: text -> keypad -> segmenter ->
//! g2p -> tone -> record -> unit_select -> render -> PCM (22050 Hz / s16le / mono).
//!
//! Module layout: public engine API in [`synthesizer`] / [`wave_render`],
//! internal pipeline in `pipeline`, one module per pipeline stage.
pub mod alphabet;
pub mod connect;
pub mod dict; // canonical Voice/*.pkg parser (single source of truth)
pub mod digit_tables;
pub mod g2p;
pub mod keypad;
pub mod kps_tables;
mod pipeline; // internal engine (Mirae2Engine) — see synthesizer for the public wrapper
pub mod postprocess_tables;
pub mod record;
pub mod render;
pub mod segmenter;
pub mod synthesizer; // TtsEngine / TtsConfig (stable public API)
pub mod tables;
pub mod tone;
pub mod unit_select;
pub mod voice_data;
pub mod voice_info;
pub mod wav;
pub mod wave_render; // encode_wav_vec / pcm_i16le_to_bytes (stable public API)

// Stable public API re-exports (paths kept stable; see README).
pub use pipeline::{DEFAULT_VOICE_DIR, VOICE_DIR_ENV, default_voice_dir, truncate_last_line_char};
pub use render::{RING_MAX_BYTES, RING_SLOTS};
pub use synthesizer::{TtsConfig, TtsEngine};
pub use wave_render::{DEFAULT_SAMPLE_RATE, SAMPLE_RATE, encode_wav_vec, pcm_i16le_to_bytes};

pub mod prelude {
    pub use super::{
        DEFAULT_SAMPLE_RATE, TtsConfig, TtsEngine, encode_wav_vec, pcm_i16le_to_bytes,
    };
}
