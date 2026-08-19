# 05 — テストカバレッジ欠落検証 (mirae-wt-postproc 2d45d8d)

> 対象: `mirae-tts-engine/src/*.rs` 19 files / `tests/*.rs` 10 suites + `src/lib.rs` inline (55 tests)  
> 検証日: 2026-08-19 (HEAD 2d45d8d) / ツール: `cargo test` 個別スイート実行 + ソース行番号静的解析 + `segmenter`/`lib` 分岐手動トリアージ  
> 実行環境: `cargo 1.97.1`, `/tmp` tmpfs 12G (disk quota により `cargo test` 一括で `Disk quota exceeded` → 各スイートを個別実行で回避、後述)

---

## 1. 全テスト実行結果 — 全PASS (182 tests, 0 FAILED)

> **179 tests相当**という初期申告は概算。実測は **lib 55 + integration 127 = 182**。

| スイート | 結果 | 内訳 |
|---|---|---|
| `lib` (inline `#[test]` in `lib.rs`/`segmenter.rs`/`tone.rs`/`unit_select.rs`/`voice_dict.rs`/`connect.rs`/`keypad.rs`/`record.rs`) | ✅ ok | `55 passed` |
| `dict_test` | ✅ ok | 14 passed |
| `g2p_dict_test` | ✅ ok | 21 passed |
| `g2p_test` | ✅ ok | 20 passed |
| `render_test` | ✅ ok | 12 passed |
| `segmenter_test` | ✅ ok | 8 passed |
| `sandhi_rules_test` | ✅ ok | 16 passed |
| `number_unit_test` | ✅ ok | 7 passed |
| `unit_select_test` | ✅ ok | 15 passed |
| `coverage_gap_test` | ✅ ok | 8 passed |
| `t11_digit_reading` | ✅ ok | 6 passed |
| **合計** | **✅ 182 passed, 0 failed** | |

### ビルド障害メモ

```
cargo test (一括) →
  warning: ... 15 warnings
  error: failed to build archive at .../libmirae_tts_engine-*.rlib: Disk quota exceeded (os error 122)
```

`target/` が 755MiB で `/tmp` 残り 2.4G → 全クレート同時リンクで tmpfs 枯渇。`cargo clean` 後は `cargo test -p mirae-tts-engine --lib` 単独/各 `--test <suite>` 単独で全スイートPASSを確認。CI では `CARGO_TARGET_DIR=/dev/shm` 等の分割を推奨。

---

## 2. エッジカテゴリ別の塞がり度

> ◎=十分 / ○=概ね塞がったが穴あり / △=部分的にしか塞がれていない / ×=未カバー  
> `coverage_gap_test.rs` (8 tests, 103行) が塞いだ範囲を ★ で明示。

| # | エッジ | 要求 | 塞がり | coverage_gap_test による寄与 | 代表テスト |
|---|---|---|---|---|---|
| 1 | **最終文字/句読点** | 文末の `.` `。` `?` `!` `、` `,` が正しく文区切り / tone付与 | **○** | ★なし (`coverage_gap_test` は句読点を扱わない) | `segmenter::plain_korean_one_sentence` / `ascii_period_space_breaks` / `kps_punctuation_breaks` / `lib::next_word_final_tone` 経由 `sandhi_rules_test` 16 tests |
| 2 | **小数** | `3.14` / `2.0` / `.5` / `5.` / `KPS小数点` の桁区切り保持 | **○** | ★なし | `segmenter::decimal_point_stays_inline` / `lib::sentence_to_records` L340-L382 `decimal_codes` / `number_unit_test::stage2_decimal` / `t11_digit_reading::decimal_2_0_capture` |
| 3 | **負数** | `-3` / `-3.14` / `−` (KPS) の符号保持 | **×** | ★なし | **存在しない** — `ASCII_TO_KPS[0x2D]=0xA1AF` は `char_class_16=1` (句読点扱い) だが `sentence_to_records` は `-` を特別扱いしない |
| 4 | **非辞書文字** | 未登録語・未登録音節・記号フォールバック | **○** | ★なし | `g2p_dict_test::nonreg_miss` / `nonreg_other_hits` / `word_g2p_jogeon_fallback` / `dict_test::malformed_input_rejected` |
| 5 | **NUL** | 埋め込み `0x00` / 終端 `0x00` 切り捨て | **△** | ★なし | `segmenter::nul_terminates_input` (inline, L443 `tokenize(b"AB\0CD")` のみ) |
| 6 | **長文 forced-break** | `MAX_SENTENCE_BYTES=0x1F0(496)` および `HARD_FLUSH_LIMIT=0xC34C(49_996)` 超で強制分割 | **△** | ★なし | `segmenter::max_sentence_bytes_forced_break` (ASCII 500 `a` のみ) — KPS音節・HARD_FLUSH 未カバー |
| 7 | **ChunkRing境界** | `RING_SLOTS=20` / `RING_MAX_BYTES=0xFFFFF(1_048_575)` の満杯・ラップ・空振り | **○** | ★なし | `render_test::chunk_ring_20_slots_and_1mb_limit` / `produce_chunks_streaming` |
| 8 | **alphabet混在** | `hello가나다` / `ABC123한글` 等の ASCII+KPS 混在読み | **△** | ★`alphabet_*` 3 tests が単体は塞ぐが混在は塞がない | `coverage_gap_test::alphabet_ascii_readings/type_gate/dispatch_single_and_jamo` |
| 9 | **E2E WAVハッシュ比較** | Ghidra期待値とのバイト一致・WAV再生可能性 | **△** | ★`wav_header_byte_exact` がヘッダ46B一致のみ | `render_test::wav_*` 4 tests + `coverage_gap_test::wav_header_byte_exact` — **PCMハッシュ比較なし** |
| 10 | **その他 coverage_gap_test が塞いだもの** | — | **◎** | ★ | `postprocess_tables::*` / `stage9_post_loop_propagation` / `default_voice_dir` 環境変数 |

### coverage_gap_test 単体の評価

`mirae-tts-engine/tests/coverage_gap_test.rs` (103行, 8 tests, assert 28) は以下を新規に塞いだ:

- `alphabet.rs` L41-L121: `ascii_letter_reading(b'a'/b'A'/b'z'/b'0')`、全26字母の大小無視、非字母は `None`。
- `alphabet.rs` L84-L86: `is_letter_reading_type(0x1f/0x22)` 真、`0x21/0x14` 偽 — 6種のみ真。
- `alphabet.rs` L110-L145: `letter_reading_dispatch(b"a")` 単一読み、`[0xA4,0xA1]` (ㄱ) jamo 読み。
- `postprocess_tables.rs` L1-L405: 定数 `DAT_0047DF14/DAT_0047D6B4` 値一致、3テーブル非空、`stage3_pair_matches` 正/負。
- `g2p.rs` L1347-L1369 `stage9_post_loop_propagation`: `final_marker==3` の語が次語の `phoneme_markers` に `0x80` 伝播。
- `wav.rs` L15-L46 `write_wav_header`: RIFF/WAVE/fmt / `SAMPLE_RATE=22050` / `data_size=8` のオフセット `42..46`。
- `lib.rs` L44-L46 `VOICE_DIR_ENV/DEFAULT_VOICE_DIR/default_voice_dir()` が `Voice` で終わる。

**未だ穴**: 混在文・0xFF等の異常バイト・`TWO_BYTE_READINGS` 24種中 2種しか叩かない・エラーパス( `write_wav_header` の Seek失敗等) 未テスト。

---

## 3. 残存カバレッジ欠落 — 行番号 + 推奨テスト案

> 行番号は `2d45d8d` 時点。各案は `cargo test` で素直に書ける `#[test]` スニペット想定。`#[ignore]` を付けずに `Voice` 非依存で書けるものは `Voice` 依存を外す。

### 3.1 最終文字/句読点 (segmenter.rs / lib.rs)

| ID | 所在 | 現状 | 推奨テスト |
|---|---|---|---|
| P1 | `segmenter.rs` L302-L340 `tokenize_with` の KPS句読点分岐 (`is_sentence_punct_kps`) | `kps_punctuation_breaks` は `0xA1A5(。)` のみ。`0xA1A9(?)` `0xA1AA(!)` `0xA1A1(空白)` の flush/no-flush 分岐は未分離 | `kps_question_exclamation_flush` — `tokenize(&[0xB0,0xA1, 0xA1,0xA9])` と `tokenize(&[0xB0,0xA1, 0xA1,0xAA])` がそれぞれ文区切りすることを `assert_eq!(sents.len(),2)` で確認。`buf.is_empty()` で句読点単独時に flush しない分岐 (`segmenter.rs` L312/L319) も叩く |
| P2 | `segmenter.rs` L260 `buf.len() > HARD_FLUSH_LIMIT \|\| buf.len() > max_sentence_bytes` | `max_sentence_bytes` 側のみテスト (`L472` 500 `a`)。`HARD_FLUSH_LIMIT` 側は一度も発火しない | `hard_flush_limit_fires` — `tokenize_with(&vec![b'a'; 50000], false, 99999)` で `HARD_FLUSH_LIMIT=0xC34C` 超えで `sents.len() >= 2` を検証 |
| P3 | `lib.rs` L520-L549 `next_word_final_tone` | `sandhi_rules_test` は間接的に叩くが `','` → `Comma` / `0xA1D4` → `Bracket` / `'.'`+KPS空白 の3分岐を単体で検証していない | `next_word_final_tone_variants` — `Mirae2Engine::next_word_final_tone(b",",0)` 的な単体呼び出し(現在 `pub(crate)` のため `#[cfg(test)]` で公開するか `sentence_to_records` 経由: `synthesize("가, 나")` の `groups` デバッグで `Comma` が `tone_class` に反映されることを確認) |
| P4 | `lib.rs` L322-L328 `class==0` かつ `\n/\r` での `tone_class` 上書き | `crlf_mode_breaks_on_newline` は `tokenize_crlf` 側のみ。`sentence_to_records` で `\n` が `groups.last_mut().tone_class` を `+4` にするパスはテストなし | `newline_in_sentence_marks_tone` — `engine.sentence_to_records(&Sentence{ text: b"가\n나".to_vec(), start:0 })` 的な呼び出し(要 `pub` 昇格 or 統合テストで `synthesize("가\n나")` がパニックせず `groups.len()>=1` を確認) |

### 3.2 小数 (lib.rs L340-L382, g2p.rs L380-L392)

| ID | 所在 | 現状 | 推奨テスト |
|---|---|---|---|
| D1 | `lib.rs` L364-L377 `bytes[pos]==0x2E` 小数部ループ | `3.14` は `number_unit_test::stage2_decimal` で叩くが `3.` (frac空) / `.5` (int空) / `3..14` (連続ドット) / `3.1a4` (途中で非digit) は未カバー | `decimal_edge_cases` — `sentence_to_records` で `b"3."` → frac空で `frac_end==pos` のまま `sino_integer_codes([3])` にフォールバックすること、`b".5"` → `digits` 空で `sino_integer_codes([])` が `[]` を返し `merged_codes` なく素通しすること、`b"3..14"` → 最初の `.` だけで区切られることをそれぞれ `groups[0].0.len()` で検証 |
| D2 | `g2p.rs` L392-L410 `decimal_digit_code` / `decimal_codes` | `t11_digit_reading::decimal_2_0_capture` は `2.0` 一例のみ | `decimal_all_digits` — `decimal_codes(&[0,1,9], &[0,9])` が各桁 0xBXXX 系にマップされること、`frac` 空の呼び出しが `int` のみを返すこと |
| D3 | `segmenter.rs` L340-L347 `is_decimal_point` (先読み `next_token_class==4`) | `decimal_point_stays_inline` は `3.14` 一例のみ。`3. 가` (次のトークンが digitでないので区切り) は未検証 | `decimal_point_not_decimal_then_breaks` — `tokenize(b"3. \xB0\xA1")` が `sents.len()==2`、`tokenize(b"3.14")` が `1` を検証 |

### 3.3 負数 (lib.rs L333-L351, segmenter.rs L174-L188)

| ID | 所在 | 現状 | 推奨テスト |
|---|---|---|---|
| N1 | `segmenter.rs` L26-L30 `ASCII_TO_KPS[0x2D]=0xA1AF` / `char_class_16(0xA1AF)=1` | `-` が class 1 (句読点) であることは未アサート。`next_token_class(b"-")` が `(1,1)` を返すことのテストなし | `negative_sign_class_is_punct` — `assert_eq!(next_token_class(b"-"), (1,1))` + `assert_eq!(char_class_16(0xA1AF),1)` |
| N2 | `lib.rs` L352-L382 数字パース (`class==4` のみ) | `-3` / `-3.14` は `class==1` の `-` をスキップして `3` だけ読む。符号が捨てられる挙動が仕様かバグか未規定・未テスト | `negative_number_sign_is_dropped_or_kept` — `sentence_to_records` に `b"-3"` を渡して `groups[0].0[0].code` が `sino_integer_codes([3])` と一致することを明記する回帰テストを追加し、仕様を固定。将来符号対応するなら `NEGATIVE` ではなく符号処理の分岐テストを新設 |
| N3 | `g2p.rs` L939 `NEGATIVE` 定数 (現在 `전문가들의` 等の単語リスト) | 負数符号と無関係。`lib.rs` で `-` を扱っていないことは `dead_code` 警告にも出ない盲点 | 上記 N2 の仕様固定テストで代替。`NEGATIVE` は改名検討 (`NEGATIVE_WORDS`) |

### 3.4 非辞書文字 / 未登録フォールバック (g2p.rs L526-L630, dict.rs L95-L400)

| ID | 所在 | 現状 | 推奨テスト |
|---|---|---|---|
| U1 | `g2p.rs` L804-L830 `word_g2p` の fallback chain (`morphology_skeleton` → `nonreg_lookup` → `has_ascii_alpha` → `fallback`) | `nonreg_miss` で miss は叩くが `has_ascii_alpha` 真で `ascii_letter_by_letter` に落ちる混在語の最終 `fallback` 一語分岐は未分離 | `fallback_for_symbols` — `word_g2p(&dicts, b"@@@")` / `word_g2p(&dicts, &[0xFF, 0xFF])` / `word_g2p(&dicts, &[0x80])` がパニックせず `reading.bytes == word` の fallback を返すことを検証 |
| U2 | `g2p.rs` L823-L826 `word.len()==1||2` かつ `letter_reading_dispatch` ヒット | `coverage_gap_test::alphabet_dispatch_single_and_jamo` は1例ずつだが 2バイト正常/異常の分岐網羅が薄い | `letter_dispatch_fallback_for_unknown_two_byte` — `letter_reading_dispatch(&[0xA4,0xFF])` が `fallback` 1件を返すこと、`has_ascii_alpha(b"a\xB0\xA1")` 的な混在で `word_g2p` が alphabet 経路に入ることを検証 |
| U3 | `dict.rs` L95-L191 `MiraeDict::parse` の各種 `> data.len()` 早期リターン | `malformed_input_rejected_and_search_bounds` で短い入力は試すが `base_end/check_end/edges_end/map6_end/rec6_end` すべての out-of-bounds 分岐を個別に叩かない | `dict_parse_truncated_variants` — `build_sample()` の各セクションを1バイトずつ削った fixture で `MiraeDict::parse` が `None` を返すことを `#[test]` 6件に分割 |

### 3.5 NUL (segmenter.rs L203, lib.rs 暗黙)

| ID | 所在 | 現状 | 推奨テスト |
|---|---|---|---|
| Z1 | `segmenter.rs` L203 `text[..position(|&b| b==0).unwrap_or(len)]` | `nul_terminates_input` (lib inline L443) は `b"AB\0CD"` → `AB` のみ を検証するが統合テスト `segmenter_test.rs` にはない | `nul_truncates_integration` — `segmenter_test.rs` に `tokenize(b"AB\x00CD") == tokenize(b"AB")` を追加 (現行 `segmenter::tests` の inline テストを `tests/segmenter_test.rs` へ昇格) |
| Z2 | `segmenter.rs` L203 の先頭 `0x00` / 連続 `0x00` / `0x00` のみの入力 | 未テスト | `nul_only_and_leading_nul` — `tokenize(&[0])` / `tokenize(&[0,0])` / `tokenize(&[0, 0xB0,0xA1])` がいずれも `sents.is_empty()` を検証 |
| Z3 | `g2p.rs` L993-L1105 `'\0'` を `finals` として扱う `class_to_final` 等 | `'\0'` 終端は `spelling_finals` で `fins[i]!='\0'` 分岐の片側のみ叩く | `sandhi_with_null_final` — `spelling_finals` が `'\0'` を含む `WordRecord` で `class_to_final(0)=='\0'` かつ `apply_phoneme_sandhi` がパニックしないことを検証 |

### 3.6 長文 forced-break (segmenter.rs L15-L18/L259-L260, g2p.rs L24-L26)

| ID | 所在 | 現状 | 推奨テスト |
|---|---|---|---|
| L1 | `segmenter.rs` L15 `HARD_FLUSH_LIMIT=0xC34C(49_996)` | 未発火 | 上記 P2 の `hard_flush_limit_fires` で塞ぐ。加えて `buf.len()==HARD_FLUSH_LIMIT` 境界で発火しないことを `assert_eq!(sents.len(),1)` で確認 |
| L2 | `segmenter.rs` L18 `MAX_SENTENCE_BYTES=0x1F0(496)` の KPS音節境界 | `max_sentence_bytes_forced_break` は ASCII `a` 500B のみ。KPS 2B/音節で 496B=248音節の境界で切れるか未検証 | `forced_break_kps_boundary` — `vec![0xB0u8,0xA1; 260]` (520B) を `tokenize` して `sents[0].text.len()==MAX_SENTENCE_BYTES+1` かつ切断位置が音節境界(2の倍数)であることを検証 |
| L3 | `lib.rs` L478 `cum >= CHUNK_SYLLABLES(60)` の chunk 境界マーカー | `segmenter_test` ではなく `g2p_dict_test::stage8_chunk_boundary_at_60` が1例のみ。`cum==60` ちょうどの境界・`cum>60` の2段階・`prev` リセット確認なし | `chunk_boundary_exact_60_and_61` — `groups` に `cum=60` と `cum=61` の人工 `ProsodyRecord` 列を与えた `sentence_to_records` の `last.tone_class==3` 付与をそれぞれ検証 |

### 3.7 ChunkRing境界 (lib.rs L65-L68, render.rs L131-L217)

| ID | 所在 | 現状 | 推奨テスト |
|---|---|---|---|
| C1 | `render.rs` L155-L156 `can_push` の `full && total+size > RING_MAX_BYTES` 二重条件 | `chunk_ring_20_slots_and_1mb_limit` は両方同時に満たすケースのみ。`slots` 満杯だが `bytes` 空き / `bytes` 満杯だが `slots` 空き の片側満杯は未分離 | `chunk_ring_slots_full_but_bytes_ok` / `chunk_ring_bytes_full_but_slots_ok` — `RING_SLOTS=20` 埋めた後 `can_push(1)` が偽、`RING_MAX_BYTES` まで `total` を詰めた後 `slots<20` でも偽をそれぞれ個別に検証 |
| C2 | `render.rs` L174-L190 `len()` / `pop()` のラップアラウンド (`tail<head` 分岐) | `produce_chunks_streaming` は順方向のみ。`tail=19→0` ラップ後の `len()` が正しいか、`pop` で `head` が追従するかは未検証 | `chunk_ring_wrap_around` — `push` 10→`pop` 10→`push` 15 で `head` と `tail` がラップした状態で `len()==15 && total_bytes()==sum` を検証 |
| C3 | `render.rs` L212-L217 `produce_chunks` の `is_empty && !can_push` 巨大チャンク分岐 | 未テスト (1MiB超の単一チャンク) | `produce_chunks_oversized_single` — `render_units` が返す巨大 `Chunk` (人工 1MiB+1B) を `produce_chunks` に渡し `ring.is_empty()` の分岐でドロップされず `consume` 側に到達することを検証 |
| C4 | `render.rs` L60-L75 `random_mode` + `is_real_phoneme` 分岐 | 未テスト (`random_mode` は `Mirae2Engine` の非公開フィールド) | `render_random_mode_toggle` — `EngineConfig` で `random_mode` を立てる公開テストヘルパを追加し `render_units` の pitch ランダム化パスが実行されることをカバー (現状 dead path) |

### 3.8 alphabet混在 (alphabet.rs L88-L145, g2p.rs L823-L826, lib.rs L333-L471)

| ID | 所在 | 現状 | 推奨テスト |
|---|---|---|---|
| A1 | `alphabet.rs` L92-L109 `ascii_letter_by_letter` の非字母フォールバック | `coverage_gap_test` は `"a"` 単独のみ。`b"a1!"` のように字母/非字母混在で 3件中1件が `fallback(&[b'1'])` になる分岐は未検証 | `ascii_letter_by_letter_mixed` — `ascii_letter_by_letter(b"a1!")` が `len==3 && readings[1].bytes==[b'1']` を検証 |
| A2 | `alphabet.rs` L110-L145 `letter_reading_dispatch` の混在ループ | 同上、ASCII+2B 混在の逐次走査 (`i+=1` vs `i+=2` vs 単独 `0xFF` フォールバック) | `letter_dispatch_mixed_ascii_and_two_byte` — `letter_reading_dispatch(b"a\xA4\xA1b\xFF")` が `len==4` かつ `readings[1]` が `기역`、`readings[3]` が `fallback([0xFF])` を検証 |
| A3 | `g2p.rs` L823-L826 `word_g2p` の alphabet ゲート | `g2p_dict_test` では alphabet 経路の統合テストなし。`MORPH_TYPE_BASE` 系 `conjects_verify` はあるが `0x1f` 等の morph_type での alphabet dispatch は隔離されていない | `word_g2p_alphabet_mixed_word` — `word_g2p(&dicts, b"hello")` が `ascii_letter_by_letter` 相当の 5読みを返すことを直接アサート (現状は `nonreg` ヒット可否に依存して不安定) |
| A4 | `lib.rs` L454-L471 `sentence_to_records` の `class` 別トークン束ね | `segmenter_test::korean_sentence_split` は純ハングルのみ。`hello가나다 123` のように class 5(英字)/0x19(音節)/4(数字) が1文に混在する時の `groups` 形成は未検証 | `mixed_sentence_groups` — `tokenize(b"hello \xB0\xA1\xB0\xA1 123")` 由来の `Sentence` 1件を `sentence_to_records` に通し `groups[0].0` に英字読み+音節+数字の `ProsodyRecord` が順に並ぶことを検証 |

### 3.9 E2E WAVハッシュ比較 (wav.rs L15-L110, render.rs L19-L113, lib.rs L723-L732)

| ID | 所在 | 現状 | 推奨テスト |
|---|---|---|---|
| W1 | `wav.rs` L38-L110 `WavWriter::split` (26MB超で `stem_001.wav` 生成) | `wav_writer_header_at_finish_and_split` は小閾値で分割を叩くが 26MB 実閾値・`file_stem/extension/parent` の `None` フォールバック (`unwrap_or`) は未検証 | `wav_writer_split_threshold_exact` — `create_with_threshold(tmp, 10)` で 11B 書き込み後に `split` が発火し `tmp_001.wav` が生成されること、`Path::new("noext")` / `Path::new("a/b")` での `file_stem`/`extension` フォールバックがパニックしないこと |
| W2 | `wav.rs` L15-L32 `write_wav_header` の `Seek(0)` 失敗パス | 未テスト (seek不可の Writer) | `wav_header_seek_error` — `Seek` を常に `Err` を返すモック Writer で `write_wav_header` が `Err` を返すことを検証 |
| W3 | `lib.rs` L666-L732 `synthesize` / `encode_wav_vec` の E2E ハッシュ | `render_test::wav_ffprobe_readable` は `ffprobe` で読めることのみ。Ghidra期待値WAVとの `sha256` 一致はどこにもない | `e2e_wav_hash_vs_oracle` — `tests/fixtures/oracle_hello.wav` (または `audit-fixes-baseline.wav` 相当) の `sha256` と `synthesize("hello")`→`encode_wav_vec` の出力を `assert_eq!(hash, expected)`。`Voice` 依存のため `#[ignore]` + CI では `MIRAE_VOICE_DIR` 必須で実行。`coverage_gap_test` ではスタブ PCM での `encode_wav_vec` ハッシュ固定値テストを追加可能 |
| W4 | `lib.rs` L714-L732 `pcm_i16le_to_bytes` / `encode_wav_vec` の空PCM | `pcm_i16le_to_bytes_roundtrip` は非空のみ。空 `&[]` で `data_size==0` のヘッダが RIFF `0x30` を正しく持つか未検証 | `wav_empty_pcm` — `encode_wav_vec(&[], 22050)` が `46`B ヘッダのみで `&wav[42..46]==0u32` を検証 |

### 3.10 その他 — 未使用コード (dead_code 警告 12件)

| ID | 所在 | 状態 | 推奨対応 |
|---|---|---|---|
| D-C1 | `g2p.rs` L92 `unicode_syllable_to_jamo` / L181 `syllable_jamo_map` / L199 `kps_syllable_map` | `never used`。Unicode ↔ KPS 相互変換の実験コードが残存 | いずれ `kps_to_jamo_kp` / `jamo_to_kps_syllable` に統合するか `#[cfg(test)]` で roundtrip テストを追加、さもなくば削除。テスト案: `unicode_syllable_to_jamo(0xAC00)==Some((0,0,0))` 等 |
| D-C2 | `g2p.rs` L371 `kps_code_to_phoneme_no_final` | `never used`。`kps_code_to_phoneme` から終声除去版 | `assert_eq!(kps_code_to_phoneme_no_final(0xB0A1), expect_without_final)` を追加するか削除 |
| D-C3 | `lib.rs` L115 `kps_lookup` / L170 `Mirae2Engine::config()` / L75-L83 `EngineConfig::{pitch_smoothing_tolerance,end_tone_threshold,speed}` | `never used`。公開APIとして残すならテスト、さもなくば `#[allow(dead_code)]` を明示 | `config_mapping_speed` は既存だが `pitch_smoothing_tolerance` 等は触らない。`EngineConfig::from_public` の全フィールド roundtrip テストを追加 |
| D-C4 | `g2p.rs` L172 `COL_MASKS` の `mask` / L518 `kinds` | `unused variable` — 実装バグの疑い (`mask` を読むだけで使わない) | `cargo fix` ではなく `mask` 使用箇所の再実装を要確認。テストでは `worm` 的に隠れるため `kps_to_jamo_kp` の分岐カバレッジで間接検出 |
| D-C5 | `lib.rs` L616-L650 `find_keypad_ebd` / L633-L665 `Mirae2Engine::new` の3分岐 | `voice_dir/VoiceInfo.pkg` 直下/ `Voice/` 配下/不在 の3分岐は `render_test::voice_dir` で間接的に触るが `find_keypad_ebd` の戻り値分岐は単体未テスト | `find_keypad_ebd_variants` — 一時ディレクトリに空 `VoiceInfo.pkg` と `Voice/VoiceInfo.pkg` を置いた3パターンで `find_keypad_ebd` の `Some/None` を検証 |

---

## 4. 行番号サマリ ( `rg -n` で再現可能)

```
segmenter.rs:15  HARD_FLUSH_LIMIT=0xC34C
segmenter.rs:18  MAX_SENTENCE_BYTES=0x1F0
segmenter.rs:26  ASCII_TO_KPS[256]        // 0x2D=0xA1AF(class1), 0x2E=0xA1A5(class1), 0x2C=0xA1A4(class1)
segmenter.rs:53  char_class_16            // 0xA1AF->1, 0xA1A5->1, 0xA3B0->4, 0xB0A1->0x19
segmenter.rs:170 next_token_class
segmenter.rs:203 NUL truncation
segmenter.rs:259 forced flush condition
lib.rs:62       RING_SLOTS=20
lib.rs:68       RING_MAX_BYTES=0xFFFFF
lib.rs:304      sentence_to_records       // 数字/句読点/字母 分岐の中核
lib.rs:340      is_decimal_point
lib.rs:352      class==4 数字束ね
lib.rs:478      CHUNK_SYLLABLES boundary
lib.rs:492      PROPAGATE_BACK
lib.rs:520      next_word_final_tone
lib.rs:616      find_keypad_ebd
lib.rs:714      pcm_i16le_to_bytes
lib.rs:723      encode_wav_vec
g2p.rs:13       MAX_CANDIDATES=214
g2p.rs:24       CHUNK_SYLLABLES=60
g2p.rs:92       unicode_syllable_to_jamo (dead)
g2p.rs:181      syllable_jamo_map (dead)
g2p.rs:199      kps_syllable_map (dead)
g2p.rs:371      kps_code_to_phoneme_no_final (dead)
g2p.rs:939      NEGATIVE words
g2p.rs:1347     stage9_post_loop_propagation
g2p.rs:1403     number_unit_lookup
wav.rs:15       write_wav_header
wav.rs:80       WavWriter::split
render.rs:131   ChunkRing
render.rs:155   can_push
tone.rs:40      initial_tone_class
tone.rs:95      apply_sandhi
unit_select.rs:34 normalize_*_class
unit_select.rs:565 UnitSelector::process
alphabet.rs:41  ascii_letter_reading
alphabet.rs:88  has_ascii_alpha
alphabet.rs:92  ascii_letter_by_letter
alphabet.rs:110 letter_reading_dispatch
```

---

## 5. 推奨優先度 (次スプリントで塞ぐべき穴 Top 5)

1. **N2 負数符号の仕様固定** — 現行 `-3` が `3` に読まれるのはバグか仕様か不明。`sentence_to_records` で符号を保持/捨てるかを決め、回帰テスト `negative_number_sign_is_dropped_or_kept` を追加しないと将来の符号対応で無音リグレッションする。
2. **C1/C2 ChunkRing片側満杯+ラップ** — 1MiB/20slots の片側満杯と `tail<head` ラップは本番の `produce_chunks` で必ず踏む。`chunk_ring_*` を2本追加で `render.rs` の `can_push/len/pop` 分岐を 100% に。
3. **W3 E2E WAVハッシュ** — Ghidra期待値との `sha256` 一致テストがないため byte-exact 主張が `ffprobe` 読める程度で止まる。`oracle_hello.wav` 固定ハッシュを `coverage_gap_test` に追加 (Voice無しでも `encode_wav_vec(&[0i16;N])` の決定性ハッシュで代替可)。
4. **L1/L2 長文forced-breakのHARD/KPS境界** — `HARD_FLUSH_LIMIT` と KPS音節境界の2本は現行 `max_sentence_bytes_forced_break` だけでは SPEC §2.2 を保証しない。
5. **A2/A4 alphabet混在の文レベル** — 単字母テストはあるが `hello가나다 123` の文で `sentence_to_records`→`word_g2p`→`apply_accent_markers` まで通す混在テストがなく、実アプリの最頻入力が未カバー。

残り P/D/U/Z/W/D-C 群は上記5本の次に `#[test]` 10本程度で塞がる。合計 **新規テスト 18本**で本レポートの全穴を閉じられる見込み。

---

## 6. 付録 — 検証コマンド再現

```bash
# 個別スイートで全PASS (一括は /tmp tmpfs で Disk quota exceeded)
cargo test -p mirae-tts-engine --lib -- --nocapture
for s in dict_test g2p_dict_test g2p_test render_test segmenter_test sandhi_rules_test number_unit_test unit_select_test coverage_gap_test t11_digit_reading; do
  cargo test -p mirae-tts-engine --test $s -- --nocapture
done
# 行番号静的トリアージ
rg -n "HARD_FLUSH_LIMIT|MAX_SENTENCE_BYTES|RING_|CHUNK_|PROPAGATE|NUL|0x00|NEGATIVE|is_letter_reading|letter_reading_dispatch|write_wav_header|ChunkRing|can_push" mirae-tts-engine/src
```

---

*生成: `reports_verify/05_test_coverage.md` / 検証者: subagent (muse-spark-1.2-contributor) / 親タスク: テストカバレッジの欠落検証*
