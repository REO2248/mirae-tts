//! Shared test-data locators for the real-data integration tests.
//!
//! `Voice/*.pkg` is committed at the repository root, so dictionary / render /
//! unit-selection tests run everywhere. `KeyPad.Ebd` is an original-app file
//! that is *not* committed; tests that need the exact table skip when it
//! cannot be found (the engine itself falls back to `kps9566` in that case).
//!
//! Resolution order for the voice dir: `$MIRAE_VOICE_DIR`,
//! `$MIRAE2_VOICE_DIR`, then repo-relative candidates. The KeyPad table is
//! looked up next to the voice dir (original app layout) or via
//! `$MIRAE_KEYPAD_EBD`.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

fn has_voice_info(p: &Path) -> bool {
    p.join("VoiceInfo.pkg").exists()
}

/// Directory containing `VoiceInfo.pkg`, or `None` when unavailable.
pub fn voice_dir() -> Option<PathBuf> {
    for d in [
        std::env::var("MIRAE_VOICE_DIR").ok(),
        std::env::var("MIRAE2_VOICE_DIR").ok(),
    ]
    .into_iter()
    .flatten()
    {
        let p = PathBuf::from(d);
        if has_voice_info(&p) {
            return Some(p);
        }
    }
    for c in ["Voice", "mirae-tts-engine/Voice", "../Voice"] {
        let p = PathBuf::from(c);
        if has_voice_info(&p) {
            return Some(p);
        }
    }
    None
}

/// `KeyPad.Ebd` path, or `None` when the exact table is not available.
pub fn keypad_ebd() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MIRAE_KEYPAD_EBD") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(voice) = voice_dir() {
        candidates.push(voice.join("KeyPad.Ebd"));
        candidates.push(voice.join("Data").join("Dictionary").join("KeyPad.Ebd"));
        if let Some(parent) = voice.parent() {
            candidates.push(parent.join("Data").join("Dictionary").join("KeyPad.Ebd"));
            candidates.push(parent.join("KeyPad.Ebd"));
        }
    }
    candidates.into_iter().find(|p| p.exists())
}

/// Print a skip notice for a test that needs unavailable real data.
pub fn skipped(what: &str) {
    eprintln!("SKIP: {what} not found; set MIRAE_VOICE_DIR to run");
}
