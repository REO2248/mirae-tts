# Segment→G2P 橋渡し検証 — NUL終端 / 最終文flush / 小数is_decimal_point / unit merge / tone_class伝播

**対象:** `mirae-tts-engine/src/{segmenter.rs, lib.rs: sentence_to_records, g2p.rs, record.rs, tone.rs}`  
**HEAD:** `69f97bb` (Merge postprocess into main) + dirty `lib.rs: truncate_last_line_char` 修正（`69f97bb` の親 `2d45d8d` 相当を追補 — 本検証は dirty 込みの現行HEADで実施）  
**検証日:** 2026-08-19  
**検証手法:** Rust 行追跡 + `cargo test -p mirae-tts-engine` 部分実行（`--lib` 55, `segmenter_test` 8, `g2p_test` 20, `g2p_dict_test` 21, `number_unit_test` 7, `sandhi_rules_test` 16 = 127 + 55 = 列挙分は全PASS） + Python で segment→G2P 境界の手動再現  
**先行レポート:** `reports_verify/01_truncate_and_segment.md`（最終文字欠落の横断検証）

---

## 0. 要約（TL;DR）

| 橋渡し観点 | 行 | 判定 | 補足 |
|---|---|---|---|
| NUL 終端 (`segmenter::tokenize`) | `segmenter.rs:203` | **SAFE** | `position(\|b==0)` で NUL 後を切るのは仕様。KPS9566 有効範囲内テキストでは発火しない。現行テスト `nul_terminates_input` が仕様を固定。 |
| `truncate_last_line_char`（合成前） | `lib.rs:51` | **FIXED** | 旧実装は `/\\n/\\r` 除去後に末尾1文字を必ず削除（`가나다` → `가나`）。現行 `trim_end_matches(['\\n','\\r'])` のみで修正。dirty 未コミットだが HEAD 差分は正。 |
| 最終文 flush | `segmenter.rs:353` | **SAFE** | `while pos<len` 終了後に `flush(..., None)`。空 buf は no-op。最終音節欠落なし。 |
| 小数 `is_decimal_point` | `lib.rs:340` | **SAFE** | `b'.' && next_token_class==4` のみ非句点。`frac_end==bytes.len()` の末尾小数も `decimal_codes` で保持。 |
| unit merge（数詞+単位連結） | `lib.rs:383-439` | **要観察だが最終音節欠落なし** | `frac.is_empty` 分岐 + `frac_end=wpos` で橋渡し。末尾は `L502 tone_class=... +4` で上書き。既知の緩いガードはあるが末尾消滅ではない。 |
| tone_class 伝播（グループ末尾/clause末） | `lib.rs:478,492,502` + `record.rs` + `tone.rs:apply_sandhi` | **SAFE** | 空でない限り末尾 `+4`、60音節 chunk で `tone_class=3`、末尾5音節 `flags=1`。`apply_sandhi` は空文早期returnで真正音節を消さない。 |
| **最終音節欠落の有無** | 全経路 | **欠落なし（現行）** | 旧 `truncate_last_line_char` だけが欠落経路だった。segmenter/G2P/tone では再現しない。 |

**結論: 現行コードで「最終音節が消える」欠落は再現しない。唯一の欠落経路だった `truncate_last_line_char` は修正済み。以降の `segment→G2P→tone` 橋渡しで二次的な消滅経路は見つからなかった。**

---

## 1. NUL 終端

### 1.1 コード

```rust
// segmenter.rs:203
let text = &text[..text.iter().position(|&b| b == 0).unwrap_or(text.len())];
```

- オリジナル `FUN_00402240` の `strlen` セマンティクス忠実移植。C 文字列由来バッファの NUL を文境界とみなす。
- 典型的な `keypad.convert_str` 出力（KPS9566 有効範囲 `0x80` 未満は ASCII、`0xA1..0xFE` の 2-byte）は NUL を含まないため通常発火しない。

### 1.2 テスト

```rust
// segmenter.rs:443
fn nul_terminates_input() {
    let t = tokenize(b"AB\0CD");
    assert_eq!(texts(&t), vec![b"AB".to_vec()]);
}
```

- NUL 後切捨ては意図仕様。UI 由来の余剰 `b'\0'` が後段 G2P に到達しないことを保証。**欠落ではない**。

### 1.3 橋渡し影響

- NUL を含む異常入力では `text.len()` が短縮され、`pos<len` ループは短縮長で終了 → 最終 `flush` で残りを排出。真正音節が NUL より後にあるケースは仕様外（外部で NUL 混入させない前提）。

---

## 2. `truncate_last_line_char`（合成前の最終行処理）

### 2.1 旧バグ（`64edf31` → `2d45d8d` 修正前）

```rust
// lib.rs 旧実装（HEAD~1）
fn truncate_last_line_char(text: &str) -> &str {
    let end = text.trim_end_matches(['\n','\r']).len();
    if end == 0 { return text; }
    let last_char_len = text[..end].chars().next_back().map(|c| c.len_utf8()).unwrap_or(0);
    &text[..end - last_char_len]   // ← 常に1文字削除
}
```

`trim_end_matches` で `\n/\r` を除いた後に **必ず末尾1文字を余分に切り**、`가나다` → `가나` となる。`01_truncate_and_segment.md §2` で同バグを確認・レポート済み。

### 2.2 現行（dirty HEAD）

```rust
// lib.rs:51
pub(crate) fn truncate_last_line_char(text: &str) -> &str {
    // Strip only trailing \n/\r characters...
    text.trim_end_matches(['\n', '\r'])
}
```

- `\n/\r` のみを除去。真正文字は削らない。
- `pub(crate)` 化で回帰テスト可能に（`06_cross_and_history.md` でも dirty 未コミットを指摘済み — 要 `git commit` 前 push）。

### 2.3 合成パイプラインでの位置

```rust
// lib.rs:193 synthesize_bytes
fn synthesize_bytes(&mut self, text: &str) -> io::Result<Vec<u8>> {
    let text = truncate_last_line_char(text);
    let internal = self.keypad.convert_str(text);
    let sentences = segmenter::tokenize(&internal);
```

- 全 segment/G2P より前。旧バグではここで最終音節が **segmenter に入る前に消滅**していた。現行は消滅なし。

---

## 3. 最終文 flush

### 3.1 コード

```rust
// segmenter.rs:193-353
pub fn tokenize(text: &[u8]) -> Vec<Sentence> {
    tokenize_with(text, false, MAX_SENTENCE_BYTES)
}
pub fn tokenize_with(text: &[u8], crlf_breaks: bool, max_sentence_bytes: usize) -> Vec<Sentence> {
    let text = &text[..text.iter().position(|&b| b == 0).unwrap_or(text.len())];
    // ...
    fn flush(sentences: &mut Vec<Sentence>, buf: &mut Vec<u8>, start: usize, prev_class: &mut u8, delim: Option<&[u8]>) {
        if buf.is_empty() { return; }           // 空は no-op
        if let Some(d) = delim { buf.extend_from_slice(d); }
        sentences.push(Sentence { text: std::mem::take(buf), start });
        *prev_class = 0;
    }
    // ...
    while pos < len { /* ... boundary flushes ... */ }
    flush(&mut sentences, &mut buf, start, &mut prev_class, None); // ← 最終文を必ず排出
    sentences
}
```

- 句点 `!.?` / KPS 句点 `0xA1A5/0xA1A9/0xA1AA` での `flush(..., Some(delim))` と区別し、ループ後の無条件 `flush(..., None)` が **句点なし末尾文**を回収。
- `buf.is_empty()` ガードで空文生成なし。
- 強制 flush（`HARD_FLUSH_LIMIT=0xC34C`, `MAX_SENTENCE_BYTES=0x1F0`）も `flush(..., None)` で同等の最終回収を経る。

### 3.2 エッジ

| 入力 | 期待 | 根拠 |
|---|---|---|
| `b"가"` 相当（句点なし、単一音節） | 1 sentence, `text=[가]` | `plain_korean_one_sentence` テストで同等を検証 |
| `b"Hi. Bye!"` | `["Hi.", " Bye!"]` | `sentence_start_offsets` / `ascii_period_space_breaks` |
| `b""` / `b"\x80\xff"` | 0 sentences | `empty_input_no_sentences` |
| `0xA1A1`（KPS 空白）のみの末尾 | KPS 空白は `is_continue_kps` で文継続だが、pos==len で最終 flush→文内に残る | `kps_space_continues_sentence` |

- いずれも最終 flush で欠落なし。

---

## 4. 小数 `is_decimal_point`（`sentence_to_records`）

### 4.1 コード

```rust
// lib.rs:334-348  class<=3 分岐（数字・句読点・空白の class 1..3）
if class <= 3 {
    let b0 = bytes[pos];
    let is_period = b0 == b'.'
        || (b0 >= 0xA1 && len==2 && ((b0 as u16)<<8 | bytes[pos+1] as u16)==KPS_FULL_STOP);
    if is_period && !groups.last().map_or(true, |g| g.0.is_empty()) {
        let is_decimal_point = b0 == b'.' && {
            let (nc, _) = next_token_class(&bytes[pos+len..]);
            nc == 4   // 次 token が数字 class
        };
        if !is_decimal_point {
            let last = groups.last_mut().unwrap().0.last_mut().unwrap();
            last.tone_class = (last.tone_class/10)*10 + 4; // 句点の tone 付与
        }
    }
    pos += len;
    continue;
}
```

```rust
// lib.rs:353-381  class==4（数字）分岐 — 小数パース
let digits: Vec<u8> = bytes[start..pos].iter().map(|b| b-0x30).collect();
let mut frac_end = pos;
let mut frac: Vec<u8> = Vec::new();
if pos < bytes.len() && bytes[pos]==0x2E {
    let mut p = pos+1;
    while p < bytes.len() {
        let (c,l) = next_token_class(&bytes[p..]);
        if c != 4 { break; }
        frac.push(bytes[p]-0x30);
        p+=l;
    }
    if !frac.is_empty() { frac_end = p; }
}
let codes = if frac.is_empty() {
    g2p_dict::sino_integer_codes(&digits)
} else {
    g2p_dict::decimal_codes(&digits, &frac)
};
```

### 4.2 検証

| ケース | `is_decimal_point` | tone 動作 | 小数 codes | 欠落 |
|---|---|---|---|---|
| `"3.14"`（文全体が小数） | loop 前では `groups` 空なので `is_period` ガードで skip。digit 分岐で `digits=[3], frac=[1,4]` → `decimal_codes` | digit block が1グループ内に `decimal_codes` 分の records を生成。末尾は `L502` で `tone +4` | 保持 | なし |
| 文末 `"… 3.14"`（小数で終わる） | `b'.'` 直後の `next_token_class==4` → `true` で tone 付与を抑止（正しい小数点扱い）。`frac_end == bytes.len()` で `next_token_class(&bytes[frac_end..])` は `(0,0)` → `nc != 0x19` → `word_ends` push | `decimal_codes` 全体が `codes` として展開。`frac_end` を `pos` に反映 | 保持 | なし |
| `"3. "`（数字+ピリオド句点） | `next_token_class==1`（空白 class 1）→ `false` → tone+4 付与 | `frac.is_empty()` → `sino_integer_codes` | 保持 | なし |
| `KPS_FULL_STOP (0xA1A5)` | `b0==b'.'` ではないので `is_decimal_point=false` → 常に句点扱い | `next_token_class` の小数例外は ASCII `.` のみに限定。KPS 句点は常に境界 | — | なし（仕様） |

- `is_decimal_point` が `false` に倒れても小数 block 自体は `class==4` 分岐で処理されるため、二重欠落は起きない。
- 末尾小数では `frac_end == len` 後の `next_token_class(&bytes[frac_end..])` が空を返し、`is_merged||nc!=0x19` で `word_ends` を確実に記録（`L439-445`）。

### 4.3 テスト裏付け

- `segmenter::decimal_point_stays_inline`（`segmenter.rs`）: `"3.14"` が one-sentence に留まることを保証（segment 層）。
- `number_unit_test` / `g2p_dict_test` で `decimal_codes` 系は exercise 済み（全PASS）。

---

## 5. unit merge（数詞+助数詞連結）

### 5.1 コード

```rust
// lib.rs:383-439
let mut merged_codes: Option<Vec<u16>> = None;
if frac.is_empty() && !codes.is_empty() {
    let last_kps = *g2p_dict::sino_integer_kps_syllables(&digits).last().unwrap();
    let lcls = g2p_dict::kps_final_class(last_kps);
    if matches!(lcls, 0|5|15) {                          // 받침なし/ㄴ/ㄹ のみ
        let (nc,_nl) = next_token_class(&bytes[pos..]);
        if nc == 0x19 {                                  // 次が音節
            // wpos まで 0x19 を消費
            // kps_bytes_to_codes → to_phoneme_code → (fcls==27 && finit==18 && is_func_medial)
            // を満たすときのみ word_g2p で連結し、All = sino + rec を生成
            merged_codes = Some(rec.phoneme_codes.clone());
            frac_end = wpos;                              // ← 単位語を pos（frac_end）に含める
        }
    }
}
let is_merged = merged_codes.is_some();
let codes = merged_codes.unwrap_or(codes);
let n_codes = codes.len();
for (i, code) in codes.into_iter().enumerate() {
    let mut rec = ProsodyRecord::new(code);
    rec.init_from_marker(if is_merged && i+1==n_codes {1} else {0}, false);
    groups.last_mut().unwrap().0.push(rec);
}
if let Some(g) = groups.last_mut() {
    let (nc,_nl) = next_token_class(&bytes[frac_end..]);
    if is_merged || nc != 0x19 {
        g.1.push(g.0.len());
    }
}
pos = frac_end;
continue;
```

### 5.2 橋渡し追跡

```
"... 3 개 ..."  →  digit block [3] + word "개"  →  mergedCodes = sino[3] + word_g2p("개")  →  frac_end = wpos  →  pos=frac_end  →  次wordはスキップ（posが進んでいる）
"... 3개"（分かち書きなし）→  同上。内部 code 上は同等の連結読み。
"3.14개"（小数+単位）→  frac 非空なので mergedCodes 分岐に入らない（frac.is_empty がゲート）。小数は単位と連結しない。
```

- マージ成功時は `frac_end` を `wpos` に進めるため、後続の `while class==0x19` ワード消費が **二重に word_g2p しない**（`pos=frac_end` でスキップ）。
- マージ失敗時（`lcls` 不適合 / `nc!=0x19` / `is_func_medial` 不成立）も `codes` は `sino_integer_codes` のまま保持されるので **数詞が消えることはない**。

### 5.3 最終音節との関係

- 数詞+単位が文末にあるケース（例 `"3개"` で終わる）でも:
  1. `groups` の末尾 records は数詞+単位連結分すべてを含む。
  2. 直後の `pos==len` ループ脱出後、`L502` の `last.tone_class = /10*10+4` が **最終 records（単位の末尾）** に付与される。
  3. `CHUNK` / `PROPAGATE` も `word_ends` 上で処理され、文末は `L502` が上書き優先。
- = **単位 merge の有無にかかわらず最終音節は保持**。merge 条件が緩い/厳しいことの副作用は「読みが連結されるかどうか」だけで、消滅ではない。

### 5.4 既知の緩い分岐（`03_alphabet_digit_unit.md`指摘の関連）

- `lcls in {0,5,15}` ガードは原典の ㄴ/ㄹ 連結規則の一部しかカバーしない可能性（他終声での連結は非対応）。**欠落ではなく不連結**（`"3명"` 等が連結されずに別読みになる）として表出する。要観察だが本検証の「欠落」観点では SAFE。

---

## 6. tone_class 伝播

### 6.1 生成側（`sentence_to_records` → `record.rs`）

```rust
// record.rs: init_from_marker
fn init_from_marker(&mut self, marker_byte: u8, sentence_final: bool) {
    self.flags = (marker_byte >> 7) & 1;
    let m = marker_byte & 0x7F;
    self.tone_class = tone::initial_tone_class(m);
    if m==0 && sentence_final { self.tone_class = 1; }
}
```

```rust
// tone.rs: initial_tone_class (FUN_0044ca50)
pub const fn initial_tone_class(marker: u8) -> u8 {
    match marker {
        0 => 0, 1 => 1, 2|5 => 3, 3 => 2, 6 => 5, 7 => 4, _ => 0,
    }
}
```

- `word_record_from_readings_final` で各 `WordRecord` の末尾に `final_tone.marker()`（Mid=1/Comma=2/ClauseEnd=7/Bracket=5）を付与。`record_to_prosody` で `init_from_marker` 経由で `tone_class` に変換。
- `sentence_to_records` ではさらに以下で **上書き伝播**：

| 位置 | コード | 意味 |
|---|---|---|
| `lib.rs:324` / `346` | `last.tone_class = /10*10+4` | 改行・句点で直前 record の tone を 4（平坦/句点末）に |
| `lib.rs:478-485` | `if cum >= CHUNK_SYLLABLES(60) { last.tone_class=3; cum=0; }` | 60音節 chunk 境界で tone=3（軽い区切り） |
| `lib.rs:488-499` | 末尾5音節相当を逆順に `flags=1` | `PROPAGATE_BACK=5` の後方伝播（有声/連結フラグ） |
| `lib.rs:502` | `last.tone_class = /10*10+4` | 文末最終 record を句点末 4 に固定（空でなければ必ず） |

- `L502` が最終手段で文末 tone を 4 に倒すため、数詞/小数/unit merge のいずれで終わっても末尾は clause-end tone として閉じる。**欠落ではなく tone 値の上書き**。

### 6.2 sandhi 側（`tone::apply_sandhi`）

```rust
// tone.rs
pub fn apply_sandhi(buf: &mut Vec<ProsodyRecord>, sentence: &mut [ProsodyRecord]) {
    let ac = buf.len();
    let n = sentence.len();
    if n==0 { return; } // 空文は no-op（欠落を生まない）
    // ...
    // ac==0（第一文）: sentence[0].tone_class = %10 + 0x28 (40台)
    // ac>0: 最初の record は前文末尾の tone を引き継ぐ
}
```

- 空 sentence は早期 return で `buf` を変えない。
- 単一 record 文でも `sentence[0].tone_class` は `0x28 + tone` または前文末 `tone*10 + ...` に正規化され、消えることはない。
- `prev_non_pause` は `code==0x1486`（pause/無声音）のスキップのみ。真正音節は飛ばさない。

### 6.3 unit_select / render への引継ぎ

- `lib.rs: synthesize_bytes` で `all_records` → `sel.process(&recs)` → `render_units`。`unit_select` の `BOUNDARY_CODE` は次音素不在時の番兵で現行音素を消さない（`03`/`04` レポートで確認済み）。

---

## 7. 最終音節欠落の有無 — 経路別判定

| 経路 | 判定 | 根拠 |
|---|---|---|
| 入力全体が `가나다`（句点なし、3音節、通常分かち） | **欠落なし** | truncate修正後、`convert_str` → `tokenize` 1文 → `sentence_to_records` 3 records → `L502` tone+4 → sandhi/ render 保持 |
| 入力 `가나다\n`（末尾改行） | **欠落なし** | `truncate_last_line_char` で `\n` のみ除去。3音節保持 |
| 入力 `가나다.`（句点付き） | **欠落なし** | `tokenize` で句点は文内 `flush(..., Some("."))` → `sentence_to_records` の `class<=3` 句点分岐で前 record tone+4 → `L502` で二重に+4だが値は4で安定 |
| 入力 `3.14`（小数で終わる） | **欠落なし** | `segmenter` は one-sentence、`sentence_to_records` digit 分岐で `decimal_codes`。小数点は `is_decimal_point` で非句点化 |
| 入力 `3개`（数詞+単位で終わる） | **欠落なし** | 分岐成否にかかわらず `codes` 保持。末尾は `L502` で閉じる |
| 入力 `60+音節長文`（chunk境界跨ぎ） | **欠落なし** | 60音節ごとに `tone=3` を挿すが、records は削らない。末尾は `L502` で+4 |
| 入力 `\0` 混入 | **仕様内切捨て** | NUL 後は `tokenize` で切捨て。KPS9566 正常入力では非該当 |

**旧 `truncate_last_line_char` を除き、現行の segment→G2P 橋渡しで最終音節が消える経路は見つからなかった。**

---

## 8. 残存する軽微な観察（欠落ではない）

| # | 位置 | 現象 | 影響 | 推奨 |
|---|---|---|---|---|
| G-1 | `lib.rs:468 let n = word_records.len();` 未使用 | `n` 未使用警告（`cargo test --lib` で検出） | 無害 | `let _n` 化 or 削除 |
| G-2 | `segmenter.rs:353` 最終 flush の `start` | 前文の `start` を引き継ぐが、空 flush では未使用。非空時の `start` は正 | 無害 | 現状維持 |
| G-3 | unit merge の `lcls in {0,5,15}` | 連結条件が限定的（報告 `03` でも指摘）。他終声の助数詞は連結しない | 読みの分離（欠落ではない） | 要仕様確認（`03` 対応と統合） |
| G-4 | `truncate_last_line_char` の dirty 未コミット | `01`/`06` で P1 として指摘済み。現行 main `69f97bb` は旧コードのままに見えるが、本検証は dirty 込みで実施 | push 前にコミット要 | `git commit -m "fix: truncate_last_line_char — don't drop last char"` |
| G-5 | `is_decimal_point` の `nc==4` 判定 | `next_token_class` を再計算するが、segment 側の `'.'` 例外（`prev_class==4/7/0x19`）とは独立。二重判定だが整合 | 無害 | 現状維持（両層で小数を inline に保つ） |

---

## 9. 検証証跡

- `cargo test -p mirae-tts-engine --lib` — 55 passed, 0 failed
- `cargo test -p mirae-tts-engine --test segmenter_test` — 8 passed
- `cargo test -p mirae-tts-engine --test g2p_test` — 20 passed
- `cargo test -p mirae-tts-engine --test g2p_dict_test` — 21 passed
- `cargo test -p mirae-tts-engine --test number_unit_test` — 7 passed
- `cargo test -p mirae-tts-engine --test sandhi_rules_test` — 16 passed
- `git diff HEAD` — `lib.rs: truncate_last_line_char` の 旧 `end - last_char_len` → `trim_end_matches` 修正を確認
- `segmenter.rs:203` NUL 切捨て / `segmenter.rs:353` 最終 flush / `lib.rs:340` `is_decimal_point` / `lib.rs:383-439` unit merge / `lib.rs:502` 最終 tone+4 の行追跡は本レポート §1-6 に記載

---

## 10. 判定サマリ

```
NUL終端:           SAFE（仕様、通常非発火）
truncate:          FIXED（dirty 修正で解消、要コミット）
最終文flush:       SAFE（pos==len 後の flush で保持）
小数is_decimal:    SAFE（小数点は非句点、decimal_codes 保持）
unit merge:        SAFE（成否とも codes 保持、最終 tone+4 で閉じる）
tone_class伝播:    SAFE（空でない限り末尾+4、chunk/propagate は付与のみ）
最終音節欠落:      なし（現行で再現せず）
```

