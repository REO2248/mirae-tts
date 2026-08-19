//! Text-to-speech library: [`TtsEngine`], [`TtsConfig`], [`encode_wav_vec`] / [`pcm_i16le_to_bytes`].
//! Rust port of the original Future.exe TTS pipeline: text -> keypad -> segmenter ->
//! g2p -> tone -> record -> unit_select -> render -> PCM (22050 Hz / s16le / mono).
pub mod alphabet;
pub mod connect;
pub mod dict;
pub mod digit_tables;
pub mod g2p;
pub mod keypad;
pub mod kps_tables;
pub mod postprocess_tables;
pub mod record;
pub mod render;
pub mod segmenter;
pub mod tables;
pub mod tone;
pub mod unit_select;
pub mod voice_data;
pub mod voice_dict;
pub mod voice_info;
pub mod wav;

use std::io;
use std::path::Path;
use std::sync::Mutex;

use connect::ConnectMatrix;
use dict::Dict;
use g2p::g2p_dict::{self, G2pDicts, WordFinalTone, WordRecord};
use keypad::KeyPad;
use record::ProsodyRecord;
use segmenter::{KPS_FULL_STOP, Sentence, next_token_class};
use unit_select::{ProcessedUnits, UnitSelectConfig, UnitSelector};
use voice_data::VoiceData;
use voice_info::VoiceInfo;

// Internal engine implementation.

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
    // Strip only trailing \n/\r characters. The previous implementation
    // unconditionally removed one extra character (even when there was no
    // trailing newline), which caused the final syllable of inputs like
    // "가나다" to be lost ("가나"). The helper should never drop a real
    // character — it only exists to tolerate file/stdin inputs ending with
    // a newline. We also expose this as pub(crate) for regression testing.
    text.trim_end_matches(['\n', '\r'])
}

/// Logical output sample rate (Hz), per original WAVEFORMATEX (speed 50 × 441).
pub const SAMPLE_RATE: u32 = 22050;

/// Chunk ring buffer slot count (original engine +0xf8, 20 slots).
pub const RING_SLOTS: usize = 20;

/// Max buffered output bytes (original engine, 1 MB).
pub const RING_MAX_BYTES: usize = 0xFFFFF;

/// Default WAV sample rate used by [`encode_wav_vec`] / [`pcm_i16le_to_bytes`].
pub const DEFAULT_SAMPLE_RATE: u32 = SAMPLE_RATE;

/// Internal engine configuration (subset of the original engine fields).
#[derive(Debug, Clone)]
pub(crate) struct EngineConfig {
    /// engine+0xe8: pitch smoothing tolerance (original 15).
    pub(crate) pitch_smoothing_tolerance: u16,
    /// engine+0xec: sentence-end tone class threshold (original 3).
    pub(crate) end_tone_threshold: u8,
    /// engine+0xdc: random (expression) mode (original 0).
    pub(crate) random_mode: bool,
    /// speed (original DAT_004a2ff8 = 50 → 22050 Hz).
    pub(crate) speed: u32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            pitch_smoothing_tolerance: 15,
            end_tone_threshold: 3,
            random_mode: false,
            speed: 50,
        }
    }
}

impl EngineConfig {
    fn from_public(cfg: &TtsConfig) -> Self {
        EngineConfig {
            pitch_smoothing_tolerance: 15,
            end_tone_threshold: 3,
            random_mode: false,
            // sample_rate = speed × 441 (original WAVEFORMATEX relation).
            speed: (cfg.sample_rate / 441).max(1),
        }
    }
}

pub(crate) fn kps_decode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len());
    kps9566::kps9566::Decoder::new().decode_to_string(bytes, &mut s, true);
    s
}

pub(crate) fn kps_lookup(code: u16) -> Option<char> {
    kps9566::kps9566::decode(&[(code >> 8) as u8, code as u8])
        .ok()?
        .chars()
        .next()
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
        let voice_info = VoiceInfo::load(&voice.join("VoiceInfo.pkg"))?;
        let voice_data = VoiceData::open(voice)?;

        let dict_load = |name: &str| -> io::Result<Dict> {
            Dict::load(voice.join(name))
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{name}: {e}")))
        };
        let colligation = dict_load("colligation.pkg")?;
        let user = dict_load("User.pkg")?;
        let nonreg = dict_load("NonReg.pkg")?;
        let conjects = dict_load("Conjects.pkg")?;
        let connect = ConnectMatrix::load(&voice.join("Connect.pkg"))?;

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

    pub(crate) fn config(&self) -> &EngineConfig {
        &self.cfg
    }

    pub(crate) fn set_config(&mut self, cfg: EngineConfig) {
        self.cfg = cfg;
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
        let sentences = segmenter::tokenize(&internal);
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

        // 4. Unit selection (FUN_0044b880 + FUN_0044a800)
        let mut sel = UnitSelector::new(&self.voice_info, UnitSelectConfig::default());
        let recs: Vec<unit_select::ProsodyRecord> = all_records
            .iter()
            .map(|r| unit_select::ProsodyRecord {
                prev_code: r.prev_code,
                code: r.code,
                marker: r.marker,
                flag: r.flags,
                tone_class: r.tone_class,
            })
            .collect();
        let processed: ProcessedUnits = sel.process(&recs);

        if self.debug_log {
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

        // 5. Waveform render (FUN_0044c2e0 + FUN_0044b700)
        let to_unit_rec = |e: voice_info::VoiceInfoEntry| render::UnitRecord {
            woff: e.woff,
            wlen: e.wlen,
            pitch: e.pitch as i16,
            classcode: (e.classcode & 0xff) as u8,
            pause: e.pause,
        };
        let units: Vec<render::RenderUnit> = processed
            .units
            .iter()
            .map(|u| render::RenderUnit {
                record: to_unit_rec(u.active_data()),
                code_cur: u.request.cur,
                code_next: u.request.next,
                extra: u.extra.map(to_unit_rec),
            })
            .collect();
        let mut out = Vec::new();
        render::render_units(&mut self.voice_data, &units, &mut out, self.cfg.random_mode)?;
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
        let mut groups: Vec<(Vec<ProsodyRecord>, Vec<usize>)> = vec![(Vec::new(), Vec::new())];
        let bytes = &sent.text;
        let mut pos = 0usize;
        while pos < bytes.len() {
            let (class, len) = next_token_class(&bytes[pos..]);
            if class == 0 {
                let b0 = bytes[pos];
                if b0 == b'\n' || b0 == b'\r' {
                    if let Some(last) = groups.last_mut().and_then(|g| g.0.last_mut()) {
                        last.tone_class = (last.tone_class / 10) * 10 + 4;
                    }
                    if !groups.last().map_or(true, |g| g.0.is_empty()) {
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
                if is_period && !groups.last().map_or(true, |g| g.0.is_empty()) {
                    let is_decimal_point = b0 == b'.' && {
                        let (nc, _) = next_token_class(&bytes[pos + len..]);
                        nc == 4
                    };
                    if !is_decimal_point {
                        let last = groups.last_mut().unwrap().0.last_mut().unwrap();
                        last.tone_class = (last.tone_class / 10) * 10 + 4;
                    }
                }
                pos += len;
                continue;
            }
            if class == 4 {
                let start = pos;
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
                            if let Some(wcodes) = g2p_dict::kps_bytes_to_codes(word_bytes) {
                                if let Some(&first) = wcodes.first() {
                                    let fc = g2p_dict::to_phoneme_code(first);
                                    let (fcls, fmed, finit) = crate::g2p::split_phoneme(fc);
                                    if finit == 18 && fcls == 27 && g2p_dict::is_func_medial(fmed) {
                                        let readings = g2p_dict::word_g2p(&dicts, word_bytes);
                                        let mut rec =
                                            g2p_dict::word_record_from_readings(&readings);
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
                                        g2p_dict::apply_phoneme_sandhi_from(
                                            &mut rec,
                                            codes.len() - 1,
                                        );
                                        merged_codes = Some(rec.phoneme_codes.clone());
                                        frac_end = wpos;
                                    }
                                }
                            }
                        }
                    }
                }
                let is_merged = merged_codes.is_some();
                let codes = merged_codes.unwrap_or(codes);
                let n_codes = codes.len();
                for (i, code) in codes.into_iter().enumerate() {
                    let mut rec = crate::record::ProsodyRecord::new(code);
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
            let n = word_records.len();
            let g = groups.last_mut().unwrap();
            g.0.extend(word_records);
            g.1.push(g.0.len());
        }
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
        if let Some(last) = groups.last_mut().unwrap().0.last_mut() {
            last.tone_class = (last.tone_class / 10) * 10 + 4;
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

#[derive(Debug, Clone)]
pub struct TtsConfig {
    /// Output sample rate in Hz (default 22050; original speed 50 × 441).
    pub sample_rate: u32,
    /// Legacy sentence-end pause in samples (accepted for compatibility, not applied).
    pub sentence_pause: i16,
    pub log_progress: bool,
}

impl Default for TtsConfig {
    fn default() -> Self {
        TtsConfig {
            sample_rate: 22050,
            sentence_pause: 4000,
            log_progress: false,
        }
    }
}

/// The TTS engine: loads voice data and runs the full pipeline (thread-safe).
pub struct TtsEngine {
    inner: Mutex<Mirae2Engine>,
    config: TtsConfig,
}

/// Candidate locations for `KeyPad.Ebd`, tried in order.
fn find_keypad_ebd(voice_dir: &Path, voice: &Path) -> Option<std::path::PathBuf> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    candidates.push(voice.join("KeyPad.Ebd"));
    candidates.push(voice.join("Data").join("Dictionary").join("KeyPad.Ebd"));
    if let Some(parent) = voice.parent() {
        candidates.push(parent.join("Data").join("Dictionary").join("KeyPad.Ebd"));
        candidates.push(parent.join("KeyPad.Ebd"));
    }
    candidates.push(voice_dir.join("KeyPad.Ebd"));
    candidates.push(voice_dir.join("Data").join("Dictionary").join("KeyPad.Ebd"));
    candidates.into_iter().find(|p| p.exists())
}

impl TtsEngine {
    /// Initialize the engine from `voice_dir` (voice dir, or install root with `Voice/`).
    /// If `voice_dir` is empty, `MIRAE_VOICE_DIR` env / `DEFAULT_VOICE_DIR` fallback is not auto-used here — callers
    /// should call `default_voice_dir()` and pass it explicitly when they need env resolution.
    pub fn new<P: AsRef<Path>>(voice_dir: P, config: TtsConfig) -> io::Result<Self> {
        let voice_dir = voice_dir.as_ref();
        let voice: std::path::PathBuf = if voice_dir.join("VoiceInfo.pkg").exists() {
            voice_dir.to_path_buf()
        } else if voice_dir.join("Voice").join("VoiceInfo.pkg").exists() {
            voice_dir.join("Voice")
        } else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "VoiceInfo.pkg not found in {:?} (expected directly or under Voice/)",
                    voice_dir
                ),
            ));
        };
        let keypad = find_keypad_ebd(voice_dir, &voice);
        if keypad.is_none() {
            eprintln!(
                "[tts] KeyPad.Ebd not found near {:?} — using approximate KPS9566 fallback (exact original conversion requires KeyPad.Ebd)",
                voice_dir
            );
        }

        let mut inner = Mirae2Engine::from_paths(&voice, keypad.as_deref())?;
        inner.debug_log = config.log_progress || std::env::var("MIRAE_DEBUG").is_ok();
        inner.set_config(EngineConfig::from_public(&config));

        Ok(TtsEngine {
            inner: Mutex::new(inner),
            config,
        })
    }

    pub fn synthesize(&self, text: &str) -> io::Result<Vec<i16>> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| io::Error::other("TTS engine mutex poisoned"))?;
        inner.synthesize(text)
    }

    pub fn synthesize_streaming<F: FnMut(Vec<i16>) -> bool>(
        &self,
        text: &str,
        mut on_chunk: F,
    ) -> io::Result<()> {
        const CHUNK_SAMPLES: usize = 16 * 1024;
        let pcm = self.synthesize(text)?;
        for chunk in pcm.chunks(CHUNK_SAMPLES) {
            if !on_chunk(chunk.to_vec()) {
                break;
            }
        }
        Ok(())
    }

    pub fn effective_sample_rate(&self) -> u32 {
        self.config.sample_rate
    }

    pub fn voice_entry_count(&self) -> usize {
        self.inner
            .lock()
            .map(|inner| inner.voice_entry_count())
            .unwrap_or(0)
    }

    pub fn config(&self) -> &TtsConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: TtsConfig) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.debug_log = config.log_progress || std::env::var("MIRAE_DEBUG").is_ok();
            inner.set_config(EngineConfig::from_public(&config));
        }
        self.config = config;
    }
}

/// Mono PCM as little-endian `i16` bytes (no WAV header).
pub fn pcm_i16le_to_bytes(pcm: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(pcm.len() * 2);
    for &s in pcm {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    bytes
}

/// Encode mono s16le PCM as a WAV file (byte-exact 46-byte header replica).
pub fn encode_wav_vec(pcm: &[i16], sample_rate: u32) -> io::Result<Vec<u8>> {
    let data = pcm_i16le_to_bytes(pcm);
    let mut out = Vec::with_capacity(wav::WAV_HEADER_SIZE + data.len());
    let mut h = [0u8; wav::WAV_HEADER_SIZE];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&((data.len() as u32) + 0x30).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&0x12u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&1u16.to_le_bytes());
    h[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    h[28..32].copy_from_slice(&(sample_rate * 2).to_le_bytes());
    h[32..34].copy_from_slice(&2u16.to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..38].copy_from_slice(&0u16.to_le_bytes());
    h[38..42].copy_from_slice(b"data");
    h[42..46].copy_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&h);
    out.extend_from_slice(&data);
    Ok(out)
}

pub mod prelude {
    pub use super::{
        DEFAULT_SAMPLE_RATE, TtsConfig, TtsEngine, encode_wav_vec, pcm_i16le_to_bytes,
    };
}

#[cfg(test)]
mod api_tests {
    use super::*;

    #[test]
    fn pcm_i16le_to_bytes_roundtrip() {
        let pcm = vec![0i16, -1, 1, 0x1234, -0x5678];
        let bytes = pcm_i16le_to_bytes(&pcm);
        assert_eq!(bytes.len(), pcm.len() * 2);
        assert_eq!(bytes, [0, 0, 0xff, 0xff, 1, 0, 0x34, 0x12, 0x88, 0xa9]);
    }

    #[test]
    fn encode_wav_vec_header_is_46_bytes() {
        let wav = encode_wav_vec(&[0i16; 100], 22050).unwrap();
        assert_eq!(wav.len(), 46 + 200);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[16..20], &0x12u32.to_le_bytes()); // fmt chunk = 18 (WAVEFORMATEX)
        assert_eq!(&wav[24..28], &22050u32.to_le_bytes());
        assert_eq!(&wav[38..42], b"data");
        assert_eq!(&wav[42..46], &200u32.to_le_bytes());
        assert_eq!(&wav[4..8], &(200u32 + 0x30).to_le_bytes()); // RIFF size quirk
    }

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
}
