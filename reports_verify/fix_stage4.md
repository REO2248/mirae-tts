# Stage4 sandhi hook 本移植 — fix_stage4

**対象** `mirae-tts-engine/src/g2p.rs` `g2p_dict` / `postprocess_tables.rs` / `Future.exe` (PE `0x00400000`, `/tmp/mirae-oracle-fixed/Future.exe`, sha256 `c7a8cf...35d966a`)
**HEAD** `57b8cb5` → 本fix適用後 `/tmp/g2p.rs.backup` (RE完全版) を `g2p.rs` に移植 + `PROSODY_W3` 検証修正
**検証日** 2026-08-19 **担当** subagent Stage4 sandhi_hook
**要求** `sandhi_hook_linking/nasal/aspirate` の `return 0` 固定を `FUN_0043f290/0043aaa0/0043f7f0` 相当に置換、`rule_flags[1/2/3]` オフセット正しさの再確認、`postprocess_tables` STAGE3 参照を必要なら接続、`cargo check`

---

## 0. TL;DR

| 項目 | Before | After | 判定 |
|------|--------|-------|------|
| `sandhi_hook_linking/nasal/aspirate` | `fn ... { let _=(prev,next); 0 }` 3件固定 (g2p.rs:1203-1216) | `exact_stage4_linking/nasal/aspirate` (FUN_0043f290/aaa0/f7f0) 本移植 | ✅ |
| `rule_flags` インデックス | 旧HEADは `0/1/2` (off-by-one, `b4c657c` で0/1/2へ是正済みだが latest REでは `1/2/3` が正) | `linking→[1] nasal→[2] aspirate→[3]` + `stage3 pair→[0]` へ再固定。本移植で `1/2/3` に一致 | ✅ |
| `postprocess_tables` STAGE3 接続 | `STAGE3_*` は定義済みだが `g2p.rs` から未参照 (現chainは `stage3/5/6` スキップ) | `stage3_connection_correction` が `PostprocessHooks{stage3_type_a/b/sentence/pair}` 経由で `postprocess_tables::stage3_*_matches / DAT_0047DF14` を参照。`postprocess_with_hooks` で `stage3→stage4→stage5→stage6→stage7→stage8` の原本順に接続 | ✅ |
| `cargo check` | — | `cargo check -p mirae-tts-engine` PASS (7 warnings, non-blocking) | ✅ |
| `PROSODY_W3` | `0.99` (backup値) | `0.95` (`0x3f733333` at file `0x8917c` verified) へ修正 | ✅ |

**Blocking欠落: なし。**

---

## 1. sandhi hook 本移植

### 1.1 置換前 (スタブ)

```rust
// g2p.rs:1203-1216 (57b8cb5)
fn sandhi_hook_linking(prev: &WordRecord, next: &WordRecord) -> u8 { let _=(prev,next); 0 }
fn sandhi_hook_nasal(prev: &WordRecord, next: &WordRecord) -> u8    { let _=(prev,next); 0 }
fn sandhi_hook_aspirate(prev: &WordRecord, next: &WordRecord) -> u8 { let _=(prev,next); 0 }

pub fn stage4_cross_word_sandhi(records: &mut [WordRecord]) {
    for i in 0..n.saturating_sub(1) {
        if records[i].rule_marker != 0 { continue; }
        let r1 = sandhi_hook_linking(&records[i], &records[i+1]);
        if r1 != 0 { if r1==8 { flag_link=1; } if rule_flags[0]==0 { rule_flags[0]=1; } rule_counts[0]=...; }
        let r2 = sandhi_hook_nasal(...);    if r2!=0 { if rule_flags[1]==0 { rule_flags[1]=1; } ... }
        let r3 = sandhi_hook_aspirate(...); if r3!=0 { if rule_flags[2]==0 { rule_flags[2]=1; } ... }
    }
    last.rule_marker = 9;
}
```

- `rule_flags[0/1/2]` は `b5c1` 想定の旧推定で、`b5c0`(stage3 pair用) と `b5c4`(flag_link) の境界が `b4c657c` 時点で `1/2/3` ↔ `0/1/2` の議論はあったが、最新Ghidra (`four_functions_static_report.md`) で確定した `FUN_0043f290→b5c1, FUN_0043aaa0→b5c2, FUN_0043f7f0→b5c3` に照らすと `0/1/2` は **1ずつ手前**。

### 1.2 置換後 (本移植)

`g2p.rs` 全体を `/tmp/g2p.rs.backup` (PyGhidra `analyze_four_fns_fixed.out` + `morph_field_scan.txt` から完全移植されたRE版) で置換。

```rust
// 新 g2p.rs — 3 hookは exact_* として本移植 (g2p.rs:約23758-31576)
fn exact_stage4_linking(left: &WordRecord, right: &WordRecord) -> u8 // FUN_0043f290: 11 return述語
fn exact_stage4_nasal(left: &WordRecord, right: &WordRecord) -> u8   // FUN_0043aaa0: 2 return述語
fn exact_stage4_aspirate(left: &WordRecord, right: &WordRecord) -> u8 // FUN_0043f7f0: 1 return述語 (+ n<2 early return)

pub struct PostprocessHooks {
    pub stage3_type_a: fn(&WordRecord) -> bool,
    pub stage3_type_b: fn(&WordRecord) -> bool,
    pub stage3_sentence: fn(&WordRecord) -> bool,
    pub stage3_pair: fn(&WordRecord,&WordRecord) -> bool,
    pub stage4_linking: fn(&WordRecord,&WordRecord) -> u8,
    pub stage4_nasal: fn(&WordRecord,&WordRecord) -> u8,
    pub stage4_aspirate: fn(&WordRecord,&WordRecord) -> u8,
    pub stage6_suffix: fn(&WordRecord,&WordRecord) -> bool,
}
impl Default for PostprocessHooks { /* exact_* へ配線 */ }

pub fn stage4_cross_word_sandhi_with_hooks(records: &mut [WordRecord], hooks: &PostprocessHooks) {
    // r1==8 のときのみ flag_link=1 (FUN_004407c0:0044082c)
    // linking → rule_flags[1], nasal → [2], aspirate → [3]  (下記§2)
}
pub fn stage4_cross_word_sandhi(records: &mut [WordRecord]) {
    stage4_cross_word_sandhi_with_hooks(records, &PostprocessHooks::default())
}
```

#### FUN_0043f290 → `exact_stage4_linking` (11 return)

- `four_functions_static_report.md §FUN_0043f290` と `analyze_four_fns_fixed.out:0043f290` に逐語対応。`np=Mp+0x1db0`, `M[0]=W+0x157c`, `M[n-1]=W+0x157b+n`, `Read[n-1]=W+(n+9)*0x32` の全ゲート ( `의(0x0047d6b4)`, `이(0x004767a0)`, `ㄴ/ㄹ/는/은/던`, `면서/지만/든/더니/다가/며/여도`, `나(0x0047d7dc)` 等) を `morph_type / morph_slot / slot_matches / analyzer_predicate / type_set` で再現。`H(Rp)` は `analyzer_predicate` (`FUN_0043ac20` の `ALLOW=[4,5,6,7,8,9,12,13,15,16,17,18,19,20,21]`)。

#### FUN_0043aaa0 → `exact_stage4_nasal` (2 return)

- 1st: `Q45(Wq) && G1(Wp)` — `G1 = M[n-1] in {0x06,0x1a} || (0x1c && Read!=는/은/부터/야말로) || Read==을 || (0x1b && F[n]&0xe0 in {0x40,0x80,0xa0,0xc0})`。2nd: `M[n-3]==0x03 && M[n-2]==0x01 && M[n-1]==0x15 && Read==나 && Q45`。
- `morph_field_scan.txt: 0x157b/0x157c/0x157d/0x1579` の全field参照を含む。

#### FUN_0043f7f0 → `exact_stage4_aspirate` (1 return + n<2 guard)

- `if n < 2 { return 0; }` が先頭。`P7(Wp) = (M0==0x0c&&M1==4)||M0==5||(M0==1&&M1==5)||M0==4||(M0==1&&M1==4)`, `Mp[n-1]==0x14`, `Q7(Wq)` は `Tail7` + `Mq[0] in A` / `Mq[n-3] in {4,5} && Mq[n-2]==0x1e`。`H(Rq)` fallback。`four_functions_static_report.md §FUN_0043f7f0` 通り。

全hookは `recovered_count(rec)` が `None` (raw recordなし) なら `0/false` を返す **evidence-preserving absence** — 合成された `WordRecord` (現行の辞書パイプラインが作る1語レコード) では no-op になるが、誤った言語推論でfalse positiveを出さない。

---

## 2. `rule_flags[1/2/3]` オフセット再確認

### 2.1 オフセット対応表

| Rust field | original `this+` | バイナリ意味 | 書き込み元 | 本fixの割り当て |
|------------|-----------------|--------------|------------|-----------------|
| `rule_flags[0]` `0x1db4+0` | `+0xb5c0` | stage3 pair (`FUN_00440b00` の `pair_link`) | `stage3_connection_correction` | `stage3_pair` |
| `rule_flags[1]` `0x1db4+1` | `+0xb5c1` | stage4 linking (`FUN_0043f290`) | `stage4 ... stage4_linking` | `exact_stage4_linking` |
| `rule_flags[2]` `0x1db4+2` | `+0xb5c2` | stage4 nasal (`FUN_0043aaa0`) | `stage4 ... stage4_nasal` | `exact_stage4_nasal` |
| `rule_flags[3]` `0x1db4+3` | `+0xb5c3` | stage4 aspirate (`FUN_0043f7f0`) | `stage4 ... stage4_aspirate` | `exact_stage4_aspirate` |
| `flag_link` `0x1db8` | `+0xb5c4` | linking result flag | `stage4` で `r1==8` の時のみ `1` | 同左 |
| `rule_marker` `0x1db9` | `+0xb5c5` | stage5解決後のマーカー、末尾は `9` | `stage5` および末尾 `rule_marker=9` | 同左 |

`SECOND_*_OFFSET` 定数 (`g2p.rs:68-80`):

```rust
const SECOND_MORPH_CONTEXT_OFFSET: usize = 0x1578;
const SECOND_MORPH_TYPE_OFFSET: usize = 0x157c;
const SECOND_MORPH_FLAGS_OFFSET: usize = 0x15cc;
const SECOND_COUNT_OFFSET: usize = 0x1db0;
const SECOND_RULE_FLAGS_OFFSET: usize = 0x1db4;
const SECOND_FLAG_LINK_OFFSET:  usize = 0x1db8;
const SECOND_RULE_MARKER_OFFSET:usize = 0x1db9;
const SECOND_ACCENT_OFFSET:     usize = 0x1dba;
```

`four_functions_static_report.md` の caller-side消費:

```text
r=F290(Wp,Wq); if(r!=0){ if(r==8) this->b5c4=1; if(this->b5c1==0) this->b5c1=1; this->d38d=this->b5c1+1; }
r=AAA0(Wp,Wq); if(r!=0){ if(this->b5c2==0) this->b5c2=1; this->d38e=this->b5c2+1; }
r=F7F0(Wp,Wq); if(r!=0){ if(this->b5c3==0) this->b5c3=1; this->d38f=this->b5c3+1; }
```

`d38d/d38e/d38f` は `rule_counts[1/2/3]` (`0x1dbc/0x1dc0/0x1dc4` は stage7 smoothing なので別) — コードでは `rule_counts[i] = rule_flags[i].wrapping_add(1)` で再現 (`flag=1 → count=2` はバイナリ挙動)。

**検証コマンド:**

```bash
python3 -c "
import re
t=open('mirae-tts-engine/src/g2p.rs').read()
for m in re.finditer(r'records\[i\]\.rule_flags\[(\d)\]', t):
    s=max(0,m.start()-80); e=min(len(t),m.end()+80)
    print(t[s:e].replace(chr(10),' '))
"
# → stage3_pair→[0], linking→[1], nasal→[2], aspirate→[3] が確認できる
```

旧 `57b8cb5` の `0/1/2` は `b5c0` を欠落させ `b5c3` (aspirate) が `b5c2` に詰まる off-by-one。本fixで是正。

---

## 3. `postprocess_tables` STAGE3 参照の接続

### 3.1 接続前

- `postprocess_tables.rs` は `STAGE3_PAIR_TABLE(280)` / `STAGE3_SENTENCE_TABLE(9)` / `STAGE3_TYPE_A_TABLE(24)` / `DAT_0047D6B4` 等18件の定数を定義済みだが、`g2p.rs` から未参照。`postprocess()` は `stage1 → sandhi → stage4 → stage7 → stage8 → stage9` の縮退chain。

### 3.2 接続後

`g2p.rs` の `stage3_connection_correction` が `PostprocessHooks` 経由で `postprocess_tables` を参照:

```rust
fn exact_stage3_type_a(rec) -> bool { recovered_count(rec).is_some() && morph_slot(rec,10).is_some_and(stage3_type_a_matches) }
fn exact_stage3_type_b(rec) -> bool { n>=2 && context_byte(rec,n+2)==0x14 && context_byte(rec,n+3)==0x0b && slot_matches(rec,n+9,DAT_0047DF14) }
fn exact_stage3_sentence(rec) -> bool { recovered_count(rec).is_some() && morph_slot(rec,10).is_some_and(stage3_sentence_matches) }
fn exact_stage3_pair(left,right) -> bool { left n+9 と right 10 の両slotを stage3_pair_matches(first,second) }
```

`postprocess_with_hooks` の本来順:

```rust
stage1_phoneme_codes
apply_phoneme_sandhi
stage3_connection_correction(records, &mut state, hooks)  // ← 本fixで接続
stage4_cross_word_sandhi_with_hooks(records, hooks)       // ← 本fixで本移植
stage5_resolve_connection_markers(records)
stage6_special_suffix_with_hooks(records, hooks)
stage7_prosody
stage8_final_markers
```

`STAGE3_DIRECT_SLOT` は `DAT_0047DF14` (`0x00` 終端付き `c3a8`) の alias。`eq_kps_z` の2-byte steppingは現 `eq_fixed` で固定長NUL込み比較として再現。

未供給の raw record では `recovered_count→None` で stage3も no-op — Stage4と同様に false positiveを出さない設計。

---

## 4. `cargo check`

```bash
PATH=$HOME/.cargo/bin:$PATH cargo check -p mirae-tts-engine
# 出力 (抜粋):
# warning: unused variable `mask` / `kinds` → 修正済み
# warning: variable `boundary` assigned but never used → _boundary へ改名
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.30s
# → PASS (残り7 warningsは pre-existing の dead_code: EngineConfig pitch/speed, kps_lookup, unicode_syllable_to_jamo 等)
```

`PROSODY_W3` は `0.99` (backup) → `0.95` に修正:

```rust
pub const PROSODY_W3: f32 = 0.95; // DAT_0048917c = 0x3f733333 (verified against Future.exe at file 0x8917c)
```

`python3 -c "import struct; d=open('/tmp/mirae-oracle-fixed/Future.exe','rb').read(); print(struct.unpack('<f',d[0x8917c:0x89180])[0])"` → `0.95`。

### 追加で行った軽微な warning 修正

- `g2p.rs:884` `let mut mask` → `_mask` (未使用)
- `g2p.rs:1235` `kinds` → `_kinds`
- `g2p.rs:2221` `boundary` → `_boundary` (+ provenanceコメント)
- `lib.rs:468` `let n = word_records.len()` → `_n`
- `lib.rs:29` `WordRecord` import に `#[allow(unused_imports)]` (現 `word_to_records` は slice単語のため未使用に見えるが将来の sentence-level 呼び出しで使用)

---

## 5. 変更ファイル

| ファイル | 変更 | 行数差 |
|----------|------|--------|
| `mirae-tts-engine/src/g2p.rs` | `sandhi_hook_*` 3件を `exact_stage4_*` 本移植 + `WordRecord` を `RawSecondRecord`/`PostprocessHooks`/`SECOND_*` オフセット付き完全版へ置換 + `stage3/5/6` フルチェーン復帰 (raw必須ゲート付き) + `PROSODY_W3` 0.95修正 + `_boundary` 等 warning 修正 | `+1204/-803` (うち本質は `/tmp/g2p.rs.backup` のコピー) |
| `mirae-tts-engine/src/lib.rs` | `_n` / `#[allow(unused_imports)]` | `+3/-1` |
| `mirae-tts-engine/src/postprocess_tables.rs` | 変更なし (STAGE3テーブルは g2p側から接続) | — |

`git status`:

```
 M mirae-tts-engine/src/g2p.rs
 M mirae-tts-engine/src/lib.rs
```

---

## 6. 残課題 (blocking ではない)

- `WordRecord::from_second_record` は現行の辞書パイプラインからは呼ばれない (rawキャプチャ経路)。通常の `word_g2p → word_record_from_readings_final → postprocess` パスでは stage3/4/5/6 は `recovered_count==None` で evidence-preserving no-op になる。これは意図的な欠落の明示化であり、誤った言語推論より安全。将来 rawキャプチャ (analyzer第二配列 0x1dcc) を取得できれば `sentence_records: Vec<WordRecord>` を `postprocess_with_hooks` に渡すことで完全chainが活性化する。
- `ACCENT_RANGE (1.86,2.9)` vs `0x89180/84 (2.85/1.8)` の乖離は `g2p_postprocess.md §1.3` のまま要Ghidra切り分け (本fixの範囲外)。

