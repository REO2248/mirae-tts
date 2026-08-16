//! 辞書 pkg (ダブル配列トライ + TAIL + サブ構造) — SPEC_tts_rewrite.md §1.3 / T2_dict.md 準拠
//!
//! オリジナル Future.exe の解析 (FUN_004113f0 / FUN_00410c50 / FUN_00410da0 / FUN_004115c0 /
//! FUN_00411190 / FUN_004116d0 / FUN_00411790 / FUN_00411990 / FUN_004119d0 / FUN_00411840 /
//! FUN_0040a930 / FUN_0040a470 / FUN_00409f40) からのみ実装。既存 mirae-tts は参照しない。
//!
//! ## ファイル形式 (全 5 pkg で `8 + 5n1 + n2 + 8 + 8b2 + 6c2 + 8 + 8b3 + 26c3 == ファイルサイズ` が成立)
//!
//! ```text
//! 0x00 u32 n1   トライ節点数 (= BASE 要素数 = CHECK バイト数)
//! 0x04 u32 n2   TAIL バイト数
//! 0x08 u32×n1   BASE 配列 (ダブル配列トライ)
//!      u8×n1    CHECK 配列
//!      u8×n2    TAIL (サフィックス/エントリ格納域)
//!      u32 c2   サブ構造A レコード件数
//!      u32 b2   サブ構造A ペア数
//!      (u32,u32)×b2 ペア表
//!      6B×c2    サブ構造A レコード [u8 種別(0x80=ラン終端)][u8 副][u16 値][u16 値]
//!      u32 c3   サブ構造B レコード件数
//!      u32 b3   サブ構造B ペア数
//!      (u32,u32)×b3 ペア表
//!      26B×c3   サブ構造B レコード
//! ```
//!
//! ## ダブル配列トライ + TAIL (FUN_004115c0 / FUN_00411190 から確定)
//!
//! - 遷移: `t = BASE[node] + c`、検証: `t < n1 && CHECK[t] == c`
//! - 終端: `BASE[node] < 0` ⇒ `-BASE[node]` = TAIL 内オフセット
//! - 開始: node = 1 (番兵、BASE[1] = CHECK[1] = 1)
//! - 検索は必ずキー末尾に 'P' (0x50, 単語終端マーカ) を付加して行う
//! - 終端ノード到達時、最後に消費した文字が 'P' なら即 `-BASE[node]` を返す。
//!   'P' 以外なら TAIL 文字列と入力の残りを比較し、一致で `-BASE[node]` を返す
//!   (FUN_004115c0 は strcmp 相当の完全一致、FUN_00411190 は strstr 相当の前置一致)。
//! - FUN_00411190 はさらに「遷移失敗文字を 'P' に置換して終端遷移を再試行」する
//!   最長一致 (プレフィクス検索) の挙動を持つ。
//!
//! ## TAIL エントリ形式
//!
//! `[suffix 文字列 (NUL 終端)] [u16 X] [u8 Y]` — X = サブ構造A レコード index (Y は種別フラグ)。
//! 完全一致で得られたオフセットから `tail_entry()` で取り出す (FUN_004116d0 相当)。
//!
//! ## キー文字コード
//!
//! - 初声 1..19 → 0x01..0x13 / 中声 1..21 → 0x14..0x28 / 終声 1..27 → 0x29..0x43 / 終端 0x50
//! - u16 音節コード (bits0-4 終声, bits5-9 中声, bits10-14 初声) → キー文字列は
//!   `syllable_to_key` / `key_from_syllables` (FUN_0040a930 相当)
//! - 特殊文字 (bit15): 数字 0x30..0x39 → 0x46..0x4F、'-' → 'E'、'.' → 'D' (FUN_0040a470 相当)
//! - NonReg.pkg はキーを反転して格納 (FUN_00409f40 相当: `reverse_key`)

use std::fmt;
use std::fs;
use std::path::Path;

/// 単語終端マーカ文字 'P'。
pub const KEY_END: u8 = 0x50;

/// サブ構造A の 6 バイトレコード。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubARecord {
    /// 種別コード。bit7 (0x80) = ラン終端フラグ (展開の先頭レコードのみ & 0x7F でマスクされる)。
    pub kind: u8,
    /// 副コード。
    pub sub: u8,
    /// 値 1。
    pub v0: u16,
    /// 値 2。
    pub v1: u16,
}

/// TAIL エントリの値 (FUN_004116d0 の `X | (Y << 16)`)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TailEntry {
    /// サブ構造A のレコード index (Alphabet では音節クラス 1..7、Conjects では Connect ブロブ index)。
    pub x: u16,
    /// 種別フラグ。
    pub y: u8,
}

impl TailEntry {
    /// オリジナルの戻り値形式 `X | (Y << 16)`。
    pub fn value(self) -> u32 {
        (self.x as u32) | ((self.y as u32) << 16)
    }
}

/// プレフィクス/最長一致検索の結果 (FUN_00411190)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrefixMatch {
    /// 終端ノードの -BASE = TAIL 内オフセット。
    pub tail_offset: usize,
    /// 入力キー (付加 'P' を除く) のうちマッチした文字数。
    pub matched: usize,
}

/// 辞書 pkg のパース/読み込みエラー。
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

/// サブ構造 (A: 6B レコード / B: 26B レコード)。
#[derive(Debug, Default)]
struct SubStruct {
    /// ペア表 (u32, u32)。
    pairs: Vec<(u32, u32)>,
    /// レコード列 (レコードサイズは `rec_size` 固定)。
    records: Vec<u8>,
    /// 1 レコードあたりのバイト数。
    rec_size: usize,
}

/// 辞書 pkg。ダブル配列トライ (BASE/CHECK) + TAIL + サブ構造A/B を保持する。
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
    /// pkg ファイルを読み込んでパースする。ファイル末尾まで完全に消費できなければエラー。
    pub fn load(path: impl AsRef<Path>) -> Result<Dict, DictError> {
        let data = fs::read(path).map_err(|e| DictError::new(format!("read: {e}")))?;
        Dict::from_bytes(&data)
    }

    /// バイト列からパースする。`8 + 5n1 + n2 + 8 + 8b2 + 6c2 + 8 + 8b3 + 26c3 == data.len()`
    /// が成立すること (全 5 pkg で検証済み)。
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
        // サブ構造A (6B レコード)
        let (sub_a, o) = parse_sub(data, o, 6)?;
        // サブ構造B (26B レコード)
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

    /// トライ節点数 (BASE 要素数 = CHECK バイト数)。
    pub fn n1(&self) -> usize {
        self.n1
    }

    /// TAIL バイト数。
    pub fn n2(&self) -> usize {
        self.n2
    }

    /// サブ構造A のレコード件数 (c2)。
    pub fn sub_a_count(&self) -> usize {
        self.sub_a.records.len() / self.sub_a.rec_size
    }

    /// サブ構造B のレコード件数 (c3)。
    pub fn sub_b_count(&self) -> usize {
        self.sub_b.records.len() / self.sub_b.rec_size
    }

    /// サブ構造A のペア表 (b2)。
    pub fn sub_a_pairs(&self) -> &[(u32, u32)] {
        &self.sub_a.pairs
    }

    /// サブ構造B のペア表 (b3)。
    pub fn sub_b_pairs(&self) -> &[(u32, u32)] {
        &self.sub_b.pairs
    }

    /// BASE 配列の要素。範囲外は None。
    pub fn base(&self, node: usize) -> Option<i32> {
        self.base.get(node).copied()
    }

    /// CHECK 配列の要素。範囲外は None。
    pub fn check(&self, node: usize) -> Option<u8> {
        self.check.get(node).copied()
    }

    /// TAIL の 1 バイト。範囲外は None。
    pub fn tail(&self, off: usize) -> Option<u8> {
        self.tail.get(off).copied()
    }

    /// TAIL 全体 (テスト/検査用)。
    pub fn tail_bytes(&self) -> &[u8] {
        &self.tail
    }

    /// 完全一致トライ検索 (FUN_004115c0 相当)。
    ///
    /// `key` は 'P' を含まないキー文字列 (音節コード列)。内部で末尾に 'P' (0x50) を
    /// 付加して探索し、終端ノードの `-BASE` = TAIL オフセットを返す。見つからなければ None。
    pub fn search_exact(&self, key: &[u8]) -> Option<usize> {
        if key.is_empty() {
            return None;
        }
        let mut node: i32 = 1;
        let mut consumed: usize = 0; // 消費済み文字数 (付加 'P' 含む)
        loop {
            let c = if consumed < key.len() {
                key[consumed]
            } else {
                KEY_END
            };
            let t = self.base[node as usize].wrapping_add(c as i32);
            if t < 0 || t as usize >= self.n1 || self.check[t as usize] != c {
                return None; // 遷移失敗 = 完全一致なし
            }
            node = t;
            consumed += 1;
            if self.base[node as usize] < 0 {
                break; // 終端ノード到達
            }
            if consumed > key.len() {
                return None; // 付加 'P' を消費しても終端でない = キーなし
            }
        }
        let off = (-self.base[node as usize]) as usize;
        if off >= self.n2 {
            return None;
        }
        if consumed == key.len() + 1 {
            // 最後に消費した文字が 'P' → TAIL サフィックスは空 → 即返却
            return Some(off);
        }
        // 残り入力 (key[consumed..] + 'P') と TAIL 文字列を完全比較 (strcmp 相当)
        let tail = self.tail_string(off)?;
        let k = &key[consumed..];
        if tail.len() != k.len() + 1 || tail[..k.len()] != *k || tail[k.len()] != KEY_END {
            return None;
        }
        Some(off)
    }

    /// プレフィクス/最長一致検索 (FUN_00411190 相当)。
    ///
    /// 遷移失敗時、失敗した文字を 'P' に置換して終端遷移を再試行する。終端ノード到達後は
    /// TAIL 文字列が残り入力の前置 (strstr 相当) であればマッチ。`matched` は入力キー
    /// ('P' 付加前) のうちマッチした文字数。NonReg 用 (FUN_00411840 → FUN_00444fb0)。
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
                // 遷移失敗 → 現在文字を 'P' に置換して終端遷移を試行
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
                break; // 終端ノード到達
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
            // 最後の消費文字が 'P' → キー全体がトライ内 (TAIL サフィックス空)
            return Some(PrefixMatch {
                tail_offset: off,
                matched: key.len(),
            });
        }
        // strstr 相当: TAIL 文字列が残り入力 (key[consumed..] + 'P') の前置であること
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

    /// TAIL[off] の NUL 終端文字列 (FUN_00410a90 相当)。NUL が見つからなければ None。
    pub fn tail_string(&self, off: usize) -> Option<&[u8]> {
        if off >= self.n2 {
            return None;
        }
        let end = self.tail[off..].iter().position(|&b| b == 0)?;
        Some(&self.tail[off..off + end])
    }

    /// TAIL エントリ [suffix][NUL][u16 X][u8 Y] の X/Y を読む (FUN_004116d0 相当)。
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

    /// サブ構造A のレコード 1 件 (index 指定)。範囲外は None。
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

    /// サブ構造A レコードを index `idx` から展開 (FUN_00411790 相当)。
    ///
    /// 先頭レコードは種別を `& 0x7F` でマスクして必ず含む。以降は生のレコードを
    /// コピーし、bit7 (0x80) が立ったレコードをコピーした時点で停止 (そのレコードも含む)。
    /// `idx` が件数以上なら空。オリジナルの 107 バイトスロット展開 (0x6B 刻み) を
    /// レコード列として表現したもの。
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
        first.kind &= 0x7f; // 先頭レコードのみマスク (FUN_00411790)
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

    /// 完全一致 → TAIL エントリ値 (FUN_004119d0 相当)。Conjects 用。
    pub fn lookup(&self, key: &[u8]) -> Option<TailEntry> {
        let off = self.search_exact(key)?;
        self.tail_entry(off)
    }

    /// 完全一致 → エントリの X → サブ構造A レコード展開 (FUN_00411990 相当)。
    /// colligation/User 用。サブ構造A を持たない pkg (Alphabet/Conjects) では
    /// 見つかっても空のレコード列を返す。
    pub fn lookup_records(&self, key: &[u8]) -> Option<Vec<SubARecord>> {
        let off = self.search_exact(key)?;
        let e = self.tail_entry(off)?;
        Some(self.expand_records(e.x as usize))
    }

    /// プレフィクス検索 → エントリの X → サブ構造A レコード展開 (FUN_00411840 相当)。
    /// NonReg 用 (FUN_00444fb0)。
    pub fn lookup_prefix_records(&self, key: &[u8]) -> Option<(PrefixMatch, Vec<SubARecord>)> {
        let m = self.search_prefix(key)?;
        let e = self.tail_entry(m.tail_offset)?;
        Some((m, self.expand_records(e.x as usize)))
    }
}

/// u16 音節コード 1 個をキー文字列へ変換して `out` に追加 (FUN_0040a930 相当)。
///
/// - 通常音節: 初声 (bits10-14) → 0x01..0x13、中声 (bits5-9) → 0x14..0x28、
///   終声 (bits0-4) → 0x29..0x43。0 の成分は出力しない。
/// - 特殊文字 (bit15): 数字 '0'-'9' (0x30..0x39) → 0x46..0x4F、'-' → 0x45 ('E')、
///   '.' → 0x44 ('D')。それ以外は false を返し `out` を変更しない (FUN_0040a470 相当)。
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
    let initial = (code >> 10) & 0x1f; // 初声 1..19
    let medial = (code >> 5) & 0x1f; // 中声 1..21
    let final_c = code & 0x1f; // 終声 1..27
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

/// u16 音節コード列 → キー文字列 (FUN_0040a930 相当)。変換不能な特殊文字があれば None。
pub fn key_from_syllables(codes: &[u16]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(codes.len() * 3);
    for &c in codes {
        if !syllable_to_key(c, &mut out) {
            return None;
        }
    }
    Some(out)
}

/// キー文字列の反転 (FUN_00409f40 相当)。NonReg.pkg はキーを反転して格納するため、
/// 検索前に適用する (SPEC D14)。
pub fn reverse_key(key: &[u8]) -> Vec<u8> {
    key.iter().rev().copied().collect()
}

fn need(data: &[u8], o: usize, n: usize) -> Result<(), DictError> {
    if o.checked_add(n).map_or(true, |end| end > data.len()) {
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

/// サブ構造ヘッダ [u32 count][u32 npairs][(u32,u32)×npairs] + レコード列 (rec_size × count) を読む。
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
