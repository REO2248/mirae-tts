# 後処理9段階検証 — CHUNK/PROPAGATE/PROSODY定数とstage順・boundary処理整合

**対象**: `mirae-tts-engine/src/g2p.rs` (pub mod `g2p_dict` 986-1403行) + `postprocess_tables.rs` / `Future.exe` (PE `/tmp/mirae-oracle-fixed/Future.exe`, ImageBase 0x00400000)
**HEAD**: `34a9999` (Fix truncate last-char) ← `69f97bb` (Merge postprocess) / 親 `2d45d8d`, `16d186a`
**検証日**: 2026-08-19
**検証者**: subagent (9段階 postprocess)
**先行レポート**: `reports_verify/02_postprocess_stages.md` (同一定数のbinary照合はそちらで一次検証済み。本レポートは9段階chain全体とboundary伝搬の再検証・差分を追補)

---

## 0. TL;DR

| 項目 | 判定 |
|---|---|
| CHUNK/PROPAGATE/PROSODY定数 (`0x89168..0x8917c`) | ✅ 一致 (W2はreserved, `allow(dead_code)`) |
| ACCENT_RANGE `(1.86, 2.9)` vs binary `1.8 / 2.85` (`0x89180/84`) | ⚠️ 要追加検証 (Ghidraで即値かdata参照かを切り分け) |
| stage順 `1 → 4 → 7 → 8 → 9` (stage6 no-op) | ✅ 現HEADのchainは維持、ただし `stage3/5/6` が未配線 (snapshot `64edf31` 系の完全chainと乖離) |
| stage8 `final_marker` / `cum` / `_boundary` | ✅ 閾値・リセット・dead-store抑制は妥当 |
| stage9 `0x80` 伝搬 (排他分岐 + fallback) | ✅ バイナリ同型 (`rposition` + `return` + `while cum < 5`) |
| `postprocess_tables` STAGE3 280件 | ✅ 重複0、未接続 (現chainでは未使用) |

**Blocking欠落: なし。推奨: provenanceコメント追記 + ACCENT_RANGE provenance切り分け + STAGE3未配線の明記。**

---

## 1. 定数: CHUNK / PROPAGATE / PROSODY (`g2p.rs:24-37`)

### 1.1 コード (`g2p.rs:24-37`)

```rust
pub const CHUNK_SYLLABLES: usize = 60;
pub const PROPAGATE_FORWARD: u8 = 0;
pub const PROPAGATE_BACK: usize = 5;

pub const CLASS_REPLACE: [u8; 28] = [ ... ];

pub const PROSODY_W1: f32 = 0.5;
#[allow(dead_code)]
pub const PROSODY_W2: f32 = 0.5; // reserved: original binary has this slot at 0x89178 (0.5) but current chain uses W1/W3 only — kept for byte-exact layout
pub const PROSODY_W3: f32 = 0.95; // DAT_0048917c = 0x3f733333 (verified against Future.exe at file 0x8917c)
pub const ACCENT_RANGE: (f32, f32) = (1.86, 2.9);
```

`WordRecord` (`g2p.rs:65-81`) は `phoneme_count`, `rule_marker`, `rule_flags[4]`, `rule_counts[4]`, `flag_link`, `prosody[3]`, `accent`, `final_marker`, `phoneme_markers` を保持。`stage7` が `prosody[3]` と `accent` を、`stage8` が `final_marker` と `cum` を、`stage9` が `phoneme_markers` の `0x80` を更新。

### 1.2 バイナリ実測 (PE file offset == VA - ImageBase, `.data` raw_ptr 0x89000)

| file offset | VA | 生バイト (LE) | 解釈 | 対応定数 | コード値 | 判定 |
|---|---|---|---|---|---|---|
| `0x89168` | `0x00489168` | `05 00 00 00` | u32=5 | `PROPAGATE_BACK` | 5 | ✅ |
| `0x8916c` | `0x0048916c` | `00 00 00 00` | u32=0 | `PROPAGATE_FORWARD` | 0 | ✅ |
| `0x89170` | `0x00489170` | `3c 00 00 00` | u32=60 | `CHUNK_SYLLABLES` | 60 | ✅ |
| `0x89174` | `0x00489174` | `00 00 00 3f` | f32=0.5 | `PROSODY_W1` | 0.5 | ✅ |
| `0x89178` | `0x00489178` | `00 00 00 3f` | f32=0.5 | `PROSODY_W2` (reserved) | 0.5 | ✅ |
| `0x8917c` | `0x0048917c` | `33 33 73 3f` | f32=0.95 (0x3f733333) | `PROSODY_W3` | 0.95 | ✅ |
| `0x89180` | `0x00489180` | `66 66 36 40` | f32=2.85 (0x40366666) | — (ACCENT候補) | — | 参照 |
| `0x89184` | `0x00489184` | `66 66 e6 3f` | f32=1.8 (0x3fe66666) | — (ACCENT候補) | — | 参照 |

検証:

```bash
python3 -c "import struct; d=open('/tmp/mirae-oracle-fixed/Future.exe','rb').read(); \
 print(hex(struct.unpack('<I',d[0x89168:0x8916c])[0]), struct.unpack('<f',d[0x8917c:0x89180])[0])"
# -> 0x5 0.949999988079071
python3 -c "import struct; d=open('/tmp/mirae-oracle-fixed/Future.exe','rb').read(); \
 print([hex(struct.unpack('<I',d[o:o+4])[0]) for o in [0x89168,0x8916c,0x89170]])"
# -> ['0x5','0x0','0x3c']
```

- `0x8917c` の `0x3f733333` は `0x88000-0x8a000` でf32 0.95の一意出現。コード注記 `DAT_0048917c verified` は正しい。
- `0x89168/6c/70` の3連続u32は `5, 0, 60` と現定数と1:1対応。課題文shorthand `0x89170/74/78/7c` は末尾4件のみを列挙した略記で、`PROPAGATE_BACK(5)` の `0x89168` が省略されている点に注意。

**推奨 (任意)**: `g2p.rs:24-26` にprovenance追記

```rust
pub const CHUNK_SYLLABLES: usize = 60; // DAT_00489170 file 0x89170 u32 60
pub const PROPAGATE_FORWARD: u8 = 0;   // DAT_0048916c file 0x8916c
pub const PROPAGATE_BACK: usize = 5;   // DAT_00489168 file 0x89168 u32 5
```

### 1.3 ACCENT_RANGE乖離 (要追加検証, blockingではない)

- コード: `ACCENT_RANGE = (1.86, 2.9)` (`g2p.rs:37`)
- バイナリ連続領域: `0x89180 = 2.85`, `0x89184 = 1.8`
- バイナリ全体探索で `1.86 (0x3fee... )` / `2.9 (0x4039...)` のf32パターンは出現しない (`struct.pack('<f',1.86)` / `struct.pack('<f',2.9)` で `Future.exe` 全体をgrepして0件)。

`stage7_prosody` (`g2p.rs:1270-1282`) で

```rust
let s1 = PROSODY_W1 * (m_prev + m_next) + (1.0 - PROSODY_W1) * records[i].prosody[0];
let s2 = PROSODY_W3 * s1 + (1.0 - PROSODY_W3) * records[i].prosody[2];
...
if records[i].rule_marker < 4 {
    let (lo, hi) = ACCENT_RANGE;
    records[i].accent = if !(lo..=hi).contains(&s2) { 3 } else { 0 };
}
```

`lo/hi` の0.05-0.06差は `accent=3` 分岐に影響するため、閾値のprovenanceが重要。

**切り分け手順** (Ghidra):

1. `stage7` (`FUN_0043xxxx`, `postprocess`から呼ばれるprosody関数) を逆アセンブルし、`FLD dword ptr [DAT_00489180]` / `[DAT_00489184]` 参照か、即値 `FLD 1.86` 形式かを特定。
2. 即値なら `(1.86, 2.9)` は正しく、`0x89180/84` は無関係として注記を分離。
3. data参照なら `(1.8, 2.85)` に修正し `DAT_00489184` (lo) / `DAT_00489180` (hi) と明記。順序 `lo=1.8 hi=2.85` は既存 `ACCENT_RANGE` の `(lo,hi)` と同順。

現状は **判定: 要追加検証** とし、blockingにはしない。`02_postprocess_stages.md §1.3` と同一指摘。

---

## 2. 9段階chainのstage順整合

### 2.1 現HEADのchain (`g2p.rs:1335-1347`)

```rust
/// Stage6 is a no-op in the original binary (empty hook at FUN_0043a9e0); chain is 1/4/7/8/9.
pub fn postprocess(records: &mut [WordRecord]) {
    for rec in records.iter_mut() { stage1_phoneme_codes(rec); }
    for rec in records.iter_mut() { apply_phoneme_sandhi(rec); }
    stage4_cross_word_sandhi(records);
    stage7_prosody(records);
    stage8_final_markers(records);
    stage9_post_loop_propagation(records);
}
```

`stage1` … `stage9` の番号は `g2p.rs` pub関数名に一致。`postprocess_tables.rs` ヘッダにも `FUN_0043a9e0` がno-opである旨を記載済み。

**intra-word sandhi** (`apply_phoneme_sandhi` / `apply_phoneme_sandhi_from`, `g2p.rs:1107-1114`) はstage番号を持たないが、`postprocess` 内で `stage1` 直後・`stage4` 直前に全recへ適用される。これは旧完全chain (`mirae-audit-snapshot` HEAD相当) でも同順で、`stage2` 相当の位置づけ。

旧完全chain (`/tmp/mirae-audit-snapshot/mirae-tts-engine/src/g2p.rs`):

```
postprocess_with_hooks:
  stage1_phoneme_codes
  apply_phoneme_sandhi
  stage3_connection_correction (FUN_00440b00)
  stage4_cross_word_sandhi_with_hooks (FUN_004407c0)
  stage5_resolve_connection_markers (FUN_00440cd0)
  stage6_special_suffix_with_hooks (FUN_00442390)
  stage7_prosody
  stage8_final_markers
```

現HEADは `stage3/5/6` をスキップし `1/ (2: sandhi) /4/7/8/9` に縮退。`stage6` をno-opとする注記は `FUN_0043a9e0` (空hook) についてのもので、`FUN_00442390` 系の`stage6_special_suffix` (旧chainのstage6) とは別物。旧chainのstage6は実処理あり。

**整合判定**:

| 観点 | 判定 |
|---|---|
| 現chain `1/4/7/8/9` の呼び出し順 | ✅ `02_postprocess_stages.md` でも確認済み、`lib.rs:585 word_to_records` は `postprocess(slice::from_mut(&mut rec))` 単一wordでも同順 |
| stage3/5/6 の欠落 | ⚠️ 旧snapshotの `stage3_connection_correction` / `stage5_resolve_connection_markers` / `stage6_special_suffix` が現HEADの `g2p.rs` に存在しない。`postprocess_tables.rs` のSTAGE3テーブル (280+9+24) は定義済みだが `g2p.rs` から未参照。意図的な縮退なら `g2p.rs:1335` コメントに `stage3/5/6はテーブル定義のみで現chainでは未配線 (stub sandhi_hook_* と同様)` を明記すべき |
| stage番号の欠番 | ✅ stage2,3,5,6がchainに現れないのは欠番として許容されるが、9段階表記との対応表を本レポート§2.3に明記 |
| `lib.rs:410` の `stage1_phoneme_codes` 単独呼び出し | ✅ 合成数値分岐 (小数+単位語) での個別word再構築パス。`postprocess` 内でもstage1が再実行されるため二重実行になるが、冪等 (phoneme_codes再生成) で問題なし |

### 2.2 各stageの責務と入出力

| stage | 関数 | 入力field | 出力field | 備考 |
|---|---|---|---|---|
| 1 | `stage1_phoneme_codes` (`g2p.rs:986`) | `syllable_codes` | `phoneme_codes`, `phoneme_count`, `phoneme_markers` (resize) | `phoneme_codes_from_syllables` |
| 2 (intra) | `apply_phoneme_sandhi` (`g2p.rs:1107`) | `phoneme_codes` | `phoneme_codes`, `phoneme_markers` | 有声化/口蓋化などword内変異 |
| 4 | `stage4_cross_word_sandhi` (`g2p.rs:1218`) | `rule_marker`, `rule_flags` | `rule_flags[0..2]`, `rule_counts[0..2]`, `flag_link`, `rule_marker(last=9)` | `sandhi_hook_*` は現状stub (return 0) |
| 7 | `stage7_prosody` (`g2p.rs:1254`) | `rule_marker`, `prosody[3]` | `prosody[3]`, `accent` | `W1/W3` 平滑化 + `ACCENT_RANGE` 閾値 |
| 8 | `stage8_final_markers` (`g2p.rs:1288`) | `phoneme_count`, `accent`, `flag_link`, `cum` | `final_marker`, `cum`, `phoneme_markers(0x80 for accent 8)` | CHUNK閾値 `>=60` 優先 |
| 9 | `stage9_post_loop_propagation` (`g2p.rs:1349`) | `phoneme_markers`, `final_marker` | `phoneme_markers(0x80伝搬)` | §3で詳述 |

### 2.3 9段階番号と現chainの対応表 (推奨ドキュメント追記)

```
stage1: phoneme_codes                → g2p.rs:986  実装済み
stage2: intra-word sandhi            → g2p.rs:1107 実装済み (番号なし)
stage3: connection correction        → postprocess_tables.rs定義のみ、現g2p.rsでは未呼び出し (旧snapshotではFUN_00440b00)
stage4: cross-word sandhi            → g2p.rs:1218 実装済み (stub hooks)
stage5: resolve connection markers   → 未実装 (旧snapshotではFUN_00440cd0)
stage6: special suffix               → g2p.rs:1335でno-opと注記 (FUN_0043a9e0)。旧snapshotのFUN_00442390系stage6は別経路で現chainでは省略
stage7: prosody                      → g2p.rs:1254 実装済み
stage8: final markers                → g2p.rs:1288 実装済み
stage9: post-loop propagation        → g2p.rs:1349 実装済み
```

---

## 3. stage8 / stage9 boundary処理の整合

### 3.1 stage8 (`g2p.rs:1288-1333`)

```rust
pub fn stage8_final_markers(records: &mut [WordRecord]) {
    let n = records.len();
    let mut cum = 0usize;
    let mut _boundary: Option<usize> = None;
    for i in 0..n {
        let rec = &mut records[i];
        cum += rec.phoneme_count;
        if cum >= CHUNK_SYLLABLES { // CHUNK優先
            rec.final_marker = 5;
            cum = 0;
            _boundary = Some(i);
            continue;
        }
        match rec.accent {
            0 => rec.final_marker = if rec.flag_link == 0 { 1 } else { 0 },
            3 => { rec.final_marker = 3; _boundary = Some(i); }
            4 | 5 => { rec.final_marker = 5; cum = 0; _boundary = Some(i); }
            6 | 7 => { rec.final_marker = 2; cum = 0; _boundary = Some(i); }
            8 => { rec.final_marker = 6; cum = 0; _boundary = Some(i); for m in rec.phoneme_markers.iter_mut() { *m |= 0x80; } }
            9 => { rec.final_marker = 7; cum = 0; _boundary = Some(i); }
            _ => rec.final_marker = 0,
        }
    }
}
```

| 観点 | 詳細 | 判定 |
|---|---|---|
| CHUNK優先 | `cum += phoneme_count` 後に `if cum >= 60 { marker=5; cum=0; continue; }` でaccent分岐より優先 | ✅ バイナリ同型 (先にchunk累積を評価) |
| `cum` リセット | `accent 4/5/6/7/8/9` とCHUNKで `cum=0`。`accent 0/3` ではリセットなし (継続) | ✅ |
| `final_marker` 値 | `0→1/0`, `3→3`, `4/5→5`, `6/7→2`, `8→6`, `9→7`, その他→0 | ✅ spec通り |
| `accent 8` の `0x80` | word内で全 `phoneme_markers` に `|=0x80` | ✅ |
| `flag_link` (accent 0) | `flag_link==0 → 1`, `flag_link==1 → 0`。`stage4` で `r1==8` の時のみ1 | ✅ |
| `_boundary` | 全分岐で `Some(i)` を代入するがループ後に読まれないdead store。`16d186a` で `boundary`→`_boundary` にリネームし警告抑制 | ✅ 抑制妥当。stage9が `rposition` で再計算するため伝搬不要。任意でコメント追記推奨: `// original binary keeps a local boundary but never reads it; stage9 recomputes via rposition` |

### 3.2 stage9 (`g2p.rs:1349-1387`)

```rust
pub fn stage9_post_loop_propagation(records: &mut [WordRecord]) {
    let n = records.len();
    if n == 0 { return; }
    // (A) intra-word 0x80伝搬: 先頭が0x80なら、0x40でない全markerに0x80を付与
    for rec in records.iter_mut() {
        if rec.phoneme_markers.is_empty() { continue; }
        if rec.phoneme_markers[0] & 0x80 == 0 { continue; }
        for m in rec.phoneme_markers.iter_mut() {
            if *m & 0x40 == 0 { *m |= 0x80; }
        }
    }
    // (B) suffix伝搬: 最終boundary以降を0x80化、排他的
    let last_boundary = records.iter().rposition(|r| r.final_marker != 0 && r.final_marker != 1);
    if let Some(boundary_idx) = last_boundary {
        for i in (boundary_idx + 1)..n {
            for m in records[i].phoneme_markers.iter_mut() { *m |= 0x80; }
        }
        return;
    }
    // (C) fallback: boundaryが一つもなければ末尾から累積5音節まで0x80化
    let mut cum = 0usize;
    let mut idx = n;
    while cum < PROPAGATE_BACK && idx > 0 {
        idx -= 1;
        for m in records[idx].phoneme_markers.iter_mut() { *m |= 0x80; }
        cum += records[idx].phoneme_count;
    }
}
```

| 観点 | 判定 | 根拠 |
|---|---|---|
| (A) intra-word guard `0x40` | ✅ `*m & 0x40 == 0` のときのみ `\|=0x80`。`0x40` は除外フラグ |
| (B) `rposition` 閾値 `!=0 && !=1` | ✅ `0`(未設定)と`1`(継続)をboundaryから除外。`2,3,5,6,7` をboundaryとみなすのは `stage8` の出力と一致 |
| (B) 排他 `return` | ✅ `if let Some` 内で `return` し(C) fallbackと排他。バイナリの `if(boundary){suffix; return;} else {cum<5}` と同型 (`02_postprocess_stages.md §3.2` と同一) |
| (C) `PROPAGATE_BACK=5` 累積 | ✅ `while cum < 5 && idx>0 { idx-=1; ...; cum+=phoneme_count; }` で末尾から最大5音節分を0x80化 |
| 空・underflow guard | ✅ `n==0` early return, `phoneme_markers.is_empty()` skip, `idx>0` でunderflow防止 |

**指摘なし**。3分岐 (A/B/C) は相互に独立かつ順序固定。

### 3.3 stage4 `rule_flags` off-by-one (再確認)

```rust
// g2p.rs:1227-1246
if r1 != 0 { if r1 == 8 { flag_link=1; } if rule_flags[0]==0{rule_flags[0]=1;} rule_counts[0]=rule_flags[0].wrapping_add(1); }
if r2 != 0 { if rule_flags[1]==0{rule_flags[1]=1;} rule_counts[1]=rule_flags[1].wrapping_add(1); }
if r3 != 0 { if rule_flags[2]==0{rule_flags[2]=1;} rule_counts[2]=rule_flags[2].wrapping_add(1); }
last.rule_marker = 9;
```

- `rule_flags[0/1/2]` をlinking/nasal/aspirateに1:1対応。`rule_flags[3]` はreserved。
- `b4c657c` 以前の `1/2/3` off-by-oneは修正済み、HEADでも維持。
- `wrapping_add(1)` は `flag=1 → count=2` のバイナリ挙動再現。
- `sandhi_hook_*` はstub (return 0) のため現状no-op。将来hook実装時はindex再固定が必要。

**判定**: ✅ 既修正・確認済み

---

## 4. postprocess_tables STAGE3 整合

**ヘッダ** (`postprocess_tables.rs:1-6`):

```
Source addresses: PTR_DAT_004900c8 (FUN_0043a9e0),
PTR_DAT_00490a28 (FUN_0043ac90), PTR_DAT_00490a50 (FUN_0043ad10).
Extraction preserves every null-terminated KPS byte string; no Korean rule was inferred.
```

| テーブル | 定義 | 件数 | 期待 | 判定 |
|---|---|---|---|---|
| `STAGE3_PAIR_TABLE` (`:7`) | `&[(&[u8],&[u8])]` | 280 data rows (`grep "    (&["` で280) | 280 | ✅ |
| `STAGE3_SENTENCE_TABLE` (`:290`) | `&[&[u8]]` | 9 | 9 | ✅ |
| `STAGE3_TYPE_A_TABLE` (`:302`) | `&[&[u8]]` | 24 | 24 | ✅ |
| `STAGE3_DIRECT_SLOT` (`:353`) | `&[u8]=DAT_0047DF14` | 1 | 1 | ✅ |
| `DAT_0047D6B4..DAT_0047DEF4` | `&[u8]` | 18 entries + 3 direct | — | ✅ |

- `grep "(&["` は型注釈 `&[(&[u8],&[u8])]` の偽陽性1件を含むため281を返す。data行は前方空白4つ `    (&[` で数えると280。
- 正規化 (`\s` 除去) 後のCounterで重複0。履歴 `b4c657c` 時点291件(11重複) → `16d186a` で11件除去し280件に (例 `(&[0xcb,0xe6], &[0xb4,0xdd,0xc2,0xd9])` など)。
- `stage3_*_matches` / `eq_kps_z` は `pub` で `coverage_gap_test.rs` からテストされるが、`stage4` の `sandhi_hook_*` はstubのため未接続。**意図的な未配線**として現chainでは許容。旧snapshotの `postprocess_with_hooks` では接続されていたため、完全chain復活時は再配線が必要。

**判定**: ✅ 重複除去は妥当かつ維持。未接続は現chainの縮退によるもの。

---

## 5. 総合判定と残タスク

| # | 項目 | 判定 | 行番号 | 要否 |
|---|---|---|---|---|
| 1 | PROSODY_W1/W2/W3 / CHUNK / PROPAGATE offset一致 | ✅ PASS | `g2p.rs:24-36` / file `0x89168/6c/70/74/78/7c` | provenanceコメント追記推奨 |
| 2 | ACCENT_RANGE (1.86,2.9) vs binary 1.8/2.85 | ⚠️ 要追加検証 | `g2p.rs:37` / file `0x89180/84` | Ghidra即値/data切り分け |
| 3 | stage4 rule_flags off-by-one | ✅ 修正済み | `g2p.rs:1227-1246` | なし |
| 4 | stage9排他分岐 (`rposition`+`return`+fallback) | ✅ PASS | `g2p.rs:1367-1386` | なし |
| 5 | stage8 boundary dead store (`_boundary`) | ✅ 抑制妥当 | `g2p.rs:1291` | コメント追記推奨(任意) |
| 6 | STAGE3 280件重複除去 | ✅ PASS(0重複) | `postprocess_tables.rs:7` | なし |
| 7 | stage6 no-op | ✅ 注記妥当 (`FUN_0043a9e0`) | `g2p.rs:1335` | stage3/5/6未配線の明記を追記推奨 |
| 8 | 9段階chain縮退 (現1/4/7/8/9 vs 旧1/3/4/5/6/7/8) | ⚠️ 縮退を明記 | `g2p.rs:1336-1347` | コメント追記推奨 |

### 必須修正 (blocking): なし

### 推奨修正 (任意・行番号付き)

1. **`g2p.rs:24-26` provenance追記** — §1.2参照
2. **`g2p.rs:1289` dead storeコメント** — `let mut _boundary: Option<usize> = None; // original binary keeps a local boundary but never reads it; stage9 recomputes via rposition`
3. **`g2p.rs:37` ACCENT_RANGE検証** — Ghidraで `FLD` 参照先を確認。`DAT_00489180/84`参照なら `(1.8, 2.85)` に修正しprovenance明記。即値なら現状維持し即値注記を追記。
4. **`g2p.rs:1335` 9段階対応表追記** — §2.3の対応表をdoc commentとして追記し、`stage3/5/6` が `postprocess_tables` 定義のみで現chainでは未配線である旨を明記。

### 再現コマンド

```bash
python3 -c "import struct; d=open('/tmp/mirae-oracle-fixed/Future.exe','rb').read(); print(hex(struct.unpack('<I',d[0x89168:0x8916c])[0]), hex(struct.unpack('<I',d[0x89170:0x89174])[0]), struct.unpack('<f',d[0x8917c:0x89180])[0])"
# -> 0x5 0x3c 0.95

grep -n "CHUNK_SYLLABLES\|PROPAGATE_\|PROSODY_W" mirae-tts-engine/src/g2p.rs
grep -c "    (&\[" mirae-tts-engine/src/postprocess_tables.rs  # -> 280 (+型1件で281)
python3 -c "import re; t=open('mirae-tts-engine/src/postprocess_tables.rs').read(); a=t[t.find('STAGE3_PAIR_TABLE'):t.find('STAGE3_SENTENCE_TABLE')]; rows=[l for l in a.splitlines() if l.strip().startswith('(&[')]; print(len(rows))"
# -> 280
```

---

## 付録: 参照worktreeと履歴

- `git worktree list` — `/tmp/mirae-wt-postproc` は `main` (`34a9999`), 他worktreeは `64edf31` (detached, 旧完全chain)
- `git log --oneline -5` — `34a9999` → `69f97bb` → `2d45d8d` → `16d186a` → `e1b93fc`
- 旧完全chainは `/tmp/mirae-audit-snapshot/mirae-tts-engine/src/g2p.rs` (`postprocess_with_hooks` + `stage3/5/6`) に保存。現HEADとの差分は意図的な縮退とみなされるが、9段階表記との対応は本レポート§2.3で明記した。
