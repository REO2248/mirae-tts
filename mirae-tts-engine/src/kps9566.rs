//! KPS9566 (DPRK standard charset) -> Unicode decoding.
//! Table source: kps9566-rs/data/KPS9566.TXT (runtime load or embedded via builtin).
use std::fmt;
use std::io::{self, Read};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mapping {
    pub kps: u16,
    pub unicode: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub text: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "KPS9566.TXT line {}: cannot parse: {:?}",
            self.line, self.text
        )
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone)]
pub struct Kps9566 {
    map: Vec<Mapping>,
}

impl Kps9566 {
    pub const EXPECTED_ENTRIES: usize = 20673;

    pub fn from_txt(text: &str) -> Result<Self, ParseError> {
        let mut map = Vec::new();
        for (idx, raw) in text.lines().enumerate() {
            let line_no = idx + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // strip trailing comment, then split on whitespace
            let body = line.split('#').next().unwrap_or("");
            let mut it = body.split_whitespace();
            let kps_tok = it.next().unwrap_or("");
            let uni_tok = it.next().unwrap_or("");
            let kps = u32::from_str_radix(kps_tok.trim_start_matches("0x"), 16).map_err(|_| {
                ParseError {
                    line: line_no,
                    text: line.to_string(),
                }
            })?;
            let unicode =
                u32::from_str_radix(uni_tok.trim_start_matches("0x"), 16).map_err(|_| {
                    ParseError {
                        line: line_no,
                        text: line.to_string(),
                    }
                })?;
            if kps > 0xFFFF {
                return Err(ParseError {
                    line: line_no,
                    text: line.to_string(),
                });
            }
            map.push(Mapping {
                kps: kps as u16,
                unicode,
            });
        }
        map.sort_unstable_by_key(|m| m.kps);
        Ok(Kps9566 { map })
    }

    pub fn load<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mut f = std::fs::File::open(path)?;
        let mut data = String::new();
        f.read_to_string(&mut data)?;
        Self::from_txt(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn builtin() -> Self {
        Self::from_txt(include_str!("../data/KPS9566.TXT"))
            .expect("embedded KPS9566.TXT must parse")
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    #[inline]
    pub fn lookup(&self, code: u16) -> Option<u32> {
        self.map
            .binary_search_by_key(&code, |m| m.kps)
            .ok()
            .map(|i| self.map[i].unicode)
    }

    #[inline]
    pub fn get(&self, code: u16) -> Option<char> {
        self.lookup(code).and_then(char::from_u32)
    }

    pub fn decode(&self, bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            let b0 = bytes[i];
            if b0 < 0x80 {
                out.push(b0 as char);
                i += 1;
                continue;
            }
            let code = match bytes.get(i + 1) {
                Some(&b1) => ((b0 as u16) << 8) | b1 as u16,
                None => {
                    out.push('\u{FFFD}');
                    break;
                }
            };
            match self.get(code) {
                Some(c) => out.push(c),
                None => out.push('\u{FFFD}'),
            }
            i += 2;
        }
        out
    }
}

impl Default for Kps9566 {
    fn default() -> Self {
        Self::builtin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Kps9566 {
        Kps9566::builtin()
    }

    #[test]
    fn builtin_parses_full_table() {
        let t = table();
        assert_eq!(t.len(), Kps9566::EXPECTED_ENTRIES);
        // spot checks
        assert_eq!(t.get(0x8141), Some('\u{AC03}'));
        assert_eq!(t.get(0xBCBF), Some('\u{C870}'));
        assert_eq!(t.get(0xB0BC), Some('\u{AC74}'));
    }

    #[test]
    fn decodes_korean_word() {
        let t = table();
        assert_eq!(t.decode(&[0xbc, 0xbf, 0xb0, 0xbc]), "조건");
    }

    #[test]
    fn decodes_ascii_and_mixed() {
        let t = table();
        assert_eq!(t.decode(b"Hello, 2026"), "Hello, 2026");
        assert_eq!(t.decode(&[0x41, 0xb0, 0xa1, 0x21]), "A가!");
    }

    #[test]
    fn unmapped_and_truncated() {
        let t = table();
        // 0xFF 0xFF is one 2-byte sequence; 0xFFFF is unmapped → one U+FFFD
        assert_eq!(t.decode(&[0xFF, 0xFF]), "\u{FFFD}");
        assert_eq!(t.decode(&[0xb0]), "\u{FFFD}"); // truncated 2-byte
        assert_eq!(t.decode(&[0x41, 0xFF, 0xFF]), "A\u{FFFD}");
        assert_eq!(t.get(0xFFFF), None);
    }

    #[test]
    fn parse_error_on_garbage() {
        assert!(Kps9566::from_txt("hello world").is_err());
        assert!(Kps9566::from_txt("0xZZZZ\t0xAC03").is_err());
        // comments and blanks are fine
        let t = Kps9566::from_txt("# comment\n\n0x8141\t0xAC03\t# 갃\n").unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t.get(0x8141), Some('\u{AC03}'));
    }
}

