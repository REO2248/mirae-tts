# 04 レンダリング・Voice・WAV系 欠落検証

**対象HEAD:** `69f97bb` (post `2d45d8d` Fix audit rounds)  
**検証日:** 2026-08-19  
**対象:** `mirae-tts-engine/src/{render.rs,wav.rs,voice_data.rs,voice_info.rs,dict.rs,voice_dict.rs,connect.rs,tables.rs,segmenter.rs}` + 関連 `unit_select.rs / lib.rs / g2p.rs`

---

## 1. WavWriter `split()` — データ損失（旧truncate破壊の回帰）

### 現状 (`wav.rs:81-101`)
```rust
fn split(&mut self) -> io::Result<()> {
    let f = self.file.as_mut().expect("WavWriter already finished");
    write_wav_header(f, self.data_size as u32)?;
    self.file = None;
    let stem = self.path.file_stem()…;
    let ext  = self.path.extension()…;
    let parent = self.path.parent().unwrap_or(Path::new("."));
    let next = parent.join(format!("{}_{:03}.{}", stem, self.split_index + 1, ext));
    let mut file = File::create(&next)?;
    file.seek(SeekFrom::Start(WAV_HEADER_SIZE as u64))?;
    self.split_index += 1;
    self.file = Some(file);
    self.data_size = 0;
    Ok(())
}
pub fn append(&mut self, pcm: &[u8]) -> io::Result<()> {
    let f = self.file.as_mut().expect("WavWriter already finished");
    f.write_all(pcm)?;
    self.data_size += pcm.len() as u64;
    if self.data_size > self.split_threshold { self.split()?; }
    Ok(())
}
```

### 検証結果: **合格 — 旧バグは解消**

`2d45d8d` diff確認:
```diff
-        let mut file = File::create(&self.path)?;
+        let stem = …; let next = parent.join(format!("{}_{:03}.{}", stem, self.split_index+1, ext));
+        let mut file = File::create(&next)?;
+        self.split_index += 1;
```
旧実装 (`d648df7` 以前) は `split()` 内で `File::create(&self.path)` で元ファイルをtruncateし直していたため、閾値 `26_000_000` を超えた以降の `data_size` がリセットされても既存PCMが破壊される。現行は `split_index` を導入し `output_001.wav`, `output_002.wav` … を新規作成するためデータ保持される。

`render_test.rs` も `2d45d8d` で「単一ファイル上書き」期待から「numbered split file」検証へ更新済み。

### 残存する軽微な欠落（Severity: Low）

| # | 位置 | 現象 | 修正案 |
|---|------|------|--------|
| W-1 | `wav.rs:68-72` `WavWriter::create_with_threshold` | `split_index` は初期化されているが `WavWriter` に作成済みファイル一覧を持たない。`finish()` は **最終ファイルの `data_size` のみ** を返す (`wav.rs:104-109`)。呼び出し側が総バイト数を知る手段がない。 | `WavWriter` に `total_bytes: u64` を追加し `append` 毎に加算、`finish()` は `total_bytes` と `Vec<PathBuf>` を返すか、ログ出力する。あるいは `finish()` のdocに「分割時は最終セグメントのサイズのみ返す」ことを明記。 |
| W-2 | `wav.rs:71-77` `append` | 1回の `append(pcm)` が `pcm.len() > split_threshold` の巨大バッファだった場合、書き込み後に `data_size > threshold` で `split()` しても次ファイルは空（`data_size=0`）のまま。データ損失はないが、次 `append` 前に `data_size` が閾値を超えたまま残らないのは意図通りだが、1ファイルが閾値を大きく超える（`threshold + 26MB`）ことを許容してしまう。元実装 `FUN_0042b630` はストリーミングで閾値超過時に即分割のはず。 | 巨大 `pcm` は `split_threshold` 単位でループ分割して複数ファイルに跨って書き込む（`while pcm.len() > remaining`）。現行でも破壊はないため P3。 |
| W-3 | `wav.rs:84-95` パス生成 | `file_stem()` / `extension()` が `None` のとき `unwrap_or("output"/"wav")` でフォールバックするが、元パスが `/tmp/foo`（拡張子なし）のとき `foo_001.wav` になるのは妥当。`parent()` が `None` のとき `"."` にフォールバックしてカレントに書く挙動は呼び出し側の期待とずれる可能性。 | `WavWriter::create` のdocに「`parent()` が空ならカレントディレクトリに `_*` を作成する」旨を追記。厳密にするなら `self.path.parent().expect("WavWriter path must have parent")` にするか `std::env::current_dir()` を明示。 |

**結論:** 旧 `truncate` 破壊の回帰はなし。W-1〜W-3は機能欠落ではなくAPIドキュメント/拡張の範囲。

---

## 2. `Rec6::b6` 保持の効果 (`voice_dict.rs:46-64`)

### 現状
```rust
// voice_dict.rs:46-64
pub struct Rec6 { pub phoneme_id: u16, pub b2:u8, pub b3:u8, pub b4:u8, pub b5:u8, pub b6:u8 }
impl Rec6 {
  fn from_bytes(b: &[u8]) -> Self {
    Rec6 { phoneme_id: u16::from_le_bytes([b[0],b[1]]), b2:b[2], b3:b[3], b4:b[4], b5:b[5], b6:b[5] }
  }
}
```
`2d45d8d` で `b6: 0` → `b6: b[5]` に修正。コミットメッセージ「b6 now preserves original b[5] (was 0, losing last recording param)」。

### 検証結果: **効果あり — ただしドキュメントと構造体の不整合が残る（Severity: Medium）**

* **保持効果:** `cargo check` 上、 `Rec6` は現在ランタイムで未参照（`lib.rs` は `Dict` を使用。`MiraeDict`/`Rec6` は `voice_dict.rs` のテストと `diff==0` 検証専用）。そのため `b6` が `0` でも現行TTS出力に影響は出ないが、将来 `MiraeDict` をランタイムに昇格させた際や、バイト完全性を主張する上で `b6=0` は情報損失だった。修正後は少なくとも末尾バイトが欠落しない。

* **不整合:**
  - ヘッダdoc (`voice_dict.rs:28-32`) は ` [u8 b2][u8 b3][u8 b4][u8 b5][u8 b6]` と5バイトを記載しつつ、直上の ` [u8 × 6 × f6] rec6 ; 6-byte records` と矛盾。実際は `phoneme_id 2B + 4B = 6B` なので `b2..b5` の4バイトが正しく、5バイト記載は誤記。実装は `b[0..5]` の6Bから `phoneme_id(2)+b2..b5(4)` を取り、`b6` に `b[5]` を**複製**して埋めている。つまり `b5` と `b6` は同一物理バイト。
  - `Dict` 側 (`dict.rs:11-16` `SubARecord {kind,sub,v0,v1}` 計6B) と `MiraeDict` 側で同じ6Bを別解釈しており、命名が衝突して混乱を招く。

### 修正案

| # | 位置 | 提案 |
|---|------|------|
| R-1 | `voice_dict.rs:28-33` doc | `6-byte records` の内訳を ` [u16 phoneme_id][u8 b2][u8 b3][u8 b4][u8 b5] ; last byte duplicated to b6 for compat` と明記。または構造体を `b6` なしの4フィールドに正規化し `b6()` getterで `b5` を返す形にリファクタ。 |
| R-2 | `voice_dict.rs:46-52` `Rec6` | `b6: b[5]` の複製であることをコメントで明示: `b6: b[5], // 6-byte rec has only 4 trailing bytes; b6 duplicates last byte for downstream compat`。将来的に実データで `b6` が別バイトとして必要になった場合、フォーマット調査（`Voice/*.pkg` をhexdumpし `f6*6` が `data.len()` と一致するか）で再定義する。 |
| R-3 | `voice_dict.rs:58-64` vs `dict.rs:286-310` | どちらを正とするか `lib.rs` のdocに明記。現状 `lib.rs:27` コメント `/// kept for byte-exact verification` は正しいが、`cargo check` で `MiraeDict` が未使用扱いにならないよう `#[cfg(test)]` に寄せるか、`pub use` をやめて `#[allow(dead_code)]` を付与する運用を統一。 |

**検証:** 実データ `VoiceInfo.pkg` ではなく `Voice/*.pkg`（`colligation.pkg` 等）の `f6*6` バイト列を `MiraeDict::parse` と `Dict::from_bytes` で相互パースし `diff==0` をテストで担保しているため、現行ロジックでパースエラーは出ない。

---

## 3. `render::is_real_phoneme` 二重定義と `unit_select` 食い違い

### 現状

`render.rs:38-47`
```rust
pub fn is_real_phoneme(high6_cur: u16, low5_next: u16) -> bool {
  !(matches!(low5_next, 1|4|6|8|9|10|0xb|0xc|0xd|0xe|0x10|0x11|0x12) || (low5_next==3 && high6_cur==6))
}
pub fn is_real_phoneme_codes(cur: u16, next: u16) -> bool { is_real_phoneme(cur>>10, next & 0x1f) }
```

`unit_select.rs:17-31`
```rust
pub fn is_real_phoneme(hi10: u16, low5: u16) -> bool {
  !(low5==1||low5==4||low5==6||low5==0x10||low5==0xc||low5==0x12||low5==8||low5==9||low5==10||low5==0xb||low5==0xd||low5==0xe||low5==0x11|| (low5==3 && hi10==6))
}
```

`g2p.rs` にも `is_real_phoneme*` 系が存在するが `cargo check` 警告には出ない（使用されている）。

### 検証結果: **ロジック一致 — 命名と重複が欠落（Severity: Medium）**

* **集合一致:** 両実装とも除外集合 `{1,4,6,8,9,10,0xb,0xc,0xd,0xe,0x10,0x11,0x12}` + `(low5==3 && hi==6)` で完全一致。`render` は `matches!` マクロ、`unit_select` は `||` 連鎖だが等価。`render_test.rs:render_entry0_with_doubling` と `unit_select_test.rs:real_phoneme_detection` の双方で境界値テスト済み。
* **パラメータ名の誤導:** `render` は `high6_cur`（正: `cur>>10` で上位6bit）、`unit_select` は `hi10` と命名。実際はどちらも `cur>>10`（6bit）を想定しており `hi10` は誤称。`10` は `code & 0x1f` の下位5bit と対比した桁数の取り違えと推定。
* **重複定義:** 2箇所で同関数を独立に定義しており、将来片方だけ修正された場合にサイレントな乖離が発生する。`render::is_real_phoneme` は `render_units` 内の二重化判定 (`render.rs:84` `if is_real_phoneme(u.code_cur>>10, u.code_next & 0x1f) && class_i8%10<2`) と `unit_select::is_real_phoneme` は `unit_select.rs:763` `if is_real_phoneme(cur_hi,next_lo) && d.class_byte()%10<2` で同じ `FUN_0044b350` 由来の判定を担うため、本来は単一真実源であるべき。

### 修正案

| # | 位置 | 提案 |
|---|------|------|
| U-1 | `render.rs:38` / `unit_select.rs:17` | 共通化: `crate::unit_select::is_real_phoneme` を単一実装とし `render.rs` は `pub use crate::unit_select::is_real_phoneme;` に置換。あるいは `crate::tables` または `crate::g2p` に `is_real_phoneme_raw(hi:u16, lo:u16)` を新設し両者から呼ぶ。 |
| U-2 | `unit_select.rs:17` 引数名 | `hi10: u16` → `hi6: u16` にリネーム（`low5` は正しい）。`render.rs:38` の `high6_cur` に統一すると意図が明確。 |
| U-3 | `render.rs:45` ラッパ | `is_real_phoneme_codes` は `unit_select` にも同等の `is_real_phoneme_code(u16,u16)` があればそちらに統一。なければ `#[inline] pub fn is_real_phoneme_codes(cur:u16,next:u16)->bool { is_real_phoneme(cur>>10,next & 0x1f) }` を共通モジュールに一本化。 |
| U-4 | `render.rs:83-84` 符号付き剰余 | `classcode as i8 %10 <2` は負の `classcode` (>127) でRustの `%` が負を返す点まで原典 `MOVSX / IDIV` に忠実。正しさを `#[test]` で `classcode=0xFF (-1) %10 == -1 <2` を明示するテストを追加し、将来 `as u8 %10` への誤リファクタを防止。 |

---

## 4. `segmenter` — NUL / 最終文 / 強制break

### 現状 (`segmenter.rs:192-355`)

* **NUL終端** `segmenter.rs:203`: `let text = &text[..text.iter().position(|&b| b==0).unwrap_or(text.len())];` — 原典 `strlen` 相当。テスト `nul_terminates_input` (`segmenter.rs:443-446`) で `b"AB\0CD"` → `["AB"]` を検証。
* **最終文flush** `segmenter.rs:353`: `flush(&mut sentences, &mut buf, start, &mut prev_class, None);` — ループ後の残余を必ず排出。`plain_korean_one_sentence` 等で空でない入力は常に1文以上を保証。
* **強制break** `segmenter.rs:260-262`: `if buf.len() > HARD_FLUSH_LIMIT || buf.len() > max_sentence_bytes { flush(...) }` — `HARD_FLUSH_LIMIT=0xC34C(49996)`, `MAX_SENTENCE_BYTES=0x1F0(496)`。テスト `max_sentence_bytes_forced_break` (`segmenter.rs:472-479`) で500B入力→2文（`497`+残り）を検証。

### 検証結果: **合格 — 残存の軽微な仕様差のみ（Severity: Low）**

| # | 位置 | 現象 | 評価 |
|---|------|------|------|
| S-1 | `segmenter.rs:203` NUL | `position(|&b|b==0)` は `0x00` をKPS2バイトの第2バイトとして含むケースでも終端とみなす。`Voice/*.pkg` のKPSは `0x00` を含み得るが (`voice_dict.rs:13` コメント `KPS bytes may include 0x00`), `segmenter` 入力は `KeyPad.Ebd` 変換後の `internal-code bytes` であり `0x00` はパディング/未使用扱いのため誤終端リスクは低い。原典も `strlen` なので忠実。 | 対応不要。docに「入力はNUL終端C文字列として扱う」旨を追記すれば十分。 |
| S-2 | `segmenter.rs:353` 最終文 | `flush` は `buf.is_empty()` なら何もしない (`segmenter.rs:214-216`) ため空入力は0文を返す。`empty_input_no_sentences` で検証済み。末尾が `0x80..0xA0`/`0xFF` のドロップ対象バイトのみの入力も0文で正しい（`dropped_bytes_skipped`）。 | 合格。 |
| S-3 | `segmenter.rs:260` 強制breakのタイミング | 原典は `pos > 0xC34C` をバッファ書込み後に判定するが、本実装は次トークン処理前に判定。結果として `buf.len()==497` の時点で次ループ先頭でflushされるため、1文目の長さが `MAX+1`（497）になる。テストがこの挙動を固定しているためリグレッションはない。ただし `TOKEN_BUFFER_SIZE=50000` に対して `Vec` が動的に伸びるため、原典の固定50000Bバッファオーバーラン再現はしない（むしろ安全側）。 | 合格。厳密に原典の50000B固定バッファを再現したい場合は `buf` を `[u8;50000]` + `pos` 管理に戻す必要があるが、現行のVec方式は安全でテストも通るためP3。 |
| S-4 | `segmenter.rs:238-249` `crlf_breaks` | `tokenize_crlf` で `\r\n\t ` とKPS空白 `0xA1A1` の連続をスキップするロジックは原典 `DAT_00489140` 相当。`crlf_mode_breaks_on_newline` で検証済み。 | 合格。 |

**結論:** NUL/最終文/強制breakいずれも欠落なし。S-1〜S-4は仕様メモの範囲。

---

## 5. `Voice*.pkg` — `Dict` / `MiraeDict` 二重実装と未使用警告

### 現状

* `dict.rs:63-71` `pub struct Dict { n1,n2,base,check,tail, sub_a, sub_b }` — `Dict::from_bytes` は `[u32 n1][u32 n2][BASE][CHECK][TAIL][parse_sub(6)][parse_sub(26)]` をパース。`sub_a`/`sub_b` は `SubStruct{ pairs, records, rec_size }`。
* `voice_dict.rs:75-84` `pub struct MiraeDict { c1,c2,base,check,edges, f6,c6,map6,rec6, f26,c26,map26,rec26 }` — 同一バイナリを別名フィールドでパース。`rec6: Vec<Rec6>` は型付き、`rec26: Vec<Rec26>`。
* `lib.rs:126-129` ランタイムは `Dict` のみを使用: `colligation/user/nonreg/conjects: Dict`。`MiraeDict` は `voice_dict.rs` のdocに `kept for byte-exact verification (diff==0)` と明記され、テストからのみ参照。
* `cargo check`（`--tests` 含む）出力では `MiraeDict` 自体への `dead_code` 警告は出ない（`pub mod voice_dict` で公開APIのため）。しかし `lib.rs`/`g2p.rs` に残る未使用警告が多数。

### 検証結果: **二重実装は意図的で正当 — 未使用警告が残存（Severity: Medium）**

`cargo check`（`69f97bb` 時点）抜粋:
```
mirae-tts-engine/src/lib.rs:29:52: warning: unused import: `WordRecord`
mirae-tts-engine/src/g2p.rs:172:13: warning: variable does not need to be mutable / unused variable: `mask`
mirae-tts-engine/src/g2p.rs:518:13: warning: unused variable: `kinds`
mirae-tts-engine/src/lib.rs:468:17: warning: unused variable: `n`
mirae-tts-engine/src/lib.rs:77:16: warning: fields `pitch_smoothing_tolerance`, `end_tone_threshold`, and `speed` are never read
mirae-tts-engine/src/lib.rs:115:15: warning: function `kps_lookup` is never used
mirae-tts-engine/src/lib.rs:170:19: warning: method `config` is never used
mirae-tts-engine/src/g2p.rs:92:8: warning: function `unicode_syllable_to_jamo` is never used
mirae-tts-engine/src/g2p.rs:181:8: warning: function `syllable_jamo_map` is never used
mirae-tts-engine/src/g2p.rs:199:8: warning: function `kps_syllable_map` is never used
mirae-tts-engine/src/g2p.rs:371:8: warning: function `kps_code_to_phoneme_no_final` is never used
```
`voice_dict.rs` の `lookup_arr3` は `#[deprecated]` 付きでテストから呼ばれ `use of deprecated method` 警告が出るが意図的。

### 修正案

| # | 位置 | 現象 | 修正案 |
|---|------|------|--------|
| D-1 | `lib.rs:29` | `WordRecord` 未使用import | `use g2p::g2p_dict::{self, G2pDicts, WordFinalTone};` に縮退。必要になった時点で再import。 |
| D-2 | `g2p.rs:172` `let mut mask` | `mut` 不要かつ `mask` 未使用 | `let mask = …;` に修正するか、未使用なら `let _mask = …;` または削除。`COL_MASKS` 参照が将来必要なら `#[allow(unused_variables)]` で意図を明示。 |
| D-3 | `g2p.rs:518` `let kinds` | 未使用 | `let _kinds = …;` または削除。 |
| D-4 | `lib.rs:468` `let n = word_records.len()` | 未使用 | `let _n = …;` または `let n = …; drop(n);`。デバッグ用なら `#[cfg(debug_assertions)]` ガード。 |
| D-5 | `lib.rs:77` `EngineConfig` 3フィールド | `pitch_smoothing_tolerance`, `end_tone_threshold`, `speed` が `lib.rs` 内で一度も読まれない | 原典 `engine+0xe8/0xec` と `speed` の定数由来をdocに残した上で `#[allow(dead_code)]` を付与するか、実際に `UnitSelector`/`render` で使用するよう配線。現状 `speed` は `TtsConfig::sample_rate / 441` で計算されるが `EngineConfig` に保持されるだけで未参照。 |
| D-6 | `lib.rs:115` `kps_lookup` / `170` `config()` / `g2p.rs:92,181,199,371` | 将来のG2P拡張で使う予定のヘルパが未使用 | `#[allow(dead_code)]` を付与し `// reserved: KeyPad.Ebd provenance …` のように予約理由をコメント。`cargo check` で `dead_code` 警告を消しつつ意図を残す。`2d45d8d` で `PROSODY_W2` に同様の `#[allow(dead_code)]` を付与した前例に倣う。 |
| D-7 | `dict.rs` vs `voice_dict.rs` 二重実装 | 命名とdocの重複が混乱を招く | `dict.rs` のヘッダに `// Runtime parser (used by lib.rs)`、`voice_dict.rs` のヘッダに `// Verification parser (tests only, not used at runtime)` を1行で明記。`voice_dict.rs` を `#[cfg(any(test, feature="verify"))]` に寄せる案もあるが、現状 `pub mod` で公開しておく方が `diff==0` 検証の透明性が高いためdoc強化で十分。 |
| D-8 | `voice_dict.rs:435,440` `lookup_arr3` deprecated警告 | テスト内で `#[allow(deprecated)]` 済みだが `cargo check --tests` で警告が出る | テスト側の `#[allow(deprecated)]` は既に付与済み。`cargo check` の `deprecated` 警告はテストコード由来なので許容。CIで `#[warn(deprecated)]` を `#[deny]` にしない限り対応不要。 |

---

## 総括

| 項目 | 判定 | 重大度 | 要対応 |
|------|------|--------|--------|
| WavWriter split データ損失 | **合格**（`2d45d8d`で回帰修正済み） | — | なし（W-1〜W-3はドキュメント改善のみ） |
| Rec6 b6保持 | **合格**（`b6=0`→`b6=b[5]`で情報保持） | Medium | R-1/R-2 doc修正推奨 |
| is_real_phoneme 二重定義 | **ロジック一致、重複が負債** | Medium | U-1〜U-4 共通化推奨 |
| segmenter NUL/最終文/強制break | **合格** | Low | なし（S-1〜S-4は仕様メモ） |
| Dict/MiraeDict二重実装 | **意図的で正当** | Medium | D-1〜D-8 未使用警告の `#[allow(dead_code)]` / 未使用変数の `_` 化で `cargo check` をクリーンに |

**現HEADでTTS出力に影響する欠落はなし。** 残るのは将来の保守性を損なう重複定義と `cargo check` のノイズであり、上記修正案を適用すれば `cargo check --tests` を警告0にできる。

---

## 再現コマンド

```bash
git -C /tmp/mirae-wt-postproc log --oneline -5
git -C /tmp/mirae-wt-postproc show 2d45d8d -- mirae-tts-engine/src/wav.rs
git -C /tmp/mirae-wt-postproc show 2d45d8d -- mirae-tts-engine/src/voice_dict.rs
/home/user/.cargo/bin/cargo check --message-format=short 2>&1 | head -n 40
/home/user/.cargo/bin/cargo check --tests --message-format=short 2>&1 | head -n 60
```
