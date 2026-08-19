# 06 横断・履歴・公開面 欠落検証

- 検証日: 2026-08-19
- 対象: `/tmp/mirae-wt-postproc` HEAD `69f97bb` (main) / 範囲 `64edf31..2d45d8d` + worktree 整合
- 検証手法: `cargo check` (rustc 1.97.1), `grep`/`git -S`/`git log --graph`, `git worktree list`/`ls-remote`, 手動コード横断
- 端末出力障害: `hermes_tools.terminal` は空出力 (exit 0, stdout 0B) を返す既知障害。`python subprocess.run(shell=True)` で迂回して全証跡を取得（本レポートの git/cargo 出力は迂回経路による実測）。

## サマリ (TL;DR)

| 区分 | 結論 |
|---|---|
| 死蔵関数 | P2 1件（未使用チェーン `unicode_syllable_to_jamo`→`syllable_jamo_map`→`kps_syllable_map` + `kps_code_to_phoneme_no_final`）。`cargo check` で 4 警告として検出。削除 or `#[allow(dead_code)]` 付与のいずれかが必要（現状は警告のまま）。 |
| `pub(crate)` 漏れ | P1 1件（dirty `truncate_last_line_char` の `pub(crate)` 化が未コミット）。P2 数件（`kps_lookup`/`EngineConfig::config` 等の未使用 pub(crate)）。 |
| terminal 出力キャプチャ障害 | P1（`hermes_tools.terminal` 空出力）。迂回（`python subprocess`）で対応済みだが、CI/他エージェントが同経路を使うと再発。恒久対策が必要。 |
| origin 付け替え (REO2248) | 正常。`origin = https://github.com/REO2248/mirae-tts.git`、commonDir config も一致。`ls-remote` で `HEAD`/`main` とも `69f97bb` を確認。 |
| main への merge (69f97bb) | 正常。`69f97bb = merge 64edf31 + 2d45d8d`、2-parent merge のみ。`origin/main` と同期済み（ahead 0 に見えるがローカル差分は未コミット dirty のみ）。 |
| 全体 | P0 なし。P1 2件、P2 4件。 |

> 重要: 本 worktree (`/tmp/mirae-wt-postproc`) は `dirty`（`mirae-tts-engine/src/lib.rs` 19行差分）で `main` 先端 `69f97bb` から乖離している。次回 `git push` 前にコミット or 破棄が必要（§4 参照）。

---

## 1. 未使用関数（死蔵）検証

### 1-1. `unicode_syllable_to_jamo` 等

**結論: 死蔵ではないが、チェーン全体が未使用警告のまま残置 — P2**

- `unicode_syllable_to_jamo` (g2p.rs:92): `fn unicode_syllable_to_jamo(uni: u32) -> Option<(u8, u8, u8)>`
  - 定義 1 + 参照 1（`syllable_jamo_map` 内 `if let Some(j) = unicode_syllable_to_jamo(...)`）。
  - `syllable_jamo_map` (g2p.rs:181) → `kps_syllable_map` (g2p.rs:199) の内部でのみ呼ばれる。
- `kps_syllable_map` はファイル内で参照 0（`grep -rn` で定義以外のヒットなし）。`kps_syllable_to_jamo`/`kps_to_jamo_kp` とは別系統（前者は `kps9566` テーブル経由、後者は Unicode 経由の逆引き）。
- `kps_code_to_phoneme_no_final` (g2p.rs:371) も参照 0（定義のみ）。
- `cargo check` 出力（実測、迂回経路）:
  ```
  mirae-tts-engine/src/g2p.rs:92:8: warning: function `unicode_syllable_to_jamo` is never used
  mirae-tts-engine/src/g2p.rs:181:8: warning: function `syllable_jamo_map` is never used
  mirae-tts-engine/src/g2p.rs:199:8: warning: function `kps_syllable_map` is never used
  mirae-tts-engine/src/g2p.rs:371:8: warning: function `kps_code_to_phoneme_no_final` is never used
  ```
- 履歴: `git log -S unicode_syllable_to_jamo` は `64edf31 Replace TTS engine with byte-exact port` の差分にのみヒット。当該関数は byte-exact port の初期移植から存在し、`16d186a` 等の audit 修正でも触られていない。
- 判定: P0 ではない。機能的には `kps_syllable_to_jamo` 系が実際に使われており、Unicode 経由の逆引きチェーンは代替経路として残置されたものと推定。ただし警告が残るため、以下のいずれかで解消すべき:
  - (a) 将来使用予定なら `#[allow(dead_code)]` をチェーン全体に付与（現状 `PROSODY_W2` のみに付与済みで不統一）。
  - (b) 不要なら削除（`syllable_jamo_map`/`kps_syllable_map`/`unicode_syllable_to_jamo` を一括削除、`kps_code_to_phoneme_no_final` も削除）。

### 1-2. その他の未使用警告

`cargo check` で同時に検出（`mirae-tts-engine` lib のみ、12 warnings 中）:

| 警告 | 所在 | 判定 |
|---|---|---|
| `unused import: WordRecord` | `lib.rs:29:52` | P2（`g2p::g2p_dict::WordRecord` の未使用 import。テストや `lib.rs` 内で `WordRecord` は別経路で参照されているが、`lib.rs` 直の import は不要）。`cargo fix` で自動修正可能。 |
| `variable does not need to be mutable` + `unused variable: mask` | `g2p.rs:172` | P2（`let mut mask` → `_mask`）。 |
| `unused variable: kinds` | `g2p.rs:518` | P2 |
| `unused variable: n` | `lib.rs:468` | P2 |
| `fields pitch_smoothing_tolerance/end_tone_threshold/speed are never read` | `lib.rs:77` | P2（`EngineConfig` の 3 フィールド。`UnitSelectConfig` への委譲で未読だが、将来の engine 設定拡張で使う想定なら `#[allow(dead_code)]` 付与）。 |
| `function kps_lookup is never used` | `lib.rs:115` | P2（`kps_lookup` は `g2p.rs:189` でのみ使用されるが、`lib.rs` の `pub(crate) fn kps_lookup` 自体は `cargo check` 時点で lib クレート単体では未使用扱い。実際は `crate::kps_lookup` として呼ばれているため、クレート境界の可視性テストで誤検出に見えるが、`cargo check` ではワークスペース全体で解決され警告は残る。`pub(crate)` のまま維持で問題なしだが、警告抑制に `#[allow(dead_code)]` or 使用箇所の `crate::` 参照を `lib.rs` 内に明示する整理が望ましい）。 |
| `method config is never used` | `lib.rs:170` | P2（`Mirae2Engine::config`）。テストや `lib.rs` 外部から呼ばれない。`pub(crate)` のままなら `#[allow(dead_code)]` 付与 or 削除。 |

いずれもビルドは成功（`Finished dev profile`）し、テスト失敗には直結しないため P2。

---

## 2. `pub` / `pub(crate)` 可視性検証

### 2-1. 全モジュール横断

| モジュール | `pub fn` 数 | `pub(crate) fn` 数 | 所見 |
|---|---|---|---|
| `alphabet.rs` | 6 | 0 | 適切。`TWO_BYTE_READINGS` は `pub static` で `LazyLock`、テストから参照。 |
| `connect.rs` | 5 | 0 | 適切。`parse`/`load`/`get` 等は `lib.rs` から利用。 |
| `dict.rs` | 25 | 0 | 適切。`expand_records` 等はテスト・`lib.rs` で利用。 |
| `keypad.rs` | 7 | 0 | 適切。 |
| `lib.rs` | 10 (`pub`) | 9 (`pub(crate)`) + dirty で +1 | 要対応（§2-2）。 |
| `postprocess_tables.rs` | 5 | 0 | 適切。大半は `pub static` 定数テーブル。 |
| `record.rs` | 2 | 1 (`init_from_marker`) | 適切。`init_from_marker` は `lib.rs` のみに公開。 |
| `render.rs` | 13 | 0 | 適切。`UnitRecord`/`RenderUnit` は `pub struct`。 |
| `segmenter.rs` | 11 | 0 | 適切。 |
| `tone.rs` | 4 | 0 | 適切。 |
| `unit_select.rs` | 10 | 0 | 適切。 |
| `voice_data.rs` | 7 | 0 | 適切。 |
| `voice_dict.rs` | 8 | 0 | **P2 漏れ**: `lookup_arr3`/`rec6_at`/`lookup_rec6`/`rec6_count`/`rec26_count` は `lib.rs` から呼ばれず、テストでも 0 参照（直近 `cargo check` では警告なしだが、公開 API として過剰）。`pub(crate)` への格下げを推奨。 |
| `voice_info.rs` | 13 | 0 | 適切。`woff_chain_ok` 等はテストで利用。 |
| `wav.rs` | 7 | 0 | 適切。 |
| `tables.rs`/`digit_tables.rs`/`kps_tables.rs` | 0 | 0 | 定数テーブルのみ。適切。 |
| `g2p.rs` | 64 (`pub`) | 4 (`pub(crate) const INIT/MED_*`) | 適切。`g2p_dict` 内の `pub` はワークスペース外に公開されない（`lib.rs: pub mod g2p` 経由だが、外部クレート `mirae-tts-server/cli` は `TtsEngine` 経由のみ利用）。`INIT_KP_TO_STD` 等 4つの `pub(crate)` const は `lib.rs` では未使用だが `g2p.rs` 内で使用、適切。 |

### 2-2. Dirty 差分 `truncate_last_line_char` の `pub(crate)` 化 — P1

```diff
- fn truncate_last_line_char(text: &str) -> &str {
-     let end = text.trim_end_matches(['\n', '\r']).len();
-     ...
-     &text[..end - last_char_len]
+ pub(crate) fn truncate_last_line_char(text: &str) -> &str {
+     text.trim_end_matches(['\n', '\r'])
  }
```

- 現 `HEAD` (69f97bb) では `fn truncate_last_line_char`（private）。
- ワークツリーでは `pub(crate)` 化 + ロジック修正（末尾文字を誤って 1 文字削るバグを修正）が未コミット。
- 修正内容自体は正しい（`가나다` → `가나` になる欠落バグの修正、コメントも適切）。
- 問題: `pub(crate)` 化の意図は「回帰テストから呼ぶため」（コメント `We also expose this as pub(crate) for regression testing.`）だが、テストはまだ追加されておらず、差分だけが dirty に残置。
- 対応: P1 — この差分をコミットするか破棄するかを即決すること。推奨はコミット（`git add mirae-tts-engine/src/lib.rs && git commit -m "Fix truncate_last_line_char: don't drop final syllable"`）。放置すると次回 `git pull`/`rebase` でコンフリクトの種。

### 2-3. その他の `pub(crate)` 漏れ（P2）

- `lib.rs: EngineConfig::{pitch_smoothing_tolerance,end_tone_threshold,speed}` は `pub(crate)` だが未読警告（§1-2）。将来の engine 設定で使うなら `#[allow(dead_code)]` を構造体に付与。
- `lib.rs: kps_lookup`/`config` も同様（P2）。

---

## 3. terminal 出力キャプチャ障害の迂回状況

- 現象: `hermes_tools.terminal`（本検証タスクの初期 4 試行）は `exit 0` かつ `stdout 0B` を返す。`pwd`/`ls`/`git log` いずれも空。
- 原因: 本環境の `hermes-web-ui` 経由 `terminal` ツールのキャプチャ障害（コンテナ `nikolaik/python-nodejs` 上の `hermes` バックエンドの stdout パイプ欠落と推定）。
- 迂回: `hermes_tools.execute_code` 内で `python subprocess.run(shell=True, capture_output=True)` を用いる迂回を確立。本レポートの全 `git`/`cargo` 証跡は迂回経路で取得し、再現性あり。
- 迂回状況の整合性: `cargo check` は同一迂回で `Finished dev profile` まで到達、`git ls-remote`/`git worktree list` も正常取得。ワークアラウンドとして有効。
- 残課題 — **P1**: 迂回は本タスク内でのみ有効。他エージェント/CI が `terminal` 直接呼び出しに依存すると再発。恒久対策として以下を推奨:
  - `hermes` 側の `terminal` ツール修正（stdout パイプ再接続）。
  - 暫定として `execute_code` 経由の `subprocess` ヘルパーを `skill` 化して共有（`skill_manage` で `terminal-bypass` スキル登録）。

---

## 4. REO2248 への origin 付け替えと main への merge (69f97bb) 整合

### 4-1. origin 付け替え

- `git remote -v`: `origin  https://github.com/REO2248/mirae-tts.git (fetch/push)` — 正常。
- `.git` は worktree: `gitdir: /home/user/reo_work/mirae-tts/.git/worktrees/mirae-wt-postproc`、commonDir `.../mirae-tts/.git/config` に `url = https://github.com/REO2248/mirae-tts.git` を保持 — 正常。
- `git ls-remote origin`（迂回取得、実測）:
  ```
  69f97bb276200a5c1166ce436d121a8140d953d9  HEAD
  69f97bb276200a5c1166ce436d121a8140d953d9  refs/heads/main
  2d45d8dd61284145b9880d0dc91b2a1e1abde8cc   refs/heads/postprocess
  ```
  `origin/HEAD` と `origin/main` がともに `69f97bb` でローカル `HEAD` と一致 — 付け替え後の push 済みを示す。
- `git log --all --graph -20` で `08c0b84 (origin/main, origin/HEAD)` だった旧 main から `69f97bb (HEAD -> main)` へ前進した履歴が確認できる。`64edf31` は `tag: main-64edf3153...` として保持。

### 4-2. merge 69f97bb の整合

- `git rev-parse HEAD` = `69f97bb276200a5c1166ce436d121a8140d953d9`
- `git rev-parse 69f97bb` 同値、`git rev-parse 2d45d8d` = `2d45d8dd...`、`git rev-parse 64edf31` = `64edf3153...`
- `git show --stat 69f97bb`:
  ```
  commit 69f97bb276200a5c1166ce436d121a8140d953d9
  Merge: 64edf31 2d45d8d
  Author: REO2248 <49685454+REO2248@users.noreply.github.com>
  Merge postprocess into main  ... 29 files changed, 2530 insertions(+), 408 deletions(-)
  ```
  2-parent merge のみ（octopus ではない）、`64edf31..2d45d8d` の 6 コミット（`b4c657c`,`d549855`,`af04954`,`e1b93fc`,`16d186a`,`2d45d8d`）を `main` に統合。
- `git diff --stat 64edf31..2d45d8d` と `git show --stat 69f97bb` の 29 ファイル一致 — 整合。
- `git reflog --all -20`:
  ```
  69f97bb refs/heads/main@{0}: merge postprocess: Merge made by the 'ort' strategy.
  69f97bb HEAD@{0}: merge postprocess: Merge made by the 'ort' strategy.
  64edf31 HEAD@{1}: checkout: moving from postprocess to main
  ```
  ローカル `postprocess` → `main` への ort merge で正当。
- `git log --oneline 64edf31..2d45d8d`: 6 コミットが正しく列挙。`b73604d Merge branch 'main' of https://github.com/yanorei32/mirae-tts` は `64edf31` の親側にあり、今回 merge の範囲外（重複 merge なし）。
- `git status`: `On branch main / Your branch is ahead of 'origin/main' by 9 commits.` と表示されるが、これは `ls-remote` で `origin/main` が既に `69f97bb` であることと矛盾。`git status` の `ahead 9` は `worktree` の `git` が commonDir の `origin/main` 参照を `fetch` 前の古い状態で読んでいるため。`git fetch origin` 後に再確認すべき（ネットワーク到達不能時は放置可）。実質的な ahead は dirty 差分 1 ファイルのみ。
- **P1 残件**: dirty 差分（§2-2）が `69f97bb` に含まれていない。`69f97bb` 自体の内容は正しいが、ワークツリーが `69f97bb` から乖離しているため、次回 push 時に `69f97bb` とは異なる内容が push される。コミットして `69f97bb` の後続コミットとするか、`git restore` で破棄すること。

### 4-3. yanorei32 → REO2248 の履歴断絶有無

- `64edf31` 以前は `yanorei32/mirae-tts` 由来（`b73604d` で `yanorei32` を merge、`f94e9f6`/`da495db`/`f8496aa` は `user@agents-vm` による byte-exact port）。
- `64edf31` で `Replace TTS engine with byte-exact port` として `REO2248` 名義で再コミット。以降 `b4c657c..2d45d8d..69f97bb` は一貫して `REO2248` 名義。
- `git remote -v` が `REO2248/mirae-tts` を指し、`ls-remote` でも `REO2248` 側に `69f97bb` が存在。履歴断絶なし。

---

## 5. 欠落一覧（優先度付き）

### P0（即時対応必須）

なし。

### P1（次回 push/CI 前に対応）

| # | 項目 | 詳細 | 対応 |
|---|---|---|---|
| P1-1 | Dirty `lib.rs` 未コミット | `truncate_last_line_char` の `pub(crate)` 化 + バグ修正が `69f97bb` に含まれずワークツリーに残置。`git diff --stat` で 1 ファイル 19 行差分。 | `git add mirae-tts-engine/src/lib.rs && git commit -m "Fix truncate_last_line_char: don't drop final syllable (pub(crate) for regression)"` 後 `git push`。 |
| P1-2 | terminal 出力キャプチャ障害 | `hermes_tools.terminal` が空出力を返す。CI/他エージェントで再発リスク。 | `execute_code` + `subprocess` 迂回を `skill` 化、または `hermes` 本体の `terminal` 修正。 |

### P2（警告解消・品質向上）

| # | 項目 | 詳細 | 対応 |
|---|---|---|---|
| P2-1 | 未使用関数チェーン | `unicode_syllable_to_jamo`→`syllable_jamo_map`→`kps_syllable_map` + `kps_code_to_phoneme_no_final` が `cargo check` で 4 warnings。 | 削除 or `#[allow(dead_code)]` 付与（`PROSODY_W2` と同様に意図をコメント）。 |
| P2-2 | `voice_dict.rs` 過剰公開 | `lookup_arr3`/`rec6_at`/`lookup_rec6`/`rec6_count`/`rec26_count` が `pub` だが `lib.rs` から未使用、テストでも 0 参照。 | `pub(crate)` へ格下げ or `#[allow(dead_code)]`。 |
| P2-3 | 軽微な未使用警告 7件 | `WordRecord` 未使用 import、`mask`/`kinds`/`n` 未使用変数、`EngineConfig` 3 フィールド未読、`kps_lookup`/`config` 未使用。 | `cargo fix --lib -p mirae-tts-engine` で自動修正可能なものは適用、残りは `#[allow(dead_code)]` or 削除。 |
| P2-4 | `git status` の `ahead 9` 表示 | `ls-remote` と矛盾する `ahead` 表示。worktree の `origin/main` 参照が stale。 | `git fetch origin` で解消（オフラインなら無視可）。 |

---

## 6. 検証コマンド（再現用、迂回経路）

```bash
# 本レポートは hermes_tools.terminal の空出力障害を迂回するため python subprocess 経由で取得
python3 -c "
import subprocess, os
env=dict(os.environ); env['PATH']='/home/user/.cargo/bin:'+env['PATH']
print(subprocess.run(['cargo','check','--message-format=short'], capture_output=True, text=True, cwd='/tmp/mirae-wt-postproc', env=env).stderr)
"
git -C /tmp/mirae-wt-postproc remote -v
git -C /tmp/mirae-wt-postproc log --oneline --all --graph -20
git -C /tmp/mirae-wt-postproc show --stat 69f97bb | head -n 40
git -C /tmp/mirae-wt-postproc diff --stat
git -C /tmp/mirae-wt-postproc ls-remote origin | head -n 10
grep -rn 'unicode_syllable_to_jamo\|kps_syllable_map\|kps_code_to_phoneme_no_final' mirae-tts-engine/
grep -rn 'pub(crate)' mirae-tts-engine/src/
```

---

## 7. 参考: `cargo check` 実測（抜粋）

```
mirae-tts-engine/src/g2p.rs:92:8: warning: function `unicode_syllable_to_jamo` is never used
mirae-tts-engine/src/g2p.rs:181:8: warning: function `syllable_jamo_map` is never used
mirae-tts-engine/src/g2p.rs:199:8: warning: function `kps_syllable_map` is never used
mirae-tts-engine/src/g2p.rs:371:8: warning: function `kps_code_to_phoneme_no_final` is never used
mirae-tts-engine/src/lib.rs:29:52: warning: unused import: `WordRecord`
mirae-tts-engine/src/lib.rs:77:16: warning: fields `pitch_smoothing_tolerance`, `end_tone_threshold`, and `speed` are never read
mirae-tts-engine/src/lib.rs:115:15: warning: function `kps_lookup` is never used
Finished `dev` profile [unoptimized + debuginfo] target(s) in 25.99s
```

## 8. 参考: `git worktree` / `ls-remote` 実測

```
gitdir: /home/user/reo_work/mirae-tts/.git/worktrees/mirae-wt-postproc
origin  https://github.com/REO2248/mirae-tts.git (fetch/push)
69f97bb276200a5c1166ce436d121a8140d953d9  HEAD
69f97bb276200a5c1166ce436d121a8140d953d9  refs/heads/main
```

---

*本レポートは `reports_verify/06_cross_and_history.md` として保存。端末出力障害の迂回は `python subprocess` で実施し、全証跡は実測に基づく。*
