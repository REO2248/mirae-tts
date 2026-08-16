//! 波形接続・チャンク生成 (FUN_0044c2e0 + FUN_0044b700 相当)
//!
//! ユニット毎に VoiceData.pkg から波形を読み出して出力へ連結する:
//! 1. 主ユニット波形をコピー (ランダムモード時はピッチ比例の再構成)
//! 2. **2重化**: 実音素 (FUN_0044b350) かつ調値<2 かつ追加ユニットあり → 追加ユニット波形を連結
//! 3. 継続時間/句読点 pause (record+0x18) > 0 → 無音 (ゼロ) を挿入
//!
//! チャンク受け渡しは 20 スロット × 16B リングバッファ (engine+0xf8) + 総量 1MB 制限
//! (FUN_0044b430 / FUN_0044b3e0 / FUN_0044b3c0 / FUN_0044b4b0 / FUN_0044b510 相当)。

use std::io;

use crate::voice_data::{VoiceData, SCRATCH_SIZE};
use crate::{RING_MAX_BYTES, RING_SLOTS};

/// 28B VoiceInfo エントリのうち波形接続に必要なフィールド。
///
/// レイアウト対応 (T1 §2 / FUN_0044c2e0 ディスアセンブリ):
/// - `woff`      = +0x08 (サンプルオフセット)
/// - `wlen`      = +0x0c (サンプル数)
/// - `pitch`     = +0x12 (i16 ピッチ特徴量)
/// - `classcode` = +0x14 の下位バイト (音素クラスコード)
/// - `pause`     = +0x18 (i16 継続時間/句読点 pause、サンプル数)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnitRecord {
    pub woff: u32,
    pub wlen: u32,
    pub pitch: i16,
    pub classcode: u8,
    pub pause: i16,
}

impl UnitRecord {
    /// woff/wlen のみ指定して生成 (他フィールド 0)。
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

/// 波形接続 1 ユニット分の要求。
///
/// オリジナルのリストノード対応:
/// - `code_cur` / `code_next` = ノード+4 のキー (28B 要求コンテキスト) の u16[0] / u16[2]。
///   FUN_0044c2e0 は実音素判定 `FUN_0044b350(code_cur>>10, code_next&0x1f)` をキーから行う。
/// - `record` = FUN_0044b320 が選択した有効レコード (ノード+8 または +0xc)。
/// - `extra`  = ノード+0x10 の追加ユニット (28B 構造)。判定は `[+0x12](pitch) != 0` で
///   行われるため、pitch==0 のエントリは None 扱いとする。
#[derive(Clone, Copy, Debug, Default)]
pub struct RenderUnit {
    pub code_cur: u16,
    pub code_next: u16,
    pub record: UnitRecord,
    pub extra: Option<UnitRecord>,
}

/// FUN_0044b350 相当 — 実音素判定。
///
/// オリジナルの呼び出し: `FUN_0044b350(param_1 = 現コード>>10, param_2 = 次コード&0x1f)`。
/// デコンパイル (t4_dump4_out.txt):
/// ```c
/// if (param_2 != 1 && param_2 != 4 && param_2 != 6 && param_2 != 0x10 && param_2 != 0xc &&
///     param_2 != 0x12 && param_2 != 8 && param_2 != 9 && param_2 != 10 && param_2 != 0xb &&
///     param_2 != 0xd && param_2 != 0xe && param_2 != 0x11 &&
///     (param_2 != 3 || param_1 != 6)) return 1;   // 実音素
/// return 0;                                       // 休止/句読点
/// ```
pub fn is_real_phoneme(high6_cur: u16, low5_next: u16) -> bool {
    !(matches!(
        low5_next,
        1 | 4 | 6 | 8 | 9 | 10 | 0xb | 0xc | 0xd | 0xe | 0x10 | 0x11 | 0x12
    ) || (low5_next == 3 && high6_cur == 6))
}

/// 2 コード (現・次) から実音素判定する便利ラッパ。
pub fn is_real_phoneme_codes(cur: u16, next: u16) -> bool {
    is_real_phoneme(cur >> 10, next & 0x1f)
}

/// 波形接続: ユニット列を VoiceData から読み出して `out` へ追記する。
///
/// FUN_0044c2e0 の処理を忠実に再現:
/// 1. 主ユニット: `FUN_0044b700` で 64,000B スクラッチへ読み出し、そのままコピー
///    (ランダムモード `random_mode=true` 時はピッチ比例の再構成 — 近似実装)
/// 2. 2重化: 実音素 && `(classcode as i8) % 10 < 2` && 追加ユニットあり → 追加波形を連結
///    (追加ユニットのコピーは常に単純コピー — オリジナル 0x44c484 の memcpy 相当)
/// 3. `record.pause > 0` → pause×2 バイトのゼロ (無音) を挿入
///
/// 戻り値: 追記した合計バイト数。
pub fn render_units(
    data: &mut VoiceData,
    units: &[RenderUnit],
    out: &mut Vec<u8>,
    random_mode: bool,
) -> io::Result<usize> {
    let mut scratch = vec![0u8; SCRATCH_SIZE];
    let mut total = 0usize;

    for u in units {
        // --- 1. 主ユニット読み出し + コピー ---
        let n = data.read_unit(u.record.woff, u.record.wlen, &mut scratch)?;
        if random_mode {
            // ランダムモード (engine+0xdc != 0): ピッチ (record+0x12) に比例したサンプル数へ
            // 再構成する簡易実装。オリジナルは FUN_0045b8d0 (RNG) でインデックスを揺らす
            // ループ (0x44c3c0–0x44c3ec) のため完全一致はしない近似。
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

        // --- 2. 2重化: 実音素 && 調値<2 && 追加ユニットあり ---
        // オリジナル 0x44c436–0x44c46f:
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
                            extra.woff, extra.wlen, extra.pitch, u.record.classcode, u.code_cur, u.code_next
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

        // --- 3. 継続時間/句読点 pause (>0) → 無音挿入 ---
        if u.record.pause > 0 {
            let silence = (u.record.pause as usize) * 2;
            out.resize(out.len() + silence, 0);
            total += silence;
        }
    }

    Ok(total)
}

/// 生成チャンク (リングスロット 16B = ptr, size, p1, p2 相当)。
#[derive(Debug, Clone)]
pub struct Chunk {
    /// PCM バイト列 (ptr/size に相当)。
    pub data: Vec<u8>,
    /// 付随情報 1 (オリジナル p1)。
    pub p1: u32,
    /// 付随情報 2 (オリジナル p2)。
    pub p2: u32,
}

impl Chunk {
    pub fn new(data: Vec<u8>) -> Self {
        Chunk { data, p1: 0, p2: 0 }
    }
}

/// チャンクリングバッファ — 20 スロット (engine+0xf8) + 総量 1MB 制限。
///
/// - `push`  = FUN_0044b430 (tail スロットへ書込、総量 +0x23c 加算)
/// - `can_push` = FUN_0044b3e0 (リング full または総量 > 0xFFFFF で生成待ち)
/// - `is_empty` = FUN_0044b3c0 (head==tail ならチャンク無し)
/// - `pop`   = FUN_0044b4b0 + FUN_0044b510 (head スロット読出 + 解放・前進)
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

    /// FUN_0044b3e0 相当: リング full または総量+size > 1MB (0xFFFFF) なら false (生成待ち)。
    pub fn can_push(&self, size: usize) -> bool {
        let full = (self.tail + 1) % RING_SLOTS == self.head;
        !full && self.total + size <= RING_MAX_BYTES
    }

    /// FUN_0044b430 相当: tail スロットへ書込、総量加算。full/1MB 超過時は false。
    pub fn push(&mut self, chunk: Chunk) -> bool {
        if !self.can_push(chunk.data.len()) {
            return false;
        }
        self.total += chunk.data.len();
        self.slots[self.tail] = Some(chunk);
        self.tail = (self.tail + 1) % RING_SLOTS;
        true
    }

    /// FUN_0044b3c0 相当: head==tail (チャンク無し)。
    pub fn is_empty(&self) -> bool {
        self.head == self.tail
    }

    /// 現在のチャンク数。
    pub fn len(&self) -> usize {
        if self.tail >= self.head {
            self.tail - self.head
        } else {
            RING_SLOTS - self.head + self.tail
        }
    }

    /// バッファ内の総バイト数 (1MB 制限の監視用)。
    pub fn total_bytes(&self) -> usize {
        self.total
    }

    /// FUN_0044b4b0 + FUN_0044b510 相当: head スロットを読み出して解放・前進。
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

/// プロデューサ: ユニット列を `per_chunk` 個ずつのチャンクにまとめてリングへ投入する。
///
/// リングが full / 1MB 超過の場合は投入を諦めず、`consume` で空きを作りながら進める
/// (オリジナルの FUN_0044b3e0 待機ループ相当をコールバックで表現)。
/// 戻り値: 投入したチャンク数。
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
                // リング full / 1MB 超過: コンシューマに空きを作らせる (生成待ち相当)
                consume(ring);
                // リングが空でも 1MB 制限で入らない巨大チャンクは諦める
                // (オリジナルは 1 チャンクが 1MB を超えない前提)
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
