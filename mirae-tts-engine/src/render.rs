//! Waveform concatenation + chunk generation (FUN_0044c2e0 + FUN_0044b700).
//! Copies each unit's waveform from VoiceData.pkg, doubles real phonemes,
//! inserts pause silence; 20-slot x 16B ring buffer + 1MB total limit.
use std::io;

use crate::voice_data::{SCRATCH_SIZE, VoiceData};
use crate::{RING_MAX_BYTES, RING_SLOTS};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnitRecord {
    pub woff: u32,
    pub wlen: u32,
    pub pitch: i16,
    pub classcode: u8,
    pub pause: i16,
}

impl UnitRecord {
    pub fn new(woff: u32, wlen: u32) -> Self {
        UnitRecord {
            woff,
            wlen,
            pitch: 0,
            classcode: 0,
            pause: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderUnit {
    pub code_cur: u16,
    pub code_next: u16,
    pub record: UnitRecord,
    pub extra: Option<UnitRecord>,
}

pub fn is_real_phoneme(high6_cur: u16, low5_next: u16) -> bool {
    !(matches!(
        low5_next,
        1 | 4 | 6 | 8 | 9 | 10 | 0xb | 0xc | 0xd | 0xe | 0x10 | 0x11 | 0x12
    ) || (low5_next == 3 && high6_cur == 6))
}

pub fn is_real_phoneme_codes(cur: u16, next: u16) -> bool {
    is_real_phoneme(cur >> 10, next & 0x1f)
}

pub fn render_units(
    data: &mut VoiceData,
    units: &[RenderUnit],
    out: &mut Vec<u8>,
    random_mode: bool,
) -> io::Result<usize> {
    let mut scratch = vec![0u8; SCRATCH_SIZE];
    let mut total = 0usize;

    for u in units {
        let n = data.read_unit(u.record.woff, u.record.wlen, &mut scratch)?;
        if random_mode {
            let pitch = if u.record.pitch > 0 {
                u.record.pitch as f64
            } else {
                120.0
            };
            let factor = 120.0 / pitch; // engine+0xe0 = 120
            let src = &scratch[..n];
            let dst_len = ((n / 2) as f64 * factor) as usize;
            for i in 0..dst_len {
                let src_i = ((i as f64) / factor) as usize;
                let src_i = src_i.min(src.len() / 2 - 1);
                out.extend_from_slice(&src[src_i * 2..src_i * 2 + 2]);
            }
            total += dst_len * 2;
        } else {
            out.extend_from_slice(&scratch[..n]);
            total += n;
        }

        //   FUN_0044b350(code_cur>>10, code_next&0x1f) != 0
        //   && MOVSX class = byte[record+0x14]; class % 10 < 2 (signed IDIV)
        //   && word[extra+0x12] (pitch) != 0
        let class_i8 = u.record.classcode as i8;
        if is_real_phoneme(u.code_cur >> 10, u.code_next & 0x1f) && class_i8 % 10 < 2 {
            if let Some(extra) = u.extra {
                if extra.pitch != 0 {
                    if std::env::var("MIRAE_DEBUG").is_ok() {
                        eprintln!(
                            "[render-extra] add woff={} wlen={} pitch={} cls={:02x} cur={:04x} next={:04x}",
                            extra.woff,
                            extra.wlen,
                            extra.pitch,
                            u.record.classcode,
                            u.code_cur,
                            u.code_next
                        );
                    }
                    let n2 = data.read_unit(extra.woff, extra.wlen, &mut scratch)?;
                    out.extend_from_slice(&scratch[..n2]);
                    total += n2;
                    if std::env::var("MIRAE_DEBUG").is_ok() {
                        eprintln!("[render-extra] added n2={} out.len={}", n2, out.len());
                    }
                }
            }
        }

        if u.record.pause > 0 {
            let silence = (u.record.pause as usize) * 2;
            out.resize(out.len() + silence, 0);
            total += silence;
        }
    }

    Ok(total)
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub data: Vec<u8>,
    pub p1: u32,
    pub p2: u32,
}

impl Chunk {
    pub fn new(data: Vec<u8>) -> Self {
        Chunk { data, p1: 0, p2: 0 }
    }
}

pub struct ChunkRing {
    slots: [Option<Chunk>; RING_SLOTS],
    head: usize,
    tail: usize,
    total: usize,
}

impl Default for ChunkRing {
    fn default() -> Self {
        Self::new()
    }
}

impl ChunkRing {
    pub fn new() -> Self {
        ChunkRing {
            slots: Default::default(),
            head: 0,
            tail: 0,
            total: 0,
        }
    }

    pub fn can_push(&self, size: usize) -> bool {
        let full = (self.tail + 1) % RING_SLOTS == self.head;
        !full && self.total + size <= RING_MAX_BYTES
    }

    pub fn push(&mut self, chunk: Chunk) -> bool {
        if !self.can_push(chunk.data.len()) {
            return false;
        }
        self.total += chunk.data.len();
        self.slots[self.tail] = Some(chunk);
        self.tail = (self.tail + 1) % RING_SLOTS;
        true
    }

    pub fn is_empty(&self) -> bool {
        self.head == self.tail
    }

    pub fn len(&self) -> usize {
        if self.tail >= self.head {
            self.tail - self.head
        } else {
            RING_SLOTS - self.head + self.tail
        }
    }

    pub fn total_bytes(&self) -> usize {
        self.total
    }

    pub fn pop(&mut self) -> Option<Chunk> {
        if self.is_empty() {
            return None;
        }
        let chunk = self.slots[self.head].take();
        self.head = (self.head + 1) % RING_SLOTS;
        if let Some(ref c) = chunk {
            self.total -= c.data.len();
        }
        chunk
    }
}

pub fn produce_chunks(
    data: &mut VoiceData,
    units: &[RenderUnit],
    ring: &mut ChunkRing,
    per_chunk: usize,
    random_mode: bool,
    mut consume: impl FnMut(&mut ChunkRing),
) -> io::Result<usize> {
    let mut produced = 0;
    for batch in units.chunks(per_chunk.max(1)) {
        let mut pcm = Vec::new();
        render_units(data, batch, &mut pcm, random_mode)?;
        let chunk = Chunk::new(pcm);
        let mut pushed = false;
        while !pushed {
            if ring.push(chunk.clone()) {
                pushed = true;
            } else {
                consume(ring);
                if ring.is_empty() && !ring.can_push(chunk.data.len()) {
                    break;
                }
            }
        }
        if pushed {
            produced += 1;
        }
    }
    Ok(produced)
}
