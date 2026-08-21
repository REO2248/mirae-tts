//! KeyPad.Ebd — UTF-16 → internal-code conversion table.
//! Falls back to the `kps9566` crate when `KeyPad.Ebd` is missing.

use std::io::{self, Read};
use std::path::Path;

pub const TABLE_BYTES: usize = 65536 * 3;
pub const DEFAULT_PATH: &str = "Data/Dictionary/KeyPad.Ebd";

/// KeyPad.Ebd table (`None` = `kps9566` crate fallback).
#[derive(Debug, Clone)]
pub struct KeyPad {
    table: Option<Vec<u8>>,
}

const FALLBACK_QM: [u8; 1] = [0x3F];

impl KeyPad {
    pub fn parse(data: &[u8]) -> Option<Self> {
        (data.len() == TABLE_BYTES).then(|| KeyPad {
            table: Some(data.to_vec()),
        })
    }

    /// Falls back to `kps9566` when the file is missing.
    pub fn load<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(KeyPad { table: None });
        }
        let mut f = std::fs::File::open(path)?;
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        Self::parse(&data).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "KeyPad.Ebd: expected exactly 65536*3 bytes",
            )
        })
    }

    pub fn fallback() -> Self {
        KeyPad { table: None }
    }

    pub fn uses_real_table(&self) -> bool {
        self.table.is_some()
    }

    /// In fallback mode every code unit reports `?` (0x3F); use convert/convert_str.
    #[inline]
    pub fn entry(&self, code: u16) -> (u8, &[u8]) {
        match &self.table {
            Some(t) => {
                let off = code as usize * 3;
                let len = (t[off] as usize).min(2);
                (len as u8, &t[off + 1..off + 1 + len])
            }
            None => (1, &FALLBACK_QM[..]),
        }
    }

    pub fn convert(&self, text: &[u16]) -> Vec<u8> {
        match &self.table {
            Some(t) => convert_table(t, text),
            None => {
                let mut out = Vec::with_capacity(text.len() * 2);
                kps9566::kps9566::Encoder.encode_to_vec(&String::from_utf16_lossy(text), &mut out);
                out
            }
        }
    }

    pub fn convert_str(&self, text: &str) -> Vec<u8> {
        match &self.table {
            Some(t) => convert_table(t, &text.encode_utf16().collect::<Vec<_>>()),
            None => {
                let mut out = Vec::with_capacity(text.len() * 2);
                kps9566::kps9566::Encoder.encode_to_vec(text, &mut out);
                out
            }
        }
    }
}

fn convert_table(t: &[u8], units: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(units.len() * 2);
    for &code in units {
        let off = code as usize * 3;
        let len = (t[off] as usize).min(2);
        out.extend_from_slice(&t[off + 1..off + 1 + len]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real `KeyPad.Ebd` next to the committed Voice data, or `None` on
    /// machines without it (tests that need the exact table then skip).
    fn real_keypad() -> Option<KeyPad> {
        if let Ok(p) = std::env::var("MIRAE_KEYPAD_EBD") {
            let p = std::path::PathBuf::from(p);
            if p.exists() {
                return Some(KeyPad::load(p).expect("KeyPad.Ebd must load"));
            }
        }
        let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut roots = vec![manifest.clone()];
        if let Some(parent) = manifest.parent() {
            roots.push(parent.to_path_buf());
        }
        for root in roots {
            for voice in [
                root.join("Voice"),
                root.join("mirae-tts-engine").join("Voice"),
            ] {
                if !voice.join("VoiceInfo.pkg").exists() {
                    continue;
                }
                for cand in [
                    voice.join("KeyPad.Ebd"),
                    voice.join("Data").join("Dictionary").join("KeyPad.Ebd"),
                    root.join("Data").join("Dictionary").join("KeyPad.Ebd"),
                ] {
                    if cand.exists() {
                        return Some(KeyPad::load(cand).expect("KeyPad.Ebd must load"));
                    }
                }
            }
        }
        None
    }

    fn skipped(what: &str) {
        eprintln!("SKIP: {what} not found; set MIRAE_KEYPAD_EBD to run");
    }

    #[test]
    fn parses_real_table() {
        let Some(kp) = real_keypad() else {
            skipped("KeyPad.Ebd");
            return;
        };
        assert!(kp.uses_real_table());
        assert_eq!(kp.entry(0x41), (1, &[0x41][..]));
        assert_eq!(kp.entry(0xAC00), (2, &[0xB0, 0xA1][..]));
        assert_eq!(kp.entry(0xFFFF), (1, &[0x3F][..]));
    }

    #[test]
    fn all_entries_have_len_1_or_2() {
        let Some(kp) = real_keypad() else {
            skipped("KeyPad.Ebd");
            return;
        };
        for code in 0u16..=0xFFFF {
            let (len, payload) = kp.entry(code);
            assert!(len == 1 || len == 2, "U+{code:04X} len={len}");
            assert_eq!(payload.len(), len as usize);
        }
    }

    #[test]
    fn converts_korean_text() {
        let kp = real_keypad().unwrap_or_else(KeyPad::fallback);
        assert_eq!(kp.convert_str("조건"), &[0xbc, 0xbf, 0xb0, 0xbc]);
        assert_eq!(
            kp.convert_str("안녕하세요"),
            &[0xca, 0xaf, 0xb2, 0xce, 0xc2, 0xd7, 0xbb, 0xbd, 0xca, 0xfd]
        );
    }

    #[test]
    fn converts_ascii_and_unmapped() {
        // Surrogate halves map independently through the real table only;
        // the kps9566 fallback collapses an unpaired surrogate to one `?`.
        let Some(kp) = real_keypad() else {
            skipped("KeyPad.Ebd");
            return;
        };
        assert_eq!(kp.convert_str("Hi!"), b"Hi!");
        assert_eq!(kp.convert(&[0x1234]), &[0x3F]);
        assert_eq!(kp.convert(&[0xD800, 0xDC00]), &[0x3F, 0x3F]);
    }

    #[test]
    fn rejects_wrong_size() {
        assert!(KeyPad::parse(&[0u8; 10]).is_none());
        assert!(KeyPad::parse(&[0u8; TABLE_BYTES - 1]).is_none());
    }

    #[test]
    fn fallback_matches_keypad_for_hangul_ascii() {
        let Some(real) = real_keypad() else {
            skipped("KeyPad.Ebd");
            return;
        };
        let fb = KeyPad::fallback();
        assert!(!fb.uses_real_table());
        for &s in &[
            "A",
            "가",
            "조건",
            "전자서고",
            "안녕하십니까",
            "전자서고《미래》2.0은",
            "윈도우즈에서 원만히 동작",
        ] {
            assert_eq!(fb.convert_str(s), real.convert_str(s), "{s:?}");
        }
        assert_eq!(fb.convert(&[0x1234]), &[0x3F]);
    }

    #[test]
    fn load_missing_file_falls_back() {
        let kp = KeyPad::load("/nonexistent/KeyPad.Ebd").unwrap();
        assert!(!kp.uses_real_table());
        if let Some(real) = real_keypad() {
            assert_eq!(kp.convert_str("안녕"), real.convert_str("안녕"));
        } else {
            skipped("KeyPad.Ebd (comparison part)");
        }
    }
}
