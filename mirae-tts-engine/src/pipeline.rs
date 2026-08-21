//! Internal synthesis pipeline ([`Mirae2Engine`]): text -> keypad -> segmenter
//! -> g2p -> tone -> unit_select -> render. The public wrapper lives in
//! [`crate::synthesizer`].
use std::io;
use std::path::Path;

use crate::connect::ConnectMatrix;
use crate::dict::Dict;
use crate::g2p::g2p_dict::{self, G2pDicts, WordFinalTone};
use crate::keypad::KeyPad;
use crate::record::ProsodyRecord;
use crate::segmenter::{KPS_FULL_STOP, Sentence, next_token_class};
use crate::synthesizer::TtsConfig;
use crate::tone;
use crate::unit_select::{ProcessedUnits, UnitSelectConfig, UnitSelector};
use crate::voice_data::VoiceData;
use crate::voice_info::{VoiceInfo, VoiceInfoEntry};

/// Voice data directory (default: relative to CWD, like the original app).
pub const DEFAULT_VOICE_DIR: &str = "Voice";
/// Environment variable for voice directory (`MIRAE_VOICE_DIR`).
pub const VOICE_DIR_ENV: &str = "MIRAE_VOICE_DIR";

/// Resolve effective voice dir: `MIRAE_VOICE_DIR` env > `DEFAULT_VOICE_DIR`.
pub fn default_voice_dir() -> std::path::PathBuf {
    if let Ok(v) = std::env::var(VOICE_DIR_ENV) {
        return std::path::PathBuf::from(v);
    }
    std::path::PathBuf::from(DEFAULT_VOICE_DIR)
}

pub fn truncate_last_line_char(text: &str) -> &str {
    // Strip trailing newlines only. The original FUN_0042bd90 driver also drops
    // the final character, but that never affects user-visible text in the GUI
    // (paragraph-terminated input); here it would lose real speech content.
    text.trim_end_matches(['\n', '\r'])
}

pub(crate) fn kps_decode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len());
    kps9566::kps9566::Decoder::new().decode_to_string(bytes, &mut s, true);
    s
}

/// KPS code → first char (used by the kept jamo-map verification helpers in `g2p`).
pub(crate) fn kps_lookup(code: u16) -> Option<char> {
    kps9566::kps9566::decode(&[(code >> 8) as u8, code as u8])
        .ok()?
        .chars()
        .next()
}

/// Internal engine configuration (subset of the original engine fields).
#[derive(Debug, Clone)]
pub(crate) struct EngineConfig {
    /// engine+0xdc: random (expression) mode (original 0).
    pub(crate) random_mode: bool,
    /// speed (original DAT_004a2ff8 = 50 → 22050 Hz); derived from
    /// `sample_rate` and retained until speed control reaches parity.
    #[allow(dead_code)]
    pub(crate) speed: u32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            random_mode: false,
            speed: 50,
        }
    }
}

impl EngineConfig {
    pub(crate) fn from_public(cfg: &TtsConfig) -> Self {
        EngineConfig {
            random_mode: false,
            // sample_rate = speed × 441 (original WAVEFORMATEX relation).
            speed: (cfg.sample_rate / 441).max(1),
        }
    }
}

/// Mark a record as clause-final: tone_class ones digit := 4.
fn mark_clause_end(rec: &mut ProsodyRecord) {
    rec.tone_class = (rec.tone_class / 10) * 10 + 4;
}

/// A group of prosody records plus the end offsets of its words.
type RecordGroup = (Vec<ProsodyRecord>, Vec<usize>);

/// Every [`g2p_dict::CHUNK_SYLLABLES`] records across word boundaries gets
/// tone_class 3 (chunk-final break).
fn apply_chunk_tone_breaks(groups: &mut [RecordGroup]) {
    for (recs, word_ends) in groups.iter_mut() {
        let mut cum = 0usize;
        let mut prev = 0usize;
        for &end in word_ends.iter() {
            cum += end - prev;
            if cum >= g2p_dict::CHUNK_SYLLABLES {
                if let Some(last) = recs.get_mut(end - 1) {
                    last.tone_class = 3;
                }
                cum = 0;
            }
            prev = end;
        }
    }
}

/// Last [`g2p_dict::PROPAGATE_BACK`] syllables of a group get flags=1.
fn propagate_flags_back(groups: &mut [RecordGroup]) {
    for (recs, word_ends) in groups.iter_mut() {
        let mut acc = 0usize;
        for w in (0..word_ends.len()).rev() {
            let end = word_ends[w];
            let start = if w == 0 { 0 } else { word_ends[w - 1] };
            if acc >= g2p_dict::PROPAGATE_BACK {
                break;
            }
            acc += end - start;
            for rec in recs[start..end].iter_mut() {
                rec.flags = 1;
            }
        }
    }
}

pub(crate) struct Mirae2Engine {
    keypad: KeyPad,
    voice_info: VoiceInfo,
    voice_data: VoiceData,
    colligation: Dict,
    user: Dict,
    nonreg: Dict,
    conjects: Dict,
    connect: ConnectMatrix,
    cfg: EngineConfig,
    /// Verbose pipeline debug output (from `TtsConfig::log_progress` or `MIRAE_DEBUG`).
    debug_log: bool,
}

impl Mirae2Engine {
    /// Load all voice data from the voice directory plus the KeyPad.Ebd table.
    pub(crate) fn from_paths(voice: &Path, keypad_ebd: Option<&Path>) -> io::Result<Self> {
        let keypad = match keypad_ebd {
            Some(p) => KeyPad::load(p)?,
            None => KeyPad::fallback(),
        };
        let voice_info = VoiceInfo::load(voice.join("VoiceInfo.pkg"))?;
        let voice_data = VoiceData::open(voice)?;

        let dict_load = |name: &str| -> io::Result<Dict> {
            Dict::load(voice.join(name))
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{name}: {e}")))
        };
        let colligation = dict_load("colligation.pkg")?;
        let user = dict_load("User.pkg")?;
        let nonreg = dict_load("NonReg.pkg")?;
        let conjects = dict_load("Conjects.pkg")?;
        let connect = ConnectMatrix::load(voice.join("Connect.pkg"))?;

        Ok(Mirae2Engine {
            keypad,
            voice_info,
            voice_data,
            colligation,
            user,
            nonreg,
            conjects,
            connect,
            cfg: EngineConfig::default(),
            debug_log: false,
        })
    }

    pub(crate) fn set_config(&mut self, cfg: EngineConfig) {
        self.cfg = cfg;
    }

    /// Toggle verbose pipeline debug output.
    pub(crate) fn set_debug_log(&mut self, enabled: bool) {
        self.debug_log = enabled;
    }

    /// VoiceInfo entry count (70,150 for the shipped data).
    pub(crate) fn voice_entry_count(&self) -> usize {
        self.voice_info.entries.len()
    }

    /// Synthesize `text` to mono PCM samples (22050 Hz, s16le).
    pub(crate) fn synthesize(&mut self, text: &str) -> io::Result<Vec<i16>> {
        let pcm_bytes = self.synthesize_bytes(text)?;
        let mut out = Vec::with_capacity(pcm_bytes.len() / 2);
        for ch in pcm_bytes.chunks_exact(2) {
            out.push(i16::from_le_bytes([ch[0], ch[1]]));
        }
        Ok(out)
    }

    pub(crate) fn synthesize_bytes(&mut self, text: &str) -> io::Result<Vec<u8>> {
        let text = truncate_last_line_char(text);

        // 1. UTF-16 → internal code bytes (KeyPad.Ebd)
        let internal = self.keypad.convert_str(text);

        // 2. Sentence split (FUN_00402240)
        let sentences = crate::segmenter::tokenize(&internal);
        if sentences.is_empty() {
            return Ok(Vec::new());
        }

        // 3. Per-sentence G2P → prosody records
        let mut all_records: Vec<ProsodyRecord> = Vec::new();
        for sent in &sentences {
            let groups = self.sentence_to_records(sent)?;
            if groups.is_empty() {
                continue;
            }
            for mut buf in groups {
                // apply_sandhi appends the (sandhi-adjusted) sentence to `all_records`.
                tone::apply_sandhi(&mut all_records, &mut buf);
            }
        }
        if all_records.is_empty() {
            return Ok(Vec::new());
        }

        // 4. Unit selection (FUN_0044b880 + FUN_0044a800); `unit_select`
        // re-exports the same ProsodyRecord type, no conversion needed.
        let mut sel = UnitSelector::new(&self.voice_info, UnitSelectConfig::default());
        let processed: ProcessedUnits = sel.process(&all_records);

        if self.debug_log {
            log_units_debug(&all_records, &processed);
        }

        // 5. Waveform render (FUN_0044c2e0 + FUN_0044b700)
        let to_unit_rec = |e: VoiceInfoEntry| crate::render::UnitRecord {
            woff: e.woff,
            wlen: e.wlen,
            pitch: e.pitch as i16,
            classcode: (e.classcode & 0xff) as u8,
            pause: e.pause,
        };
        let units: Vec<crate::render::RenderUnit> = processed
            .units
            .iter()
            .map(|u| crate::render::RenderUnit {
                record: to_unit_rec(u.active_data()),
                code_cur: u.request.cur,
                code_next: u.request.next,
                extra: u.extra.map(to_unit_rec),
            })
            .collect();
        let mut out = Vec::new();
        crate::render::render_units(&mut self.voice_data, &units, &mut out, self.cfg.random_mode)?;
        if self.debug_log {
            eprintln!(
                "[mirae2-tts] render out.len={} units={}",
                out.len(),
                units.len()
            );
        }
        Ok(out)
    }

    fn sentence_to_records(&self, sent: &Sentence) -> io::Result<Vec<Vec<ProsodyRecord>>> {
        let dicts = G2pDicts {
            colligation: &self.colligation,
            user: &self.user,
            nonreg: &self.nonreg,
            conjects: &self.conjects,
            connect: &self.connect,
        };

        // Split the sentence into word tokens (whitespace/punct delimited),
        // run the G2P word pipeline for each, concatenate readings.
        let mut groups: Vec<RecordGroup> = vec![(Vec::new(), Vec::new())];
        let bytes = &sent.text;
        let mut pos = 0usize;
        while pos < bytes.len() {
            let (class, len) = next_token_class(&bytes[pos..]);
            if class == 0 {
                let b0 = bytes[pos];
                if b0 == b'\n' || b0 == b'\r' {
                    if let Some(last) = groups.last_mut().and_then(|g| g.0.last_mut()) {
                        mark_clause_end(last);
                    }
                    if !groups.last().is_none_or(|g| g.0.is_empty()) {
                        groups.push((Vec::new(), Vec::new()));
                    }
                }
                pos += if b0 < 0x80 { 1 } else { 2 };
                continue;
            }
            if class <= 3 {
                let b0 = bytes[pos];
                let is_period = b0 == b'.'
                    || (b0 >= 0xA1
                        && len == 2
                        && ((b0 as u16) << 8 | bytes[pos + 1] as u16) == KPS_FULL_STOP);
                if is_period && !groups.last().is_none_or(|g| g.0.is_empty()) {
                    let is_decimal_point = b0 == b'.' && {
                        let (nc, _) = next_token_class(&bytes[pos + len..]);
                        nc == 4
                    };
                    if !is_decimal_point {
                        let last = groups.last_mut().unwrap().0.last_mut().unwrap();
                        mark_clause_end(last);
                    }
                }
                pos += len;
                continue;
            }
            if class == 4 {
                let (codes, frac_end, is_merged) = self.number_token_codes(bytes, pos, &dicts);
                let n_codes = codes.len();
                for (i, code) in codes.into_iter().enumerate() {
                    let mut rec = ProsodyRecord::new(code);
                    rec.init_from_marker(if is_merged && i + 1 == n_codes { 1 } else { 0 }, false);
                    groups.last_mut().unwrap().0.push(rec);
                }
                if let Some(g) = groups.last_mut() {
                    let (nc, _nl) = next_token_class(&bytes[frac_end..]);
                    if is_merged || nc != 0x19 {
                        g.1.push(g.0.len());
                    }
                }
                pos = frac_end;
                continue;
            }
            let start = pos;
            while pos < bytes.len() {
                let (c, l) = next_token_class(&bytes[pos..]);
                if c == 0 || (c != class) {
                    break;
                }
                pos += l;
            }
            let token = &bytes[start..pos];
            if token.is_empty() {
                continue;
            }
            let final_tone = self.next_word_final_tone(bytes, pos);
            let word_records = self.word_to_records(&dicts, token, final_tone);
            let g = groups.last_mut().unwrap();
            g.0.extend(word_records);
            g.1.push(g.0.len());
        }
        apply_chunk_tone_breaks(&mut groups);
        propagate_flags_back(&mut groups);
        if let Some(last) = groups.last_mut().unwrap().0.last_mut() {
            mark_clause_end(last);
        }
        groups.retain(|g| !g.0.is_empty());
        if self.debug_log {
            for (gi, g) in groups.iter().enumerate() {
                let codes: Vec<String> = g.0.iter().map(|r| format!("{:04x}", r.code)).collect();
                let flags: Vec<u8> = g.0.iter().map(|r| r.flags).collect();
                eprintln!(
                    "[tts-debug] group {gi}: n={} codes={} flags={:02x?}",
                    g.0.len(),
                    codes.join(","),
                    flags
                );
            }
        }
        Ok(groups.into_iter().map(|(recs, _)| recs).collect())
    }

    /// Digit token (token class 4): `[0-9]+` with optional `.fraction`,
    /// plus the original counter-word merge (integer reading ending in a
    /// classifier final followed by a word token). Returns the phoneme
    /// codes, the offset just past the consumed input, and whether the
    /// counter-word merge fired (its last syllable carries marker 1).
    fn number_token_codes(
        &self,
        bytes: &[u8],
        start: usize,
        dicts: &G2pDicts,
    ) -> (Vec<u16>, usize, bool) {
        let mut pos = start;
        while pos < bytes.len() {
            let (c, l) = next_token_class(&bytes[pos..]);
            if c != 4 {
                break;
            }
            pos += l;
        }
        let digits: Vec<u8> = bytes[start..pos].iter().map(|b| b - 0x30).collect();
        let mut frac_end = pos;
        let mut frac: Vec<u8> = Vec::new();
        if pos < bytes.len() && bytes[pos] == 0x2E {
            let mut p = pos + 1;
            while p < bytes.len() {
                let (c, l) = next_token_class(&bytes[p..]);
                if c != 4 {
                    break;
                }
                frac.push(bytes[p] - 0x30);
                p += l;
            }
            if !frac.is_empty() {
                frac_end = p;
            }
        }
        let codes = if frac.is_empty() {
            g2p_dict::sino_integer_codes(&digits)
        } else {
            g2p_dict::decimal_codes(&digits, &frac)
        };
        let mut merged_codes: Option<Vec<u16>> = None;
        if frac.is_empty() && !codes.is_empty() {
            let last_kps = *g2p_dict::sino_integer_kps_syllables(&digits)
                .last()
                .unwrap();
            let lcls = g2p_dict::kps_final_class(last_kps);
            if matches!(lcls, 0 | 5 | 15) {
                let (nc, _nl) = next_token_class(&bytes[pos..]);
                if nc == 0x19 {
                    let wstart = pos;
                    let mut wpos = pos;
                    while wpos < bytes.len() {
                        let (c, l) = next_token_class(&bytes[wpos..]);
                        if c != 0x19 {
                            break;
                        }
                        wpos += l;
                    }
                    let word_bytes = &bytes[wstart..wpos];
                    if let Some(wcodes) = g2p_dict::kps_bytes_to_codes(word_bytes)
                        && let Some(&first) = wcodes.first()
                    {
                        let fc = g2p_dict::to_phoneme_code(first);
                        let (fcls, fmed, finit) = crate::g2p::split_phoneme(fc);
                        if finit == 18 && fcls == 27 && g2p_dict::is_func_medial(fmed) {
                            let readings = g2p_dict::word_g2p(dicts, word_bytes);
                            let mut rec = g2p_dict::word_record_from_readings(&readings);
                            g2p_dict::stage1_phoneme_codes(&mut rec);
                            let mut all = codes.clone();
                            all.extend(rec.phoneme_codes.iter().copied());
                            rec.phoneme_codes = all;
                            let mut markers = vec![1u8; codes.len()];
                            markers.extend(rec.phoneme_markers.iter().copied());
                            rec.phoneme_markers = markers;
                            rec.phoneme_count = rec.phoneme_codes.len();
                            let mut sp = Vec::new();
                            for k in g2p_dict::sino_integer_kps_syllables(&digits) {
                                sp.push((k >> 8) as u8);
                                sp.push((k & 0xff) as u8);
                            }
                            sp.extend_from_slice(word_bytes);
                            rec.spelling = sp;
                            g2p_dict::apply_phoneme_sandhi_from(&mut rec, codes.len() - 1);
                            merged_codes = Some(rec.phoneme_codes.clone());
                            frac_end = wpos;
                        }
                    }
                }
            }
        }
        let is_merged = merged_codes.is_some();
        let codes = merged_codes.unwrap_or(codes);
        (codes, frac_end, is_merged)
    }

    fn next_word_final_tone(&self, bytes: &[u8], mut pos: usize) -> WordFinalTone {
        loop {
            if pos >= bytes.len() {
                return WordFinalTone::ClauseEnd;
            }
            let (class, len) = next_token_class(&bytes[pos..]);
            if class == 0 || class > 3 {
                return WordFinalTone::Mid;
            }
            let b0 = bytes[pos];
            if b0 == b',' {
                return WordFinalTone::Comma;
            }
            if b0 == b'.'
                || (b0 >= 0xA1
                    && len == 2
                    && ((b0 as u16) << 8 | bytes[pos + 1] as u16) == KPS_FULL_STOP)
            {
                return WordFinalTone::ClauseEnd;
            }
            if class == 2 || class == 3 {
                return WordFinalTone::ClauseEnd;
            }
            if b0 >= 0xA1 && len == 2 {
                let code = (b0 as u16) << 8 | bytes[pos + 1] as u16;
                if code == 0xA1D4 {
                    return WordFinalTone::Bracket;
                }
            }
            pos += len;
        }
    }

    /// G2P for one word token -> prosody records (dictionary pipeline + postprocess).
    fn word_to_records(
        &self,
        dicts: &G2pDicts,
        word: &[u8],
        final_tone: WordFinalTone,
    ) -> Vec<ProsodyRecord> {
        // word_g2p: exception table → morphology (colligation/User) → NonReg
        let readings = g2p_dict::word_g2p(dicts, word);
        if self.debug_log {
            eprintln!("[tts-debug] word={:02x?} readings={}", word, readings.len());
            for (i, r) in readings.iter().enumerate() {
                let decoded = kps_decode(&r.bytes);
                eprintln!(
                    "[tts-debug]   reading {i}: bytes={:02x?} dec={decoded} packed={:?} marker={}",
                    r.bytes, r.packed, r.marker
                );
            }
        }
        let mut rec = g2p_dict::word_record_from_readings_final(&readings, final_tone);
        if self.debug_log {
            eprintln!(
                "[tts-debug]   final_tone={final_tone:?} phoneme_markers={:02x?} final_marker={}",
                rec.phoneme_markers, rec.final_marker
            );
        }
        rec.spelling = word.to_vec();
        g2p_dict::apply_morph_boundaries(&mut rec);
        g2p_dict::apply_accent_markers(&mut rec);
        if self.debug_log {
            eprintln!("[tts-debug]   final_markers={:02x?}", rec.phoneme_markers);
        }
        g2p_dict::postprocess(std::slice::from_mut(&mut rec));
        g2p_dict::record_to_prosody(&rec)
    }
}

/// Verbose dump of selected units (`MIRAE_DEBUG` / `log_progress`).
fn log_units_debug(recs: &[ProsodyRecord], processed: &ProcessedUnits) {
    let n_extra = processed.units.iter().filter(|u| u.extra.is_some()).count();
    eprintln!(
        "[tts-debug] records={} units={} extras={} total_samples={}",
        recs.len(),
        processed.units.len(),
        n_extra,
        processed.total_samples
    );
    for (i, u) in processed.units.iter().enumerate() {
        let d = u.active_data();
        let x = u.extra.map(|e| {
            format!(
                "extra=({:04x},{:04x},{:04x}) woff={} wlen={} pitch={}",
                e.phone_prev, e.phone_cur, e.phone_next, e.woff, e.wlen, e.pitch
            )
        });
        eprintln!(
            "[tts-debug] unit {i}: req=({:04x},{:04x},{:04x}) reqcls={:02x} reqpitch={} reqflags={:02x} sel=({:04x},{:04x},{:04x}) woff={} wlen={} pitch={} class={:02x} pause={} marker={} {}",
            u.request.prev,
            u.request.cur,
            u.request.next,
            u.request.class,
            u.request.pitch,
            u.request.flags,
            d.phone_prev,
            d.phone_cur,
            d.phone_next,
            d.woff,
            d.wlen,
            d.pitch,
            d.classcode,
            d.pause,
            u.marker,
            x.unwrap_or_default()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_mapping_speed() {
        let cfg = TtsConfig {
            sample_rate: 22050,
            ..Default::default()
        };
        let inner = EngineConfig::from_public(&cfg);
        assert_eq!(inner.speed, 50);
        assert_eq!(cfg.sample_rate / 441, 50);
    }

    #[test]
    fn truncate_strips_trailing_newlines_only() {
        assert_eq!(truncate_last_line_char("가\n"), "가");
        assert_eq!(truncate_last_line_char("a\r\n"), "a");
        assert_eq!(truncate_last_line_char("\n"), "");
    }
}
