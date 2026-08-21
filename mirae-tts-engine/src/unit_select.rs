//! Unit selection (FUN_0044b880 + FUN_0044a800): linear scan of VoiceInfo entries,
//! scoring, fallback, pitch smoothing, duration assignment, pause.
//! Spec from the original Future.exe Ghidra decompilation.
use crate::record::MARKER_SENTENCE_END;
use crate::tables::{
    FALLBACK_ALLOW, FALLBACK_REPL_HI, FALLBACK_REPL_LO, FILTER_TABLE, PHON_CLASS_FLAG_A,
    PHON_CLASS_FLAG_B, PHON_CLASS_FLAG_C, PHON_CLASS_FLAG_D, TONE_CLASS_MAP, TONE_TRANS_COST,
};
use crate::voice_info::{VoiceInfo, VoiceInfoEntry};

pub fn is_pause(hi10: u16, low5: u16) -> bool {
    ((hi10 == 2 || hi10 == 0xe || hi10 == 0x12 || hi10 == 0x1b)
        && (low5 == 1 || low5 == 4 || low5 == 0x12))
        || (hi10 == 6 && (low5 == 3 || low5 == 4 || low5 == 0x12))
}

pub fn is_real_phoneme(hi10: u16, low5: u16) -> bool {
    !(low5 == 1
        || low5 == 4
        || low5 == 6
        || low5 == 0x10
        || low5 == 0xc
        || low5 == 0x12
        || low5 == 8
        || low5 == 9
        || low5 == 10
        || low5 == 0xb
        || low5 == 0xd
        || low5 == 0xe
        || low5 == 0x11
        || (low5 == 3 && hi10 == 6))
}

fn normalize_target_class(c: u8) -> u8 {
    let mut c = c as i32;
    if c / 10 == 2 {
        c = c % 10 + 0x1e;
    }
    if c % 10 == 2 {
        c = (c / 10) * 10 + 3;
    } else if c % 10 == 5 {
        c = (c / 10) * 10 + 4;
    }
    c as u8
}

fn normalize_candidate_class_a(c: u8) -> u8 {
    let mut c = c as i32;
    if c / 10 == 2 {
        c = c % 10 + 0x1e;
    }
    if c % 10 == 2 {
        c = (c / 10) * 10 + 1;
    }
    c as u8
}

fn normalize_candidate_class_b(c: u8) -> u8 {
    let mut c = c as i32;
    if c / 10 == 2 {
        c = c % 10 + 0x1e;
    }
    if c % 10 == 2 {
        c = (c / 10) * 10 + 3;
    }
    c as u8
}

fn tone_class_row(norm_class: u8) -> usize {
    for row in 0..16 {
        if TONE_CLASS_MAP[row * 16] == norm_class {
            return row;
        }
    }
    0
}

fn tone_class_col(row: usize, norm_class: u8) -> usize {
    for col in 0..16 {
        if TONE_CLASS_MAP[row * 16 + col] == norm_class {
            return col;
        }
    }
    15
}

fn flag_d(hi10: u16) -> i32 {
    let i = hi10 as usize;
    if i < 32 {
        PHON_CLASS_FLAG_D[i]
    } else {
        let off = (i - 32) * 4;
        i32::from_le_bytes([
            TONE_CLASS_MAP[off],
            TONE_CLASS_MAP[off + 1],
            TONE_CLASS_MAP[off + 2],
            TONE_CLASS_MAP[off + 3],
        ])
    }
}

fn flag_a(low5: usize) -> i32 {
    PHON_CLASS_FLAG_A[low5 & 0x1f]
}

fn flag_b(low5: usize) -> i32 {
    PHON_CLASS_FLAG_B[low5 & 0x1f]
}

fn flag_c(low5: usize) -> i32 {
    PHON_CLASS_FLAG_C[low5 & 0x1f]
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnitRequest {
    pub cur: u16,
    pub prev: u16,
    pub next: u16,
    pub pitch: u16,
    pub class: u8,
    pub flags: u8,
}

pub use crate::record::ProsodyRecord;

impl UnitRequest {
    fn pitch_signed(&self) -> i32 {
        self.pitch as i16 as i32
    }
}

fn score_left(
    target_prev: u16,
    target_cur: u16,
    entry_prev: u16,
    entry_cur: u16,
    w: u8,
    norm_class: u8,
    search_flag: bool,
) -> i32 {
    let full = |base: i32, off: i32| if search_flag { base + off } else { base };
    let target_level = norm_class as i32 / 10;
    match w {
        2 => {
            let u16_ = entry_prev;
            let u17 = u16_ >> 10;
            if (u17 == 0x1b || u17 == 0x12)
                && (entry_cur & 0x1f == 0x12 || entry_cur & 0x1f == 0xc)
                && target_level < 1
            {
                -200
            } else {
                let u15 = target_prev;
                if u15 == u16_ {
                    full(0x14, 0x50) // 100 / 20
                } else if (u16_ ^ u15) & 0xffe0 == 0 {
                    full(0x14, 0x46) // 90 / 20
                } else if u15 >> 10 == u17 {
                    full(0x14, 0x28) // 60 / 20
                } else if flag_d(u16_ >> 10) != 0 {
                    0x14 // 20
                } else {
                    0
                }
            }
        }
        3 | 5 => {
            let u16_ = target_prev;
            let u17 = entry_prev;
            if u16_ == u17 {
                full(0x3c, 0x28) // 100 / 60
            } else if (u17 ^ u16_) & 0xffe0 == 0 {
                full(0x3c, 0x1e) // 90 / 60
            } else {
                let u15 = u17 >> 10;
                if u15 == u16_ >> 10 {
                    if FALLBACK_REPL_LO[((u16_ & 0x3e0) >> 5) as usize]
                        == FALLBACK_REPL_LO[((u17 & 0x3e0) >> 5) as usize]
                    {
                        full(0x3c, 0x1e) // 90 / 60
                    } else {
                        0x32 // 50
                    }
                } else if u15 == 0x1b || u15 == 0x12 {
                    0x14 // 20
                } else {
                    0
                }
            }
        }
        _ => {
            let u16_ = entry_prev >> 10;
            if (u16_ == 0x1b || u16_ == 0x12)
                && (entry_cur & 0x1f == 0x12 || entry_cur & 0x1f == 0xc)
                && target_level < 1
            {
                -200
            } else {
                let u17 = target_prev;
                if u17 == entry_prev {
                    full(0x14, 0x50) // 100 / 20
                } else if (entry_prev ^ u17) & 0xffe0 == 0 {
                    full(0x14, 0x46) // 90 / 20
                } else if u17 >> 10 == u16_ {
                    full(0x14, 0x32) // 70 / 20
                } else {
                    let i8 = (target_cur & 0x1f) as usize;
                    let e8 = (entry_cur & 0x1f) as usize;
                    if (flag_a(i8) == 0 || flag_a(e8) == 0) && (flag_b(i8) == 0 || flag_b(e8) == 0)
                    {
                        if flag_d(entry_prev >> 10) == 0 || flag_c(e8) == 0 {
                            0x14 // 20
                        } else {
                            0
                        }
                    } else {
                        full(0x14, 0x14) // 40 / 20
                    }
                }
            }
        }
    }
}

fn score_right(
    target_next: u16,
    _target_cur: u16,
    entry_next: u16,
    entry_cur: u16,
    w: u8,
    norm_class: u8,
    search_flag: bool,
) -> i32 {
    let full = |base: i32, off: i32| if search_flag { base + off } else { base };
    let target_tone = norm_class as i32 % 10;
    match w {
        2 => {
            if (entry_cur >> 10 == 0x1b || entry_cur >> 10 == 0x12)
                && (entry_next & 0x1f == 0x12 || entry_next & 0x1f == 0xc)
                && target_tone < 1
            {
                -200
            } else {
                let u16_ = target_next;
                let u17 = entry_next;
                if u16_ == u17 {
                    full(0x14, 0x50) // 100 / 20
                } else if (u17 ^ u16_) & 0x3ff == 0 {
                    full(0x14, 0x46) // 90 / 20
                } else if (u17 & 0x1f) == (u16_ & 0x1f) {
                    full(0x14, 0x28) // 60 / 20
                } else if flag_c((u17 & 0x1f) as usize) != 0 {
                    0x14 // 20
                } else {
                    0
                }
            }
        }
        3 => {
            let u16_ = target_next;
            let u17 = entry_next;
            if u16_ == u17 {
                full(0x3c, 0x28) // 100 / 60
            } else if (u17 ^ u16_) & 0x3ff == 0 {
                full(0x3c, 0x1e) // 90 / 60
            } else {
                let u15 = u17 & 0x1f;
                if u15 == (u16_ & 0x1f) {
                    if FALLBACK_REPL_HI[((u16_ & 0x3e0) >> 5) as usize]
                        == FALLBACK_REPL_HI[((u17 & 0x3e0) >> 5) as usize]
                    {
                        full(0x3c, 0x1e) // 90 / 60
                    } else {
                        0x32 // 50
                    }
                } else if u15 == 0xc {
                    0x14 // 20
                } else {
                    0
                }
            }
        }
        5 => {
            let u16_ = target_next;
            let u17 = entry_next;
            if u16_ == u17 {
                full(0x3c, 0x28) // 100 / 60
            } else if (u17 ^ u16_) & 0x3ff == 0 {
                full(0x3c, 0x1e) // 90 / 60
            } else {
                let u15 = u17 & 0x1f;
                if u15 == (u16_ & 0x1f) {
                    if FALLBACK_REPL_HI[((u16_ & 0x3e0) >> 5) as usize]
                        == FALLBACK_REPL_HI[((u17 & 0x3e0) >> 5) as usize]
                    {
                        full(0x3c, 0x1e) // 90 / 60
                    } else {
                        0x32 // 50
                    }
                } else if u15 == 0x12 {
                    0x14 // 20
                } else {
                    0
                }
            }
        }
        _ => {
            if (entry_cur >> 10 == 0x1b || entry_cur >> 10 == 0x12)
                && (entry_next & 0x1f == 0x12 || entry_next & 0x1f == 0xc)
                && target_tone < 1
            {
                -200
            } else {
                let u16_ = target_next;
                let u17 = entry_next;
                if u16_ == u17 {
                    full(0x14, 0x50) // 100 / 20
                } else if (u17 ^ u16_) & 0x3ff == 0 {
                    full(0x14, 0x46) // 90 / 20
                } else if (u17 & 0x1f) == (u16_ & 0x1f) {
                    full(0x14, 0x32) // 70 / 20
                } else {
                    let i9 = (u16_ & 0x1f) as usize;
                    let e9 = (u17 & 0x1f) as usize;
                    if (flag_a(i9) == 0 || flag_a(e9) == 0) && (flag_b(i9) == 0 || flag_b(e9) == 0)
                    {
                        if flag_d(entry_cur >> 10) == 0 || flag_c(e9) == 0 {
                            0x14 // 20
                        } else {
                            0
                        }
                    } else {
                        full(0x14, 0x14) // 40 / 20
                    }
                }
            }
        }
    }
}

pub fn duration_for(class: u8, values: &[u16; 4], enabled: &[bool; 4]) -> u16 {
    match class % 10 {
        1 => {
            if enabled[0] {
                values[0]
            } else {
                0
            }
        }
        2 => {
            if enabled[1] {
                values[1]
            } else {
                0
            }
        }
        3 | 5 => {
            if enabled[2] {
                values[2]
            } else {
                0
            }
        }
        4 => {
            if enabled[3] {
                values[3]
            } else {
                0
            }
        }
        _ => 0,
    }
}

pub const BOUNDARY_CODE: u16 = 0x6EB3;

#[derive(Clone, Copy, Debug)]
pub struct UnitSelectConfig {
    pub pitch_tolerance: i32,
    pub request_pitch_default: u16,
    /// FUN_0044b2a0 pause table (engine +0xc8/+0xcc/+0xd0/+0xd4).
    pub pause_values: [i32; 4],
    /// Pause enable flags (engine +0xb8/+0xbc/+0xc0/+0xc4). Ctor: [false,true,true,true].
    pub pause_enabled: [bool; 4],
    pub duration_values: [u16; 4],
    pub duration_enabled: [bool; 4],
    pub special_dist_init: i32,
}

impl Default for UnitSelectConfig {
    fn default() -> Self {
        UnitSelectConfig {
            pitch_tolerance: 15,
            request_pitch_default: 90,
            pause_values: [1000, 3000, 5000, 20000],
            pause_enabled: [false, true, true, true],
            duration_values: [1000, 3000, 5000, 20000],
            duration_enabled: [false, true, true, true],
            special_dist_init: 200,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitSelection {
    pub request: UnitRequest,
    pub data: VoiceInfoEntry,
    pub marker: i32,
    pub data2: Option<VoiceInfoEntry>,
    pub marker2: Option<i32>,
    pub extra: Option<VoiceInfoEntry>,
}

impl UnitSelection {
    pub fn active_data(&self) -> VoiceInfoEntry {
        match self.data2 {
            Some(d2) if self.marker2.map_or(false, |m| m % 10000 != 0) => d2,
            _ => self.data,
        }
    }

    pub fn pause(&self) -> i32 {
        self.active_data().pause as i32
    }
}

#[derive(Clone, Debug)]
pub struct ProcessedUnits {
    pub units: Vec<UnitSelection>,
    pub total_samples: i64,
}

pub struct UnitSelector<'a> {
    info: &'a VoiceInfo,
    cfg: UnitSelectConfig,
    prev_tone: u8,
    prev_best_pitch: u16,
    prev_best_class_hi: u8,
}

impl<'a> UnitSelector<'a> {
    pub fn new(info: &'a VoiceInfo, cfg: UnitSelectConfig) -> Self {
        UnitSelector {
            info,
            cfg,
            prev_tone: 4,
            prev_best_pitch: 0,
            prev_best_class_hi: 0,
        }
    }

    pub fn config(&self) -> &UnitSelectConfig {
        &self.cfg
    }

    pub fn scan(&self, req: &UnitRequest, search_flag: bool) -> Option<(VoiceInfoEntry, i32)> {
        let norm_class = normalize_target_class(req.class);
        let target_level = norm_class as i32 / 10;
        let tone_row = tone_class_row(norm_class);
        let w_prev = weight_prev(req, norm_class);
        let w_next = weight_next(req, norm_class);
        let wsum = (w_prev + w_next) as i32;

        let mut best_score: i32 = 0;
        let mut best_pitch_dist: i32 = i32::MAX;
        let mut best: Option<VoiceInfoEntry> = None;

        for e in &self.info.entries {
            if !e.is_normal() || e.phone_cur != req.cur {
                continue;
            }
            let cand_norm = normalize_candidate_class_a(e.class_byte());
            let cand_idx = tone_class_row(cand_norm).min(15);
            let f = &FILTER_TABLE[cand_idx];
            let pitch = e.pitch_signed();
            let wlen = e.wlen as i32;
            let pass = if (req.flags & 0x80) != 0 && target_level <= 1 {
                f[0] <= pitch && f[2] <= wlen && wlen <= f[3]
            } else {
                f[0] <= pitch && pitch <= f[1] && f[2] <= wlen && wlen <= f[3]
            };
            if !pass {
                continue;
            }
            let i18 = score_left(
                req.prev,
                req.cur,
                e.phone_prev,
                e.phone_cur,
                w_prev,
                norm_class,
                search_flag,
            );
            let i12 = score_right(
                req.next,
                req.cur,
                e.phone_next,
                e.phone_cur,
                w_next,
                norm_class,
                search_flag,
            );
            let cand_norm_b = normalize_candidate_class_b(e.class_byte());
            let col = tone_class_col(tone_row, cand_norm_b);
            let cost = TONE_TRANS_COST[tone_row * 16 + col];
            //   (w_next×i12)/sum + (w_prev×i18)/sum + 100 + cost
            let s = (w_next as i32 * i12) / wsum + 100 + (w_prev as i32 * i18) / wsum + cost;
            if std::env::var("MIRAE_SCAN_DEBUG").is_ok() {
                eprintln!(
                    "[scan-debug] woff={} i18={} i12={} cost={} s={} pitch={} wlen={}",
                    e.woff, i18, i12, cost, s, pitch, wlen
                );
            }
            if std::env::var("MIRAE_SCAN_DEBUG").is_ok() {
                eprintln!(
                    "[scan] req=({:04x},{:04x},{:04x}) cls={:02x} pitch={} wp={} wn={} row={} | cand woff={} eprev={:04x} enext={:04x} epitch={} ewlen={} ecls={:02x} i18={} i12={} cost={} score={}",
                    req.prev,
                    req.cur,
                    req.next,
                    req.class,
                    req.pitch,
                    w_prev,
                    w_next,
                    tone_row,
                    e.woff,
                    e.phone_prev,
                    e.phone_next,
                    pitch,
                    wlen,
                    e.classcode & 0xff,
                    i18,
                    i12,
                    cost,
                    s
                );
            }
            let pd = (req.pitch_signed() - pitch).abs();
            if s > best_score || (s == best_score && pd < best_pitch_dist) {
                best_score = s;
                best_pitch_dist = pd;
                best = Some(*e);
            }
        }

        best.map(|e| (e, best_score))
    }

    pub fn scan_special(&self, target_pitch: i16) -> Option<VoiceInfoEntry> {
        let mut best_dist: i32 = self.cfg.special_dist_init;
        let mut best: Option<VoiceInfoEntry> = None;
        for e in &self.info.entries {
            if !e.is_special() {
                continue;
            }
            let d = (target_pitch as i32 - e.pitch_signed()).abs();
            if d < best_dist {
                best_dist = d;
                best = Some(*e);
            }
        }
        best
    }

    pub fn process(&mut self, records: &[ProsodyRecord]) -> ProcessedUnits {
        let mut units: Vec<UnitSelection> = Vec::new();

        for (idx, rec) in records.iter().enumerate() {
            if idx > 0 && records[idx - 1].tone_class % 10 >= 3 {
                self.prev_tone = 4;
                self.prev_best_class_hi = 0;
            }
            let mut class = rec.tone_class;
            let pt = (self.prev_tone % 10) as i32;
            if pt > 0 && (pt as u8) < class / 10 {
                class = class % 10 + (pt as u8) * 10;
            }
            let level = class / 10;
            let tone = class % 10;

            let mut req = UnitRequest {
                cur: rec.code,
                prev: if idx != 0 && level < 2 {
                    records[idx - 1].code
                } else {
                    BOUNDARY_CODE
                },
                next: if idx + 1 < records.len() && rec.marker != MARKER_SENTENCE_END && tone <= 1 {
                    records[idx + 1].code
                } else {
                    BOUNDARY_CODE
                },
                pitch: if level >= 2 {
                    self.cfg.request_pitch_default
                } else {
                    self.prev_best_pitch
                },
                class,
                flags: 0,
            };

            let mut flags: u8 = if rec.marker == 2 {
                (self.prev_best_class_hi % 10) * 10
            } else if level < 1 && is_pause(req.prev >> 10, req.cur & 0x1f) {
                10
            } else {
                0
            };
            if tone < 1 && is_pause(req.cur >> 10, req.next & 0x1f) {
                flags += 1;
            }
            if rec.flags == 1 {
                flags |= 0x80;
            }
            req.flags = flags;

            if std::env::var("MIRAE_SCAN_DEBUG").is_ok() {
                eprintln!(
                    "[process-scan] idx={} rec=({:04x},{:04x},{:04x}) reccls={:02x} sandhi_class={:02x} prev_tone={} reqcls={:02x}",
                    idx,
                    rec.prev_code,
                    rec.code,
                    rec.code,
                    rec.tone_class,
                    class,
                    self.prev_tone,
                    req.class
                );
            }
            let mut hit = self.scan(&req, true);
            if std::env::var("MIRAE_SCAN_DEBUG").is_ok() {
                eprintln!("[process-hit] idx={} hit={}", idx, hit.is_some());
            }
            let mut marker_base: i32 = 0;

            if hit.is_none() {
                let mid5 = ((req.cur >> 5) & 0x1f) as usize;
                if FALLBACK_ALLOW[mid5] == 0 {
                    let repl = (FALLBACK_REPL_LO[mid5] as u16) & 0x1f;
                    let mut req_a = req;
                    req_a.cur = (req_a.cur & 0xfc1f) | (repl << 5);
                    hit = self.scan(&req_a, true);
                    if hit.is_some() {
                        req = req_a;
                        marker_base = 10000;
                    } else {
                        let mid5b = ((req_a.cur >> 5) & 0x1f) as usize;
                        if FALLBACK_ALLOW[mid5b] == 0 || (req_a.cur & 0xfc00) != 0x6c00 {
                            let copy = req_a;
                            let b = (FALLBACK_REPL_HI[mid5b] as u16) & 0x1f;
                            let cur_hi = (((b | 0x360) << 5) | (req_a.cur & 0x1f)) as u16;
                            let lo = (((FALLBACK_REPL_LO[((copy.cur >> 5) & 0x1f) as usize]
                                as u16)
                                & 0x1f)
                                << 5)
                                | (copy.cur & 0xfc12);
                            let mut req_b1 = req_a;
                            req_b1.cur = cur_hi;
                            req_b1.next = lo | 0x12;
                            req_b1.pitch = 0;
                            req_b1.class = (copy.class / 10) * 10;
                            req_b1.flags = (copy.flags / 10) * 10 + 1;
                            hit = self.scan(&req_b1, true);
                            if hit.is_some() {
                                req = req_b1;
                                marker_base = 20000;
                            } else {
                                let mut req_b2 = req_b1;
                                req_b2.cur = lo | 0x12;
                                req_b2.prev = cur_hi;
                                req_b2.next = copy.next;
                                req_b2.class = (self.prev_tone % 10) * 10 + (copy.class % 10);
                                req_b2.flags = (copy.flags % 10) + 10;
                                hit = self.scan(&req_b2, true);
                                if hit.is_some() {
                                    req = req_b2;
                                    marker_base = 20000;
                                }
                            }
                        }
                    }
                }
            }

            if let Some((entry, score)) = hit {
                self.prev_tone = entry.class_byte() % 10;
                self.prev_best_pitch = entry.pitch;
                self.prev_best_class_hi = entry.class_hi_byte();
                units.push(UnitSelection {
                    request: req,
                    data: entry,
                    marker: score + marker_base,
                    data2: None,
                    marker2: None,
                    extra: None,
                });
            }
        }

        if units.len() > 2 {
            let tol = self.cfg.pitch_tolerance;
            for i in 0..units.len() - 2 {
                let prev_d = units[i].active_data();
                let mid_d = units[i + 1].active_data();
                let next_d = units[i + 2].active_data();
                let mid_req_flags = units[i + 1].request.flags;
                let bvar = (mid_req_flags & 0x80) != 0;
                let mid_class = mid_d.class_byte();
                let mp = mid_d.pitch_signed();
                let pp = prev_d.pitch_signed();
                let np = next_d.pitch_signed();

                let do_replace = |sel: &mut UnitSelector, req: &mut UnitRequest, avg: u16| {
                    req.pitch = avg;
                    if let Some((e2, s2)) = sel.scan(req, false) {
                        return Some((e2, s2 + 30000));
                    }
                    None
                };

                if !bvar && mid_class / 10 < 2 && mid_class % 10 < 2 {
                    if (mp - pp).abs() >= tol
                        && (np - mp).abs() >= tol
                        && (2 * mp - np - pp).abs() >= tol
                    {
                        let avg = ((np + pp) / 2) as u16;
                        let mut r = units[i + 1].request;
                        if let Some((e2, m2)) = do_replace(self, &mut r, avg) {
                            units[i + 1].data2 = Some(e2);
                            units[i + 1].marker2 = Some(m2);
                        }
                    }
                } else if bvar && mid_class / 10 < 2 && mid_class % 10 < 2 {
                    if (mp + 10 < pp) || (tol < mp - pp) {
                        let avg = ((np + pp) / 2) as u16;
                        let mut r = units[i + 1].request;
                        if let Some((e2, m2)) = do_replace(self, &mut r, avg) {
                            units[i + 1].data2 = Some(e2);
                            units[i + 1].marker2 = Some(m2);
                        }
                    }
                }
            }
        }

        for u in &mut units {
            let d = duration_for(
                u.request.class,
                &self.cfg.duration_values,
                &self.cfg.duration_enabled,
            );
            set_active_pause(u, d as i16);
        }

        let mut total: i64 = 0;
        let n = units.len();
        // Snapshot class bytes for FUN_0044b2a0 min-chain (avoids borrow conflict)
        let class_bytes: Vec<u8> = units.iter().map(|u| u.active_data().class_byte()).collect();
        let req_classes: Vec<u8> = units.iter().map(|u| u.request.class).collect();
        for (i, u) in units.iter_mut().enumerate() {
            let req = u.request;
            let d = u.active_data();
            let cur_hi = req.cur >> 10;
            let next_lo = req.next & 0x1f;
            total += d.wlen as i64 * 2;

            if is_real_phoneme(cur_hi, next_lo) && d.class_byte() % 10 < 2 {
                if let Some(extra) = self.scan_special(d.pitch as i16) {
                    u.extra = Some(extra);
                    total += extra.wlen as i64 * 2;
                }
            }

            // FUN_0044b2a0 pause (verified by disassembly 0x4bfb0-0x4c057):
            // pause_index = min-chain of class digits across prev/cur/next selected units,
            // then pause = PAUSE_VALUES[index] gated by enable flags.
            // Engine ctor defaults: values [_,1000,3000,5000,20000] with enables
            // [+0xb8=0(off), +0xbc=1, +0xc0=1, +0xc4=1]; VoiceInfoEntry.pause is never read.
            // cl = cur.class(i8)/10   (movsx → signed)
            let cur_cb = class_bytes[i];
            let mut idx_i8 = (cur_cb as i8) / 10;
            // mid entry class %10 (signed byte mod)
            let mid_lo = (cur_cb as i8) % 10;
            if mid_lo < idx_i8 {
                idx_i8 = mid_lo;
            }
            // other (+4 link) class /10 — request class hi of this unit
            let other_hi = ((req_classes[i] / 10) as i8).min(127);
            if idx_i8 > other_hi {
                idx_i8 = other_hi;
            }
            // next entry class %10 == 2 → cl = 2
            if i + 1 < n {
                let nlo = (class_bytes[i + 1] as i8) % 10;
                if nlo == 2 {
                    idx_i8 = 2;
                }
            }
            // next2 class %10 == 2 → cl = 2 ; prev class /10 clamp
            if i > 0 {
                let p_lo = (class_bytes[i - 1] as i8) % 10;
                if p_lo == 2 {
                    idx_i8 = 2;
                }
                let p_hi = (class_bytes[i - 1] as i8) / 10;
                if idx_i8 > p_hi {
                    idx_i8 = p_hi;
                }
            }
            // FUN_0044b2a0: arg = class%10, cases for (arg-1) in 0..4
            let arg = (idx_i8.max(0) as usize) % 10;
            let pause_values = self.cfg.pause_values;
            let pause_enabled = self.cfg.pause_enabled;
            let pause = if arg >= 1 && arg <= 4 && pause_enabled[arg - 1] {
                pause_values[arg - 1]
            } else {
                0i32
            };
            set_active_pause(u, pause as i16);
            total += pause as i64 * 2;
        }

        ProcessedUnits {
            units,
            total_samples: total,
        }
    }
}

fn set_active_pause(u: &mut UnitSelection, pause: i16) {
    if u.data2.is_some() {
        if let Some(d2) = u.data2.as_mut() {
            d2.pause = pause;
        }
    } else {
        u.data.pause = pause;
    }
}

fn weight_prev(req: &UnitRequest, norm_class: u8) -> u8 {
    let prev_hi = req.prev >> 10;
    let cur_lo = (req.cur & 0x1f) as usize;
    let level = norm_class as i32 / 10;
    if (prev_hi == 0x1b || prev_hi == 0x12) && cur_lo == 0x12 && level < 2 {
        return 5;
    }
    if (prev_hi == 0x1b || prev_hi == 0x12) && cur_lo == 0xc && level < 1 {
        return 3;
    }
    if flag_d(prev_hi) == 0 || flag_c(cur_lo) == 0 {
        return 1;
    }
    2
}

fn weight_next(req: &UnitRequest, norm_class: u8) -> u8 {
    let cur_hi = req.cur >> 10;
    let next_lo = (req.next & 0x1f) as usize;
    let tone = norm_class as i32 % 10;
    if (cur_hi == 0x1b || cur_hi == 0x12) && next_lo == 0x12 && tone < 2 {
        return 5;
    }
    if (cur_hi == 0x1b || cur_hi == 0x12) && next_lo == 0xc && tone < 1 {
        return 3;
    }
    if flag_d(cur_hi) == 0 || flag_c(next_lo) == 0 {
        return 1;
    }
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(cur: u16, prev: u16, next: u16, pitch: u16, class: u8, flags: u8) -> UnitRequest {
        UnitRequest {
            cur,
            prev,
            next,
            pitch,
            class,
            flags,
        }
    }

    #[test]
    fn pause_detection_matches_spec() {
        assert!(is_pause(2, 1));
        assert!(is_pause(0xe, 4));
        assert!(is_pause(0x12, 0x12));
        assert!(is_pause(0x1b, 0x12)); // 0x6c12
        assert!(is_pause(6, 3));
        assert!(is_pause(6, 0x12));
        assert!(!is_pause(2, 2));
        assert!(!is_pause(6, 1));
        assert!(!is_pause(0, 0));
    }

    #[test]
    fn real_phoneme_detection() {
        assert!(!is_real_phoneme(0, 1));
        assert!(!is_real_phoneme(6, 3));
        assert!(!is_real_phoneme(27, 0x12));
        assert!(!is_real_phoneme(5, 8));
        assert!(!is_real_phoneme(0, 0x10));
        assert!(!is_real_phoneme(6, 1));
        assert!(!is_real_phoneme(27, 6));
        assert!(!is_real_phoneme(0, 0x11));
        assert!(is_real_phoneme(27, 5));
        assert!(is_real_phoneme(6, 2));
    }

    #[test]
    fn class_normalization() {
        assert_eq!(normalize_target_class(0x14), 30); // 20 → 30
        assert_eq!(normalize_target_class(0x15), 31); // 21 → 31
        assert_eq!(normalize_target_class(0x02), 3); // 2 → 3
        assert_eq!(normalize_candidate_class_a(0x02), 1); // 2 → 1
        assert_eq!(normalize_candidate_class_b(0x02), 3);
        assert_eq!(normalize_target_class(0x05), 4);
        assert_eq!(normalize_candidate_class_a(0x05), 5);
        assert_eq!(normalize_target_class(0x28), 40);
        assert_eq!(normalize_target_class(0x01), 1);
        assert_eq!(normalize_target_class(0x0a), 10);
        assert_eq!(tone_class_row(40), 0);
        assert_eq!(tone_class_row(4), 1);
        assert_eq!(tone_class_row(30), 2);
        assert_eq!(tone_class_row(3), 3);
        assert_eq!(tone_class_row(10), 4);
        assert_eq!(tone_class_row(1), 5);
        assert_eq!(tone_class_row(41), 6);
        assert_eq!(tone_class_row(14), 7);
        assert_eq!(tone_class_row(31), 8);
        assert_eq!(tone_class_row(13), 9);
        assert_eq!(tone_class_row(11), 10);
        assert_eq!(tone_class_row(33), 11);
        assert_eq!(tone_class_row(34), 12);
        assert_eq!(tone_class_row(43), 13);
        assert_eq!(tone_class_row(44), 14);
        assert_eq!(tone_class_row(0), 15);
    }

    #[test]
    fn tone_cost_indexing() {
        assert_eq!(tone_class_col(0, 30), 1);
        assert_eq!(TONE_TRANS_COST[1], 595);
        assert_eq!(tone_class_col(15, 25), 15);
    }

    #[test]
    fn duration_mapping() {
        let cfg = UnitSelectConfig::default();
        // default enabled = [false,true,true,true] (original ctor): class 1 disabled
        for (class, expect) in [
            (1u8, 0u16),
            (11, 0),
            (2, 3000),
            (3, 5000),
            (5, 5000),
            (4, 20000),
            (0, 0),
            (6, 0),
        ] {
            assert_eq!(
                duration_for(class, &cfg.duration_values, &cfg.duration_enabled),
                expect,
                "class {}",
                class
            );
        }
        // All-enabled config: class 1 -> 1000
        let all = [true, true, true, true];
        assert_eq!(duration_for(1, &cfg.duration_values, &all), 1000);
        assert_eq!(duration_for(11, &cfg.duration_values, &all), 1000);
        let orig = [false, true, true, true];
        assert_eq!(
            duration_for(1, &cfg.duration_values, &orig),
            0,
            "original enable flags disable class 1"
        );
    }

    #[test]
    fn flag_d_overflow_reads_tone_map() {
        let expect = i32::from_le_bytes([
            TONE_CLASS_MAP[0],
            TONE_CLASS_MAP[1],
            TONE_CLASS_MAP[2],
            TONE_CLASS_MAP[3],
        ]);
        assert_eq!(flag_d(32), expect);
        assert_eq!(flag_d(27), PHON_CLASS_FLAG_D[27]);
        assert_eq!(flag_d(0), PHON_CLASS_FLAG_D[0]);
    }

    #[test]
    fn weight_computation() {
        let r = req(0x6c12, 0x6c12, 0, 90, 0x03, 0);
        assert_eq!(weight_prev(&r, normalize_target_class(r.class)), 5);
        // 0x4801: hi10=18 (FLAG_D[18]=1), lo=1 (FLAG_C[1]=1)
        let r2 = req(0x4801, 0x4801, 0x6d81, 90, 0x28, 0);
        assert_eq!(weight_next(&r2, normalize_target_class(r2.class)), 2);
        assert_eq!(weight_prev(&r2, normalize_target_class(r2.class)), 2);
        let r3 = req(0x6d86, 0x6eb3, 0x6d81, 90, 0x28, 0);
        assert_eq!(weight_next(&r3, normalize_target_class(r3.class)), 1);
    }

    #[test]
    fn context_score_values() {
        assert_eq!(score_left(0x6eb3, 0x6d86, 0x6eb3, 0x6d86, 2, 40, true), 100);
        // search_flag=false → 20
        assert_eq!(score_left(0x6eb3, 0x6d86, 0x6eb3, 0x6d86, 2, 40, false), 20);
        assert_eq!(score_left(0x6eb3, 0x6d86, 0x6eb2, 0x6d86, 2, 40, true), 90);
        assert_eq!(
            score_left(0x6c12, 0x6c12, 0x6c12, 0x6c12, 2, 0x03, true),
            -200
        );
        assert_eq!(score_left(0x6eb3, 0x6d86, 0x6eb3, 0x6d86, 3, 40, true), 100);
        assert_eq!(score_left(0x6eb3, 0x6d86, 0x6eb3, 0x6d86, 3, 40, false), 60);
        assert_eq!(
            score_right(0x6d80, 0x6d86, 0x6d80, 0x6d86, 2, 40, true),
            100
        );
        assert_eq!(score_right(0x6d80, 0x6d86, 0x4d80, 0x6d86, 2, 40, true), 90);
        assert_eq!(score_right(0x6d80, 0x6d86, 0x6c80, 0x6d86, 2, 40, true), 60);
        assert_eq!(score_right(0x6d80, 0x6d86, 0x6c80, 0x6d86, 3, 40, true), 50);
    }
}
