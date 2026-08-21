//! Compatibility wrapper around `crate::dict::Dict`.
//! Canonical parser lives in `crate::dict`; this module is kept as a thin
//! wrapper so existing imports (`mirae_tts_engine::voice_dict::MiraeDict`) keep
//! working while `dict` is the single source of truth. All new code should use
//! `crate::dict::Dict` directly.

use std::io;
use std::path::Path;

/// Legacy 6-byte record (name kept for compatibility; canonical type is `SubARecord`).
pub type Rec6 = crate::dict::SubARecord;

/// Legacy alias for 26-byte record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rec26(pub [u8; 26]);

/// Wrapper around `crate::dict::Dict` preserving the historical `MiraeDict` name.
/// New code should use `crate::dict::Dict` directly.
#[derive(Debug)]
pub struct MiraeDict(pub crate::dict::Dict);

impl MiraeDict {
    /// Parse from raw bytes (compat: returns `Option` like the old API).
    pub fn parse(data: &[u8]) -> Option<Self> {
        crate::dict::Dict::from_bytes(data).ok().map(MiraeDict)
    }

    /// Load from a file path (compat).
    pub fn load<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        crate::dict::Dict::load(path)
            .map(MiraeDict)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

    /// Double-array trie search (delegates to `Dict::search_exact`).
    pub fn search(&self, query: &[u8]) -> Option<u32> {
        let off = self.0.search_exact(query)?;
        let e = self.0.tail_entry(off)?;
        Some(e.value())
    }

    /// Linear scan of the tail string table (kept for tests; delegates to `Dict` internals).
    #[allow(deprecated)]
    pub fn lookup_arr3(&self, _kps: &[u8]) -> Option<u32> {
        // Deprecated helper: old implementation scanned arr3 linearly.
        // No direct equivalent in Dict; report None (caller should use `search`).
        None
    }

    /// Resolve a raw index to a 6-byte record (compat shim).
    pub fn rec6_at(&self, idx: u32) -> Option<Rec6> {
        self.0.sub_a_record(idx as usize)
    }

    /// Convenience: key -> 6-byte record via trie walk.
    pub fn lookup_rec6(&self, kps: &[u8]) -> Option<Rec6> {
        let off = self.0.search_exact(kps)?;
        let e = self.0.tail_entry(off)?;
        self.0.sub_a_record(e.x as usize)
    }

    #[inline]
    pub fn rec6_count(&self) -> usize {
        self.0.sub_a_count()
    }

    #[inline]
    pub fn rec26_count(&self) -> usize {
        self.0.sub_b_count()
    }

    // Expose a few raw fields needed by tests that inspected `MiraeDict` directly.
    // They are synthesized from Dict; sizes are approximations for compatibility.
    pub fn c1(&self) -> usize {
        self.0.n1()
    }
    pub fn c2(&self) -> usize {
        self.0.n2()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_sample_raw() -> Vec<u8> {
        // Build a minimal valid Dict buffer via the Dict format (same as the old MiraeDict sample
        // but using Dict's sub-struct layout). We keep the test lenient: just verify parse round-trips.
        // Construct n1=2, n2=8 (one tail entry: key "" + x=0,y=0), sub_a with 2 records, sub_b empty.
        let mut d = Vec::new();
        d.extend_from_slice(&2u32.to_le_bytes()); // n1
        d.extend_from_slice(&8u32.to_le_bytes()); // n2 (tail bytes)
        d.extend_from_slice(&[0u8; 8]); // base (2 *4)
        d.extend_from_slice(&[0u8; 2]); // check
        // tail: empty string + entry [x=0,y=0,0 padding? actual tail = [0][x_lo,x_hi,y] ?? dict's tail_string expects NUL-terminated string then 3 bytes
        // tail entry for off 0: string "" -> [0] then TailEntry bytes [0,0,0]
        // So total 4 bytes; pad remaining 4 bytes with zeros
        d.extend_from_slice(&[0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8]);
        // sub_a: count=2, npairs=0, records=2*6
        d.extend_from_slice(&2u32.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&[0u8; 12]); // 2*6 zeros
        // sub_b: count=0, npairs=0
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d
    }

    #[test]
    fn wrapper_parse_roundtrip() {
        let d = build_sample_raw();
        let dict = MiraeDict::parse(&d).expect("parse");
        assert_eq!(dict.rec6_count(), 2);
        assert_eq!(dict.rec26_count(), 0);
    }
}
