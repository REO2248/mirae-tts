//! COLLIGATION.PKG: double-array trie. Header: u32 N (nodes), u32 E (edge bytes).
//! base[i]>0: child = base+byte; base[i]<0: leaf, suffix offset = -base. check[i] = incoming byte.
//! edges: NUL-terminated KPS suffix + u16 LE secondary index; then M×6 rules (type bit7 = last in chain).
//! Append `P` (0x50) to every query.

use std::io::{self, Read};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColligRule {
    pub rule_type: u8,
    pub is_last: bool,
    pub params: [u8; 5],
}

pub struct Colligation {
    n: usize,
    base: Vec<i32>,
    check: Vec<u8>,
    edges: Vec<u8>,
    records: Vec<u8>,
}

fn read_u32_le(data: &[u8], offset: usize) -> Option<u32> {
    let b = data.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn read_i32_le(data: &[u8], offset: usize) -> Option<i32> {
    read_u32_le(data, offset).map(|v| v as i32)
}

impl Colligation {
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut pos = 0usize;

        let n = read_u32_le(data, pos)? as usize;
        pos += 4;
        let e = read_u32_le(data, pos)? as usize;
        pos += 4;

        let base_end = pos + n * 4;
        if base_end > data.len() {
            return None;
        }
        let base: Vec<i32> = (0..n)
            .map(|i| read_i32_le(data, pos + i * 4))
            .collect::<Option<Vec<i32>>>()?;
        pos = base_end;

        let check_end = pos + n;
        if check_end > data.len() {
            return None;
        }
        let check = data[pos..check_end].to_vec();
        pos = check_end;

        let edge_end = pos + e;
        if edge_end > data.len() {
            return None;
        }
        let edges = data[pos..edge_end].to_vec();
        pos = edge_end;

        let m = read_u32_le(data, pos)? as usize;
        pos += 4;
        let k_extra = read_u32_le(data, pos)? as usize;
        pos += 4;
        pos += k_extra * 8;

        let rec_end = pos + m * 6;
        if rec_end > data.len() {
            return None;
        }
        let records = data[pos..rec_end].to_vec();

        Some(Colligation {
            n,
            base,
            check,
            edges,
            records,
        })
    }

    pub fn load<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mut f = std::fs::File::open(path)?;
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        Self::parse(&data).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "failed to parse COLLIGATION.PKG",
            )
        })
    }

    /// Query ends with 0x50; walk DAT from node 1.
    fn search(&self, query: &[u8]) -> Option<usize> {
        let mut node = 1usize;
        let ext_len = query.len() + 1; // virtual: query ++ [b'P']

        for qi in 0..ext_len {
            let byte = if qi < query.len() { query[qi] } else { b'P' };

            let base_val = *self.base.get(node)?;
            if base_val < 0 {
                // Leaf node — compressed suffix stored in edge array.
                let suffix_off = (-base_val) as usize;
                let suffix = self.read_edge_string(suffix_off)?;
                // Check: suffix is a prefix of the virtual remaining extended[qi..].
                // Compare byte-by-byte without allocating the concatenated buffer.
                let remaining_len = ext_len - qi;
                if suffix.len() > remaining_len {
                    return None;
                }
                let matches = suffix.iter().enumerate().all(|(j, &sb)| {
                    let pos = qi + j;
                    let rb = if pos < query.len() { query[pos] } else { b'P' };
                    sb == rb
                });
                if matches {
                    let payload_off = suffix_off + suffix.len() + 1;
                    return Some(payload_off);
                }
                return None;
            }

            let child = (base_val as isize + byte as isize) as usize;
            if child >= self.n {
                return None;
            }
            if self.check[child] != byte {
                return None;
            }

            // Check for 'P' sentinel match directly in the trie.
            if byte == b'P' {
                let child_base = *self.base.get(child)?;
                if child_base < 0 {
                    return Some((-child_base) as usize);
                }
                return Some(child);
            }

            node = child;
        }
        None
    }

    /// Read a null-terminated string from the edge array at `offset`.
    ///
    /// Returns a borrowed slice into `self.edges` — no heap allocation.
    fn read_edge_string(&self, offset: usize) -> Option<&[u8]> {
        let start = offset;
        let mut end = start;
        loop {
            if end >= self.edges.len() {
                return None;
            }
            if self.edges[end] == 0 {
                break;
            }
            end += 1;
        }
        Some(&self.edges[start..end])
    }

    /// Read the 2-byte payload (rule-record index) from the edge array.
    ///
    /// After the null-terminated suffix, the next 2 bytes are the record index (LE).
    fn read_payload_index(&self, payload_off: usize) -> Option<u16> {
        let b0 = *self.edges.get(payload_off)?;
        let b1 = *self.edges.get(payload_off + 1)?;
        Some(u16::from_le_bytes([b0, b1]))
    }

    /// Get a single 6-byte rule record at `index`.
    pub fn get_record(&self, index: usize) -> Option<ColligRule> {
        let off = index * 6;
        let raw = self.records.get(off..off + 6)?;
        let is_last = (raw[0] & 0x80) != 0;
        let rule_type = raw[0] & 0x7f;
        let params = [raw[1], raw[2], raw[3], raw[4], raw[5]];
        Some(ColligRule {
            rule_type,
            is_last,
            params,
        })
    }

    /// Look up `query` in the colligation trie and return the matching rule
    /// chain (a `Vec` of consecutive rule records starting at the matched index).
    ///
    /// Returns an empty `Vec` if no match is found.
    ///
    /// Reads consecutive records until `is_last` is true.
    pub fn lookup(&self, query: &[u8]) -> Vec<ColligRule> {
        let payload_off = match self.search(query) {
            Some(off) => off,
            None => return Vec::new(),
        };
        let record_idx = match self.read_payload_index(payload_off) {
            Some(idx) => idx as usize,
            None => return Vec::new(),
        };
        self.rule_chain(record_idx)
    }

    /// Collect the rule chain starting at `record_idx`.
    ///
    /// Rules are chained consecutively until a record with `is_last = true`.
    fn rule_chain(&self, start: usize) -> Vec<ColligRule> {
        let total = self.records.len() / 6;
        let mut result = Vec::new();
        let mut idx = start;
        loop {
            if idx >= total {
                break;
            }
            let rec = match self.get_record(idx) {
                Some(r) => r,
                None => break,
            };
            let last = rec.is_last;
            result.push(rec);
            if last {
                break;
            }
            idx += 1;
        }
        result
    }

    /// Number of nodes in the trie.
    pub fn node_count(&self) -> usize {
        self.n
    }

    /// Number of secondary rule records.
    pub fn record_count(&self) -> usize {
        self.records.len() / 6
    }

    /// Whether a rule's params satisfy the **default variant** condition for type-4/5.
    ///
    /// Filter: `(params[0] | (params[1] << 8)) >> 2 & 0xf == 0x01`
    ///       ↔ bits[5:2] of the 16-bit param word == 1
    ///       ↔ `params[0] & 0x3c == 0x04` (when params[1] == 0)
    fn is_default_variant_match(rule: &ColligRule) -> bool {
        let uvar3 = (rule.params[0] as u16) | ((rule.params[1] as u16) << 8);
        (uvar3 >> 2) & 0xf == 0x01
    }

    /// Apply type-4 and type-5 colligation rules for the default voice variant.
    ///
    /// For each entry in `syllable_ids`:
    /// 1. Computes the variable-length phoneme bytes for that single syllable
    ///    (`[final_c, medium+0x13, initial+0x28]`, skipping absent components).
    /// 2. Looks up those bytes in the colligation trie.
    /// 3. Checks each matching rule: if rule type ∈ {4, 5} and the rule
    ///    satisfies the default-variant condition
    ///    (`(params[0] | params[1]<<8) >> 2 & 0xf == 1`), marks the position.
    ///
    /// Returns a `Vec<bool>` parallel to `syllable_ids`:
    ///   `true`  = a type-4/5 default-variant colligation rule applies here,
    ///             meaning VoiceInfo.pkg likely has an explicit recording for
    ///             this specific phoneme boundary context.
    ///   `false` = no direct colligation rule found.
    ///
    /// Lets unit selection prefer exact-context recordings at colligation-confirmed positions.
    pub fn apply_type45_rules(&self, syllable_ids: &[u16]) -> Vec<bool> {
        let mut result = vec![false; syllable_ids.len()];

        for (i, &sid) in syllable_ids.iter().enumerate() {
            // Skip sentinels, silence, and non-Korean
            if sid == 0xFFFF || sid == 0x0000 || sid & 0x8000 != 0 {
                continue;
            }

            // Build per-syllable phoneme bytes [final_c, medium+0x13, initial+0x28]
            let final_c = ((sid >> 10) & 0x1f) as u8;
            let medium = ((sid >> 5) & 0x1f) as u8;
            let initial = (sid & 0x1f) as u8;

            let mut key_buf = [0u8; 3];
            let mut key_len = 0;
            if final_c != 0 {
                key_buf[key_len] = final_c;
                key_len += 1;
            }
            if medium != 0 {
                key_buf[key_len] = medium + 0x13;
                key_len += 1;
            }
            if initial != 0 {
                key_buf[key_len] = initial + 0x28;
                key_len += 1;
            }

            if key_len == 0 {
                continue;
            }

            let rules = self.lookup(&key_buf[..key_len]);
            for rule in &rules {
                if (rule.rule_type == 4 || rule.rule_type == 5)
                    && Self::is_default_variant_match(rule)
                {
                    result[i] = true;
                    break;
                }
            }
        }

        result
    }
}

/// Convert a slice of packed 16-bit syllable IDs to the byte-phoneme sequence
/// used as COLLIGATION.PKG trie keys.
///
/// Each packed syllable has the bit layout:
///   bits  4:0  = initial consonant, 0 = none
///   bits  9:5  = medium vowel, 0 = none
///   bits 14:10 = final consonant, 0 = none
///
/// For each syllable, the output bytes are written in the order
/// `[final_c, medium+0x13, initial+0x28]`, skipping any component that is zero.
/// Non-Korean (high bit set) or sentinel (0xFFFF) syllable IDs are skipped.
///
/// The returned `Vec<u8>` is NOT null-terminated; trie lookup appends `b'P'` (0x50) internally.
pub fn syllable_ids_to_phoneme_bytes(syllable_ids: &[u16]) -> Vec<u8> {
    let mut result = Vec::with_capacity(syllable_ids.len() * 3);

    for &sid in syllable_ids {
        // Skip sentinel and silence
        if sid == 0xFFFF || sid == 0x0000 {
            continue;
        }
        // Skip non-Korean (high bit set)
        if sid & 0x8000 != 0 {
            continue;
        }

        let final_c = ((sid >> 10) & 0x1f) as u8; // bits 14:10 = coda
        let medium = ((sid >> 5) & 0x1f) as u8; // bits  9:5  = vowel
        let initial = (sid & 0x1f) as u8; // bits  4:0  = onset

        // Output order: final_c, medium+0x13, initial+0x28 (skip if zero)
        if final_c != 0 {
            result.push(final_c);
        }
        if medium != 0 {
            result.push(medium + 0x13);
        }
        if initial != 0 {
            result.push(initial + 0x28);
        }
    }

    result
}

/// One entry from the 140-entry Type-1 template table shipped with Mirae voice data.
///
/// Each entry defines:
/// - A fixed phoneme-stream search pattern (`search[0..search_len]`).
/// - A replacement byte sequence   (`replace[0..replace_len]`).
/// - The required stream position modulo 3 (`pos_type`: 0=final_c, 1=medium, 2=initial).
/// - Optional filter bytes (0x46=`'F'`=any; otherwise restrict to compound-coda types).
///
/// Stream format: each syllable encodes
/// as three consecutive bytes `[final_c, medium, initial]`, with 0 replaced by 0x20
/// (ASCII space).
///
/// When the search pattern is found at stream position P where P % 3 == pos_type,
/// it spawns a "Type-1 hypothesis": the search bytes at P are replaced by the
/// replacement bytes, and the resulting stream is decoded back to syllable IDs
/// Unit selection is then run on both the original and all
/// hypotheses, keeping the best-scoring match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Type1Entry {
    /// Required position modulo 3: 0=final_c, 1=medium, 2=initial.
    pub pos_type: u8,
    /// Number of valid bytes in `search`.
    pub search_len: u8,
    /// Search pattern (byte values; trailing zeros are padding).
    pub search: [u8; 8],
    /// Number of valid bytes in `replace`.
    pub replace_len: u8,
    /// Replacement bytes.
    pub replace: [u8; 8],
    /// Filter bytes: 0x46='F'=any; other values constrain compound-coda type.
    pub filter: [u8; 3],
}

/// All 140 Type-1 template entries (67-byte stride in the reference data).
///
/// Generated offline (`gen_type1_rust.py`) from the reference COLLIGATION / phoneme pipeline.
pub const TYPE1_TABLE: &[Type1Entry] = &[
    // [  0] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 2, 9, 3, 0, 0, 0, 0],
        replace_len: 1,
        replace: [7, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [  1] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 2, 1, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [7, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [  2] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 2, 2, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [7, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [  3] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 2, 3, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [7, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [  4] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 2, 5, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [7, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [  5] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 2, 7, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [7, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [  6] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 2, 9, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [7, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [  7] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 2, 10, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [7, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [  8] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 2, 13, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [7, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [  9] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 2, 1, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 10] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 2, 2, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 11] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 2, 3, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 12] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 2, 5, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 13] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 2, 7, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 14] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 2, 9, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 15] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 2, 10, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 16] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 2, 13, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 17] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [16, 2, 10, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [7, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 18] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [16, 2, 10, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 19] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [16, 3, 10, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [7, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 20] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [16, 3, 10, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 21] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [16, 3, 13, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [7, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 22] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [16, 3, 13, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 23] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [16, 7, 10, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [7, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 24] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [16, 7, 10, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 25] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 1,
        search: [3, 0, 0, 0, 0, 0, 0, 0],
        replace_len: 1,
        replace: [7, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 26] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 1,
        search: [3, 0, 0, 0, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 27] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 5, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [7, 0, 0, 0, 0, 0, 0, 0],
        filter: [23, 70, 70],
    },
    // [ 28] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 5, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [23, 70, 70],
    },
    // [ 29] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 3,
        search: [32, 7, 10, 0, 0, 0, 0, 0],
        replace_len: 1,
        replace: [7, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 30] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 1,
        search: [7, 0, 0, 0, 0, 0, 0, 0],
        replace_len: 1,
        replace: [7, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 31] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 1,
        search: [7, 0, 0, 0, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 32] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 2,
        search: [32, 4, 0, 0, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 33] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 1, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [18, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 34] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 2, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [18, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 35] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 3, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [18, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 36] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 5, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [18, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 37] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 7, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [18, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 38] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 9, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [18, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 39] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 10, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [18, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 40] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 13, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [18, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 41] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 1, 27, 0, 0, 0, 0],
        replace_len: 1,
        replace: [18, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 42] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 2, 27, 0, 0, 0, 0],
        replace_len: 1,
        replace: [18, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 43] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 3, 27, 0, 0, 0, 0],
        replace_len: 1,
        replace: [18, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 44] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 5, 27, 0, 0, 0, 0],
        replace_len: 1,
        replace: [18, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 45] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 7, 27, 0, 0, 0, 0],
        replace_len: 1,
        replace: [18, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 46] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 9, 27, 0, 0, 0, 0],
        replace_len: 1,
        replace: [18, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 47] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 10, 27, 0, 0, 0, 0],
        replace_len: 1,
        replace: [18, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 48] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 13, 27, 0, 0, 0, 0],
        replace_len: 1,
        replace: [18, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 49] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 3,
        search: [32, 19, 9, 0, 0, 0, 0, 0],
        replace_len: 1,
        replace: [18, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 50] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 9, 3, 0, 0, 0, 0],
        replace_len: 1,
        replace: [18, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 51] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 9, 7, 0, 0, 0, 0],
        replace_len: 1,
        replace: [18, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 52] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 5, 4, 3, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 2, 70],
    },
    // [ 53] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 3,
        search: [32, 5, 4, 0, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 2, 70],
    },
    // [ 54] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 3,
        search: [32, 5, 1, 0, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 2, 70],
    },
    // [ 55] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 3,
        search: [32, 5, 9, 0, 0, 0, 0, 0],
        replace_len: 1,
        replace: [25, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 2, 70],
    },
    // [ 56] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 1, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 57] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 2, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 58] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 3, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 59] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 5, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 60] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 7, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 61] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 9, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 62] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 10, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 63] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 13, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 64] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 1, 27, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 65] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 2, 27, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 66] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 3, 27, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 67] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 5, 27, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 68] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 7, 27, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 69] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 9, 27, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 70] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 10, 27, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 71] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 13, 27, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 72] pos=0
    Type1Entry {
        pos_type: 0,
        search_len: 5,
        search: [4, 9, 32, 4, 3, 0, 0, 0],
        replace_len: 3,
        replace: [4, 9, 32, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 73] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 18, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [16, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 74] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 18, 27, 0, 0, 0, 0],
        replace_len: 1,
        replace: [16, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 75] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 19, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [16, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 76] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 19, 27, 0, 0, 0, 0],
        replace_len: 1,
        replace: [16, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 77] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 7, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [16, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 78] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 7, 3, 0, 0, 0, 0],
        replace_len: 1,
        replace: [16, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 79] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 7, 7, 0, 0, 0, 0],
        replace_len: 1,
        replace: [16, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 80] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [32, 19, 7, 15, 0, 0, 0, 0],
        replace_len: 1,
        replace: [16, 0, 0, 0, 0, 0, 0, 0],
        filter: [30, 70, 70],
    },
    // [ 81] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 9, 32, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 82] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 9, 3, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 83] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 9, 7, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 84] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 19, 9, 15, 0, 0, 0, 0],
        replace_len: 1,
        replace: [6, 0, 0, 0, 0, 0, 0, 0],
        filter: [30, 70, 70],
    },
    // [ 85] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 4, 1, 32, 0, 0, 0, 0],
        replace_len: 4,
        replace: [32, 4, 9, 32, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 86] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 4, 2, 32, 0, 0, 0, 0],
        replace_len: 4,
        replace: [32, 4, 9, 32, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 87] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 4, 3, 32, 0, 0, 0, 0],
        replace_len: 4,
        replace: [32, 4, 9, 32, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [ 88] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 4, 5, 32, 0, 0, 0, 0],
        replace_len: 4,
        replace: [32, 4, 9, 32, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 89] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 4, 7, 32, 0, 0, 0, 0],
        replace_len: 4,
        replace: [32, 4, 9, 32, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 90] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 4, 9, 32, 0, 0, 0, 0],
        replace_len: 4,
        replace: [32, 4, 9, 32, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 91] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 4, 10, 32, 0, 0, 0, 0],
        replace_len: 4,
        replace: [32, 4, 9, 32, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 92] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 4, 13, 32, 0, 0, 0, 0],
        replace_len: 4,
        replace: [32, 4, 9, 32, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 93] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 4, 1, 27, 0, 0, 0, 0],
        replace_len: 4,
        replace: [32, 4, 9, 32, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 94] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 4, 2, 27, 0, 0, 0, 0],
        replace_len: 4,
        replace: [32, 4, 9, 32, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 95] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 4, 3, 27, 0, 0, 0, 0],
        replace_len: 4,
        replace: [32, 4, 9, 32, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 96] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 4, 5, 27, 0, 0, 0, 0],
        replace_len: 4,
        replace: [32, 4, 9, 32, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 97] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 4, 7, 27, 0, 0, 0, 0],
        replace_len: 4,
        replace: [32, 4, 9, 32, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 98] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 4, 9, 27, 0, 0, 0, 0],
        replace_len: 4,
        replace: [32, 4, 9, 32, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [ 99] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 4, 10, 27, 0, 0, 0, 0],
        replace_len: 4,
        replace: [32, 4, 9, 32, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [100] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 4,
        search: [7, 4, 13, 27, 0, 0, 0, 0],
        replace_len: 4,
        replace: [32, 4, 9, 32, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [101] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [11, 32, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [1, 25, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [102] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [11, 32, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [1, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [103] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [11, 32, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [11, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [104] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [11, 27, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [1, 25, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [105] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [11, 27, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [1, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [106] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [11, 27, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [11, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [107] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [13, 32, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [3, 25, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [108] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [13, 32, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [13, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [109] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [13, 27, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [3, 25, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [110] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [13, 27, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [13, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [111] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [12, 32, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [4, 25, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [112] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [12, 32, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [2, 25, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [113] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [12, 27, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [4, 25, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [114] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [12, 27, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [2, 25, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [115] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [3, 32, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [9, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [116] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [3, 32, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [7, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [117] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [3, 32, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [3, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [118] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [3, 27, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [9, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [119] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [3, 27, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [7, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [120] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [3, 27, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [3, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [121] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [1, 32, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [9, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [122] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [1, 32, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [1, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [123] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [1, 27, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [9, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [124] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [1, 27, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [1, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [125] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [4, 32, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [10, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [126] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [4, 32, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [4, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [127] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [4, 27, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [10, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [128] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [4, 27, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [4, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [129] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [18, 32, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [5, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [130] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [18, 27, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [5, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [131] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [19, 32, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [7, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [132] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [19, 27, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [7, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [133] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 1,
        search: [20, 0, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [15, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [134] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 2,
        search: [15, 32, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [15, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [135] pos=1
    Type1Entry {
        pos_type: 1,
        search_len: 1,
        search: [21, 0, 0, 0, 0, 0, 0, 0],
        replace_len: 2,
        replace: [16, 32, 0, 0, 0, 0, 0, 0],
        filter: [70, 0, 3],
    },
    // [136] pos=0
    Type1Entry {
        pos_type: 0,
        search_len: 2,
        search: [10, 13, 0, 0, 0, 0, 0, 0],
        replace_len: 3,
        replace: [13, 1, 32, 0, 0, 0, 0, 0],
        filter: [26, 70, 70],
    },
    // [137] pos=0
    Type1Entry {
        pos_type: 0,
        search_len: 2,
        search: [9, 10, 0, 0, 0, 0, 0, 0],
        replace_len: 3,
        replace: [13, 1, 32, 0, 0, 0, 0, 0],
        filter: [21, 70, 70],
    },
    // [138] pos=0  (empty replacement — skip in implementation)
    Type1Entry {
        pos_type: 0,
        search_len: 2,
        search: [7, 4, 0, 0, 0, 0, 0, 0],
        replace_len: 0,
        replace: [0, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
    // [139] pos=2
    Type1Entry {
        pos_type: 2,
        search_len: 1,
        search: [9, 0, 0, 0, 0, 0, 0, 0],
        replace_len: 1,
        replace: [7, 0, 0, 0, 0, 0, 0, 0],
        filter: [70, 70, 70],
    },
];

/// Encode a slice of packed 16-bit syllable IDs to the phoneme byte stream
/// used by the Type-1 colligation table.
///
/// Each syllable encodes as three bytes:
/// `[final_c, medium, initial]` where 0-components are replaced with 0x20 (space).
///
/// Stream positions:
///   byte[3i+0] = final_c  = bits[14:10] of sid_i  (or 0x20 if zero)
///   byte[3i+1] = medium   = bits[ 9: 5] of sid_i  (or 0x20 if zero)
///   byte[3i+2] = initial  = bits[ 4: 0] of sid_i  (or 0x20 if zero)
///
/// Non-Korean syllable IDs (bit 15 set) are encoded specially:
///   [0x20, 0x20, (sid & 0xFF).wrapping_add(0x14)]
pub fn phoneme_stream_encode(ids: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(ids.len() * 3);
    for &sid in ids {
        if sid == 0xFFFF {
            // sentinel — stop
            break;
        }
        if sid & 0x8000 != 0 {
            // Non-Korean special encoding (high-bit syllable IDs)
            let lo = (sid & 0xFF) as u8;
            out.push(0x20);
            out.push(0x20);
            out.push(lo.wrapping_add(0x14));
        } else {
            let final_c = ((sid >> 10) & 0x1f) as u8;
            let medium = ((sid >> 5) & 0x1f) as u8;
            let initial = (sid & 0x1f) as u8;
            out.push(if final_c == 0 { 0x20 } else { final_c });
            out.push(if medium == 0 { 0x20 } else { medium });
            out.push(if initial == 0 { 0x20 } else { initial });
        }
    }
    out
}

/// Decode a phoneme byte stream back to packed 16-bit syllable IDs.
///
/// Each group of three bytes `[final_c, medium, initial]` gives:
///   sid = (final_c << 10) | (medium << 5) | initial   (with 0x20 → 0 for each component)
///
/// Non-Korean entries (initial >= 0x45 = 'E') set high bit 0x8000 and decode as
///   initial - 0x14 = original special value.
pub fn phoneme_stream_decode(stream: &[u8]) -> Vec<u16> {
    let mut out = Vec::with_capacity(stream.len() / 3);
    let mut i = 0;
    while i + 2 < stream.len() {
        let final_c_b = stream[i];
        let medium_b = stream[i + 1];
        let initial_b = stream[i + 2];

        let final_c = if final_c_b == 0x20 {
            0u16
        } else {
            final_c_b as u16
        };
        let medium = if medium_b == 0x20 {
            0u16
        } else {
            medium_b as u16
        };

        let sid = if initial_b >= 0x45 {
            // Non-Korean special value
            let special = initial_b as u16 - 0x14;
            0x8000u16 | (final_c << 10) | (medium << 5) | special
        } else {
            let initial = if initial_b == 0x20 {
                0u16
            } else {
                initial_b as u16
            };
            (final_c << 10) | (medium << 5) | initial
        };
        out.push(sid);
        i += 3;
    }
    out
}

/// A Type-1 hypothesis match: one entry in the template table matched the
/// phoneme stream at a specific syllable boundary.
#[derive(Debug, Clone)]
pub struct Type1Match {
    /// 0-based index of the first syllable in the original sequence affected.
    pub syllable_idx: usize,
    /// How many consecutive original syllables this match spans (≥ 1).
    /// If > 1, syllables syllable_idx+1 .. syllable_idx+syllables_covered-1
    /// are "absorbed" by the hypothesis unit at syllable_idx.
    pub syllables_covered: usize,
    /// The hypothesis syllable ID at position syllable_idx (after applying
    /// the template replacement).  May differ from the original ID.
    pub hyp_sid: u16,
}

/// Scan `ids` for Type-1 template matches and return all found hypotheses.
///
/// Pattern-search over the Type-1 template table.
///
/// For each entry in `TYPE1_TABLE`:
/// 1. Encode `ids` to the phoneme byte stream (three bytes per syllable, space padding).
/// 2. Search for all occurrences of the entry's search bytes in the stream.
/// 3. For each hit at position P where P % 3 == pos_type: apply the replacement,
///    decode the resulting stream, and compute the hypothesis syllable ID at
///    position syllable_idx = P / 3.
///
/// Entries with empty replacement (`replace_len == 0`) are skipped because
/// the resulting stream alignment cannot be trivially decoded to syllable IDs.
///
/// Note: filter bytes (compound-coda type checks in the full engine) are NOT
/// applied here; false-positive hypotheses simply fail to find a matching
/// VoiceInfo entry and are discarded during unit selection.
pub fn find_type1_matches(ids: &[u16]) -> Vec<Type1Match> {
    if ids.is_empty() {
        return Vec::new();
    }
    let stream = phoneme_stream_encode(ids);
    let mut matches = Vec::new();
    // Reusable buffer for hypothesis streams (avoids per-match allocation).
    let mut hyp_stream = Vec::with_capacity(stream.len() + 16);

    for entry in TYPE1_TABLE {
        if entry.search_len == 0 || entry.replace_len == 0 {
            continue; // skip empty patterns and empty replacements
        }
        let slen = entry.search_len as usize;
        let rlen = entry.replace_len as usize;
        let pt = entry.pos_type as usize;
        let search_pat = &entry.search[..slen];
        let replace_pat = &entry.replace[..rlen];

        // Find all occurrences of search_pat in the stream.
        let mut start = 0usize;
        while start + slen <= stream.len() {
            // Locate the next occurrence starting at `start`.
            let found_rel = stream[start..].windows(slen).position(|w| w == search_pat);
            let pos = match found_rel {
                Some(r) => start + r,
                None => break,
            };

            // Only accept if the position satisfies the required modulo.
            if pos % 3 == pt {
                let syl_idx = pos / 3;
                let end_pos = pos + slen;
                // Ceiling division: which syllable comes just after the match end?
                let syl_end = end_pos.div_ceil(3);
                let covered = syl_end.saturating_sub(syl_idx).max(1);

                // Build hypothesis stream: prefix + replacement + suffix.
                hyp_stream.clear();
                hyp_stream.extend_from_slice(&stream[..pos]);
                hyp_stream.extend_from_slice(replace_pat);
                hyp_stream.extend_from_slice(&stream[pos + slen..]);

                // Decode hypothesis stream and extract the syllable ID at syl_idx.
                let hyp_ids = phoneme_stream_decode(&hyp_stream);
                if let Some(&hyp_sid) = hyp_ids.get(syl_idx) {
                    matches.push(Type1Match {
                        syllable_idx: syl_idx,
                        syllables_covered: covered,
                        hyp_sid,
                    });
                }
            }

            // Advance by one to allow overlapping matches (like strstr).
            start = pos + 1;
        }
    }

    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn pkg_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("mirae2.0/Voice/COLLIGATION.PKG")
    }

    #[test]
    fn test_parse_header() {
        let path = pkg_path();
        if !path.exists() {
            return;
        } // skip if file not present in CI
        let coll = Colligation::load(&path).expect("load failed");
        // Known values from binary analysis: N=200048, E=814804.
        assert!(coll.node_count() > 0, "no nodes");
        assert!(coll.base.len() == coll.n, "base len mismatch");
        assert!(coll.check.len() == coll.n, "check len mismatch");
        assert!(!coll.edges.is_empty(), "empty edges");
        println!(
            "nodes={}, edges={}, records={}",
            coll.node_count(),
            coll.edges.len(),
            coll.record_count()
        );
    }

    #[test]
    fn test_get_record_bounds() {
        let path = pkg_path();
        if !path.exists() {
            return;
        }
        let coll = Colligation::load(&path).expect("load failed");
        // Record 0 should be readable.
        if coll.record_count() > 0 {
            let rec = coll.get_record(0).unwrap();
            // rule_type high bit must be stripped.
            assert!(rec.rule_type < 128);
        }
    }

    #[test]
    fn test_colligation_rule_struct() {
        let r = ColligRule {
            rule_type: 3,
            is_last: true,
            params: [1, 2, 3, 4, 5],
        };
        assert!(!r.is_last || r.rule_type == 3);
    }

    #[test]
    fn test_syllable_ids_to_phoneme_bytes_empty() {
        // Empty input → empty output
        assert_eq!(syllable_ids_to_phoneme_bytes(&[]), Vec::<u8>::new());
    }

    #[test]
    fn test_syllable_ids_to_phoneme_bytes_sentinel() {
        // Sentinel 0xFFFF and 0x0000 are skipped
        assert_eq!(
            syllable_ids_to_phoneme_bytes(&[0xFFFF, 0x0000]),
            Vec::<u8>::new()
        );
    }

    #[test]
    fn test_syllable_ids_to_phoneme_bytes_syllable() {
        // Syllable with initial=1, medium=1, final_c=0:
        // sid = (0<<10)|(1<<5)|1 = 0x0021
        // final_c=0 (skip), medium+0x13=1+19=20, initial+0x28=1+40=41
        let sid: u16 = 0x0021;
        let bytes = syllable_ids_to_phoneme_bytes(&[sid]);
        assert_eq!(bytes, vec![20u8, 41u8]);
    }

    #[test]
    fn test_syllable_ids_to_phoneme_bytes_with_coda() {
        // Syllable with initial=18, medium=1, final_c=2:
        // sid = (2<<10)|(1<<5)|18 = 0x0832
        // final_c=2 (keep), medium+19=20, initial+40=58
        let sid: u16 = (2u16 << 10) | (1u16 << 5) | 18u16;
        let bytes = syllable_ids_to_phoneme_bytes(&[sid]);
        assert_eq!(bytes, vec![2u8, 20u8, 58u8]);
    }

    #[test]
    fn test_syllable_ids_to_phoneme_bytes_non_korean_skipped() {
        // High bit set = non-Korean → skip
        let sid: u16 = 0x8000 | 42;
        assert_eq!(syllable_ids_to_phoneme_bytes(&[sid]), Vec::<u8>::new());
    }

    /// Verify is_default_variant_match directly.
    ///
    /// The condition: (params[0] | (params[1] << 8)) >> 2 & 0xf == 1
    /// i.e., params[0] & 0x3c == 0x04 when params[1] == 0.
    #[test]
    fn test_default_variant_match_condition() {
        // params[0]=4 (0x04): bits[5:2] = 0001 → cvar=1 → match
        let r = ColligRule {
            rule_type: 4,
            is_last: true,
            params: [4, 0, 0, 0, 0],
        };
        assert!(
            Colligation::is_default_variant_match(&r),
            "params[0]=4 should match default variant"
        );

        // params[0]=69 (0x45): 69>>2=17, 17&0xf=1 → match
        let r = ColligRule {
            rule_type: 4,
            is_last: true,
            params: [69, 0, 0, 0, 0],
        };
        assert!(
            Colligation::is_default_variant_match(&r),
            "params[0]=69 should match default variant"
        );

        // params[0]=133 (0x85): 133>>2=33, 33&0xf=1 → match
        let r = ColligRule {
            rule_type: 4,
            is_last: true,
            params: [133, 0, 0, 0, 0],
        };
        assert!(
            Colligation::is_default_variant_match(&r),
            "params[0]=133 should match default variant"
        );

        // params[0]=197 (0xC5): 197>>2=49, 49&0xf=1 → match
        let r = ColligRule {
            rule_type: 4,
            is_last: true,
            params: [197, 0, 0, 0, 0],
        };
        assert!(
            Colligation::is_default_variant_match(&r),
            "params[0]=197 should match default variant"
        );

        // params[0]=172 (0xAC): 172>>2=43, 43&0xf=11 → no match
        let r = ColligRule {
            rule_type: 4,
            is_last: true,
            params: [172, 0, 0, 0, 0],
        };
        assert!(
            !Colligation::is_default_variant_match(&r),
            "params[0]=172 should NOT match default variant (cvar=11)"
        );

        // params[0]=44 (0x2C): 44>>2=11, 11&0xf=11 → no match
        let r = ColligRule {
            rule_type: 4,
            is_last: true,
            params: [44, 0, 0, 0, 0],
        };
        assert!(
            !Colligation::is_default_variant_match(&r),
            "params[0]=44 should NOT match default variant (cvar=11)"
        );

        // params[0]=40 (0x28): 40>>2=10, 10&0xf=10 → no match (type-1 common value)
        let r = ColligRule {
            rule_type: 1,
            is_last: true,
            params: [40, 0, 8, 0, 0],
        };
        assert!(
            !Colligation::is_default_variant_match(&r),
            "params[0]=40 should NOT match default variant (cvar=10)"
        );
    }

    /// apply_type45_rules must return a Vec parallel to input.
    #[test]
    fn test_apply_type45_rules_empty() {
        let path = pkg_path();
        if !path.exists() {
            return;
        }
        let coll = Colligation::load(&path).expect("load failed");
        let marks = coll.apply_type45_rules(&[]);
        assert_eq!(marks, Vec::<bool>::new());
    }

    #[test]
    fn test_apply_type45_rules_sentinels() {
        let path = pkg_path();
        if !path.exists() {
            return;
        }
        let coll = Colligation::load(&path).expect("load failed");
        // Sentinels and non-Korean → never marked
        let sids = vec![0xFFFFu16, 0x0000u16, 0x8000u16 | 5];
        let marks = coll.apply_type45_rules(&sids);
        assert_eq!(marks.len(), 3);
        assert!(!marks[0]);
        assert!(!marks[1]);
        assert!(!marks[2]);
    }

    #[test]
    fn test_apply_type45_rules_length_matches_input() {
        let path = pkg_path();
        if !path.exists() {
            return;
        }
        let coll = Colligation::load(&path).expect("load failed");

        // Build some synthetic syllable IDs (initial=1=ㄱ, medium=1=ㅏ).
        // sid = (0<<10) | (1<<5) | 1 = 0x0021
        let sid: u16 = (1u16 << 5) | 1;
        let sids = vec![sid; 5];
        let marks = coll.apply_type45_rules(&sids);
        assert_eq!(marks.len(), 5, "output length must match input length");
    }

    /// Verify that the function actually returns `true` for at least one
    /// syllable from a list containing the known matching phoneme context
    /// [final_c=13=ㅍ, medium=1=ㅏ, initial=15=ㅃ] → key `0d1437` → params[0]=4 → default.
    ///
    /// Syllable: final_c=13, medium=1, initial=15
    ///   sid bits: final_c<<10 | medium<<5 | initial = (13<<10)|(1<<5)|15 = 0x341F
    #[test]
    fn test_apply_type45_rules_known_match() {
        let path = pkg_path();
        if !path.exists() {
            return;
        }
        let coll = Colligation::load(&path).expect("load failed");

        // sid for final_c=13, medium=1, initial=15
        // bits 14:10 = 13, bits 9:5 = 1, bits 4:0 = 15
        let sid: u16 = ((13u16) << 10) | ((1u16) << 5) | 15u16;
        // Verify key decoding: final_c=13, medium=1, initial=15
        //   phoneme bytes: [13, 1+0x13=0x14, 15+0x28=0x37] = [0x0D, 0x14, 0x37]
        let sids = vec![sid];
        let marks = coll.apply_type45_rules(&sids);
        assert_eq!(marks.len(), 1);
        // This key `0d1437` was confirmed to match a type-4 rule with params[0]=4
        // (default variant) from binary analysis.
        assert!(marks[0],
            "Syllable final_c=13,medium=1,initial=15 (key 0d1437) should have a colligation type-4 match");
    }

    #[test]
    fn test_phoneme_stream_encode_decode_roundtrip_pure_korean() {
        // Build a two-syllable sequence: 가(final_c=0,medium=1,initial=1) 나(final_c=0,medium=2,initial=2)
        let s1: u16 = 0x0021;
        let s2: u16 = 0x0042;
        let ids = vec![s1, s2];

        let stream = super::phoneme_stream_encode(&ids);
        // Expected: [0x20,1,1,  0x20,2,2]
        assert_eq!(stream, vec![0x20, 1, 1, 0x20, 2, 2]);

        let decoded = super::phoneme_stream_decode(&stream);
        assert_eq!(decoded, ids, "roundtrip should be identity");
    }

    #[test]
    fn test_phoneme_stream_encode_decode_with_coda() {
        // 닭(final_c=15, medium=1, initial=7) == sid (15<<10)|(1<<5)|7 = 0x3C27
        let sid: u16 = (15u16 << 10) | (1u16 << 5) | 7u16;
        let ids = vec![sid];

        let stream = super::phoneme_stream_encode(&ids);
        assert_eq!(stream, vec![15, 1, 7]);

        let decoded = super::phoneme_stream_decode(&stream);
        assert_eq!(decoded, ids);
    }

    #[test]
    fn test_phoneme_stream_encode_decode_roundtrip_multi() {
        // Several syllables including ones with zero components
        let ids: Vec<u16> = vec![
            (1u16 << 10) | (1 << 5) | 1, // full: final_c=1,medium=1,initial=1
            5u16 << 5,                   // medium-only: final_c=0,medium=5,initial=0
            3u16 << 10,                  // final_c-only
            9u16,                        // initial-only
        ];
        let stream = super::phoneme_stream_encode(&ids);
        let decoded = super::phoneme_stream_decode(&stream);
        assert_eq!(decoded, ids, "roundtrip failed for multi-syllable sequence");
    }

    #[test]
    fn test_phoneme_stream_encode_empty() {
        assert!(super::phoneme_stream_encode(&[]).is_empty());
        assert!(super::phoneme_stream_decode(&[]).is_empty());
    }

    #[test]
    fn test_phoneme_stream_encode_sentinel_stops() {
        let ids = vec![0x0021u16, 0xFFFF, 0x0042];
        let stream = super::phoneme_stream_encode(&ids);
        // Should only encode the first syllable before the sentinel
        assert_eq!(stream, vec![0x20, 1, 1]);
    }

    #[test]
    fn test_find_type1_matches_empty_input() {
        let matches = super::find_type1_matches(&[]);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_find_type1_matches_table_size() {
        // Ensure TYPE1_TABLE constant is fully populated.
        assert_eq!(
            super::TYPE1_TABLE.len(),
            140,
            "TYPE1_TABLE should have exactly 140 entries"
        );
    }

    #[test]
    fn test_find_type1_matches_entry138_skipped() {
        // Entry 138 has replace_len=0 and should never produce a match.
        let entry = &super::TYPE1_TABLE[138];
        assert_eq!(
            entry.replace_len, 0,
            "Entry 138 should have empty replacement (replace_len=0)"
        );
    }

    #[test]
    fn test_find_type1_matches_known_pattern_entry25() {
        // Entry 25: pos_type=2, search=[3], replace=[7]
        // A syllable with initial=3 should produce a match with initial=7.
        // Build a syllable: final_c=0=0x20, medium=1, initial=3.
        // sid = (0<<10)|(1<<5)|3 = 0x0023
        let sid: u16 = (1u16 << 5) | 3;
        let matches = super::find_type1_matches(&[sid]);
        // At least one match should be found for this syllable.
        let has_match = matches.iter().any(|m| m.syllable_idx == 0);
        assert!(has_match,
            "Entry 25 search=[3] should match a syllable with initial=3 at stream pos 2 (mod 3 == 2)");
    }

    #[test]
    fn test_type1_match_syllable_covered_single() {
        // A single-byte search at initial position spans exactly one syllable.
        // Entry 25: search_len=1 at pos_type=2; the initial byte of syllable 0 is at pos=2.
        // After replacement, syllables_covered should be 1 (end_pos=3, syl_end=1).
        let sid: u16 = (1u16 << 5) | 3;
        let matches = super::find_type1_matches(&[sid]);
        let m = matches.iter().find(|m| m.syllable_idx == 0).unwrap();
        assert_eq!(
            m.syllables_covered, 1,
            "Single-byte match at initial slot covers exactly 1 syllable"
        );
    }

    #[test]
    fn test_type1_match_hyp_sid_correct() {
        // Entry 25: search=[3], replace=[7], pos_type=2.
        // sid final_c=0,medium=1,initial=3 → stream=[0x20,1,3]; match at pos=2; replace → [0x20,1,7].
        // hyp_sid = decode([0x20,1,7])[0] = (0<<10)|(1<<5)|7 = 0x0027
        let sid: u16 = (1u16 << 5) | 3;
        let expected_hyp: u16 = (1u16 << 5) | 7;
        let matches = super::find_type1_matches(&[sid]);
        let m = matches
            .iter()
            .find(|m| m.syllable_idx == 0 && m.hyp_sid == expected_hyp);
        assert!(
            m.is_some(),
            "Entry 25: initial 3→7 should produce hyp_sid with initial=7"
        );
    }
}
