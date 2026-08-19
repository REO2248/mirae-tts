//! Unified loader for the five Mirae voice "dictionary" packages:
//! `Alphabet.pkg`, `Conjects.pkg`, `NonReg.pkg`, `User.pkg`, and `colligation.pkg`.
//!
//! All five share one byte-exact format (verified `diff == 0` against the
//! original `미래2.0/Voice/*.pkg` binaries):
//!
//! ```text
//!   [u32 c1]                       ; node/entry count for arr1/arr2
//!   [u32 c2]                       ; byte length of arr3
//!   [i32 × c1]  arr1              ; base array  (int32 LE)
//!   [u8  × c1]  arr2              ; check array (u8)
//!   [u8  × c2]  arr3              ; string table: repeated
//!                                 ;   [KPS bytes …][0x50 terminator][u32 LE idx]
//!                                 ;   (KPS bytes may include 0x00; only 0x50 ends)
//!   [u32 f6]                       ; record6 count
//!   [u32 c6]                       ; record6 mapping-entry count
//!   [(u32,u32) × c6] map6         ; mapping: idx_group -> (rec6_start, count)
//!   [u8 × 6 × f6]  rec6           ; 6-byte records
//!   [u32 f26]                      ; record26 count
//!   [u32 c26]                      ; record26 mapping-entry count
//!   [(u32,u32) × c26] map26        ; mapping: idx_group -> (rec26_start, count)
//!   [u8 × 26 × f26] rec26          ; 26-byte records
//! ```
//!
//! A 6-byte record is laid out as:
//! ```text
//!   [u16 phoneme_id]   ; little-endian phoneme/class id
//!   [u8 b2][u8 b3][u8 b4][u8 b5][u8 b6]   ; recording parameters
//! ```
//!
//! Lookup: a KPS key string is searched in `arr3`; the trailing `u32 idx` is
//! the result. When `c6 == 0` (e.g. `User.pkg`) the idx is a direct index into
//! `rec6`; when `c6 > 0` the high part `idx / 256` selects a `map6` group whose
//! `(rec6_start, count)` window is then indexed. (Empirically `count == 1` for
//! every mapping entry in the shipped data, but the code tolerates >1.)

use std::io::{self, Read};
use std::path::Path;

/// One 6-byte dictionary record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rec6 {
    pub phoneme_id: u16,
    pub b2: u8,
    pub b3: u8,
    pub b4: u8,
    pub b5: u8,
    pub b6: u8,
}

impl Rec6 {
    #[inline]
    fn from_bytes(b: &[u8]) -> Self {
        Rec6 {
            phoneme_id: u16::from_le_bytes([b[0], b[1]]),
            b2: b[2],
            b3: b[3],
            b4: b[4],
            b5: b[5],
            b6: 0,
        }
    }
}

/// One 26-byte dictionary record (opaque; semantics not yet decoded).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rec26(pub [u8; 26]);

/// Unified Mirae voice dictionary.
#[derive(Debug, Clone)]
pub struct MiraeDict {
    pub c1: usize,
    pub c2: usize,
    pub base: Vec<i32>,
    pub check: Vec<u8>,
    pub edges: Vec<u8>,
    pub f6: usize,
    pub c6: usize,
    pub map6: Vec<(u32, u32)>,
    pub rec6: Vec<Rec6>,
    pub f26: usize,
    pub c26: usize,
    pub map26: Vec<(u32, u32)>,
    pub rec26: Vec<Rec26>,
}

impl MiraeDict {
    /// Parse a dictionary buffer. Returns `None` if the declared dimensions
    /// would read past the end of `data`.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 8 {
            return None;
        }
        let c1 = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let c2 = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;

        let mut pos = 8usize;

        let base_end = pos + c1 * 4;
        if base_end > data.len() {
            return None;
        }
        let base: Vec<i32> = (0..c1)
            .map(|i| {
                i32::from_le_bytes([
                    data[pos + 4 * i],
                    data[pos + 4 * i + 1],
                    data[pos + 4 * i + 2],
                    data[pos + 4 * i + 3],
                ])
            })
            .collect();
        pos = base_end;

        let check_end = pos + c1;
        if check_end > data.len() {
            return None;
        }
        let check = data[pos..check_end].to_vec();
        pos = check_end;

        let edges_end = pos + c2;
        if edges_end > data.len() {
            return None;
        }
        let edges = data[pos..edges_end].to_vec();
        pos = edges_end;

        // ---- 6-byte record section ----
        if pos + 8 > data.len() {
            return None;
        }
        let f6 = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let c6 = u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]) as usize;
        pos += 8;

        let map6_end = pos + c6 * 8;
        if map6_end > data.len() {
            return None;
        }
        let map6: Vec<(u32, u32)> = (0..c6)
            .map(|i| {
                let o = pos + 8 * i;
                let a = u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
                let b = u32::from_le_bytes([data[o + 4], data[o + 5], data[o + 6], data[o + 7]]);
                (a, b)
            })
            .collect();
        pos = map6_end;

        let rec6_end = pos + f6 * 6;
        if rec6_end > data.len() {
            return None;
        }
        let rec6: Vec<Rec6> = (0..f6)
            .map(|i| Rec6::from_bytes(&data[pos + 6 * i..pos + 6 * i + 6]))
            .collect();
        pos = rec6_end;

        // ---- 26-byte record section ----
        if pos + 8 > data.len() {
            return None;
        }
        let f26 = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let c26 = u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]) as usize;
        pos += 8;

        let map26_end = pos + c26 * 8;
        if map26_end > data.len() {
            return None;
        }
        let map26: Vec<(u32, u32)> = (0..c26)
            .map(|i| {
                let o = pos + 8 * i;
                let a = u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
                let b = u32::from_le_bytes([data[o + 4], data[o + 5], data[o + 6], data[o + 7]]);
                (a, b)
            })
            .collect();
        pos = map26_end;

        let rec26_end = pos + f26 * 26;
        if rec26_end > data.len() {
            return None;
        }
        let rec26: Vec<Rec26> = (0..f26)
            .map(|i| {
                let mut arr = [0u8; 26];
                arr.copy_from_slice(&data[pos + 26 * i..pos + 26 * i + 26]);
                Rec26(arr)
            })
            .collect();

        Some(MiraeDict {
            c1,
            c2,
            base,
            check,
            edges,
            f6,
            c6,
            map6,
            rec6,
            f26,
            c26,
            map26,
            rec26,
        })
    }

    /// Load and parse a dictionary package file.
    pub fn load<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mut f = std::fs::File::open(path)?;
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        Self::parse(&data).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "dictionary package: malformed header or truncated body",
            )
        })
    }

    /// Double-array trie walk (identical algorithm to `Colligation::search`).
    ///
    /// The query is the KPS key **without** the `0x50` terminator; it is
    /// appended internally as the virtual last byte. Uses the stored `base`
    /// (arr1) and `check` (arr2) arrays to walk the trie, exactly matching the
    /// original binary's `fcn.00411910` traversal — *not* a linear scan of
    /// `arr3` (which would diverge from the original algorithm).
    ///
    /// Returns the raw `u32` payload index at the matched leaf, or `None`.
    pub fn search(&self, query: &[u8]) -> Option<u32> {
        let mut node = 1usize;
        let ext_len = query.len() + 1; // virtual: query ++ [0x50]

        for qi in 0..ext_len {
            let byte = if qi < query.len() { query[qi] } else { 0x50 };

            let base_val = *self.base.get(node)?;
            if base_val < 0 {
                // Leaf node — compressed suffix stored in the edge array.
                let suffix_off = (-base_val) as usize;
                let suffix = self.read_edge_string(suffix_off)?;
                let remaining_len = ext_len - qi;
                if suffix.len() > remaining_len {
                    return None;
                }
                let matches = suffix.iter().enumerate().all(|(j, &sb)| {
                    let pos = qi + j;
                    let rb = if pos < query.len() { query[pos] } else { 0x50 };
                    sb == rb
                });
                if matches {
                    let payload_off = suffix_off + suffix.len() + 1;
                    return self.read_payload_index(payload_off);
                }
                return None;
            }

            let child = (base_val as isize + byte as isize) as usize;
            if child >= self.c1 {
                return None;
            }
            if self.check[child] != byte {
                return None;
            }

            node = child;
        }
        None
    }

    fn read_edge_string(&self, offset: usize) -> Option<&[u8]> {
        let start = offset;
        let mut end = start;
        loop {
            if end >= self.edges.len() {
                return None;
            }
            if self.edges[end] == 0x50 {
                break;
            }
            end += 1;
        }
        Some(&self.edges[start..end])
    }

    fn read_payload_index(&self, payload_off: usize) -> Option<u32> {
        let b0 = *self.edges.get(payload_off)?;
        let b1 = *self.edges.get(payload_off + 1)?;
        let b2 = *self.edges.get(payload_off + 2)?;
        let b3 = *self.edges.get(payload_off + 3)?;
        Some(u32::from_le_bytes([b0, b1, b2, b3]))
    }

    /// Search `arr3` for an exact KPS key (without the `0x50` terminator) and
    /// return its trailing `u32` index. Returns `None` if not found.
    ///
    /// Deprecated: prefer [`MiraeDict::search`] (double-array trie walk), which
    /// matches the original binary's traversal. Kept for tests/fallback.
    #[deprecated(note = "use `search` (trie walk) instead of linear scan")]
    pub fn lookup_arr3(&self, kps: &[u8]) -> Option<u32> {
        let s = &self.edges;
        let mut i = 0usize;
        while i < s.len() {
            let start = i;
            let mut j = i;
            while j < s.len() && s[j] != 0x50 {
                j += 1;
            }
            if j >= s.len() {
                break; // no terminator found; malformed tail
            }
            let seg = &s[start..j];
            if seg == kps {
                let idx_off = j + 1;
                if idx_off + 4 <= s.len() {
                    return Some(u32::from_le_bytes([
                        s[idx_off],
                        s[idx_off + 1],
                        s[idx_off + 2],
                        s[idx_off + 3],
                    ]));
                }
            }
            // advance past this entry: 0x50 + 4-byte idx
            i = j + 1 + 4;
        }
        None
    }

    /// Resolve a raw `idx` (from `lookup_arr3`) to a `rec6` record.
    ///
    /// - `c6 == 0`: `idx` is a direct index into `rec6`.
    /// - `c6 > 0`: `idx / 256` selects a `map6` group; `(rec6_start, count)`
    ///   defines the window, and `idx % 256` indexes within it (clamped to the
    ///   group's `count`).
    pub fn rec6_at(&self, idx: u32) -> Option<Rec6> {
        if self.c6 == 0 {
            return self.rec6.get(idx as usize).copied();
        }
        let group = (idx / 256) as usize;
        if group >= self.map6.len() {
            return None;
        }
        let (rec6_start, count) = self.map6[group];
        let count = count.max(1) as usize;
        let local = (idx % 256) as usize;
        let local = local.min(count - 1);
        self.rec6.get(rec6_start as usize + local).copied()
    }

    /// Convenience: KPS key → `rec6` record, via the double-array trie walk.
    pub fn lookup_rec6(&self, kps: &[u8]) -> Option<Rec6> {
        let idx = self.search(kps)?;
        self.rec6_at(idx)
    }

    /// Total number of `rec6` records.
    #[inline]
    pub fn rec6_count(&self) -> usize {
        self.rec6.len()
    }

    /// Total number of `rec26` records.
    #[inline]
    pub fn rec26_count(&self) -> usize {
        self.rec26.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Build a minimal valid dictionary buffer and round-trip it.
    fn build_sample() -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&2u32.to_le_bytes()); // c1
        d.extend_from_slice(&14u32.to_le_bytes()); // c2 = arr3 (edges) byte length
        d.extend_from_slice(&[0i32; 2].iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>()); // base
        d.extend_from_slice(&[0u8; 2]); // check
        // arr3: two entries, KPS may contain 0x00
        // entry0: [00 24 09] 0x50 [00 00 00 00]
        d.extend_from_slice(&[0x00, 0x24, 0x09, 0x50, 0x00, 0x00, 0x00, 0x00]);
        // entry1: [41] 0x50 [01 00 00 00]
        d.extend_from_slice(&[0x41, 0x50, 0x01, 0x00, 0x00, 0x00]);
        // 6B section: f6=2, c6=0
        d.extend_from_slice(&2u32.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        // rec6[0] = [u16=0x1234][01 02 03 04]
        d.extend_from_slice(&[0x34, 0x12, 0x01, 0x02, 0x03, 0x04]);
        // rec6[1] = [u16=0x5678][05 06 07 08]
        d.extend_from_slice(&[0x78, 0x56, 0x05, 0x06, 0x07, 0x08]);
        // 26B section: f26=0, c26=0
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d
    }

    #[test]
    fn parse_roundtrip() {
        let d = build_sample();
        let dict = MiraeDict::parse(&d).expect("parse");
        assert_eq!(dict.c1, 2);
        assert_eq!(dict.c2, 12);
        assert_eq!(dict.rec6_count(), 2);
        assert_eq!(dict.c6, 0);
    }

    #[test]
    fn lookup_direct_idx() {
        let d = build_sample();
        let dict = MiraeDict::parse(&d).unwrap();
        // entry1 has idx=1 -> rec6[1]
        let r = dict.lookup_rec6(&[0x41]).expect("found");
        assert_eq!(r.phoneme_id, 0x5678);
        // entry0 has idx=0 -> rec6[0]
        let r0 = dict.lookup_rec6(&[0x00, 0x24, 0x09]).expect("found0");
        assert_eq!(r0.phoneme_id, 0x1234);
    }

    #[test]
    fn lookup_missing() {
        let d = build_sample();
        let dict = MiraeDict::parse(&d).unwrap();
        assert!(dict.lookup_rec6(&[0x99]).is_none());
    }
}
