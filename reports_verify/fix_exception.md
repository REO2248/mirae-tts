# fix_exception — EXCEPTION_TABLE early-return 本実装 (tables.rs→g2p.rs)

**対象:** `mirae-tts-engine/src/g2p.rs` (`g2p_dict::word_g2p`, `crate::g2p::EXCEPTION_TABLE`/`lookup_exception` in `g2p.rs`), `mirae-tts-engine/src/tables.rs` は音色系テーブル専用で EXCEPTION_TABLEは含まない
**HEAD:** `57b8cb5` + 本fix (cargo check PASS) / branch `main` dirty
**検証日:** 2026-08-19
**要求:** `tables.rs の EXCEPTION_TABLE を word_g2p の先頭で参照し、該当単語は例外読みで早期returnする本来の分岐 (FUN_0041f020系) を復元。ダミー実装は置き換え。cargo check を通し、unit test 1件を追加。出力: reports_verify/fix_exception.md`

---

## 0. TL;DR

| 項目 | Before | After | 判定 |
|------|--------|-------|------|
| `word_g2p` 先頭の例外分岐 | なし — `EXCEPTION_TABLE(60)` と `lookup_exception()` は定義のみ。`word_g2p` は morphology → NonReg → alphabet → fallback のみで E:exception 経路は死蔵。`reports_verify/g2p_paths.md` で P0欠落と指摘 | **復元** — `crate::g2p::lookup_exception(word)` を先頭で呼び、`Lookup`/`Hard` を本来の FUN_0041f020 / FUN_0043b010 分岐として早期return | ✅ |
| Dummy 実装 | `g2p_paths.md` の「exception skeleton」コメント (`exception → morphology(9w Viterbi, currently 1w skeleton) → NonReg → alphabet → fallback` だが実際は exception未配線) | コメントを「EXCEPTION_TABLE 60 entries, FUN_0041f020 / FUN_0043b010 は先頭で処理」へ更新、ダミー分岐は置換 | ✅ |
| `tables.rs` 所在確認 | `EXCEPTION_TABLE` は `g2p.rs` 内 (`pub static EXCEPTION_TABLE[60]` / `lookup_exception` / `HardReading` / `ExceptionOutcome`) であり `tables.rs` には存在せず (tables.rs は PHON_CLASS/TONE等)。要求の `tables.rs の EXCEPTION_TABLE` は `g2p.rs` の当該テーブルを指すことを明記 | 参照元を `crate::g2p::lookup_exception` として解決、所在誤記をレポートで補足 | ✅ |
| `cargo check` | PASS (警告7件 dead_code のみ) | PASS 変動なし | ✅ |
| unit test 1件追加 | — | `mirae-tts-engine/tests/g2p_dict_test.rs::word_g2p_exception_early_return` 追加、PASS | ✅ |

**Blocking欠落: なし。**

---

## 1. 所在と誤記整理

`tables.rs` 全文は PHON_CLASS_FLAG_A/B/C/D, FALLBACK_ALLOW/REPL, TONE_CLASS_MAP/TRANS_COST, FILTER_TABLE のみで `EXCEPTION` 文字列は含まない (`grep -n EXCEPTION` 0件)。実体はすべて `g2p.rs`:

- `HardReading { main, sub, sub2, marker, morphemes, f1389, f1400 }` (g2p.rs ~1498)
- `ExceptionOutcome::{ Lookup(&[u8]), Hard(HardReading) }` (1509)
- `ExceptionRule { input, out }` (1515)
- `EXCEPTION_TABLE[60]` (1521–2030: Lookup 27件 + Hard 33件、3形態素 `b4ddc3cdba b7b2f7`→`대하/여서/는` 等を含む)
- `lookup_exception(input:&[u8])->Option<ExceptionRule>` (2032: `find(|r| r.input==input).cloned()` / 空は None)

要求 `tables.rs の EXCEPTION_TABLE` は当該 `g2p.rs` テーブルへの参照として本fixで `crate::g2p::lookup_exception` / `crate::g2p::ExceptionOutcome` を用いた。`tables.rs` 自体への不要な再エクスポートは行わず、所在誤記は本レポートで注記する。

---

## 2. 置換前 (dummy / skeleton)

```rust
// g2p.rs:808-836 (57b8cb5 / main)
/// Word G2P path: exception → morphology(9w Viterbi, currently 1w skeleton) → NonReg → alphabet → fallback.
/// `exception` and the 9-word Viterbi are documented as skeleton in reports_verify/g2p_paths.md.
pub fn word_g2p(dicts: &G2pDicts, word: &[u8]) -> Vec<Reading> {
    let Some(codes) = kps_bytes_to_codes(word) else { return vec![Reading::fallback(word)]; };
    if !context_check_skeleton(&codes) { return vec![Reading::fallback(word)]; }
    if let Some(readings) = morphology_skeleton(dicts, &codes, word) { return readings; }
    if let Some(hit) = nonreg_lookup(dicts, word) { return vec![Reading { bytes: hit.reading, packed: None, marker: hit.marker }]; }
    if word.len()==1||2 { let readings = crate::alphabet::letter_reading_dispatch(word); if !readings.is_empty() && ... { return readings; } }
    vec![Reading::fallback(word)]
}
```

`EXCEPTION_TABLE` / `lookup_exception` は本ファイル内に存在するが呼び出し0件 (`grep -rn lookup_exception` は tests 以外 0件)。`g2p_paths.md` で E:exception P0 と評価されていた。

---

## 3. 置換後 (本来の分岐 FUN_0041f020系 復元)

```rust
// g2p.rs:808-906 (本fix)
//  g2p_dict::word_g2p 先頭 — E: exception table (60) before morphology
if let Some(rule) = crate::g2p::lookup_exception(word) {
    match rule.out {
        crate::g2p::ExceptionOutcome::Lookup(form) => {
            if form == word { return vec![Reading { bytes: form.to_vec(), packed: None, marker: MARKER_FALLBACK }]; }
            if let Some(codes) = kps_bytes_to_codes(form) {
                if context_check_skeleton(&codes) { if let Some(r) = morphology_skeleton(dicts, &codes, form) { return r; } }
                if let Some(hit) = nonreg_lookup(dicts, form) { return vec![Reading { bytes: hit.reading, packed: None, marker: hit.marker }]; }
                let direct = word_to_readings_codes(dicts, &codes, form);
                let is_pure_fallback = direct.len()==1 && direct[0].marker==MARKER_FALLBACK && direct[0].bytes==form;
                if !is_pure_fallback { return direct; }
                return vec![Reading { bytes: form.to_vec(), packed: None, marker: MARKER_FALLBACK }];
            }
            return vec![Reading::fallback(word)];
        }
        crate::g2p::ExceptionOutcome::Hard(h) => {
            let mut out = Vec::new();
            for part in [Some(h.main), Some(h.sub), h.sub2].into_iter().flatten() {
                if part.is_empty() { continue; }
                if let Some(codes) = kps_bytes_to_codes(part) {
                    let r = word_to_readings_codes(dicts, &codes, part);
                    let is_pure_fallback = r.len()==1 && r[0].marker==MARKER_FALLBACK && r[0].bytes==part;
                    if is_pure_fallback { out.push(Reading { bytes: part.to_vec(), packed: None, marker: h.marker }); }
                    else { let mut first=true; for mut rr in r { if first { rr.marker=h.marker; first=false; } out.push(rr); } }
                } else { out.push(Reading { bytes: part.to_vec(), packed: None, marker: h.marker }); }
            }
            if out.is_empty() { return vec![Reading::fallback(word)]; }
            return out;
        }
    }
}
// — ここから既存の morphology → NonReg → alphabet → fallback は変わらず
```

ポイント:

- `Lookup(&[u8])` — 置換形を `kps_bytes_to_codes` → `morphology_skeleton` / `nonreg_lookup` / `word_to_readings_codes` の順で解決し、純fallback のみ未解決として `MARKER_FALLBACK` で返す。恒等エントリ (例 `비하여→비하여`) は無限ループ回避のため早期に `MARKER_FALLBACK` で返す。
- `Hard(HardReading)` — `main/sub/sub2` を辞書引きし、先頭 Reading の `marker` を `h.marker` (0x01/0x02/0x04/0x05) で付与。3形態素例 `대해서는`(f1389=0x15/f1400=0x91) の `main=대하 sub=여서 sub2=는` も分解される。
- 既存 `morphology_skeleton`/`nonreg_lookup`/`alphabet` は変更なし。例外はあくまで `word_g2p` の先頭で早期return する。

---

## 4. cargo check / cargo test

```
$ /home/user/.cargo/bin/cargo check -p mirae-tts-engine
    Checking mirae-tts-engine v0.1.0
    Finished dev profile [unoptimized + debuginfo] target(s) in 0.25s
    (warnings: EngineConfig 未使用3件 + kps_lookup dead_code + unicode_syllable_to_jamo 等4件 = 7 warnings、blockingなし)
```

```
$ cargo test -p mirae-tts-engine --test g2p_dict_test word_g2p_exception_early_return
running 1 test
test word_g2p_exception_early_return ... ok
$ cargo test -p mirae-tts-engine --test g2p_dict_test
running 22 tests ... ok. 22 passed; 0 failed
$ cargo test -p mirae-tts-engine --test g2p_test exception
running 6 tests ... ok. 6 passed
$ cargo test -p mirae-tts-engine --tests
running ... (g2p_test 20 + g2p_dict_test 22 + dict_test 14 + render_test 12 + segmenter_test 8 + sandhi 16 + others) all ok
```

---

## 5. 追加した unit test 1件

`mirae-tts-engine/tests/g2p_dict_test.rs::word_g2p_exception_early_return`

- 入力 `해서` (KPS `0xc3 0xcd 0xba 0xb7`) は `EXCEPTION_TABLE` 上 `해서 → 하여서` (Lookup) に該当する。本来の FUN_0041f020 分岐では `하여서` 側で morphology/NonReg を再解決して読みを確定する。
- 検証: `word_g2p(dicts, kp.convert_str("해서"))` が空でなく、`readings[0].bytes != input` かつ `하여서` (期待置換形) が出力バイト列中に含まれることを assert。ダミー実装では `해서` はそのまま `MARKER_FALLBACK(0x11)` で返り `input==output` になっていたが、本実装では `하여서` 起源の読みで早期return される。
- `cargo test --test g2p_dict_test word_g2p_exception_early_return -- --nocapture` で PASS を確認。

---

## 6. File分離遵守

- 本fixは `g2p.rs` の `word_g2p` 先頭（`g2p_dict` mod 内）のみを触る。`tables.rs` は参照元誤記の在処確認のみで編集なし。`dict.rs`/`connect.rs`/`alphabet.rs`/`tone.rs`/`unit_select.rs` 等は未変更。
- 新規ファイルは本レポート `reports_verify/fix_exception.md` のみ。`mirae-tts-engine/tests/g2p_dict_test.rs` に1テスト追記 (同一PR内扱い)。

---

## 7. 残課題 (blocking ではない)

- 9語 Viterbi (FUN_0044a100 outer window) の sentence-level 化は別タスク `fix_morphology.md` 対応 (intra-word Viterbi は実装済み)。本fixの例外early-return はその手前で完結する。
- `tables.rs` に EXCEPTION_TABLE を二重定義しない方針を維持。要求文の `tables.rs の EXCEPTION_TABLE` は本レポート注記で `g2p.rs` 実体への参照として解決する。
