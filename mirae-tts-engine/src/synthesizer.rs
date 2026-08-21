//! Public engine entry point: [`TtsEngine`] / [`TtsConfig`].
//! Wraps the internal [`crate::pipeline::Mirae2Engine`] behind a thread-safe API.
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::pipeline::{EngineConfig, Mirae2Engine};

#[derive(Debug, Clone)]
pub struct TtsConfig {
    /// Output sample rate in Hz (default 22050; original speed 50 × 441).
    pub sample_rate: u32,
    /// Legacy sentence-end pause in samples (accepted for compatibility, not applied).
    pub sentence_pause: i16,
    pub log_progress: bool,
}

impl Default for TtsConfig {
    fn default() -> Self {
        TtsConfig {
            sample_rate: 22050,
            sentence_pause: 4000,
            log_progress: false,
        }
    }
}

/// Candidate locations for `KeyPad.Ebd`, tried in order.
fn find_keypad_ebd(voice_dir: &Path, voice: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    candidates.push(voice.join("KeyPad.Ebd"));
    candidates.push(voice.join("Data").join("Dictionary").join("KeyPad.Ebd"));
    if let Some(parent) = voice.parent() {
        candidates.push(parent.join("Data").join("Dictionary").join("KeyPad.Ebd"));
        candidates.push(parent.join("KeyPad.Ebd"));
    }
    candidates.push(voice_dir.join("KeyPad.Ebd"));
    candidates.push(voice_dir.join("Data").join("Dictionary").join("KeyPad.Ebd"));
    candidates.into_iter().find(|p| p.exists())
}

/// The TTS engine: loads voice data and runs the full pipeline (thread-safe).
pub struct TtsEngine {
    inner: Mutex<Mirae2Engine>,
    config: TtsConfig,
}

impl TtsEngine {
    /// Initialize the engine from `voice_dir` (voice dir, or install root with `Voice/`).
    /// If `voice_dir` is empty, `MIRAE_VOICE_DIR` env / `DEFAULT_VOICE_DIR` fallback is not auto-used here — callers
    /// should call `default_voice_dir()` and pass it explicitly when they need env resolution.
    pub fn new<P: AsRef<Path>>(voice_dir: P, config: TtsConfig) -> io::Result<Self> {
        let voice_dir = voice_dir.as_ref();
        let voice: PathBuf = if voice_dir.join("VoiceInfo.pkg").exists() {
            voice_dir.to_path_buf()
        } else if voice_dir.join("Voice").join("VoiceInfo.pkg").exists() {
            voice_dir.join("Voice")
        } else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "VoiceInfo.pkg not found in {:?} (expected directly or under Voice/)",
                    voice_dir
                ),
            ));
        };
        let keypad = find_keypad_ebd(voice_dir, &voice);
        if keypad.is_none() {
            eprintln!(
                "[tts] KeyPad.Ebd not found near {:?} — using approximate KPS9566 fallback (exact original conversion requires KeyPad.Ebd)",
                voice_dir
            );
        }

        let mut inner = Mirae2Engine::from_paths(&voice, keypad.as_deref())?;
        inner.set_debug_log(config.log_progress || std::env::var("MIRAE_DEBUG").is_ok());
        inner.set_config(EngineConfig::from_public(&config));

        Ok(TtsEngine {
            inner: Mutex::new(inner),
            config,
        })
    }

    pub fn synthesize(&self, text: &str) -> io::Result<Vec<i16>> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| io::Error::other("TTS engine mutex poisoned"))?;
        inner.synthesize(text)
    }

    pub fn synthesize_streaming<F: FnMut(Vec<i16>) -> bool>(
        &self,
        text: &str,
        mut on_chunk: F,
    ) -> io::Result<()> {
        const CHUNK_SAMPLES: usize = 16 * 1024;
        let pcm = self.synthesize(text)?;
        for chunk in pcm.chunks(CHUNK_SAMPLES) {
            if !on_chunk(chunk.to_vec()) {
                break;
            }
        }
        Ok(())
    }

    pub fn effective_sample_rate(&self) -> u32 {
        self.config.sample_rate
    }

    pub fn voice_entry_count(&self) -> usize {
        self.inner
            .lock()
            .map(|inner| inner.voice_entry_count())
            .unwrap_or(0)
    }

    pub fn config(&self) -> &TtsConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: TtsConfig) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.set_debug_log(config.log_progress || std::env::var("MIRAE_DEBUG").is_ok());
            inner.set_config(EngineConfig::from_public(&config));
        }
        self.config = config;
    }
}
