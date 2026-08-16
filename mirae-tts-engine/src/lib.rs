//! Text-to-speech library: [`TtsEngine`], [`TtsConfig`], plus [`encode_wav_vec`] / [`pcm_i16le_to_bytes`].
//!
//! The engine is a byte-exact Rust port of the original 《미래》2.0 (Future.exe) TTS
//! engine (see `mirae2_tts2`): text → keypad (internal code) → segmenter →
//! g2p (dictionaries) → tone (sandhi) → record (prosody) → unit_select →
//! render (VoiceData concatenation) → PCM (22050 Hz / s16le / mono).
//!
//! The public API is stable: [`TtsEngine::new`] loads voice data from a directory
//! containing `VoiceInfo.pkg` (the `Voice/` directory), plus `KeyPad.Ebd`
//! (UTF-16 → internal code table, normally at `Data/Dictionary/KeyPad.Ebd`
//! next to the voice directory). [`TtsEngine::synthesize`] is `&self` (thread-safe;
//! internal state is behind a mutex).

pub mod connect;
pub mod dict;
pub mod digit_tables;
pub mod g2p;
pub mod keypad;
pub mod kps9566;
pub mod kps_tables;
pub mod record;
pub mod render;
pub mod segmenter;
pub mod tables;
pub mod tone;
pub mod unit_select;
pub mod voice_data;
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
use segmenter::{next_token_class, KPS_FULL_STOP, Sentence};
use unit_select::{ProcessedUnits, UnitSelectConfig, UnitSelector};
use voice_data::VoiceData;
use voice_info::VoiceInfo;

// ---------------------------------------------------------------------------
// Internal engine (byte-exact port of mirae2_tts2, renamed to avoid clashing
// with the public API).
// ---------------------------------------------------------------------------

/// Voice data directory (default: relative to CWD, like the original app).
pub const DEFAULT_VOICE_DIR: &str = "Voice";

/// t21: オリジナル WAV 保存ドライバ (FUN_0042bd90) の文分割オフバイワンの再現。
///
/// 記事テキスト (UTF-16) は段落コード 0xff4f で終端する。ドライバは
/// `local_20 = len-1` の範囲で `local_14` を走査し、
/// `text[local_14]==0xff4f || local_14==local_20-1` のときピース
/// `text[iVar4..local_14]` を合成する。最終 0xff4f はループ上限のため
/// 発火せず、最後のピースは「最終行の最終文字の直前」で切り出される —
/// すなわちテキスト最終行の最終文字は決して合成されない。
/// 改行区切り入力 (\n ≒ 0xff4f) に対し「最終行の最終文字」を除去して返す。
fn truncate_last_line_char(text: &str) -> &str {
    // 末尾の改行 (CR/LF) を除いた最終行の終端バイト位置
    let end = text.trim_end_matches(['\n', '\r']).len();
    if end == 0 {
        return text;
    }
    let last_char_len = text[..end]
        .chars()
        .next_back()
        .map(|c| c.len_utf8())
        .unwrap_or(0);
    &text[..end - last_char_len]
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
    /// Map the public [`TtsConfig`] onto the internal engine fields.
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

/// The internal TTS engine: owns all voice data and runs the full pipeline.
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
    /// Verbose pipeline debug output (maps from `TtsConfig::log_progress` or
    /// the `MIRAE_DEBUG` environment variable).
    debug_log: bool,
}

impl Mirae2Engine {
    /// Load all voice data from the resolved voice directory (`VoiceInfo.pkg`
    /// etc. inside `voice`) plus the KeyPad.Ebd table.
    pub(crate) fn from_paths(voice: &Path, keypad_ebd: &Path) -> io::Result<Self> {
        let keypad = KeyPad::load(keypad_ebd)?;
        let voice_info = VoiceInfo::load(&voice.join("VoiceInfo.pkg"))?;
        let voice_data = VoiceData::open(voice)?;

        let dict_load = |name: &str| -> io::Result<Dict> {
            Dict::load(voice.join(name)).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{name}: {e}"),
                )
            })
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
    ///
    /// Pipeline: keypad → segmenter → per-word G2P (dicts + exceptions) →
    /// prosody records → tone sandhi → unit selection → waveform render.
    pub(crate) fn synthesize(&mut self, text: &str) -> io::Result<Vec<i16>> {
        let pcm_bytes = self.synthesize_bytes(text)?;
        let mut out = Vec::with_capacity(pcm_bytes.len() / 2);
        for ch in pcm_bytes.chunks_exact(2) {
            out.push(i16::from_le_bytes([ch[0], ch[1]]));
        }
        Ok(out)
    }

    /// Synthesize `text` to PCM bytes (s16le mono).
    pub(crate) fn synthesize_bytes(&mut self, text: &str) -> io::Result<Vec<u8>> {
        // t21 (동작 fix): オリジナル WAV 保存ドライバ (FUN_0042bd90) の
        // 文分割オフバイワンを再現する。記事テキストは末尾に段落コード
        // 0xff4f を持つため、分割ループ (local_20 = len-1, 発火条件:
        // `text[local_14]==0xff4f || local_14 == local_20-1`) は最後の
        // ピース [iVar4, len-2) を生成し、テキスト最終行の最終文字は
        // 決して合成されない (実測: 記事末尾「동작」の 작 が REQ に現れず
        // REQ291=동(4882, cls=0e, next=6eb3) で終端 — 281 レコード)。
        // 改行区切り入力 (\n ≒ 0xff4f) に対し「最終行の最終文字」を除去する。
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
            // 改行 (G2P 分節化の CRLF 文境界) で分割された文グループごとに
            // サンディを適用する (t16b 主因①)。改行直前は文末調値 4 に上書き
            // 済みなので、次のグループの文頭は 4×10+init にリンキングされる。
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
                "[mirae2-tts-debug] records={} units={} extras={} total_samples={}",
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
                    "[mirae2-tts-debug] unit {i}: req=({:04x},{:04x},{:04x}) reqcls={:02x} reqpitch={} reqflags={:02x} sel=({:04x},{:04x},{:04x}) woff={} wlen={} pitch={} class={:02x} pause={} marker={} {}",
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
            eprintln!("[mirae2-tts] render out.len={} units={}", out.len(), units.len());
        }
        Ok(out)
    }

    /// G2P for one sentence (internal-code bytes) → prosody records.
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
        //
        // オリジナルとのユニット数一致 (t10_record_count.md):
        // 句読点・括弧トークン (クラス 1/2/3: 空白 0xA1A1・「.」「,」「《》《」等) は
        // レコードを生成しない (オリジナルはスペース/括弧に無音レコードを出さない —
        // 実測: 「전자서고《미래》2.0은」で空白・《》相当のレコードは 0 個)。
        // 従来は packed 記号 0x2933 → 音素 0x1486 (実ユニット ~170ms) が
        // 記号トークン毎に 1 レコード生成され、344 vs 283 の差の主因だった。
        // 数字 (クラス 4)・音節 (クラス 0x19)・英字 (5/6) は従来どおり処理する。
        // t16b 主因① (改行=文境界): トークン化 (FUN_00402240) は改行を無視するが、
        // G2P 分節化 (FUN_004428b0, SPEC §2.3) は CRLF で文を区切る。改行位置で
        // records を「文グループ」に分割し、各グループを apply_sandhi に個別に
        // 渡すことで、改行直前レコードは文末調値 4、改行直後の語頭レコードは
        // 文境界リンキング (前調値 4×10 + init = 0x28 + init) になる
        // (オリジナル実測 REQ と一致 — t16b_pos_map.md)。
        // 各グループは (レコード列, 語境界オフセット列) で持つ — 語境界は
        // 段階8 の 60 音節境界 (t13 §4 / t19) の計数に使う。
        let mut groups: Vec<(Vec<ProsodyRecord>, Vec<usize>)> = vec![(Vec::new(), Vec::new())];
        let bytes = &sent.text;
        let mut pos = 0usize;
        while pos < bytes.len() {
            let (class, len) = next_token_class(&bytes[pos..]);
            if class == 0 {
                let b0 = bytes[pos];
                // CR/LF は文境界: 直前レコードを調値 4 に上書きし、文を区切る
                // (「.」と同じ文末処理 — 分節化 FUN_004428b0 の CRLF 区切り)。
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
            // 句読点 (1)・括弧 (2/3) トークンは読み飛ばす (オリジナル挙動)。
            if class <= 3 {
                // 文末音素: 「.」(ASCII 0x2E と KPS 0xA1A5) の直前レコードのみ
                // 声調クラス 4 を割り当てる (t15 訂正: 「,」は Comma マーカ
                // init 3 が語末音節に付与されるため +4 しない。実測で「,」直前の
                // cls は 03 系 — 旧実装の「,」9 件 +4 は誤りだった)。
                let b0 = bytes[pos];
                let is_period = b0 == b'.'
                    || (b0 >= 0xA1
                        && len == 2
                        && ((b0 as u16) << 8 | bytes[pos + 1] as u16) == KPS_FULL_STOP);
                if is_period && !groups.last().map_or(true, |g| g.0.is_empty()) {
                    // 「2.0」等の小数点: 「.」の次が数字 (クラス4) なら文末でない
                    // (小数パターンは class==4 の先読みで消費済みのため通常は来ないが、
                    // 念のため小数点判定で除外)。
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
            // 数字トークン (クラス 4): オリジナルの専用数字読み (FUN_0040afb0 /
            // FUN_0043c230 系) で読む。従来は packed 0x152D → 0x388e の 1 レコードに
            // 潰していたが、オリジナルは漢数詞の位取り読みを出力する
            // (実測: 「2.0」→ 0x1532/0x3851/0x4863、「1500」→ 천오백 3 レコード、
            //  「35」→ 삼십오 3 レコード — t11_digit_reading.md)。
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
                // 小数パターン: 数字 + 「.」(ASCII 0x2E) + 数字
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
                // 数字語 + 直後ハングル語の語境界跨ぎ連音 (t12 残課題):
                // オリジナルは「1500여권의」→[천,오,배,겨,꿰,늬] と、数字語末の
                // ㄱ/ㄷ/ㅂ 終声 (백) と次語頭の ㅇ 初声開音節機能語 (여) の間で
                // 連音 + 3 音節濃音化 (권→꿰) を適用する。数字内部のペア
                // (천,오 / 오,백) には規則を適用しない (実測)。
                let mut merged_codes: Option<Vec<u16>> = None;
                if frac.is_empty() && !codes.is_empty() {
                    // 数字語末の「綴りの終声クラス」で判定する (読みコードは
                    // 終声 ㄱ 系を落とすためクラス 27 になり判定できない —
                    // 綴り (sino_integer_kps_syllables) は 백 0xB9CA のまま)。
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
                                        // マージ: 数字コード列 + 次語の読みを 1 語レコードとし、
                                        // 境界ペア (数字語末, 語頭) から規則を適用する。
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
                                        // 綴り: 数字読み (천오백) + 次語の綴り
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
                                        frac_end = wpos; // 次語まで消費
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
                    // t19: 数字レコードはマーカ 0 (tone 0 — t13 §6.1 実測)。
                    // 数字+ハングル連音マージ語 (1500여권의) の最終音節 (의) のみ
                    // 語末マーカ 1 (tone 1) を立てる (実測 REQ: 의=0x01)。
                    rec.init_from_marker(if is_merged && i + 1 == n_codes { 1 } else { 0 }, false);
                    groups.last_mut().unwrap().0.push(rec);
                }
                // 語境界 (60 音節計数・bit7 後方伝搬用):
                // 直後にハングル語が続く場合は数字+ハングルを 1 語として扱う
                // (t21 確定: オリジナルは「35만개의」= 1 語 6 音節 — bit7 伝搬
                // の実測 REQ227-235 連続マークより。語境界を置くと
                // 올림말(3)+만개의(3)=6≥5 で停止し 35(삼십오) に bit7 が
                // 付かないが、オリジナルは 35만개의(6) 全体で 9 レコード連続)。
                // マージ済み (数字+連音語 1500여권의 等) は次語まで消費済みなの
                // で常に境界を置く。
                if let Some(g) = groups.last_mut() {
                    let (nc, _nl) = next_token_class(&bytes[frac_end..]);
                    if is_merged || nc != 0x19 {
                        g.1.push(g.0.len()); // 数字語も語境界として記録 (60 音節計数用)
                    }
                }
                pos = frac_end;
                continue;
            }
            // collect the token run
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
        // 段階8 (FUN_004425c0) の 60 音節境界: 文グループ内で累積 60 音節ごとに
        // 語末マーカ 5 (tone 3) を上書きする (t13 §4 実測: 집필활동을 の 을、
        // t16b 仕様: 改行でリセット)。
        for (recs, word_ends) in groups.iter_mut() {
            let mut cum = 0usize;
            let mut prev = 0usize;
            for &end in word_ends.iter() {
                cum += end - prev;
                if cum >= g2p_dict::CHUNK_SYLLABLES {
                    if let Some(last) = recs.get_mut(end - 1) {
                        last.tone_class = 3; // マーカ 5 → tone 3
                    }
                    cum = 0;
                }
                prev = end;
            }
        }
        // 段階8 の bit7 後方伝搬 (FUN_004425c0 @0x44271a-0x442777, モード0):
        // DAT_00489160=0 (実行時書込みなし — 参照は stage8 内 0x4426b4/0x44270a
        // の読取りのみ) のため前方伝搬は実行されず、常に後方モード。
        // DAT_00489168=5: 文末 (グループ末尾) から遡って累積 5 音節以上を覆う
        // まで、各語の全レコードに bit7 (フラグ 1) を立てる。
        // 実測 (orig_capture_t9.json f5 bit7): 各行の末尾語群のみ
        // (例: line1 の 제작되였습니다、line10 の 35만개의 올림말)。
        // ピッチ平滑化 (unit_select) の bit7 分岐 (再探索) のトリガとなる。
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
        // 文末レコード (「,」「.」なしで終わる文の最後) も声調クラス 4
        // (オリジナル: 記事末尾に 907ms ポーズ = クラス4 の 14 件目 — 実測一致)。
        if let Some(last) = groups.last_mut().unwrap().0.last_mut() {
            last.tone_class = (last.tone_class / 10) * 10 + 4;
        }
        // 改行直後の空グループ (行末改行など) を除去
        groups.retain(|g| !g.0.is_empty());
        if self.debug_log {
            for (gi, g) in groups.iter().enumerate() {
                let codes: Vec<String> = g.0.iter().map(|r| format!("{:04x}", r.code)).collect();
                let flags: Vec<u8> = g.0.iter().map(|r| r.flags).collect();
                eprintln!(
                    "[mirae2-tts-debug] group {gi}: n={} codes={} flags={:02x?}",
                    g.0.len(),
                    codes.join(","),
                    flags
                );
            }
        }
        Ok(groups.into_iter().map(|(recs, _)| recs).collect())
    }

    /// 語トークン直後のスキップ対象トークン (空白・句読点・括弧) を先読みし、
    /// 語末音節の声調種別 (t15) を決める。
    /// - 直後 (空白を挟んでも) 「,」→ Comma (初期クラス 3)
    /// - 直後 「.」「。」「《》」または文末 → ClauseEnd (初期クラス 4)
    /// - それ以外 (数字・音節・英字) → Mid (初期クラス 1)
    fn next_word_final_tone(&self, bytes: &[u8], mut pos: usize) -> WordFinalTone {
        loop {
            if pos >= bytes.len() {
                return WordFinalTone::ClauseEnd;
            }
            let (class, len) = next_token_class(&bytes[pos..]);
            if class == 0 || class > 3 {
                // 数字 (4)・音節 (0x19)・英字 (5/6) が続く
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
                // 括弧 《》等
                return WordFinalTone::ClauseEnd;
            }
            if b0 >= 0xA1 && len == 2 {
                let code = (b0 as u16) << 8 | bytes[pos + 1] as u16;
                if code == 0xA1D4 {
                    // 《 (KPS 開き括弧 0xA1D4 — char_class_16 ではクラス 1 と
                    // 同じ扱い) — 実測 (t13_boundary §2.1: 3|4 境界 直前 0x03)
                    // では句境界として調値 3 (マーカ 5) を要求する。
                    // 閉じ括弧 》 (0xA1D5) は句境界にしない (実測: 《》の後の
                    // 「미래」の 래 は cls 01 — 通常語末)。
                    return WordFinalTone::Bracket;
                }
            }
            // 空白 (0xA1A1, class 1) は読み飛ばして次を見る
            pos += len;
        }
    }

    /// G2P for one word token → prosody records (dictionary pipeline +
    /// postprocess + record conversion).
    fn word_to_records(
        &self,
        dicts: &G2pDicts,
        word: &[u8],
        final_tone: WordFinalTone,
    ) -> Vec<ProsodyRecord> {
        // word_g2p: exception table → morphology (colligation/User) → NonReg
        let readings = g2p_dict::word_g2p(dicts, word);
        if self.debug_log {
            let kps = crate::kps9566::Kps9566::builtin();
            eprintln!(
                "[mirae2-tts-debug] word={:02x?} readings={}",
                word,
                readings.len()
            );
            for (i, r) in readings.iter().enumerate() {
                let decoded = kps.decode(&r.bytes);
                eprintln!(
                    "[mirae2-tts-debug]   reading {i}: bytes={:02x?} dec={decoded} packed={:?} marker={}",
                    r.bytes, r.packed, r.marker
                );
            }
        }
        let mut rec = g2p_dict::word_record_from_readings_final(&readings, final_tone);
        if self.debug_log {
            eprintln!(
                "[mirae2-tts-debug]   final_tone={final_tone:?} phoneme_markers={:02x?} final_marker={}",
                rec.phoneme_markers, rec.final_marker
            );
        }
        // 語間音韻規則は辞書読みでなく元の綴りで終声を判定する (증가→즈가 等)。
        rec.spelling = word.to_vec();
        // t19: 形態素境界マーカ 1 (段階1 分析器相当) と語末 b5c6 相当の
        // 上書き (段階8 相当) を語レコードに適用する。
        g2p_dict::apply_morph_boundaries(&mut rec);
        g2p_dict::apply_accent_markers(&mut rec);
        if self.debug_log {
            eprintln!(
                "[mirae2-tts-debug]   final_markers={:02x?}",
                rec.phoneme_markers
            );
        }
        g2p_dict::postprocess(std::slice::from_mut(&mut rec));
        g2p_dict::record_to_prosody(&rec)
    }
}

// ---------------------------------------------------------------------------
// Public API (stable).
// ---------------------------------------------------------------------------

/// Public TTS engine configuration.
#[derive(Debug, Clone)]
pub struct TtsConfig {
    /// Output sample rate in Hz (default 22050; original speed 50 × 441).
    pub sample_rate: u32,
    /// Legacy sentence-end pause in samples. The byte-exact engine renders
    /// pauses from the voice data itself, so this field is accepted for
    /// compatibility but not applied (treated as 0).
    pub sentence_pause: i16,
    /// Progress and warnings on stderr (maps to the engine's debug output).
    /// Default `false` for library embeds; enable for CLI-style feedback.
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

/// The TTS engine: loads voice data and runs the full pipeline.
///
/// Thread-safe: [`synthesize`](Self::synthesize) takes `&self`; the mutable
/// engine state is guarded by an internal mutex, so a single engine can be
/// shared across threads (e.g. behind `Arc` in an HTTP server).
pub struct TtsEngine {
    inner: Mutex<Mirae2Engine>,
    config: TtsConfig,
}

/// Candidate locations for `KeyPad.Ebd` (UTF-16 → internal code table), tried
/// in order. The original app layout is `<root>/Data/Dictionary/KeyPad.Ebd`
/// next to `<root>/Voice/`; some deployments place it inside the voice dir.
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
    /// Initialize the engine from `voice_dir`.
    ///
    /// `voice_dir` is either the directory containing `VoiceInfo.pkg` /
    /// `VoiceData.pkg` directly (server/CLI convention, e.g. `/var/mirae-tts/Voice`),
    /// or an install root with a `Voice/` subdirectory (original app layout).
    /// `KeyPad.Ebd` is located automatically (`Data/Dictionary/KeyPad.Ebd`
    /// next to the voice directory, or inside it).
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
        let keypad = find_keypad_ebd(voice_dir, &voice).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "KeyPad.Ebd not found near {:?} (looked for Data/Dictionary/KeyPad.Ebd next to the voice dir and inside it)",
                    voice_dir
                ),
            )
        })?;

        let mut inner = Mirae2Engine::from_paths(&voice, &keypad)?;
        inner.debug_log = config.log_progress || std::env::var("MIRAE_DEBUG").is_ok();
        inner.set_config(EngineConfig::from_public(&config));

        Ok(TtsEngine {
            inner: Mutex::new(inner),
            config,
        })
    }

    /// Synthesize `text` to mono PCM samples (s16le) at
    /// [`effective_sample_rate`](Self::effective_sample_rate).
    pub fn synthesize(&self, text: &str) -> io::Result<Vec<i16>> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| io::Error::other("TTS engine mutex poisoned"))?;
        inner.synthesize(text)
    }

    /// Synthesize `text` and deliver the PCM in chunks (16 KiB samples each)
    /// to `on_chunk`. Returning `false` from `on_chunk` stops synthesis early.
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

    /// Logical output sample rate (Hz) — equals `config.sample_rate`.
    pub fn effective_sample_rate(&self) -> u32 {
        self.config.sample_rate
    }

    /// Number of loaded voice index entries.
    pub fn voice_entry_count(&self) -> usize {
        self.inner
            .lock()
            .map(|inner| inner.voice_entry_count())
            .unwrap_or(0)
    }

    /// Current public configuration.
    pub fn config(&self) -> &TtsConfig {
        &self.config
    }

    /// Replace the public configuration and apply it to the engine.
    pub fn set_config(&mut self, config: TtsConfig) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.debug_log = config.log_progress || std::env::var("MIRAE_DEBUG").is_ok();
            inner.set_config(EngineConfig::from_public(&config));
        }
        self.config = config;
    }
}

/// Mono PCM as little-endian `i16` bytes (no WAV header). Same layout as HTTP
/// `application/octet-stream` / `audio/l16` bodies.
pub fn pcm_i16le_to_bytes(pcm: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(pcm.len() * 2);
    for &s in pcm {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    bytes
}

/// Encode mono s16le PCM as a WAV file (byte-exact replica of the original
/// engine's 46-byte header: WAVEFORMATEX with `cbSize=0`, RIFF size =
/// data + 0x30).
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

/// Same as the crate root: [`TtsEngine`], [`TtsConfig`], WAV/PCM helpers.
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
