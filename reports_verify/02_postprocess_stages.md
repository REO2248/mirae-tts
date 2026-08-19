# 後処理 stage 群 整合検証 — 02_postprocess_stages

**対象**: `mirae-tts-engine/src/g2p.rs`, `postprocess_tables.rs` / Future.exe file offsets / HEAD=69f97bb (親 2d45d8d)
**検証日**: 2026-08-19
**検証者**: subagent (binary ground truth: `/tmp/mirae-oracle-fixed/Future.exe`)

---

## 1. PROSODY / CHUNK / PROPAGATE 定数の file offset 照合

### 1.1 バイナリ実測 (PE file offset / VA)

PE `ImageBase=0x00400000`, `.data` は `raw_ptr=0x00089000 == VA 0x00089000` で file==VA。

```
file 0x89168  VA 0x00489168  u32=5          -> PROPAGATE_BACK
file 0x8916c  VA 0x0048916c  u32=0          -> PROPAGATE_FORWARD (padding)
file 0x89170  VA 0x00489170  u32=60 0x3c    -> CHUNK_SYLLABLES
file 0x89174  VA 0x00489174  f32=0.5 0x3f000000 -> PROSODY_W1
file 0x89178  VA 0x00489178  f32=0.5 0x3f000000 -> PROSODY_W2 (reserved)
file 0x8917c  VA 0x0048917c  f32=0.95 0x3f733333 -> PROSODY_W3
file 0x89180  VA 0x00489180  f32=2.85 0x40366666 -> (ACCENT_RANGE hi? 要調査)
file 0x89184  VA 0x00489184  f32=1.8  0x3fe66666 -> (ACCENT_RANGE lo? 要調査)
```

`0x8917c` の `0x3f733333` は 0.95 に一致し、コード注記 `DAT_0048917c=0x3f733333 verified` は正しい。
周辺 float 初出は `0x8917c` のみ (`0x88000-0x8a000` で 0.95 はこの1件) で一意。

### 1.2 コード対応 (`g2p.rs:24-36`)

```rust
// g2p.rs:24-36
pub const CHUNK_SYLLABLES: usize = 60;
pub const PROPAGATE_FORWARD: u8 = 0;
pub const PROPAGATE_BACK: usize = 5;
pub const PROSODY_W1: f32 = 0.5;
#[allow(dead_code)]
pub const PROSODY_W2: f32 = 0.5; // reserved: original binary has this slot at 0x89178 (0.5) but current chain uses W1/W3 only
pub const PROSODY_W3: f32 = 0.95; // DAT_0048917c = 0x3f733333 (verified against Future.exe at file 0x8917c)
pub const ACCENT_RANGE: (f32, f32) = (1.86, 2.9);
```

| 定数 | コード値 | file offset | 期待値 | 判定 |
|---|---|---|---|---|
| `CHUNK_SYLLABLES` | 60 | `0x89170` DAT_00489170 | 60 (u32 `0x3c000000` LE) | ✅ 一致 |
| `PROPAGATE_BACK` | 5 | `0x89168` DAT_00489168 | 5 (`0x05000000` LE) | ✅ 一致 |
| `PROPAGATE_FORWARD` | 0 | `0x8916c` DAT_0048916c | 0 | ✅ 一致 |
| `PROSODY_W1` | 0.5 | `0x89174` DAT_00489174 | `0x3f000000` | ✅ 一致 |
| `PROSODY_W2` | 0.5 (reserved) | `0x89178` DAT_00489178 | `0x3f000000` | ✅ 一致 (`allow(dead_code)` で警告抑制済み) |
| `PROSODY_W3` | 0.95 | `0x8917c` DAT_0048917c | `0x3f733333` | ✅ 一致 |

**指摘 (軽微)**:
- 課題文の shorthand `0x89170/74/78/7c` は CHUNK+W1/W2/W3 を列挙したものだが、`PROPAGATE_BACK(5)` は実際は `0x89168` にある。コード内コメントでは `W2` の provenance (`0x89178`) のみ言及しており、`CHUNK` / `PROPAGATE` の provenance コメントが無い。
- **推奨**: `g2p.rs:24-26` に同様の `DAT_00489170 / DAT_00489168` 注記を追記:

```rust
// 修正案 g2p.rs:24-26
pub const CHUNK_SYLLABLES: usize = 60; // DAT_00489170 file 0x89170 u32 60
pub const PROPAGATE_FORWARD: u8 = 0;   // DAT_0048916c file 0x8916c
pub const PROPAGATE_BACK: usize = 5;   // DAT_00489168 file 0x89168 u32 5
```

### 1.3 `ACCENT_RANGE` 乖離 — 要調査

コード `ACCENT_RANGE=(1.86, 2.9)` (`g2p.rs:37`) に対し、連続データ領域 `0x89180/84` は `2.85 / 1.8`。バイナリ全体を `<f` / `<d` で探索しても `1.86` (`7b14ee3f`) / `2.9` (`9a993940`) は出現しない。

- 直近データ `0x89180=2.85`, `0x89184=1.8` が ACCENT_RANGE 由来なら値は不一致。
- 別可能性: ACCENT_RANGE は `.text` の即値 (FLD) として埋め込まれており、`.data` の `0x89180/84` は無関係。

`stage7_prosody` (`g2p.rs:1274-1275`) は `if !(lo..=hi).contains(&s2)` で閾値を使うため、0.05/0.06 差は prosody 分岐に影響する。**欠落ではないが要検証**:

**推奨調査**:
1. Ghidra で `stage7` (`FUN_0043xxxx`) を逆アセンブルし、`FLD dword ptr [DAT_00489180]` か即値かを特定。
2. 即値なら `1.86/2.9` は正しい可能性があり、`0x89180/84` との対応付けをコード注記から外す。
3. メモリ参照なら `1.8 / 2.85` (順序 `lo=1.8 hi=2.85`) に修正し provenance を `DAT_00489184/80` と明記。

現状は **判定: 要追加検証 (blocking ではない)**。

---

## 2. stage4 `rule_flags` off-by-one

### 2.1 現状 (`g2p.rs:1216-1250`)

```rust
// g2p.rs:1227-1244
if r1 != 0 {
    if r1 == 8 { records[i].flag_link = 1; }
    if records[i].rule_flags[0] == 0 { records[i].rule_flags[0] = 1; }
    records[i].rule_counts[0] = records[i].rule_flags[0].wrapping_add(1);
}
if r2 != 0 {
    if records[i].rule_flags[1] == 0 { records[i].rule_flags[1] = 1; }
    records[i].rule_counts[1] = records[i].rule_flags[1].wrapping_add(1);
}
if r3 != 0 {
    if records[i].rule_flags[2] == 0 { records[i].rule_flags[2] = 1; }
    records[i].rule_counts[2] = records[i].rule_flags[2].wrapping_add(1);
}
```

- `rule_flags: [u8;4]` / `rule_counts: [u8;4]` (`g2p.rs:73-75`) に対し index `0/1/2` を linking/nasal/aspirate に 1:1 対応。
- `rule_flags[3]` は未使用 (将来の第4 sandhi 用 reserved)。
- `last.rule_marker = 9` は `n.saturating_sub(1)` ループ外で最終要素のみに付与。

**履歴**: `b4c657c` 以前は `rule_flags[1]/[2]/[3]` を使う off-by-one が指摘されていたが、`b4c657c` で `0/1/2` に修正済み。本 HEAD でも維持。

**判定**: ✅ **既修正・確認済み**。再発なし。`wrapping_add(1)` は flag=1 -> count=2 のバイナリ挙動を再現。

**補足**: `sandhi_hook_*` は現在スタブ (`return 0`, `g2p.rs:1201-1214`) で常に no-op。将来 hook を実装する際は `rule_flags` index を再固定すること。

---

## 3. stage9 排他分岐と stage8 `boundary` 未使用警告

### 3.1 stage8 (`g2p.rs:1286-1330`)

```rust
pub fn stage8_final_markers(records: &mut [WordRecord]) {
    let n = records.len();
    let mut cum = 0usize;
    let mut _boundary: Option<usize> = None; // 16d186a で boundary -> _boundary に改名
    for i in 0..n {
        let rec = &mut records[i];
        cum += rec.phoneme_count;
        if cum >= CHUNK_SYLLABLES { rec.final_marker = 5; cum = 0; _boundary = Some(i); continue; }
        match rec.accent { 0 => ..., 3 => { _boundary=Some(i); }, 4|5 => ..., 6|7 => ..., 8 => ..., 9 => ... }
    }
}
```

- `boundary` は全分岐で `Some(i)` が代入されるがループ後に一切読まれない dead store。
- `16d186a` で `boundary` -> `_boundary` にリネームし `unused_variables` 警告を抑制。妥当だが値は捨てられている。
- 本来 `stage9` で `rposition(final_marker !=0 && !=1)` として再計算しており、`stage8` の値は伝搬されない設計。

**判定**: ✅ **警告抑制は妥当**。`_boundary` は clippy 的にも正しい抑制。現状維持で可。

**推奨 (任意)**: 意図をコメントで明記:

```rust
// g2p.rs:1289 修正案
let mut _boundary: Option<usize> = None; // original binary keeps a local boundary but never reads it; stage9 recomputes via rposition
```

### 3.2 stage9 (`g2p.rs:1347-1384`)

```rust
let last_boundary = records.iter().rposition(|r| r.final_marker !=0 && r.final_marker !=1);
if let Some(boundary_idx) = last_boundary {
    for i in (boundary_idx+1)..n { for m in records[i].phoneme_markers.iter_mut() { *m|=0x80; } }
    return; // 排他: fallback に落ちない
}
// fallback: boundary が一つも無ければ末尾から累積 5 音節まで 0x80 伝搬
let mut cum=0usize; let mut idx=n;
while cum < PROPAGATE_BACK && idx>0 { idx-=1; for m in records[idx].phoneme_markers.iter_mut(){*m|=0x80;} cum+=records[idx].phoneme_count; }
```

| 観点 | 判定 |
|---|---|
| 排他分岐 | ✅ `if let Some` 内で `return` し fallback (`while cum<5`) と排他。バイナリの `if (boundary) { suffix; return; } else { cum<5 }` と同型 |
| 境界条件 `!=0 && !=1` | ✅ `0` (未設定) と `1` (継続) を boundary から除外。`2,3,5,6,7` を boundary とみなすのは spec 通り |
| 飽和 | ✅ `n==0` guard, `idx>0` で underflow 防止, `cum` は `phoneme_count` 累積 |

**指摘なし**。

---

## 4. `postprocess_tables` STAGE3 280件の重複除去

### 4.1 件数

| テーブル | 定義位置 | 件数 | 期待 | 判定 |
|---|---|---|---|---|
| `STAGE3_PAIR_TABLE` | `postprocess_tables.rs:7` | 280 (data rows) | 280 | ✅ 一致 |
| `STAGE3_SENTENCE_TABLE` | `:290` | 9 | 9 | ✅ 一致 |
| `STAGE3_TYPE_A_TABLE` | `:302` | 24 | 24 | ✅ 一致 |
| `STAGE3_DIRECT_SLOT` | `:353` | 1 (`DAT_0047DF14`) | 1 | ✅ 一致 |

> 注意: `grep "(&["` で 281 を返すが、うち1件は型注釈 `&[(&[u8], &[u8])]` の `&[u8]` 偽陽性。data 行 (`"    (&["` 前方空白4つ) で数えると 280。`re.DOTALL` でも型行を含むため同様。

### 4.2 重複

- 正規化 (`\s` 除去) 後の `Counter` で **重複 0**。
- 履歴: `b4c657c` 時点 291件 (内 11重複) -> `16d186a` で 11件を除去し 280件に (例: `(&[0xcb,0xe6], &[0xb4,0xdd,0xc2,0xd9])` x3 など)。

**判定**: ✅ **重複除去は妥当かつ維持**。

### 4.3 未接続

`stage3_*_matches` / `eq_kps_z` は `pub` として公開され `coverage_gap_test.rs` でテストされるが、`stage4` の `sandhi_hook_*` は stub のため未接続。これは意図的な未配線。

---

## 5. stage6 no-op の妥当性

- コード: `g2p.rs:1333` に `/// Stage6 is a no-op in the original binary (empty hook at FUN_0043a9e0); chain is 1/4/7/8/9.` と明記。
- `postprocess()` (`g2p.rs:1334-1345`) の呼び出し順は `stage1 -> apply_sandhi -> stage4 -> stage7 -> stage8 -> stage9` で stage6 をスキップ。
- `16d186a` で `postprocess_tables.rs` ヘッダにも `FUN_0043a9e0` が no-op である旨を追記済み。

**判定**: ✅ **no-op 扱いは妥当**。

---

## 6. 総合判定と残タスク

| 項目 | 判定 | 行番号 | 要否 |
|---|---|---|---|
| PROSODY_W1/W2/W3 / CHUNK / PROPAGATE offset 一致 | ✅ PASS | `g2p.rs:24-36` / file `0x89168/6c/70/74/78/7c` | provenance コメント追記推奨 |
| ACCENT_RANGE (1.86,2.9) vs binary 1.8/2.85 | ⚠️ 要追加検証 | `g2p.rs:37` / file `0x89180/84` | Ghidra 即値 vs data 参照の切り分けが必要 |
| stage4 rule_flags off-by-one | ✅ 修正済み確認 | `g2p.rs:1227-1244` | なし |
| stage9 排他分岐 | ✅ PASS | `g2p.rs:1365-1375` | なし |
| stage8 boundary 未使用警告 | ✅ 抑制妥当 | `g2p.rs:1289` | コメント追記推奨 (任意) |
| STAGE3 280件 重複除去 | ✅ PASS (0重複) | `postprocess_tables.rs:7` | なし |
| stage6 no-op | ✅ 妥当 | `g2p.rs:1333` | なし |

### 必須修正 (blocking): なし

### 推奨修正 (任意・行番号付き)

1. **`g2p.rs:24-26` provenance 追記**
   ```rust
   pub const CHUNK_SYLLABLES: usize = 60; // DAT_00489170 file 0x89170 u32 60
   pub const PROPAGATE_FORWARD: u8 = 0;   // DAT_0048916c file 0x8916c
   pub const PROPAGATE_BACK: usize = 5;   // DAT_00489168 file 0x89168 u32 5
   ```
2. **`g2p.rs:1289` dead store コメント**
   ```rust
   let mut _boundary: Option<usize> = None; // original binary keeps a local boundary but never reads it; stage9 recomputes via rposition
   ```
3. **`g2p.rs:37` ACCENT_RANGE 検証** — Ghidra で `FLD` 参照先を確認。`DAT_00489180/84` 参照なら `(1.8, 2.85)` に修正し provenance 明記。即値なら現状維持し即値注記を追記。

---

**検証コマンド (再現)**:
```bash
python3 -c "import struct; d=open('/tmp/mirae-oracle-fixed/Future.exe','rb').read(); print(hex(struct.unpack('<I', d[0x89170:0x89174])[0]), struct.unpack('<f', d[0x8917c:0x89180])[0])"
# -> 0x3c 0.95

grep -n "CHUNK_SYLLABLES\|PROPAGATE_\|PROSODY_W" mirae-tts-engine/src/g2p.rs
grep -c "    (&\[" mirae-tts-engine/src/postprocess_tables.rs  # -> 280
python3 -c "import re; t=open('mirae-tts-engine/src/postprocess_tables.rs').read(); ps=re.findall(r'\(\s*&\[[^\]]*\]\s*,\s*&\[[^\]]*\]', t[t.find('STAGE3_PAIR_TABLE'):t.find('STAGE3_SENTENCE_TABLE')], re.DOTALL); print(len([p for p in ps if '0x' in p]))"  # -> 280
```
