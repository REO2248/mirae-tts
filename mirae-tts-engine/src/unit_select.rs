//! ユニット選択 (FUN_0044b880 + FUN_0044a800 相当) — VoiceInfo 全件線形走査・スコアリング・
//! フォールバック・ピッチ平滑化・継続時間割当・句読点 pause。
//!
//! オリジナル Future.exe (Ghidra デコンパイル, T1_voiceinfo.md §5 / T4_pipeline.md §2.4 /
//! dump_g3.txt / dump_g4.txt) からの完全仕様。既存 mirae-tts 実装は参照していない。
//!
//! 主要関数対応:
//! - [`UnitSelector::scan`]        = FUN_0044a800 (全件走査 + スコア + タイブレーク)
//! - [`UnitSelector::scan_special`] = FUN_0044b220 (特殊ユニット bit7 のピッチ近接走査)
//! - [`UnitSelector::process`]     = FUN_0044b880 (ドライバ: 要求組立/休止判定/フォールバック/
//!                                   ピッチ平滑化/継続時間割当/pause/総サンプル数)
//! - [`duration_for`]              = FUN_0044b2a0 (継続時間)
//! - [`is_pause`]                  = FUN_0042a3c0 (休止判定)
//! - [`is_real_phoneme`]           = FUN_0044b350 (実音素判定)

use crate::record::MARKER_SENTENCE_END;
use crate::tables::{
    FALLBACK_ALLOW, FALLBACK_REPL_HI, FALLBACK_REPL_LO, FILTER_TABLE, PHON_CLASS_FLAG_A,
    PHON_CLASS_FLAG_B, PHON_CLASS_FLAG_C, PHON_CLASS_FLAG_D, TONE_CLASS_MAP, TONE_TRANS_COST,
};
use crate::voice_info::{VoiceInfo, VoiceInfoEntry};

// ---------------------------------------------------------------------------
// 音素コード・クラスユーティリティ
// ---------------------------------------------------------------------------

/// 休止/句読点判定 (FUN_0042a3c0): 上位10bit ∈ {2,0xe,0x12,0x1b} かつ下位5bit ∈ {1,4,0x12}、
/// または 上位=6 かつ下位 ∈ {3,4,0x12}。
pub fn is_pause(hi10: u16, low5: u16) -> bool {
    ((hi10 == 2 || hi10 == 0xe || hi10 == 0x12 || hi10 == 0x1b)
        && (low5 == 1 || low5 == 4 || low5 == 0x12))
        || (hi10 == 6 && (low5 == 3 || low5 == 4 || low5 == 0x12))
}

/// 実音素判定 (FUN_0044b350): 下位5bit ∈ {1,4,6,8,9,10,0xb,0xc,0xd,0xe,0x10,0x11,0x12} または
/// (下位=3 かつ 上位=6) なら非実音素。
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

/// クラスコード byte+0x14 の正規化 (対象側, a800 local_23):
/// `/10==2 → %10+0x1e`, `%10==2 → (c/10)*10+3`, `%10==5 → (c/10)*10+4`。
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

/// 候補側正規化 (a800 local_20 — FILTER_TABLE 用): `%10==2 → (c/10)*10+1`。
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

/// 候補側正規化 (a800 local_1d — 声調遷移コスト表列用): `%10==2 → (c/10)*10+3`。
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

/// TONE_CLASS_MAP (16×16) の行ヘッダ検索: 行先頭 == 正規化クラス となる行 index 0-15。
/// 見つからなければ 0 (実データでは必ず見つかる)。
fn tone_class_row(norm_class: u8) -> usize {
    for row in 0..16 {
        if TONE_CLASS_MAP[row * 16] == norm_class {
            return row;
        }
    }
    0
}

/// TONE_CLASS_MAP 行 `row` 内で正規化クラスを逆引き (a800 local_1d)。
/// 見つからなければ 15 にクランプ (オリジナルは行外/配列外を読む不定動作; 実データでは発生しない)。
fn tone_class_col(row: usize, norm_class: u8) -> usize {
    for col in 0..16 {
        if TONE_CLASS_MAP[row * 16 + col] == norm_class {
            return col;
        }
    }
    15
}

/// PHON_CLASS_FLAG_D 参照 (0x48bc90): オリジナルは `code>>10` (0..63) で索引し、
/// index ≥ 32 は隣接する TONE_CLASS_MAP のバイト列を LE i32 として読む (フラットメモリ挙動)。
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

// ---------------------------------------------------------------------------
// 28B ユニット要求
// ---------------------------------------------------------------------------

/// 28B ユニット要求 (FUN_0044b880 が組立て、FUN_0044a800 が消費するフィールドのみ)。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnitRequest {
    /// +0x00 現在(右)音素コード。
    pub cur: u16,
    /// +0x02 前(左)音素コード。
    pub prev: u16,
    /// +0x04 次音素コード。
    pub next: u16,
    /// +0x12 ピッチ (要求側; タイブレーク/フィルタ緩和に使用)。
    pub pitch: u16,
    /// +0x14 声調クラス byte (サンディ適用済み)。
    pub class: u8,
    /// +0x15 フラグ byte (bit7 = ピッチフィルタ緩和要求、下位 = 休止コード)。
    pub flags: u8,
}

/// 12B 韻律レコード (FUN_00442ae0 出力、FUN_0044ca50 が声調サンディ適用後に渡す)。
/// record.rs 実装時に移譲可能な最小形。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProsodyRecord {
    /// +0 u16 前ユニットコード (ポンプが複製; 本モジュールは未使用)。
    pub prev_code: u16,
    /// +2 u16 現ユニットコード。
    pub code: u16,
    /// +4 u8 マーカ (2 = 特殊)。
    pub marker: u8,
    /// +5 u8 フラグ (bit7 → 選択時 0x80)。
    pub flag: u8,
    /// +6 u8 声調クラス = レベル×10 + 調値。
    pub tone_class: u8,
}

impl UnitRequest {
    fn pitch_signed(&self) -> i32 {
        self.pitch as i16 as i32
    }
}

// ---------------------------------------------------------------------------
// 文脈スコア (a800 iVar18/iVar12)
// ---------------------------------------------------------------------------

/// 左文脈スコア iVar18。`search_flag` = a800 param_4 (1 = 通常探索, 0 = ピッチ平滑化再探索)。
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

/// 右文脈スコア iVar12。
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

// ---------------------------------------------------------------------------
// 継続時間 (FUN_0044b2a0)
// ---------------------------------------------------------------------------

/// 継続時間割当: クラス%10 → 1/2/3,5/4 → 1000/3000/5000/20000 samples。
/// オリジナルは有効フラグ (+0xb8..0xc4) で各クラスを無効化可能 (ctor 既定は 0/1/1/1)。
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

// ---------------------------------------------------------------------------
// ユニット選択エンジン
// ---------------------------------------------------------------------------

/// 境界コード (DAT_00499234): リクエスト組立で前後文脈が無いとき prev/next に
/// 入るセンチネル。ブート時に 0x6EB3 へ初期化される (FUN_00428080:
/// `movw $0x6eb3, DAT_00499234`)。文頭・文末・レベル>1/調値>1 のレコードで
/// 使用され、ユニット選択 (cur) には決してならない (実測: 293 reqs 中 cur=0
/// 回)。0x6EB3 は hi10=0x1B・下位5bit=0x13 — 休止クラス 0x1B 系のコード。
pub const BOUNDARY_CODE: u16 = 0x6EB3;

/// エンジン設定 (オリジナルの this+0xe8 / +0xe4 / +0xc8..0xd4 に相当)。
#[derive(Clone, Copy, Debug)]
pub struct UnitSelectConfig {
    /// ピッチ平滑化許容差 (this+0xe8 = 15)。
    pub pitch_tolerance: i32,
    /// 要求ピッチ既定値 (this+0xe4 = 90; レベル≥2 のとき使用)。
    pub request_pitch_default: u16,
    /// 継続時間値 (this+0xc8..0xd4): クラス 1/2/3,5/4 → 1000/3000/5000/20000。
    pub duration_values: [u16; 4],
    /// 継続時間クラス別有効フラグ (this+0xb8..0xc4)。
    /// オリジナル ctor は [false,true,true,true]; タスク契約どおり既定は全て有効。
    pub duration_enabled: [bool; 4],
    /// 特殊ユニット走査 (FUN_0044b220) の初期ピッチ距離 (iVar5 = 200)。
    pub special_dist_init: i32,
}

impl Default for UnitSelectConfig {
    fn default() -> Self {
        UnitSelectConfig {
            pitch_tolerance: 15,
            request_pitch_default: 90,
            duration_values: [1000, 3000, 5000, 20000],
            // オリジナル ctor (this+0xb8..0xc4) は [false,true,true,true]:
            // クラス 1 (1000) の継続時間は既定で無効。
            duration_enabled: [false, true, true, true],
            special_dist_init: 200,
        }
    }
}

/// 選択された 1 ユニット (オリジナルのリストノード 0x14B に相当)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitSelection {
    /// 28B 要求 (ノード+4)。
    pub request: UnitRequest,
    /// 選択エントリ (ノード+8、+0x10 にスコアマーカ)。
    pub data: VoiceInfoEntry,
    /// スコアマーカ: 通常 = score、フォールバック変種A = score+10000、B = score+20000。
    pub marker: i32,
    /// ピッチ平滑化による差し替えエントリ (ノード+0xc; 無ければ None)。
    pub data2: Option<VoiceInfoEntry>,
    /// 差し替えマーカ (score+30000)。
    pub marker2: Option<i32>,
    /// 追加ユニット (ノード+0x10; FUN_0044b220 の特殊走査結果)。
    pub extra: Option<VoiceInfoEntry>,
}

impl UnitSelection {
    /// レンダリング時に使用される実データ (b320 相当: marker2 が 10000 の倍数でなければ data2)。
    pub fn active_data(&self) -> VoiceInfoEntry {
        match self.data2 {
            Some(d2) if self.marker2.map_or(false, |m| m % 10000 != 0) => d2,
            _ => self.data,
        }
    }

    /// データ側 pause フィールド (継続時間 + 句読点加算後の値)。
    pub fn pause(&self) -> i32 {
        self.active_data().pause as i32
    }
}

/// process() の結果。
#[derive(Clone, Debug)]
pub struct ProcessedUnits {
    /// 選択ユニット列 (成功分のみ)。
    pub units: Vec<UnitSelection>,
    /// 総サンプル数 = Σ(wlen×2 + pause×2 + 追加ユニット wlen×2)。
    pub total_samples: i64,
}

/// ユニット選択エンジン (FUN_0044b880 ドライバ状態)。
pub struct UnitSelector<'a> {
    info: &'a VoiceInfo,
    cfg: UnitSelectConfig,
    /// local_6d: 直前選択エントリのクラス%10 (サンディ/変種B クラス組立に使用)。初期値 4。
    prev_tone: u8,
    /// 直前選択エントリのピッチ (要求ピッチ; レベル<2 のとき使用)。初期値 0。
    prev_best_pitch: u16,
    /// 直前選択エントリのクラス高位 byte (特殊レコード要求フラグに使用)。
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

    /// FUN_0044a800 相当: 全 70,150 エントリ線形走査。
    /// 一致条件: byte+0x16 bit7 クリア && エントリ u16[0] == 要求 cur。
    /// クラス別フィルタ (FILTER_TABLE): 要求フラグ bit7 かつレベル≤1 のときピッチ上限を省略。
    /// スコア = (w_next×iVar12)/(w_next+w_prev) + 100 + (w_prev×iVar18)/(w_next+w_prev)
    ///        + TONE_TRANS_COST[行(対象調)×16 + 列(候補調)]
    /// タイブレーク: 要求ピッチとの差が小さい方。
    /// 戻り値: (最良エントリ, スコア)。スコア 0 は未ヒット (フォールバック対象)。
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
            // 照合: byte+0x16 bit7 クリア && u16[0] == 対象 cur 音素
            if !e.is_normal() || e.phone_cur != req.cur {
                continue;
            }
            // クラス正規化 (候補) → FILTER_TABLE 行
            let cand_norm = normalize_candidate_class_a(e.class_byte());
            let cand_idx = tone_class_row(cand_norm).min(15);
            let f = &FILTER_TABLE[cand_idx];
            let pitch = e.pitch_signed();
            let wlen = e.wlen as i32;
            // フィルタ (要求フラグ bit7 かつレベル≤1 → ピッチ上限省略)
            let pass = if (req.flags & 0x80) != 0 && target_level <= 1 {
                f[0] <= pitch && f[2] <= wlen && wlen <= f[3]
            } else {
                f[0] <= pitch && pitch <= f[1] && f[2] <= wlen && wlen <= f[3]
            };
            if !pass {
                continue;
            }
            // 文脈スコア
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
            // 声調遷移コスト表 (対象行内で候補クラスを逆引き)
            let cand_norm_b = normalize_candidate_class_b(e.class_byte());
            let col = tone_class_col(tone_row, cand_norm_b);
            let cost = TONE_TRANS_COST[tone_row * 16 + col];
            // 合成スコア (ディスアセンブリ 0x44b119-0x44b1ae 確定):
            //   [ESP+0x30]=w_prev, [ESP+0x20]=w_next で
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
                    req.prev, req.cur, req.next, req.class, req.pitch, w_prev, w_next, tone_row,
                    e.woff, e.phone_prev, e.phone_next, pitch, wlen, e.classcode & 0xff, i18, i12, cost, s
                );
            }
            // タイブレーク: 要求ピッチとの差
            let pd = (req.pitch_signed() - pitch).abs();
            if s > best_score || (s == best_score && pd < best_pitch_dist) {
                best_score = s;
                best_pitch_dist = pd;
                best = Some(*e);
            }
        }

        best.map(|e| (e, best_score))
    }

    /// FUN_0044b220 相当: 特殊ユニット (byte+0x16 bit7 セット) だけを対象に、
    /// 要求ピッチとの絶対差が最小の 28B を返す (コード一致・フィルタなし)。
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

    /// FUN_0044b880 相当: レコード列からユニット選択を実行する。
    /// 1. 28B 要求組立 (サンディ・休止判定・境界コード 0)
    /// 2. 全件走査 → 未ヒット時フォールバック変種 A/B (マーカ +10000/+20000)
    /// 3. ピッチ平滑化 (3 連、差 ≥ 許容差 15 のスパイクを平均へ置換して再探索、+30000)
    /// 4. 継続時間割当 (クラス%10)
    /// 5. 追加ユニット (実音素かつ調値<2) + 句読点 pause (+1000/+1500) + 総サンプル数
    pub fn process(&mut self, records: &[ProsodyRecord]) -> ProcessedUnits {
        let mut units: Vec<UnitSelection> = Vec::new();

        // ---- 1. レコードループ: 要求組立 + 走査 + フォールバック ----
        for (idx, rec) in records.iter().enumerate() {
            // t18 (改行=文境界検証): チャンク境界でドライバ状態をリセットする。
            // オリジナルのポンプ (FUN_0044ca50) は「調値 ≤2 の連続 + 調値 ≥3 の
            // 終端レコード 1 件」を 1 チャンクとして FUN_0044b880 を呼び直し、
            // 関数冒頭で [esp+0x13]=4 (prev_tone, @0x44b89c) と [esp+0x5d]=0
            // (prev_best_class_hi, @0x44b8a5) を毎回初期化する。このため
            // 文末/句読点レコード (調値 3/4) の直後 (改行直後語頭・コンマ直後語頭等)
            // では prev_tone=4 となり、ドライバ平滑化 (下記) は発火しない
            // (実測: REQ17 정=0x1E・REQ194 상=0x28・REQ256 콤=0x28 — t18 確定)。
            // 従来は全レコードを 1 連続列として扱い、直前選択ユニットの調値
            // (1/3) で 0x1E→0x0A・0x28→0x1E に誤低減していた。
            if idx > 0 && records[idx - 1].tone_class % 10 >= 3 {
                self.prev_tone = 4;
                self.prev_best_class_hi = 0;
            }
            let mut class = rec.tone_class;
            // サンディ (ドライバ内): 直前調値がレベルに繰り上がり
            let pt = (self.prev_tone % 10) as i32;
            if pt > 0 && (pt as u8) < class / 10 {
                class = class % 10 + (pt as u8) * 10;
            }
            let level = class / 10;
            let tone = class % 10;

            let mut req = UnitRequest {
                cur: rec.code,
                // 前コード: 先頭でなければ直前レコード+2、レベル<2 のときのみ。
                // 文頭・レベル≥2 では境界コード 0x6EB3 (DAT_00499234) を使う
                // (FUN_0044b880 @0x44b924-0x44b947; 旧実装の 0 は誤り — t13)。
                prev: if idx != 0 && level < 2 {
                    records[idx - 1].code
                } else {
                    BOUNDARY_CODE
                },
                // 次コード: 末尾・文末レコード・調値>1 なら 0x6EB3
                // (FUN_0044b880 @0x44b954-0x44b984: idx == count-1 → 境界)。
                // 文末 (marker==1) はバッファ末尾と同じ扱い。
                next: if idx + 1 < records.len()
                    && rec.marker != MARKER_SENTENCE_END
                    && tone <= 1
                {
                    records[idx + 1].code
                } else {
                    BOUNDARY_CODE
                },
                // ピッチ: レベル≥2 → 90 (this+0xe4)、それ以外 → 直前選択エントリのピッチ
                pitch: if level >= 2 {
                    self.cfg.request_pitch_default
                } else {
                    self.prev_best_pitch
                },
                class,
                flags: 0,
            };

            // 要求フラグ byte+0x15 (休止判定 + bit7)
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
            if rec.flag == 1 {
                flags |= 0x80;
            }
            req.flags = flags;

            // 全件走査
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

            // ---- フォールバック (未ヒット時) ----
            if hit.is_none() {
                let mid5 = ((req.cur >> 5) & 0x1f) as usize;
                if FALLBACK_ALLOW[mid5] == 0 {
                    // 変種A: 下位5bit クラステーブルで中位5bit を置換して再検索 (+10000)
                    let repl = (FALLBACK_REPL_LO[mid5] as u16) & 0x1f;
                    let mut req_a = req;
                    req_a.cur = (req_a.cur & 0xfc1f) | (repl << 5);
                    hit = self.scan(&req_a, true);
                    if hit.is_some() {
                        req = req_a;
                        marker_base = 10000;
                    } else {
                        // 変種B: 上位5bit 置換 (0x6c00 系) の 2 段検索 (+20000)
                        let mid5b = ((req_a.cur >> 5) & 0x1f) as usize;
                        if FALLBACK_ALLOW[mid5b] == 0 || (req_a.cur & 0xfc00) != 0x6c00 {
                            let copy = req_a;
                            let b = (FALLBACK_REPL_HI[mid5b] as u16) & 0x1f;
                            // 上位変種コード: ((REPL_HI|0x360)<<5)|low5
                            let cur_hi = (((b | 0x360) << 5) | (req_a.cur & 0x1f)) as u16;
                            // 下位変種コード: (REPL_LO<<5)|(cur&0xfc12)
                            let lo = (((FALLBACK_REPL_LO[((copy.cur >> 5) & 0x1f) as usize]
                                as u16)
                                & 0x1f)
                                << 5)
                                | (copy.cur & 0xfc12);
                            // 検索1: cur=上位変種, next=下位変種|0x12, クラス=レベル×10, flags=(flags/10)*10+1
                            let mut req_b1 = req_a;
                            req_b1.cur = cur_hi;
                            req_b1.next = lo | 0x12;
                            req_b1.pitch = 0; // オリジナルは要求+0x16 (未初期化) を使用
                            req_b1.class = (copy.class / 10) * 10;
                            req_b1.flags = (copy.flags / 10) * 10 + 1;
                            hit = self.scan(&req_b1, true);
                            if hit.is_some() {
                                req = req_b1;
                                marker_base = 20000;
                            } else {
                                // 検索2: cur=下位変種|0x12, prev=上位変種, next=元の next
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
                // ドライバ状態更新 (local_6d / 要求ピッチ / 特殊レコード用クラス高位)
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

        // ---- 2. ピッチ平滑化 (3 連ユニット) ----
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
                    // 通常: 前後差がすべて許容差以上かつ中央が外れ値 (スパイク) なら平均に置換
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
                    // bit7 特殊ユニット: 前ピッチより 10 以上低い/許容差 15 以上高いとき平均に置換
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

        // ---- 3. 継続時間割当 (クラス%10 → 1000/3000/5000/20000) ----
        for u in &mut units {
            let d = duration_for(
                u.request.class,
                &self.cfg.duration_values,
                &self.cfg.duration_enabled,
            );
            set_active_pause(u, d as i16);
        }

        // ---- 4. 追加ユニット + 句読点 pause + 総サンプル数 ----
        // オリジナル FUN_0044b880 (総サンプル数ループ, ディスアセンブリ 0x44c22b/0x44c295 確認):
        // 句読点 pause は選択レコード +0x18 に「加算」される (継続時間割当の duration の上に
        // 加算)。FUN_0044c2e0 は +0x18 > 0 のときその値分ゼロを挿入するため、
        // ゼロ区間 = duration + 句読点 pause の合計となる (実測: 1000/1500/2000/2500/
        // 3000/5000/20000/21000 samples)。
        let mut total: i64 = 0;
        for u in &mut units {
            let req = u.request;
            let d = u.active_data();
            let cur_hi = req.cur >> 10;
            let next_lo = req.next & 0x1f;
            total += d.wlen as i64 * 2;

            // 追加ユニット (2重化): 実音素かつ調値<2 → FUN_0044b220 でピッチ近接の特殊ユニット
            if is_real_phoneme(cur_hi, next_lo) && d.class_byte() % 10 < 2 {
                if let Some(extra) = self.scan_special(d.pitch as i16) {
                    u.extra = Some(extra);
                    total += extra.wlen as i64 * 2;
                }
            }

            // 句読点 pause 加算 (+0x18 に加算 — ゼロ挿入に反映される)
            let mut pause = d.pause as i32;
            if cur_hi == 0 || cur_hi == 5 || cur_hi == 0xf {
                if !(cur_hi == 5 && next_lo == 0x10) {
                    pause += 1000;
                }
            }
            let cls = req.class;
            if next_lo == 8 || next_lo == 9 || next_lo == 10 || next_lo == 0xb {
                pause += 1000;
            } else if next_lo == 0xd
                || next_lo == 0xe
                || next_lo == 0xf
                || next_lo == 0x11
                || ((cur_hi == 0 || cur_hi == 5 || cur_hi == 0xf) && (cls / 10 > 0 && cls % 10 > 0))
            {
                pause += 1500;
            }
            set_active_pause(u, pause as i16);
            total += pause as i64 * 2;
        }

        ProcessedUnits {
            units,
            total_samples: total,
        }
    }
}

/// 実データ側 (data2 があれば data2) の pause フィールドを更新。
fn set_active_pause(u: &mut UnitSelection, pause: i16) {
    if u.data2.is_some() {
        if let Some(d2) = u.data2.as_mut() {
            d2.pause = pause;
        }
    } else {
        u.data.pause = pause;
    }
}

// ---------------------------------------------------------------------------
// 文脈重み (a800 local_22/local_21)
// ---------------------------------------------------------------------------

/// 左文脈重み w_prev (local_22): 5/3/1/2。
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

/// 右文脈重み w_next (local_21): 5/3/1/2。
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
        // 上位10bit ∈ {2,0xe,0x12,0x1b} かつ下位5bit ∈ {1,4,0x12}
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
        assert!(!is_real_phoneme(6, 1)); // lo=1 → 非実音素
        assert!(!is_real_phoneme(27, 6)); // lo=6 → 非実音素
        assert!(!is_real_phoneme(0, 0x11));
        assert!(is_real_phoneme(27, 5)); // lo=5 は実音素
        assert!(is_real_phoneme(6, 2)); // lo=2 は実音素 (lo==3 && hi==6 のみ非実音素)
    }

    #[test]
    fn class_normalization() {
        // 対象側: レベル2 → 30+調値
        assert_eq!(normalize_target_class(0x14), 30); // 20 → 30
        assert_eq!(normalize_target_class(0x15), 31); // 21 → 31
                                                      // 調値2 → +3 (対象) / +1 (候補a)
        assert_eq!(normalize_target_class(0x02), 3); // 2 → 3
        assert_eq!(normalize_candidate_class_a(0x02), 1); // 2 → 1
        assert_eq!(normalize_candidate_class_b(0x02), 3);
        // 調値5 → +4 (対象のみ)
        assert_eq!(normalize_target_class(0x05), 4);
        assert_eq!(normalize_candidate_class_a(0x05), 5);
        // 通常クラス
        assert_eq!(normalize_target_class(0x28), 40);
        assert_eq!(normalize_target_class(0x01), 1);
        assert_eq!(normalize_target_class(0x0a), 10);
        // 行ヘッダ検索 (各行の先頭要素 = クラス → 行 index)
        assert_eq!(tone_class_row(40), 0);
        assert_eq!(tone_class_row(4), 1);
        assert_eq!(tone_class_row(30), 2);
        assert_eq!(tone_class_row(3), 3);
        assert_eq!(tone_class_row(10), 4); // 行15 にも 10 があるが先頭一致で行4
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
        // 0 → 行15 (行15 のヘッダは 0)
        assert_eq!(tone_class_row(0), 15);
    }

    #[test]
    fn tone_cost_indexing() {
        // 対象 40 (行0) 内で候補 30 (列1) → cost[0*16+1] = 595
        assert_eq!(tone_class_col(0, 30), 1);
        assert_eq!(TONE_TRANS_COST[1], 595);
        // 行15 (44) 候補 25 は存在しない → クランプ 15
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
        // オリジナル ctor の有効フラグ 0/1/1/1 ではクラス1 が無効
        let orig = [false, true, true, true];
        assert_eq!(
            duration_for(1, &cfg.duration_values, &orig),
            0,
            "original enable flags disable class 1"
        );
    }

    #[test]
    fn flag_d_overflow_reads_tone_map() {
        // hi10 ≥ 32 は TONE_CLASS_MAP バイト列を LE i32 として読む (フラットメモリ挙動)
        let expect = i32::from_le_bytes([
            TONE_CLASS_MAP[0],
            TONE_CLASS_MAP[1],
            TONE_CLASS_MAP[2],
            TONE_CLASS_MAP[3],
        ]);
        assert_eq!(flag_d(32), expect);
        // 32 未満は直接テーブル
        assert_eq!(flag_d(27), PHON_CLASS_FLAG_D[27]);
        assert_eq!(flag_d(0), PHON_CLASS_FLAG_D[0]);
    }

    #[test]
    fn weight_computation() {
        // w_prev: 特殊コード (prev 上位 ∈ {0x12,0x1b} && cur 下位 0x12 && レベル<2) → 5
        let r = req(0x6c12, 0x6c12, 0, 90, 0x03, 0);
        assert_eq!(weight_prev(&r, normalize_target_class(r.class)), 5);
        // w_next: 通常 → 2 (cur 上位10bit の FLAG_D と next 下位5bit の FLAG_C が非 0 必要)
        // 0x4801: hi10=18 (FLAG_D[18]=1), lo=1 (FLAG_C[1]=1)
        let r2 = req(0x4801, 0x4801, 0x6d81, 90, 0x28, 0);
        assert_eq!(weight_next(&r2, normalize_target_class(r2.class)), 2);
        // w_prev も同条件 (prev 上位10bit の FLAG_D 非 0)
        assert_eq!(weight_prev(&r2, normalize_target_class(r2.class)), 2);
        // FLAG_D[hi10]==0 のコード (例 0x6d86: hi10=27) → 重み 1
        let r3 = req(0x6d86, 0x6eb3, 0x6d81, 90, 0x28, 0);
        assert_eq!(weight_next(&r3, normalize_target_class(r3.class)), 1);
    }

    #[test]
    fn context_score_values() {
        // 完全一致 (search_flag=true): 100
        assert_eq!(score_left(0x6eb3, 0x6d86, 0x6eb3, 0x6d86, 2, 40, true), 100);
        // search_flag=false → 20
        assert_eq!(score_left(0x6eb3, 0x6d86, 0x6eb3, 0x6d86, 2, 40, false), 20);
        // 下位10bit 一致: 0xffe0 マスク (上位6bit 一致)
        assert_eq!(score_left(0x6eb3, 0x6d86, 0x6eb2, 0x6d86, 2, 40, true), 90);
        // 特殊コード (entry.prev 上位 ∈ {0x12,0x1b}, entry.cur 下位 0x12/0xc, レベル<1) → -200
        assert_eq!(
            score_left(0x6c12, 0x6c12, 0x6c12, 0x6c12, 2, 0x03, true),
            -200
        );
        // w=3 完全一致 → 100 (flag) / 60 (no flag)
        assert_eq!(score_left(0x6eb3, 0x6d86, 0x6eb3, 0x6d86, 3, 40, true), 100);
        assert_eq!(score_left(0x6eb3, 0x6d86, 0x6eb3, 0x6d86, 3, 40, false), 60);
        // 右文脈: 完全一致 100
        assert_eq!(
            score_right(0x6d80, 0x6d86, 0x6d80, 0x6d86, 2, 40, true),
            100
        );
        // 下位10bit 一致 ((^)&0x3ff==0: 上位6bit のみ相違) → 90
        assert_eq!(score_right(0x6d80, 0x6d86, 0x4d80, 0x6d86, 2, 40, true), 90);
        // 下位5bit 一致 → 60
        assert_eq!(score_right(0x6d80, 0x6d86, 0x6c80, 0x6d86, 2, 40, true), 60);
        // w=3: 下位5bit 一致 + REPL_HI 不一致 → 50
        assert_eq!(score_right(0x6d80, 0x6d86, 0x6c80, 0x6d86, 3, 40, true), 50);
    }
}
