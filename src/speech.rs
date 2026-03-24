//! Speech.pkg: KPS9566, TAB-separated `word\treading`. Mirae pronunciation overrides.

use std::collections::HashMap;
use std::io;
use std::path::Path;

#[derive(Debug, Default)]
pub struct SpeechDict {
    entries: HashMap<String, String>,
}

impl SpeechDict {
    pub fn new() -> Self {
        SpeechDict {
            entries: HashMap::new(),
        }
    }

    pub fn load<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let bytes = std::fs::read(path.as_ref())?;
        let mut dict = SpeechDict::new();
        dict.parse_bytes(&bytes);
        Ok(dict)
    }

    fn parse_bytes(&mut self, bytes: &[u8]) {
        let mut start = 0;
        while start < bytes.len() {
            let end = bytes[start..]
                .windows(2)
                .position(|w| w == b"\r\n")
                .map(|p| start + p)
                .or_else(|| {
                    bytes[start..]
                        .iter()
                        .position(|&b| b == b'\n')
                        .map(|p| start + p)
                })
                .unwrap_or(bytes.len());

            let line = &bytes[start..end];
            if !line.is_empty() {
                self.parse_line(line);
            }

            start = end;
            if start < bytes.len() && bytes[start] == b'\r' {
                start += 1;
            }
            if start < bytes.len() && bytes[start] == b'\n' {
                start += 1;
            }
        }
    }

    /// Parse a single line of the form `KPS_word TAB KPS_pronunciation`.
    fn parse_line(&mut self, line: &[u8]) {
        // Split on TAB (0x09)
        let tab_pos = match line.iter().position(|&b| b == 0x09) {
            Some(p) => p,
            None => return, // Malformed line: skip
        };

        let word_bytes = &line[..tab_pos];
        let pron_bytes = &line[tab_pos + 1..];

        // Decode KPS 9566 → UTF-8
        let word = crate::kps9566_decode(word_bytes);
        let pron = crate::kps9566_decode(pron_bytes);

        if !word.is_empty() && !pron.is_empty() {
            self.entries.insert(word, pron);
        }
    }

    pub fn lookup<'a>(&'a self, word: &str) -> Option<&'a str> {
        self.entries.get(word).map(|s| s.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let mut dict = SpeechDict::new();
        dict.parse_bytes(b"hello\tworld\r\nfoo\tbar\r\n");
        assert_eq!(dict.len(), 2);
        assert_eq!(dict.lookup("hello"), Some("world"));
        assert_eq!(dict.lookup("foo"), Some("bar"));
        assert_eq!(dict.lookup("missing"), None);
    }

    #[test]
    fn test_parse_empty() {
        let mut dict = SpeechDict::new();
        dict.parse_bytes(b"");
        assert_eq!(dict.len(), 0);
    }

    #[test]
    fn test_parse_no_tab() {
        let mut dict = SpeechDict::new();
        dict.parse_bytes(b"no_tab_here\r\n");
        assert_eq!(dict.len(), 0);
    }

    #[test]
    fn test_parse_lf_only() {
        let mut dict = SpeechDict::new();
        dict.parse_bytes(b"word\tpron\n");
        assert_eq!(dict.len(), 1);
        assert_eq!(dict.lookup("word"), Some("pron"));
    }
}
