//! Dictionary pkg parser: double-array trie + TAIL + sub-structures.
//! Reimplemented from the original Future.exe disassembly only.
//! Layout: [u32 n1][u32 n2][BASE u32 x n1][CHECK u8 x n1][TAIL u8 x n2][sub-structs].
use std::fmt;
use std::fs;
use std::path::Path;

pub const KEY_END: u8 = 0x50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubARecord {
    pub kind: u8,
    pub sub: u8,
    pub v0: u16,
    pub v1: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TailEntry {
    pub x: u16,
    pub y: u8,
}

impl TailEntry {
    pub fn value(self) -> u32 {
        (self.x as u32) | ((self.y as u32) << 16)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrefixMatch {
    pub tail_offset: usize,
    pub matched: usize,
}

#[derive(Debug)]
pub struct DictError {
    msg: String,
}

impl DictError {
    fn new(msg: impl Into<String>) -> Self {
        DictError { msg: msg.into() }
    }
}

impl fmt::Display for DictError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "dict pkg error: {}", self.msg)
    }
}

impl std::error::Error for DictError {}

#[derive(Debug, Default)]
struct SubStruct {
    pairs: Vec<(u32, u32)>,
    records: Vec<u8>,
    rec_size: usize,
}

#[derive(Debug)]
pub struct Dict {
    n1: usize,
    n2: usize,
    base: Vec<i32>,
    check: Vec<u8>,
    tail: Vec<u8>,
    sub_a: SubStruct,
    sub_b: SubStruct,
}

impl Dict {
    pub fn load(path: impl AsRef<Path>) -> Result<Dict, DictError> {
        let data = fs::read(path).map_err(|e| DictError::new(format!("read: {e}")))?;
        Dict::from_bytes(&data)
    }

    pub fn from_bytes(data: &[u8]) -> Result<Dict, DictError> {
        let mut o = 0usize;
        let n1 = read_u32(data, &mut o)? as usize;
        let n2 = read_u32(data, &mut o)? as usize;
        // BASE (u32 × n1)
        let base_len = n1
            .checked_mul(4)
            .ok_or_else(|| DictError::new("n1 overflow"))?;
        need(data, o, base_len)?;
        let mut base = Vec::with_capacity(n1);
        for i in 0..n1 {
            let b = u32::from_le_bytes(data[o + i * 4..o + i * 4 + 4].try_into().unwrap());
            base.push(b as i32);
        }
        o += base_len;
        // CHECK (u8 × n1)
        need(data, o, n1)?;
        let check = data[o..o + n1].to_vec();
        o += n1;
        // TAIL (u8 × n2)
        need(data, o, n2)?;
        let tail = data[o..o + n2].to_vec();
        o += n2;
        let (sub_a, o) = parse_sub(data, o, 6)?;
        let (sub_b, o) = parse_sub(data, o, 26)?;
        if o != data.len() {
            return Err(DictError::new(format!(
                "size mismatch: consumed {o} of {} bytes",
                data.len()
            )));
        }
        Ok(Dict {
            n1,
            n2,
            base,
            check,
            tail,
            sub_a,
            sub_b,
        })
    }

    pub fn n1(&self) -> usize {
        self.n1
    }

    pub fn n2(&self) -> usize {
        self.n2
    }

    pub fn sub_a_count(&self) -> usize {
        self.sub_a.records.len() / self.sub_a.rec_size
    }

    pub fn sub_b_count(&self) -> usize {
        self.sub_b.records.len() / self.sub_b.rec_size
    }

    pub fn sub_a_pairs(&self) -> &[(u32, u32)] {
        &self.sub_a.pairs
    }

    pub fn sub_b_pairs(&self) -> &[(u32, u32)] {
        &self.sub_b.pairs
    }

    pub fn base(&self, node: usize) -> Option<i32> {
        self.base.get(node).copied()
    }

    pub fn check(&self, node: usize) -> Option<u8> {
        self.check.get(node).copied()
    }

    pub fn tail(&self, off: usize) -> Option<u8> {
        self.tail.get(off).copied()
    }

    pub fn tail_bytes(&self) -> &[u8] {
        &self.tail
    }

    pub fn search_exact(&self, key: &[u8]) -> Option<usize> {
        if key.is_empty() {
            return None;
        }
        let mut node: i32 = 1;
        let mut consumed: usize = 0;
        loop {
            let c = if consumed < key.len() {
                key[consumed]
            } else {
                KEY_END
            };
            let t = self.base[node as usize].wrapping_add(c as i32);
            if t < 0 || t as usize >= self.n1 || self.check[t as usize] != c {
                return None;
            }
            node = t;
            consumed += 1;
            if self.base[node as usize] < 0 {
                break;
            }
            if consumed > key.len() {
                return None;
            }
        }
        let off = (-self.base[node as usize]) as usize;
        if off >= self.n2 {
            return None;
        }
        if consumed == key.len() + 1 {
            return Some(off);
        }
        let tail = self.tail_string(off)?;
        let k = &key[consumed..];
        if tail.len() != k.len() + 1 || tail[..k.len()] != *k || tail[k.len()] != KEY_END {
            return None;
        }
        Some(off)
    }

    pub fn search_prefix(&self, key: &[u8]) -> Option<PrefixMatch> {
        if key.is_empty() {
            return None;
        }
        let mut node: i32 = 1;
        let mut consumed: usize = 0;
        loop {
            let c = if consumed < key.len() {
                key[consumed]
            } else {
                KEY_END
            };
            let t = self.base[node as usize].wrapping_add(c as i32);
            if t < 0 || t as usize >= self.n1 || self.check[t as usize] != c {
                let tp = self.base[node as usize].wrapping_add(KEY_END as i32);
                if tp < 0 || tp as usize >= self.n1 {
                    return None;
                }
                if self.check[tp as usize] != KEY_END || self.base[tp as usize] >= 0 {
                    return None;
                }
                let off = (-self.base[tp as usize]) as usize;
                if off >= self.n2 {
                    return None;
                }
                return Some(PrefixMatch {
                    tail_offset: off,
                    matched: consumed.min(key.len()),
                });
            }
            node = t;
            consumed += 1;
            if self.base[node as usize] < 0 {
                break;
            }
            if consumed > key.len() {
                return None;
            }
        }
        let off = (-self.base[node as usize]) as usize;
        if off >= self.n2 {
            return None;
        }
        if consumed == key.len() + 1 {
            return Some(PrefixMatch {
                tail_offset: off,
                matched: key.len(),
            });
        }
        let tail = self.tail_string(off)?;
        let k = &key[consumed..];
        if tail.len() > k.len() + 1 {
            return None;
        }
        for (i, &b) in tail.iter().enumerate() {
            let expect = if i < k.len() { k[i] } else { KEY_END };
            if b != expect {
                return None;
            }
        }
        Some(PrefixMatch {
            tail_offset: off,
            matched: (consumed + tail.len()).min(key.len()),
        })
    }

    pub fn tail_string(&self, off: usize) -> Option<&[u8]> {
        if off >= self.n2 {
            return None;
        }
        let end = self.tail[off..].iter().position(|&b| b == 0)?;
        Some(&self.tail[off..off + end])
    }

    pub fn tail_entry(&self, off: usize) -> Option<TailEntry> {
        let s = self.tail_string(off)?;
        let p = off + s.len() + 1;
        if p + 3 > self.n2 {
            return None;
        }
        let x = u16::from_le_bytes([self.tail[p], self.tail[p + 1]]);
        let y = self.tail[p + 2];
        Some(TailEntry { x, y })
    }

    pub fn sub_a_record(&self, idx: usize) -> Option<SubARecord> {
        if idx >= self.sub_a_count() {
            return None;
        }
        let r = &self.sub_a.records[idx * 6..idx * 6 + 6];
        Some(SubARecord {
            kind: r[0],
            sub: r[1],
            v0: u16::from_le_bytes([r[2], r[3]]),
            v1: u16::from_le_bytes([r[4], r[5]]),
        })
    }

    pub fn expand_records(&self, idx: usize) -> Vec<SubARecord> {
        let count = self.sub_a_count();
        if idx >= count {
            return Vec::new();
        }
        let mut out = Vec::new();
        let rec = |i: usize| -> SubARecord {
            let r = &self.sub_a.records[i * 6..i * 6 + 6];
            SubARecord {
                kind: r[0],
                sub: r[1],
                v0: u16::from_le_bytes([r[2], r[3]]),
                v1: u16::from_le_bytes([r[4], r[5]]),
            }
        };
        let mut first = rec(idx);
        first.kind &= 0x7f;
        out.push(first);
        let mut i = idx + 1;
        while i < count {
            let r = rec(i);
            let stop = r.kind & 0x80 != 0;
            out.push(r);
            if stop {
                break;
            }
            i += 1;
        }
        out
    }

    pub fn lookup(&self, key: &[u8]) -> Option<TailEntry> {
        let off = self.search_exact(key)?;
        self.tail_entry(off)
    }

    pub fn lookup_records(&self, key: &[u8]) -> Option<Vec<SubARecord>> {
        let off = self.search_exact(key)?;
        let e = self.tail_entry(off)?;
        Some(self.expand_records(e.x as usize))
    }

    pub fn lookup_prefix_records(&self, key: &[u8]) -> Option<(PrefixMatch, Vec<SubARecord>)> {
        let m = self.search_prefix(key)?;
        let e = self.tail_entry(m.tail_offset)?;
        Some((m, self.expand_records(e.x as usize)))
    }
}

pub fn syllable_to_key(code: u16, out: &mut Vec<u8>) -> bool {
    if code & 0x8000 != 0 {
        let u = code & 0x7fff;
        let c = match u {
            0x30..=0x39 => (u + 0x16) as u8,
            0x2d => 0x45,
            0x2e => 0x44,
            _ => return false,
        };
        out.push(c);
        return true;
    }
    let initial = (code >> 10) & 0x1f;
    let medial = (code >> 5) & 0x1f;
    let final_c = code & 0x1f;
    if initial != 0 {
        out.push(initial as u8);
    }
    if medial != 0 {
        out.push((medial + 0x13) as u8);
    }
    if final_c != 0 {
        out.push((final_c + 0x28) as u8);
    }
    true
}

pub fn key_from_syllables(codes: &[u16]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(codes.len() * 3);
    for &c in codes {
        if !syllable_to_key(c, &mut out) {
            return None;
        }
    }
    Some(out)
}

pub fn reverse_key(key: &[u8]) -> Vec<u8> {
    key.iter().rev().copied().collect()
}

fn need(data: &[u8], o: usize, n: usize) -> Result<(), DictError> {
    if o.checked_add(n).is_none_or(|end| end > data.len()) {
        return Err(DictError::new(format!(
            "truncated: need {n} bytes at offset {o} (len {})",
            data.len()
        )));
    }
    Ok(())
}

fn read_u32(data: &[u8], o: &mut usize) -> Result<u32, DictError> {
    need(data, *o, 4)?;
    let v = u32::from_le_bytes(data[*o..*o + 4].try_into().unwrap());
    *o += 4;
    Ok(v)
}

fn parse_sub(data: &[u8], mut o: usize, rec_size: usize) -> Result<(SubStruct, usize), DictError> {
    let count = read_u32(data, &mut o)? as usize;
    let npairs = read_u32(data, &mut o)? as usize;
    let pairs_len = npairs
        .checked_mul(8)
        .ok_or_else(|| DictError::new("pair count overflow"))?;
    need(data, o, pairs_len)?;
    let mut pairs = Vec::with_capacity(npairs);
    for i in 0..npairs {
        let a = u32::from_le_bytes(data[o + i * 8..o + i * 8 + 4].try_into().unwrap());
        let b = u32::from_le_bytes(data[o + i * 8 + 4..o + i * 8 + 8].try_into().unwrap());
        pairs.push((a, b));
    }
    o += pairs_len;
    let rec_len = count
        .checked_mul(rec_size)
        .ok_or_else(|| DictError::new("record count overflow"))?;
    need(data, o, rec_len)?;
    let records = data[o..o + rec_len].to_vec();
    o += rec_len;
    Ok((
        SubStruct {
            pairs,
            records,
            rec_size,
        },
        o,
    ))
}
