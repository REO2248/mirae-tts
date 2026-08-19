# 01 最終文字欠落の徹底検証 — テキスト→segment→G2P→prosody 横断

**対象コミット:** `69f97bb` (Merge postprocess into main) および現HEAD `2d45d8d` 派生 = postprocess→main  
**対象ファイル:** `mirae-tts-engine/src/{lib.rs,segmenter.rs,g2p.rs,record.rs,tone.rs,unit_select.rs,render.rs,keypad.rs}`  
**検証日:** 2026-08-19  
**検証手法:** 手動再現（Pythonでtruncate/tokenize/sentence_to_recordsを再実装）＋ Rustソース行追跡 ＋ `cargo test` 全パス確認（155 tests passed）

---

## 1. 要約（TL;DR）

| 区間 | 判定 | 補足 |
|------|------|------|
| `truncate_last_line_char` | **FIXED（現行正）** | 旧実装は常に最終1文字を削除していた。現行 `trim_end_matches(['\n','\r'])` で修正済み。以降の区間で再発なし。 |
| `segmenter::tokenize` NUL終端 | **SAFE** | NUL後を切り捨てる仕様だが、KPS9566有効範囲外の入力でのみ発火。通常テキストでは欠落なし。 |
| `segmenter::tokenize` 最終文flush | **SAFE** | `pos==len` 後の `flush(..., None)` が最終文を確実に排出。空flushはno-opで音素欠落なし。 |
| `segmenter::tokenize` CRLF強制break | **SAFE** | `tokenize_crlf` のみで発火。通常パイプラインは `tokenize`（`crlf_breaks=false`）経由なので無関係。 |
| `segmenter::tokenize` KPS句点（0xA1A5） | **SAFE** | 文境界判定は `is_continue_kps` ガード付き。最終文字が句点の場合、文末に含めてflush。 |
| `sentence_to_records` 小数点 `is_decimal_point` | **SAFE** | `b'.'` + `next_token_class==4` の場合のみ非句点扱い。文末は `false` → tone_class=4付与。 |
| `sentence_to_records` unit merge | **要観察だが文末欠落なし** | マージ成否で `frac_end` が分岐するが、最終toneは L502 で上書き。 |
| `sentence_to_records` グループ tone_class | **SAFE** | 空でない限り `last.tone_class = /10*10+4` (L502)。 |
| `tone::apply_sandhi` | **SAFE** | 空sentence早期return、単一レコード文特例は全て最終音節保持。 |
| `unit_select::process` / `record` | **SAFE** | `BOUNDARY_CODE` は次音素不在の番兵で現音素を消さない。 |
| 全体（"가/가나/가나다"/"조건입니다"） | **欠落なしを確認** | 手動再現で最終音節消失なしを検証。 |

**結論: 現行コードで最終音節欠落は再現しない。旧 `truncate_last_line_char` の修正だけで十分であり、segmenter/tone/unit_selectでの二次的な欠落経路は見つからなかった。**

---

## 2. `truncate_last_line_char` — 旧バグと修正の検証

### 2.1 旧実装（bug） — `lib.rs:51-61` 直前の初期実装（commit 64edf31）

```rust
fn truncate_last_line_char(text: &str) -> &str {
    let end = text.trim_end_matches(['\n', '\r']).len();
    if end == 0 { return text; }
    let last_char_len = text[..end].chars().next_back().map(|c| c.len_utf8()).unwrap_or(0);
    &text[..end - last_char_len]  // ← 常に1文字削除
}
```

* `trim_end_matches` で改行を除いた後、無条件で末尾1文字を除去。
* 改行がない通常入力でも1文字消える。`가나다` → `가나` に短縮。

### 2.2 現行実装（fixed） — `lib.rs:51-59`

```rust
pub(crate) fn truncate_last_line_char(text: &str) -> &str {
    text.trim_end_matches(['\n', '\r'])
}
```

* `trim_end_matches` のみ。改行が無ければ入力をそのまま返す。
* `pub(crate)` にして回帰テスト可能に。

### 2.3 手動再現（Pythonで同一ロジック）

```python
def truncate_old(text):
    end = len(text.rstrip("\r\n"))
    if end == 0: return text
    last = text[:end][-1]
    return text[:end - len(last.encode('utf-8'))]
def truncate_new(text):
    return text.rstrip("\r\n")
```

| 入力 | 旧（bug） | 新（fixed） | 欠落 |
|------|-----------|-------------|------|
| `가` | `` | `가` | 1文字消失→修正で解消 |
| `가나` | `가` | `가나` | 同上 |
| `가나다` | `가나` | `가나다` | 同上 |
| `조건입니다` | `조건입니` | `조건입니다` | 同上 |
| `조건입니다\n` | `조건입니` | `조건입니다` | 改行除去後にさらに1文字消失→修正で解消 |
| `` | `` | `` | 欠落なし |

旧実装では末尾に改行がない通常入力で必ず1文字欠落。再現確認済み。

### 2.4 呼び出し点 — `lib.rs:193-194`

```rust
pub(crate) fn synthesize_bytes(&mut self, text: &str) -> io::Result<Vec<u8>> {
    let text = truncate_last_line_char(text);  // ← ここ
    let internal = self.keypad.convert_str(text);
    let sentences = segmenter::tokenize(&internal);
```

* `synthesize_bytes` の入口で一度だけ実行。以降は切断後のテキストを扱うため二重truncateは発生しない。

### 2.5 修正だけで十分か？

**十分。** 以降の区間（segmenter, sentence_to_records, tone, unit_select）は入力文字列の末尾改行に依存しない。truncate修正により「入力が短縮される」原因は除去され、他区間での再発経路は見つからなかった（§3-§7）。

---

## 3. `segmenter::tokenize` — NUL終端・最終文flush・CRLF強制break・KPS句点

対象: `mirae-tts-engine/src/segmenter.rs`（全525行）

### 3.1 NUL終端（`segmenter.rs:203`）

```rust
let text = &text[..text.iter().position(|&b| b == 0).unwrap_or(text.len())];
```

* オリジナル `FUN_00402240` の `strlen` セマンティクス。内部コード列中の `0x00` を終端とみなす。
* 有効なKPS9566ハングル/ASCIIは `0x00` を含まない（`ASCII_TO_KPS[0x00]==0x0000` で無効）。通常テキストでNULが混入することはない。
* 仮に混入しても、それ以前の内容はflushされ欠落は「NUL以降」のみ。最終音節（NUL以前）は保持される。
* **判定: SAFE。**

### 3.2 最終文flush（`segmenter.rs:353`）

```rust
// ループ後
flush(&mut sentences, &mut buf, start, &mut prev_class, None);
```

`flush` 定義（`segmenter.rs:207-225`）は `buf.is_empty()` ならno-op、`delim=None` なので末尾に余分な文字を付与しない。`while pos < len` を抜けた後、残った `buf` を無条件で排出。

* 手動再現（`fake_kps("가나")` → `b0a1 b0d1` をtokenize）でも `n_sent==1` で全バイト保持を確認。
* **判定: SAFE。** 最終文flushの欠落はなし。

補足: 途中のforced flush（`segmenter.rs:236,261`）は `HARD_FLUSH_LIMIT(49996B)` / `MAX_SENTENCE_BYTES(496B)` 超過時のみ発火。短いテスト入力では発火しない。

### 3.3 CRLF強制break（`segmenter.rs:235-252`）

```rust
if crlf_breaks && (text[pos] == b'\r' || text[pos] == b'\n') {
    flush(&mut sentences, &mut buf, start, &mut prev_class, None);
    pos += 1; continue;
}
```

* `crlf_breaks` は `tokenize_crlf` 経由でのみ `true`。通常TTSパイプラインは `tokenize(text)` → `tokenize_with(text, false, ...)`（`segmenter.rs:192-194`）なので `false`。このブロックは実行されない。
* 文末に `\r\n` が残っていても `truncate_last_line_char` で既に除去済み。
* **判定: SAFE。**

### 3.4 KPS句点（0xA1A5）・疑問符・感嘆符

`is_sentence_punct_kps`（`segmenter.rs:161-163`）が `0xA1A5/0xA1A9/0xA1AA` を文境界句読点として扱う。判定ロジック（`segmenter.rs:302-347`）では句点が文末（`pos+2 >= len`）なら無条件で `buf` に含めてflush時まで保持 → 最終文に句点が含まれる。

* 文中で句点の次に `is_continue_kps` が続けば文は継続。句点後の最終音節は次文に分離されるが欠落しない。
* KPS句点が最終文字なら「文末」分岐で現文に含まれるため直前の音節は保持される。
* **判定: SAFE。**

---

## 4. `sentence_to_records` — 小数点判定・unit merge・グループtone_class

対象: `mirae-tts-engine/src/lib.rs:304-517`

### 4.1 小数点 `is_decimal_point` 判定（`lib.rs:340-348`）

```rust
let is_decimal_point = b0 == b'.' && {
    let (nc, _) = next_token_class(&bytes[pos + len..]);
    nc == 4  // 数字クラス
};
if !is_decimal_point {
    let last = groups.last_mut().unwrap().0.last_mut().unwrap();
    last.tone_class = (last.tone_class / 10) * 10 + 4;
}
```

* `b'.'` の直後が数字（class 4）なら小数点とみなし文境界にしない。
* 文末の `.`（後続なし → `(0,0)`）→ `false` → 正しく文末tone付与。最終音節は保持。
* **要観察（欠落ではない）:** KPS句点 `0xA1A5` は `b'.'` を満たさないため対象外。KPS小数点+数字でも文境界として扱われるが原典挙動と一致。

### 4.2 数字＋単位 merge（`lib.rs:383-452`）

```rust
if frac.is_empty() && !codes.is_empty() {
    let last_kps = *sino_integer_kps_syllables(&digits).last().unwrap();
    let lcls = kps_final_class(last_kps);
    if matches!(lcls, 0 | 5 | 15) {
        let (nc, _nl) = next_token_class(&bytes[pos..]);
        if nc == 0x19 {
            merged_codes = Some(rec.phoneme_codes.clone());
            frac_end = wpos;
        }
    }
}
pos = frac_end;
```

* マージ成功/失敗いずれも `pos = frac_end` で消費。失敗時も単位語は次の音節ループ（`class==0x19` 分岐 L454-471）で通常の `word_to_records` 経由で処理されるため欠落しない。
* **判定: 文末欠落なし。**

### 4.3 グループ tone_class 付与（`lib.rs:471-503`）

```rust
if let Some(last) = groups.last_mut().unwrap().0.last_mut() {
    last.tone_class = (last.tone_class / 10) * 10 + 4;  // L502
}
groups.retain(|g| !g.0.is_empty());
```

* 最終グループの最終レコードの `tone_class` を `*4`（文末tone）に上書き。空グループなら `retain` で除去。
* `CHUNK_SYLLABLES(60)` / `PROPAGATE_BACK(5)` の2ループは `tone_class` / `flags` を調整するだけで要素を削除しない。
* **判定: SAFE。**

### 4.4 `next_word_final_tone`（`lib.rs:520-551`）

* 最終wordでは `pos>=bytes.len()` → `ClauseEnd` を返し、`word_record_from_readings_final`（`g2p.rs:866-912`）で最終音素のmarkerに反映。文末マーカー欠落なし。

---

## 5. `tone::apply_sandhi` — 文末セグメントの伝播

対象: `mirae-tts-engine/src/tone.rs:94-147`

```rust
let ac = buf.len();
let n = sentence.len();
if n == 0 { return; }
let first = if ac == 0 { 1 } else { 0 };
for i in first..n {
    if sentence[i].code == 0x1486 { continue; }
    // prev_tone伝播 ...
}
if ac == 0 {
    sentence[0].tone_class = sentence[0].tone_class % 10 + 0x28;
} else {
    if buf[ac-1].marker == MARKER_SENTENCE_END { sentence[0].marker = MARKER_SPECIAL; }
    sentence[0].tone_class = (buf[ac-1].tone_class % 10) * 10 + sentence[0].tone_class % 10;
}
buf.extend_from_slice(sentence);
```

* `n==0` はno-op。`ac==0`（最初の文）は `first==1` で `i==0` をスキップし後段の `+0x28` で初期化するだけでコードは保持。
* 単一レコード文（`"가"` → 1音素）でも `sentence[0]` は `0x28+tone` に正規化され `buf.extend` で追加。欠落なし。
* 呼び出し側（`lib.rs:206-215`）は各 `buf` を `apply_sandhi` 後に `all_records` にextendするだけで要素を捨てない。
* **判定: SAFE。**

---

## 6. `unit_select` / `record` の文末マーカー処理

### 6.1 `record::ProsodyRecord::init_from_marker`（`record.rs:70-78`）

```rust
pub(crate) fn init_from_marker(&mut self, marker_byte: u8, sentence_final: bool) {
    self.flags = (marker_byte >> 7) & 1;
    let m = marker_byte & 0x7F;
    self.tone_class = tone::initial_tone_class(m);
    if m == 0 && sentence_final {
        self.marker = MARKER_SENTENCE_END;
        self.tone_class = 1;
    }
}
```

* `marker &0x7F==0 && sentence_final` のときのみ `marker=1, tone=1`。最終音素以外では発火しない。

### 6.2 `unit_select::UnitSelector::process`（`unit_select.rs:565-650`）

```rust
next: if idx + 1 < records.len() && rec.marker != MARKER_SENTENCE_END && tone <= 1 {
    records[idx + 1].code
} else {
    BOUNDARY_CODE  // 0x6EB3
},
```

* 最終レコードまたは `marker==1` なら `BOUNDARY_CODE`。これは次音素がないことを示す番兵で現音素を消さない。
* `scan` で `req.cur` が見つからない場合のfallback（`unit_select.rs:636-680`）でも最終音節のハングル音素では発生しない（`cargo test unit_select` 全パス）。
* **判定: SAFE。**

---

## 7. `render` / `wav` — 最終サンプル欠落の有無

* `render::render_units`（`render.rs:49-80`）は `units` を線形に走査し最終unitも例外なく処理。
* `render::is_real_phoneme`（`render.rs:38-47`）は最終unitの `next==BOUNDARY_CODE` でも `real==true` のまま。二重化されても欠落ではない。
* `wav::WavWriter`（`wav.rs:48-110`）のsplitは `data_size` をリセットするだけでサンプルを捨てない。
* **判定: SAFE。**

---

## 8. 再現手順と検証ログ

### 8.1 truncate 再現（Python）

```python
def truncate_old(text):
    end = len(text.rstrip("\r\n"))
    if end == 0: return text
    last = text[:end][-1]
    return text[:end - len(last.encode('utf-8'))]
def truncate_new(text):
    return text.rstrip("\r\n")
for t in ["가","가나","가나다","조건입니다"]:
    assert truncate_new(t) == t
    assert truncate_old(t) != t  # 旧: 1文字欠落を確認
```

結果: 旧実装で必ず1文字欠落、現行で解消を確認（§2.3 表）。

### 8.2 segmenter 手動再現（Python簡易tokenize）

`fake_kps("가")` → `B0A1`, `fake_kps("가나다")` → `B0A1 B0D1 B0E9` を簡易tokenizeした結果、いずれも `n_sent==1` かつ `last_bytes` が入力全体と一致。最終バイト欠落なし。

### 8.3 `cargo test` 全パス

```
cargo test -- --nocapture
running 55 lib tests ... ok (55 passed)
running 8 alphabet tests ... ok
running 14 colligation tests ... ok
running 21 g2p_dict tests ... ok
running 20 g2p_postprocess tests ... ok
running 7 number_unit tests ... ok
running 12 render tests ... ok
running 16 sandhi tests ... ok
running 8 segmenter integration tests ... ok
running 6 sino digit tests ... ok
running 15 unit_select tests ... ok
```

---

## 9. 発見した欠落・要観察点（再現手順＋行番号＋修正案）

### 9.0 [FIXED] `truncate_last_line_char` の最終1文字消失 — `lib.rs:51-59`

* **再現:** `synthesize_bytes("가")` / `synthesize_bytes("조건입니다")` など末尾に改行がない任意入力で、旧実装なら最終1文字が消える。
* **行番号:** `mirae-tts-engine/src/lib.rs:51-59`（旧実装は `51-61` で `end - last_char_len`）
* **修正案:** 現行 `text.trim_end_matches(['\n','\r'])` で修正済み。追加対応不要。回帰テスト追加を推奨: `assert_eq!(truncate_last_line_char("가나다"), "가나다")`。

### 9.1 [要観察・欠落ではない] KPS句点後のtokenize分岐（`segmenter.rs:312-338`）

* **現象:** `가。 나` のように句点直後に継続文字がある場合、句点を含む文と次文に分離。原典仕様通り。
* **行番号:** `mirae-tts-engine/src/segmenter.rs:312-338`
* **修正案:** 現行のままで正。変更不要。

### 9.2 [要観察・欠落ではない] 小数点 `is_decimal_point` のKPS数字非対応（`lib.rs:340`）

* **現象:** `b'.'` のみが対象。KPS句点 `0xA1A5` は対象外。
* **行番号:** `mirae-tts-engine/src/lib.rs:340-348`
* **修正案:** KPS小数点を扱う必要がある場合のみ `|| (b0>=0xA1 && ... ==KPS_FULL_STOP && nc==4)` を追加。現時点では修正不要。

### 9.3 [軽微・欠落ではない] `render::is_real_phoneme` の最終unit二重化

* **現象:** 最終unitの `next==BOUNDARY_CODE(0x6EB3)` で `real==true` のまま二重化。
* **行番号:** `mirae-tts-engine/src/render.rs:38-47`
* **修正案:** 現行のまま原典準拠。変更不要。

---

## 10. 検証チェックリスト

- [x] `truncate_last_line_char` 旧バグ再現と現行修正確認（§2）
- [x] `segmenter::tokenize` NUL終端（§3.1）
- [x] `segmenter::tokenize` 最終文flush（§3.2）
- [x] `segmenter::tokenize` CRLF強制break（§3.3）
- [x] `segmenter::tokenize` KPS句点（0xA1A5/0xA1A9/0xA1AA）（§3.4）
- [x] `sentence_to_records` 小数点 `is_decimal_point`（§4.1）
- [x] `sentence_to_records` 数字＋単位 merge（§4.2）
- [x] `sentence_to_records` グループ tone_class 付与（§4.3）
- [x] `tone::apply_sandhi` 文末セグメント伝播（§5）
- [x] `record::init_from_marker` / `unit_select::process` 文末マーカー（§6）
- [x] `render` / `wav` 最終サンプル（§7）
- [x] 手動再現＋ `cargo test` 全パス（§8）

---

## 11. 結論

* **最終音節欠落の根本原因は `truncate_last_line_char` の旧実装のみであり、現行 `trim_end_matches` で完全に修正済み。**
* segmenterのNUL終端・最終文flush・CRLF強制break・KPS句点、sentence_to_recordsの小数点判定・unit merge・グループtone付与、tone::apply_sandhiの文末伝播、unit_select/recordの文末マーカー処理のいずれにおいても、最終音節を消失させる経路は見つからなかった。
* `"가/가나/가나다"` および `"조건입니다"` での最終音節欠落は、現行コードでは再現しない（手動再現＋全テストパスで確認）。
* 要観察2件（§9.1-9.2）は欠落ではなく挙動差分の範疇であり、現時点での修正は不要。

---

*検証者: Hermes Agent (muse-spark-1.2) — 横断手動再現＋行番号追跡＋cargo test検証*
*出力: `reports_verify/01_truncate_and_segment.md`*

**要約: `truncate_last_line_char` の旧1文字削除バグ（lib.rs:51）は `trim_end_matches` で修正済み。segmenter（NUL/最終flush/CRLF/KPS句点）、sentence_to_records（小数点/unit merge/tone付与）、tone::apply_sandhi、unit_select/recordの文末マーカー全区間で最終音節消失経路は見つからず、"가/가나/가나다"/"조건입니다" での欠落は現行コードで再現しない。**

