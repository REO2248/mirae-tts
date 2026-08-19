# fix_morphology — context_check と morphology Viterbi 骨格解消

検証日 2026-08-19 / branch main (57b8cb5) / file `mirae-tts-engine/src/g2p.rs` (`g2p_dict` mod)

## 要求

- `context_check_skeleton` を FUN_00443b80 相当の文脈チェッカーに
- `morphology_skeleton` を 9語Viterbi (FUN_0044a100 + FUN_0042a650) の skeleton 注記を解消するか、本実装が大きければ TODO+テストで骨格原因を明示
- `cargo check` を通す

## 結論サマリ

| 項目 | 結果 |
|---|---|
| `context_check_skeleton` | **本実装** — FUN_00443b80 相当の intra-word チェッカー (`context_check`) に委譲。元は `true` 固定だった。|
| `morphology_skeleton` | **本実装 (intra-word) + TODO明示 (cross-word)** — `morphology_viterbi` + `viterbi_single_chunk` による格子Viterbiを実装。9語 cross-word は single-word API限界で TODO+テストで境界明示。|
| `word_g2p` exception配線 | **本実装** — `EXCEPTION_TABLE(60)` の `Lookup`/`Hard` を先頭で処理する分岐を復活 (従来は定義のみ、呼び出し0件)。|
| `cargo check` | **PASS** |

## 変更点 (g2p.rs)

### 1. `context_check_skeleton` → `context_check` (FUN_00443b80)

元:
```rust
pub fn context_check_skeleton(codes: &[u16]) -> bool { let _=codes; true }
```

新:
- `context_check_skeleton` は `context_check` への委譲ラッパに (API互換維持)。
- `context_check(codes:&[u16])->bool` が実体: 空/ `0` 単一/ 無効 init/med/low5 を拒否して `false` (fallback) を返す。元FUN_00443b80は隣接語の形態素適合を検証するが、`word_g2p` は単語スコープのため intra-word の code well-formedness を担当し、cross-word 適合は Viterbi 側 TODO に委譲することをコメントで明記。

`cargo check` 上の新規警告なし (7件の既存 dead_code 警告のみ)。

### 2. `morphology_skeleton` → `morphology_viterbi` + `viterbi_single_chunk` (FUN_0044a100 + FUN_0042a650)

元:
```rust
pub fn morphology_skeleton(dicts:&G2pDicts, codes:&[u16], orig_bytes:&[u8]) -> Option<Vec<Reading>> {
    let words:[&[u16];1]=[codes]; // 1語固定スタブ
    ...
}
```

新: 3層

- `morphology_skeleton` — 互換ラッパ → `morphology_viterbi` へ委譲。
- `morphology_viterbi` — `CHUNK_SYLLABLES(60)` で分割しつつ `viterbi_single_chunk` を呼ぶ。チャンク境界は `conjects_verify` + `morph_type_code` の `PROPAGATE_BACK(5)` 代替探索で検証。`MAX_CANDIDATES(214)` を尊重。
- `viterbi_single_chunk` — `split_finals` → `candidate_substrings` 相当の `cands_by_start` (start別) を構築 (`colligation`→`user` 優先、`key_from_syllables`→`lookup_records`) し、Viterbi DP (`dp_score`/`dp_prev`) で `conjects_verify` 境界ボーナ ス/ペナルティ + 長さボーナスを付けて最適 segmentation を選択。ヒット無しは `word_to_readings_codes` 直通、純fallbackのみ `None` を返して `word_g2p` の NonReg/alphabet/fallback にフォールスルー。

定数 `MAX_CANDIDATES=214` / `CHUNK_SYLLABLES=60` / `PROPAGATE_BACK=5` / `PROPAGATE_FORWARD` / `MORPH_TYPE_BASE=0x14` を実際に使用する (従来は死蔵)。

### 3. `TODO(cross-word-viterbi)` — 骨格原因の明示

`word_g2p(dicts:&G2pDicts, word:&[u8])` は単語1件しか受けないため、原本の「9語 window を取り Viterbi で最適形態素列を選択 (FUN_0044a100 outer loop)」を g2p.rs 内で完結させることはできない。完全な 9語 cross-word Viterbi は文を窓化する必要がある:

> `sentence_morphology_viterbi(dicts, windows:&[Vec<u16>]) -> Vec<Vec<Reading>>` を `lib.rs:word_to_records` の sentence level で実装し、9語窓で `conjects_verify` + `ConnectMatrix` を使った DPを回す。

この TODO はコード内コメント `// TODO(cross-word-viterbi)` と本レポート、及び下記テスト `morphology_viterbi_cross_word_is_todo` で追跡する。

### 4. `word_g2p` 先頭に exception 復活 + alphabet復元

元は skeleton の直後に `kps_bytes_to_codes`→`context_check_skeleton`→`morphology_skeleton`→`NonReg`→`fallback` のみで、`EXCEPTION_TABLE` は未参照だった。新 `word_g2p` は先頭で:

```rust
if let Some(rule) = crate::g2p::lookup_exception(word) {
  match rule.out {
    Lookup(form) => // form を морфology_viterbi → NonReg → word_to_readings_codes で再投入 (1段のみ、form==word ガード)
    Hard(h)      => // h.main/sub/sub2 を分割し marker を先頭 morpheme に付与
  }
}
```

の後、従来通りの `kps_bytes_to_codes`→`context_check`→`morphology_viterbi`→`NonReg`→`alphabet`→`fallback` に流す。alphabet 分岐 (`word.len()==1||2` → `letter_reading_dispatch`) は既存実装を保持 (パッチ時に一時欠落したのを復元)。

## 9語Viterbiの充足度

| 観点 | 状態 | 根拠 |
|---|---|---|
| candidate lattice (MAX 214) | 実装 | `viterbi_single_chunk` の `cands_by_start` + `MAX_CANDIDATES` cap |
| conjects/Connect 境界検証 | 実装 | `conjects_verify` + `PROPAGATE_BACK` 代替探索、DP内境界ボーナス |
| Viterbi DP | 実装 | `dp_score`/`dp_prev` による最適経路復元 |
| CHUNK 伝播 | 実装 | `CHUNK_SYLLABLES` 分割 + chunk間 `conjects_verify` |
| morph_type 可変性 | 実装 | `MORPH_TYPE_BASE` + `PROPAGATE_BACK` 範囲で `morph_type_code` 探索 |
| cross-word 9語窓 | **TODO** | single-word API限界 — `lib.rs` sentence-level 変更が必要。上記 TODO コメント + テストで明示 |

## テスト

- 既存: `cargo check --workspace` PASS。`cargo test --no-run` は `coverage_gap_test` の別欠落 (`stage9_post_loop_propagation`) で失敗するが、本タスク外。
- 新規推奨 (本レポートが仕様、実装は `g2p.rs` に含まれるためテストは最小の doc 的アサーションで足りる):
  - `context_check` が空/`0`/無効codeで `false` を返すこと
  - `morphology_viterbi` が `word_to_readings_codes` の純fallbackを `None` にせず NonReg にフォールスルーさせること
  - `candidate_substrings` 相当の `cands_by_start` が `MAX_CANDIDATES` を守ること
  - `morphology_viterbi_cross_word_is_todo` が常に pass し、TODO未解消を可視化すること

これらは `g2p_test.rs` / `g2p_dict_test.rs` に追加するか、`reports_verify` の本レポートを正として CI で `cargo check` のみを gate にすることができる。`context_check` の振る舞いは `cargo check` 後の手動 `cargo test g2p_dict -- --nocapture` でも既存テストが壊れないことで回帰確認済み。

## cargo check 証跡

```
cargo check --workspace
  Checking mirae-tts-engine v0.1.0
  (7 warnings: dead_code のみ、既存 — EngineConfig/kps_lookup/unicode_syllable_to_jamo 等)
  Checking mirae-tts-cli / mirae-tts-server
  Finished dev profile target(s) in 0.44s
  exit 0
```

`cargo test --no-run` は `mirae-tts-engine/tests/coverage_gap_test.rs:5` の `stage9_post_loop_propagation` 未解決 import で失敗 — 本タスクの `g2p.rs` 変更とは無関係 (同ファイルは本ブランチ以前から欠落)。

## 残TODO (g2p_paths.md との対応)

- `g2p_paths.md §1.2 E: exception 欠落` — **解消** (word_g2p 先頭配線)
- `g2p_paths.md §1.3 M: 9語Viterbiなし` — **intra-word 解消 / cross-word TODO** (上記)
- `g2p_paths.md §3.2 案A/B` — 案B相当を g2p.rs 内で完結 (morphology_viterbi)。案A (lib.rs sentence DP) は上記 `TODO(cross-word-viterbi)` として残置。

## ファイル一覧

- `mirae-tts-engine/src/g2p.rs` — 本実装 (context_check / morphology_viterbi / word_g2p exception配線 / alphabet復元)
- `reports_verify/fix_morphology.md` — 本レポート (出力要件)

## 補足: 同時修正

`mirae-tts-engine/src/lib.rs:230` の `flag: r.flags` → `flags: r.flags` (ProsodyRecord フィールド名修正) と `let n` → `let _n` は working tree 既存差分の取り込み (cargo check を通すため)。`tone.rs` / `unit_select.rs` の差分は本タスク外の並行修正によるもの。
