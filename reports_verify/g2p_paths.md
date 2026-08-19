# G2P 辞書3種と morph 分岐の全経路 徹底追跡

**対象** `mirae-tts-engine/src/g2p.rs` pub mod `g2p_dict` (全2293行) + `lib.rs:word_to_records` / 辞書型 `dict.rs` / `connect.rs` / `alphabet.rs`  
**HEAD** `69f97bb` (Merge postprocess into main) — 直前 fix `2d45d8d` 含む / `lib.rs:51 truncate` はfix済確認  
**検証日** 2026-08-19 **検証者** subagent g2p_paths  
**要求** `exception → morphology(9語+Viterbi) → NonReg → fallback` の全経路を**関数行番号付き**で検証、欠落があれば修正案を併記

---

## 0. TL;DR — 欠落の有無

| 経路 | 期待 (原本) | 現行実装 | 判定 |
|------|-------------|----------|------|
| **E: exception** | `lookup_exception(word)` が morphology より先頭で1回だけ呼ばれる。`Lookup`なら置換語を辞書引き、`Hard`なら形態素分解結果を直接確定 | `g2p::lookup_exception()` / `EXCEPTION_TABLE(60)` は定義(1519/2030行)のみ。`g2p_dict::word_g2p()`(804行) と `lib.rs:word_to_records()`(554行) の**どちらからも呼ばれない**。コメント `// word_g2p: exception table → morphology → NonReg` (lib.rs:560) は嘘 | **欠落 P0** |
| **M: morphology(9語+Viterbi)** | 最大9語ウィンドウ上で 214候補を Viterbi/DPで最適形態素列を選択。各境界で `conjects_verify`(形態素接続行列) を検証 | `morphology_skeleton()` (780行) は `let words:[&[u16];1]=[codes]` の**1語固定**スタブ。`candidate_substrings(MAX=214)` も Viterbi DP も未実装。`conjects_verify` は1境界のみ呼ばれ常に `MORPH_TYPE_BASE(0x14)` 固定。`context_check_skeleton` は `true` 固定 | **欠落 P0 (スタブ)** |
| **N: NonReg** | `NonReg.pkg` の `lookup_prefix_records(reverse_key)` で最長一致の例外読み(不規則活用)を返す | `nonreg_lookup()` (623行) は正しく `reverse_key`→`lookup_prefix_records` を呼び、`word_g2p` (814行) から morphology 失敗時に1回呼ばれる | **実装あり (OK)** |
| **F: fallback** | いずれにもマッチしなければ原語を `MARKER_FALLBACK(0x11)` で返す。1-2byte語は alphabet レーンへ分岐 | `Reading::fallback()`(47行) と `word_g2p` 末尾 `vec![fallback]` (829行)、`word_to_readings_codes` 内 syllable単位 fallback(602行)、alphabet 1-2byte 分岐(822行) は実装済 | **実装あり (OK, ただし到達順序がE欠落で歪む)** |
| **3辞書種** | Colligation / User / NonReg + Conjects(接続) の4辞書。Colligation/Userは morphology 内で、`NonReg`は独立、`Conjects`は境界検証 | `G2pDicts{colligation,user,nonreg,conjects,connect}`(84行) は保持。`word_to_readings_codes`(575/584行) で Colligation→User の順に `lookup_records`。NonReg(623行)、Conjects(673行)は上記の通り | **辞書自体は3種揃うが morphology スタブにより User到達が希薄** |

> **結論:** 4経路のうち **E と M が欠落(P0)**。N/F は実装あり。3辞書の型は揃うが Mが1語スタブのため User/Conjects の本来のViterbi経路は実質未検証。

---

## 1. 現行 `word_g2p` の全経路 (行番号付き)

### 1.1 入口 — `lib.rs:553 word_to_records()` → `g2p.rs:804 word_g2p()`

```
lib.rs:560  // word_g2p: exception table → morphology (colligation/User) → NonReg   ←コメントは実体と乖離
lib.rs:561  let readings = g2p_dict::word_g2p(dicts, word);
  └─ g2p.rs:804  pub fn word_g2p(dicts:&G2pDicts, word:&[u8]) -> Vec<Reading> {
       805    kps_bytes_to_codes(word) else fallback                // E0: bytes→codes 失敗→F
       808    if !context_check_skeleton(&codes) → fallback        // M0: 常に true なので never
       811    if let Some(r) = morphology_skeleton(..) → return r  // M1: 1語スタブ
       814    if let Some(hit) = nonreg_lookup(dicts, word) → return // N
       822    if word.len()==1||2 { alphabet dispatch }             // A: alphabet レーン
       829    vec![Reading::fallback(word)]                          // F: 最終 fallback
     }
```

図 (本来の期待):

```
word ──► [E exception] ──hit──► (Lookupなら再g2p / Hardなら確定) ──► postprocess
          │ miss
          ▼
        [M morphology: 9語Viterbi + Conjects] ──hit──► Reading列 ──► postprocess
          │ miss
          ▼
        [N nonreg_lookup] ──hit──► NonRegHit.reading ──► postprocess
          │ miss
          ▼
        [A alphabet 1-2B] ──hit──► letter_reading ──► postprocess
          │ miss
          ▼
        [F fallback] MARKER 0x11 原語パススルー
```

現行の実体:

```
word ──► (Eなし!) ──► [M 1語スタブ] ──hit?──► 全語が1語なので9語分割もDPもない; ほとんど常にhitして N/A に到達しない
                     │ miss (極稀)
                     ▼
                   [N nonreg] ──► alphabet ──► F
```

### 1.2 E: exception (欠落) — `g2p.rs:1496-2035`

- 型: `HardReading{main,sub,sub2,marker,morphemes,f1389,f1400}` (1496行), `ExceptionOutcome::{Lookup(&[u8]),Hard(HardReading)}` (1508行), `ExceptionRule{input,out}` (1513行), `EXCEPTION_TABLE[60]` (1519-2028行), `lookup_exception()` (2030行)。
- 内容: `Lookup 27件`(〜1599行) と `Hard 33件`(1600行〜) を含む60件。`f1389=0x15/f1400=0x91` を持つ `b4ddc3cdba b7b2f7` の3形態素例(1885行)も含む。
- **欠落:** `lookup_exception` の呼び出しがリポジトリ内で `tests/g2p_test.rs` (テスト) 以外 **0件** (`grep -rn lookup_exception` で word_g2p/lib.rs から参照なし)。`word_g2p` 先頭に本来必要な:
  
  ```rust
  if let Some(rule) = crate::g2p::lookup_exception(word) {
      match rule.out { Lookup(form) => { /* form を word_g2p ではなく colligation/user 再検索 */ }, Hard(h) => { /* h.main/sub/sub2 を Reading化 */ } }
  }
  ```
  が丸ごと無い。`EXCEPTION_TABLE` は静的データとして死蔵。

### 1.3 M: morphology (スタブ) — `g2p.rs:780-802 morphology_skeleton` / `533-621 word_to_readings_codes` / `673-773 conjects_verify`

**`morphology_skeleton` (780行):**

```rust
pub fn morphology_skeleton(dicts:&G2pDicts, codes:&[u16], orig_bytes:&[u8]) -> Option<Vec<Reading>> {
    let words: [&[u16]; 1] = [codes];          // ← 9語ウィンドウが1語固定
    let mut all: Vec<Reading> = Vec::new();
    let mut segments: Vec<Vec<u16>> = Vec::new();
    for w in words.iter().take(9) {             // take(9) だが len==1 なので1回のみ
        let readings = word_to_readings_codes(dicts, w, orig_bytes);
        // ...
        if let Some(prev)=segments.last() {
            if !conjects_verify(dicts, prev, MORPH_TYPE_BASE, w, MORPH_TYPE_BASE) { return None; }
            // ← morph_type が常に 0x14(基本形) 固定。原本は品詞毎に 0x14..0x29 を切り替える
        }
        segments.push(w.to_vec());
        all.extend(readings);
    }
    if all.is_empty() { None } else { Some(all) }
}
context_check_skeleton(codes: &[u16]) -> bool { let _=codes; true } // 775行: 常に true
```

欠落点:

1. **9語Viterbiなし** — 原本は `candidate_substrings` (501行, MAX 214) で文脈9語の格子を生成し、`conjects_verify` + `ConnectMatrix` (file 0x89168等の伝播定数) でViterbi DPの最適経路を求める。現行は1語を `word_to_readings_codes` に素通しするだけでDPがない。
2. **`candidate_substrings`未使用** — 定義(501行)はあるが `word_g2p`/`morphology_skeleton` から呼ばれない。テストのみで参照または未参照。
3. **`morph_type_code` の可変性なし** — `morph_type_code(morph_type:u8)->Option<&[u16]>` (644行) は `0x14..0x29` の16種+suffixを正しく持つが、`morphology_skeleton` は常に `MORPH_TYPE_BASE` を渡すため `0x15..0x29` が死蔵。原本は形態素境界ごとに `SubARecord.kind` 等から型を決定。
4. **`context_check_skeleton` はスタブ** — 原本の文脈適合チェックが `true` 固定。

**`word_to_readings_codes` (533行):** Colligation→User の順に `key_from_syllables`→`lookup_records`→`reading_from_hit` を最長一致で走査。純音節 `0x10` のみ辞書引き、数字(1)/記号(2)はpackedダミー、fallbackは1音節 `MARKER_FALLBACK(0x11)`。ロジック自体は正だが、1語スキャンのため**文脈9語の分割点が現れない**。

**`conjects_verify` (673行):** `morph_type_code` の新テーブルsuffixと旧線形公式の両方で `left+lc / right+rc` の `key_from_syllables`→`conjects.lookup`→`connect.row(xl)[xr]!=0` を2回試し、最後にテーブル方式のみで再試行。関数自体は正しいが、呼び出し元が1語=境界0個なので検証回数は0〜1回。

### 1.4 N: NonReg — `g2p.rs:623 nonreg_lookup` / `dict.rs:341 lookup_prefix_records`

```rust
// g2p.rs:623  pub fn nonreg_lookup(dicts:&G2pDicts, word:&[u8]) -> Option<NonRegHit> {
    let codes = kps_bytes_to_codes(word)?;
    let key = key_from_syllables(&codes)?;
    let rev = reverse_key(&key);
    let (pm, records) = dicts.nonreg.lookup_prefix_records(&rev)?; // dict.rs:341
    let m = pm.matched; // 最長一致長
    let entry_key: Vec<u8> = rev[..m].iter().rev().copied().collect();
    let entry_codes = key_str_to_codes(&entry_key)?;
    let reading = codes_to_kps_bytes(&entry_codes)?;
    NonRegHit{ reading, marker:records[0].kind, records, matched:m }
// dict.rs:341  lookup_prefix_records: trieのsearch_prefix(201行)→tail_string→sub_a_record→expand_records
```

- 経路: `word_g2p:814` で `morphology_skeleton` が `None` を返した時のみ到達。`reverse_key`(dict.rs:385) は NonReg の逆引きtrie(原本仕様)に対応。
- **OK**だが、Mが常に成功するため到達頻度が極低。Mを正しく失敗させるケース(未知語)でのみ発動。

### 1.5 F/A: fallback + alphabet — `g2p.rs:47 Reading::fallback` / `822 alphabet dispatch` / `602 splitfallback`

- `Reading::fallback(word)` (47行): `bytes=word.to_vec(), packed=None, marker=0x11(MARKER_FALLBACK,15行)`。
- `word_to_readings_codes` 内 fallback (602行): `split[i..i+1]` を `merge_finals`→`codes_to_kps_bytes` で1音節ずつ `MARKER_FALLBACK` 化。`any_hit` が false なら全体を `fallback` に潰す(614-620行) — ほぼデッドだが安全側。
- alphabet (822行): `word.len()==1||2` かつ `letter_reading_dispatch` が原語と異なる読みを返した時のみ採用。`mirae-tts-engine/src/alphabet.rs:ASCII_LETTER_READINGS(26)+TWO_BYTE_READINGS(24)`。近傍の別レポート(03)で子音14件のbyte不一致が指摘されているが、パイプライン到達自体はここで塞がる。
- 最終fallback (829行): `vec![Reading::fallback(word)]` — 全経路miss時の番兵。後段 `word_record_from_readings_final`(866行)で `syllable_codes/phoneme_markers` に展開され `lib.rs:572-586 postprocess→record_to_prosody` へ渡る。

### 1.6 辞書3種の型 — `g2p.rs:84 G2pDicts` / `dict.rs:63 Dict`

```rust
// g2p.rs:84
pub struct G2pDicts<'a> { colligation:&'a Dict, user:&'a Dict, nonreg:&'a Dict, conjects:&'a Dict, connect:&'a ConnectMatrix }
// dict.rs:63 Dict { base, check, tail, sub_a/b_pairs, ... } + lookup(330)/lookup_records(335)/lookup_prefix_records(341)
// connect.rs: ConnectMatrix (file 0x89168等の行列)
```

- 3種(実質4種: Colligation/User/NonReg/Conjects)の辞書は `lib.rs:Mirae2Engine` で `Voice/Data/Dictionary/*.pkg` からロードされ `G2pDicts` に束ねられる。
- 現行の3辞書呼び出し箇所:
  - Colligation/User: `word_to_readings_codes:575/584` `lookup_records`
  - NonReg: `nonreg_lookup:627` `lookup_prefix_records`
  - Conjects+Connect: `conjects_verify:721/758` `lookup` + `connect.row(xl)[xr]`

---

## 2. 欠落の詳細と影響

### 欠落 E1: exception 先頭分岐がない (P0)

- `word_g2p` の **先頭** に `lookup_exception(word)` が無いため、60件の例外語(例 `해서→하여서`(0xc3cdba b7)、`탄→타는`(0xc0b2)、`만나→만나아`(0xb6ed b1fd)等)が **通常の形態素解析に素通し** される。これらはColligation/Userに無い/あっても読みが異なるため、誤読みまたは `MARKER_FALLBACK` に落ちる。
- `Hard` 33件(例 `가도→가+도`(morph 2, marker 4)、`대해서는→3形態素`)の marker/morphemes/f1389/f1400 情報も全て無視される。
- 影響: 北朝鮮語の不規則活用・例外読みの再現率が直接落ちる。`g2p_test::exception_*` はテーブル自体はテストするがパイプライン到達はテストしないため検出されない。

### 欠落 M1: 9語Viterbi/DPがスタブ (P0)

- `morphology_skeleton` が1語固定のため、本来9語窓で競合する読み候補(214候補)の最適選択が発生しない。常に1語の最長一致が採用されるため、境界曖昧な語(例 `일반화해서` の `일반+화+해서` vs `일반화+해서`)の正解率が落ちる。
- `conjects_verify` が境界0〜1回のみで、品詞接続のViterbiスコアリングになっていない。`CHUNK_SYLLABLES=60`(24行)/`PROPAGATE_BACK=5`(26行) 等の伝播定数は `g2p.rs` に定義済みだが使用箇所なし。

### 欠落 M2: `candidate_substrings` と Viterbi 定数が死蔵

- `candidate_substrings(codes:&[u16])->Vec<Vec<u16>>`(501行, `MAX_CANDIDATES=214`(13行))は9語Viterbiの格子生成器だが現行呼び出し0。`CHUNK_SYLLABLES/PROPAGATE_*` も同様。

---

## 3. 修正案 (パッチ方針)

### 3.1 E: `word_g2p` 先頭に exception 分岐を追加

`g2p.rs:804 word_g2p` の先頭 (805行直後) に挿入:

```rust
pub fn word_g2p(dicts: &G2pDicts, word: &[u8]) -> Vec<Reading> {
    // E: exception table (60 entries) — must be before morphology
    if let Some(rule) = crate::g2p::lookup_exception(word) {
        match rule.out {
            crate::g2p::ExceptionOutcome::Lookup(form) => {
                // 置換形を通常の辞書経路に再投入 (原本: 置換語を Colligation/User で再検索)
                // 1) form が辞書にあればその読みを、なければ form bytes を Reading 化
                let readings = word_to_readings(dicts, form);
                // fallback潰しを避ける: 置換語がfallback(0x11)なら置換bytes自体を返す
                if readings.len()==1 && readings[0].marker==MARKER_FALLBACK {
                    return vec![Reading{ bytes: form.to_vec(), packed: None, marker: readings[0].marker }];
                }
                return readings;
            }
            crate::g2p::ExceptionOutcome::Hard(h) => {
                // Hard: main/sub/sub2 を個別 Reading 化し marker/morphemes を反映
                let mut out = Vec::new();
                for part in [Some(h.main), Some(h.sub), h.sub2].into_iter().flatten() {
                    if part.is_empty() { continue; }
                    // a4a2 等の jamo も含むため kps_bytes_to_codes 経由せず bytes をそのまま Reading 化
                    // marker は h.marker、morphemes は後段 WordRecord 側で参照
                    out.push(Reading{ bytes: part.to_vec(), packed: None, marker: h.marker });
                }
                if out.is_empty() {
                    return vec![Reading::fallback(word)];
                }
                return out;
            }
        }
    }
    let Some(codes) = kps_bytes_to_codes(word) else { return vec![Reading::fallback(word)]; };
    // ... 以下既存の morphology → NonReg → alphabet → fallback
}
```

注意:
- `crate::g2p::lookup_exception` は `g2p_dict` の外(`g2p.rs:2030`)にあるため `crate::g2p::lookup_exception` で参照する。`g2p_dict` 内からは `super::lookup_exception` でも可だが `crate::` が最短。
- `Lookup` 分岐は無限再帰を避けるため `lookup_exception(form)` の再入を1回に留める(上記は `word_to_readings` 経由で再び exception に入らないよう `word_to_readings` ではなく `word_to_readings_codes` 相当を直接呼ぶ選択肢もある。置換形が例外テーブルに再ヒットするケースは現行60件では無いことを確認済みだが、ガードとして `if form==word { return fallback }` を入れても良い)。
- `Hard` の `a4a2`(ㄴ)等の1音節 jamo は後段 `word_record_from_readings` で `kps_bytes_to_codes` が単独 jamo を `packed=None/bytes` として扱うため素通しで良い。

### 3.2 M: 9語Viterbi スケルトンの段階的復元

最小修正(互換を保ちつつ欠落を塞ぐ) — 1語スタブを「**文分割後の9語窓を `lib.rs:word_to_records` 側で生成して渡す**」形に拡張。`g2p.rs` 単体では文脈を持たないため `morphology_skeleton` のシグネチャを拡張するか、`lib.rs` 側でViterbiを実装するのが自然。

**案A (推奨・小):** 当面は `morphology_skeleton` を `lib.rs` から呼ばない形で温存し、`candidate_substrings` を `word_to_readings_codes` 内の最長一致ループに統合したことをコメントで明示し、9語Viterbiは `lib.rs:word_to_records` の sentence-level で `conjects_verify` を使ったDPとして実装。定数 `MAX_CANDIDATES/CHUNK_SYLLABLES/PROPAGATE_*` を使用する。

```rust
// lib.rs:word_to_records 付近に sentence窓のViterbi 雛形 (擬似):
let window: Vec<&[u8]> = sentence.words()[i..(i+9).min(n)].iter().map(|w| w.bytes).collect();
// 各wordについて candidate_substrings→word_to_readings→conjects_verify でDP
```

**案B (g2p内完結):** `morphology_skeleton` の引数を `codes:&[u16]` から `windows: &[Vec<u16>]` に変更し、DPで最適 `Vec<Reading>` を選択。`context_check_skeleton` も本来の文脈適合ロジックに置換。

いずれも `morph_type` は `SubARecord.kind` や `HardReading.morphemes` から決定し `MORPH_TYPE_BASE(0x14)` 固定を解く必要がある。

### 3.3 検証観点 (修正後に追加すべきテスト)

- `word_g2p` が `EXCEPTION_TABLE` の全60件で `word_to_readings` 直通より優先されること (Lookup 27件は `dec(form)` と一致、Hard 33件は `main/sub/sub2` 分解と一致)。
- `morphology_skeleton` が 2語以上の窓で `conjects_verify` により誤接続を棄却すること。
- `candidate_substrings` が `MAX_CANDIDATES=214` 上限を守ること (既存 `g2p_dict_test` に準ずる)。

---

## 4. 付録: 行番号索引

| シンボル | 行 | ファイル | 備考 |
|----------|----|----------|------|
| `g2p_dict` mod | 4 | g2p.rs | `pub mod g2p_dict {` |
| `G2pDicts` | 84 | g2p.rs | `colligation/user/nonreg/conjects/connect` |
| `MARKER_FALLBACK` | 15 | g2p.rs | `0x11` |
| `MORPH_TYPE_BASE` | 22 | g2p.rs | `0x14` |
| `CHUNK_SYLLABLES/PROPAGATE_*` | 24-26 | g2p.rs | Viterbi伝播定数 (現在未使用) |
| `PROSODY_W1/W2/W3` | 33-36 | g2p.rs | postprocess用 (W2はreserved) |
| `MAX_CANDIDATES` | 13 | g2p.rs | 214 (現在未使用) |
| `Reading::fallback` | 47 | g2p.rs | `marker=0x11` |
| `reading_from_hit` | 514 | g2p.rs | `merge_finals`→`codes_to_kps_bytes` |
| `word_to_readings` | 526 | g2p.rs | `kps_bytes_to_codes`→`word_to_readings_codes` |
| `word_to_readings_codes` | 533 | g2p.rs | Colligation/User 最長一致本体 |
| `classify_candidate` | 472 | g2p.rs | 0x10/1/2/3 判定 |
| `candidate_substrings` | 501 | g2p.rs | 格子生成器 (現在未使用) |
| `nonreg_lookup` | 623 | g2p.rs | `reverse_key`→`lookup_prefix_records` |
| `morph_type_code` | 644 | g2p.rs | `0x14..0x29 → &[u16]` suffix |
| `conjects_verify` | 673 | g2p.rs | `conjects.lookup`+`connect.row` |
| `context_check_skeleton` | 775 | g2p.rs | 現状 `true` 固定 |
| `morphology_skeleton` | 780 | g2p.rs | 1語スタブ |
| `word_g2p` | 804 | g2p.rs | E→M→N→A→F の現行本体(ただしE欠落) |
| `alphabet dispatch` | 822 | g2p.rs | 1-2B jamo/ASCII レーン |
| `ExceptionOutcome/Rule` | 1508/1513 | g2p.rs | 例外型 |
| `EXCEPTION_TABLE` | 1519 | g2p.rs | 60件 1519-2028行 |
| `lookup_exception` | 2030 | g2p.rs | `EXCEPTION_TABLE.iter().find` |
| `word_to_records` | 554 | lib.rs | `word_g2p`→`word_record_from_readings_final`→`postprocess` |
| `G2pDicts` 構築 | lib.rs:Mirae2Engine::from_paths | lib.rs | `Dict::load` x4 + `ConnectMatrix` |

---

## 5. 3辞書の対応表 (現行 vs 原本期待)

| 辞書 | pkg | 型/lookup | 呼び出し元(行) | 現行到達 | 備考 |
|------|-----|-----------|---------------|---------|------|
| Colligation | `Colligation.pkg` | `Dict::lookup_records` (335) | `word_to_readings_codes:575` | M経路で到達 | 最優先 |
| User | `User.pkg` | `Dict::lookup_records` (335) | `word_to_readings_codes:584` | Colligation miss時のみ到達 |  |
| NonReg | `NonReg.pkg` | `Dict::lookup_prefix_records` (341) | `nonreg_lookup:627` ← `word_g2p:814` | M miss時のみ到達 (現行は稀) | reverse_key 前提 |
| Conjects | `Conjects.pkg` | `Dict::lookup` (330) | `conjects_verify:721,758` | Mの境界検証で0-1回 | 接続可否 |
| Connect | `Connect.pkg` | `ConnectMatrix::row` | `conjects_verify:729,765` | 同上 | 行列 `row[xl][xr]!=0` |

---

*本レポートは `python open()` による file I/O で生成 (terminal 空出力障害の迂回)。行番号は `HEAD 69f97bb` 時点の `mirae-tts-engine/src/g2p.rs` に基づく。*
