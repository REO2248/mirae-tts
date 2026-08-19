# g2p→tone→unit_select→render→WAV 文末マーカー欠落検証

**対象HEAD:** `69f97bb` (post `2d45d8d`)  
**検証日:** 2026-08-19  
**対象:** `mirae-tts-engine/src/{record.rs,tone.rs,unit_select.rs,render.rs,wav.rs,voice_dict.rs,voice_info.rs,g2p.rs,lib.rs}`  
**観点:** tone/prosody→unit選択→render→WAV まで文末マーカー (`MARKER_SENTENCE_END=1`, `MARKER_SPECIAL=2`) が欠落なく伝播するか

---

## 1. 結論（TL;DR）

| 区間 | 判定 | 要旨 |
|------|------|------|
| `record::init_from_marker` | **正** | `m==0 && sentence_final` で `marker=1, tone=1`。`build_sentence` では正しく付与 |
| `g2p::record_to_prosody` | **マーカー欠落（意図的代替）** | 常に `init_from_marker(..., false)` + 末尾 `tone_class=1` 代替。`marker=1` は付与されない |
| `lib::sentence_to_records` | **同上 — markerは常に0** | 数値マージ分岐も `false`、通常wordも `record_to_prosody` 経由で `marker=0`。最終 `tone_class= (* /10*10+4)` のみ付与 |
| `tone::apply_sandhi` 境界リンク | **dead code化** | `buf[ac-1].marker==1` なら `sentence[0].marker=2` だが、現行パイプラインでは `marker==1` が発生しないため `MARKER_SPECIAL` は付与されない。tone連結は `%10` 経由で継続するため音声途切れはなし |
| `unit_select::process` `next` 決定 | **欠落なし（maskで回避）** | `rec.marker != 1 && tone<=1` で `BOUNDARY_CODE` 判定。文末 `tone=4` なので `marker` に依らず `BOUNDARY_CODE` が選択され、最終ユニットは保持 |
| `render::render_units` | **欠落なし** | 全 `units` を線形走査。`extra` は `class%10<2 && pitch!=0` で二重化するのみで削除なし。`pause>0` は末尾でも無音付加 |
| `wav::WavWriter` | **欠落なし（旧truncateは修正済み）** | `split()` は `output_001.wav` を新規作成。`data_size` リセットのみでサンプルは捨てない |
| `voice_dict::Rec6::b6` | **ランタイム無影響** | `b6=b5` 複製は検証用 `MiraeDict` のみ。ランタイムは `Dict` 使用 |

**総合:** PCMサンプル欠落はなし。文末マーカー `1` の欠落は存在するが、下流が `tone_class` で代替判定するため可聴な文末消失には至らない。ただし原典binaryとの `marker` 完全一致を主張するなら乖離。

---

## 2. 詳細

### 2.1 `record.rs` — 正

```rust
pub(crate) fn init_from_marker(&mut self, marker_byte: u8, sentence_final: bool) {
    self.flags = (marker_byte >> 7) & 1;
    let m = marker_byte & 0x7F;
    self.tone_class = tone::initial_tone_class(m);
    if m == 0 && sentence_final { self.marker = 1; self.tone_class = 1; }
}
```

- `tone::build_sentence` は `i+1==n` を `sentence_final` に渡すため最終レコードに `marker=1` が立つ（テスト `build_sentence_records` で検証）。
- ランタイムでは `build_sentence` は未使用（`lib::synthesize_bytes` は `g2p::record_to_prosody` 経由）。

### 2.2 `g2p.rs` / `lib.rs` — 文末 `marker=1` が付与されない

`g2p::record_to_prosody`:
```rust
p.init_from_marker(marker, false); // 常にfalse
if i+1==n && (marker & 0x7f)==0 { p.tone_class = 1; } // markerは0のまま
```

`lib::sentence_to_records` 数値merge分岐:
```rust
rec.init_from_marker(if is_merged && i+1==n_codes {1} else {0}, false);
```

通常word分岐は `word_to_records` → `record_to_prosody` 経由で同上。ループ末尾:
```rust
if let Some(last)=groups.last_mut().unwrap().0.last_mut(){
    last.tone_class = (last.tone_class/10)*10+4; // tone=4で文末化、markerは触らない
}
```

結果: 生成される全 `ProsodyRecord.marker` は `0`。`MARKER_SENTENCE_END` はランタイムで一度も立たない。

### 2.3 `tone::apply_sandhi` — 境界リンクがdead

```rust
if ac==0 { sentence[0].tone_class = sentence[0].tone_class%10 + 0x28; }
else {
    if buf[ac-1].marker == MARKER_SENTENCE_END { sentence[0].marker = MARKER_SPECIAL; }
    sentence[0].tone_class = (buf[ac-1].tone_class%10)*10 + sentence[0].tone_class%10;
}
buf.extend_from_slice(sentence);
```

- `ac==0` 初回文は `0x28` 付与で正。
- `ac!=0` で `marker==1` 分岐は現行では never taken → `MARKER_SPECIAL(=2)` は付与されない。
- しかし `sentence[0].tone_class` の連結は `buf[ac-1]%10` で実行されるため prosody は継続。`is_real_phoneme` 等への影響はなし。
- **評価:** 音欠落なし。原典との `marker` 差分として記録すべき。

### 2.4 `unit_select::process` — `BOUNDARY_CODE` で文末を保持

```rust
next: if idx+1 < records.len() && rec.marker != MARKER_SENTENCE_END && tone <= 1 {
    records[idx+1].code
} else { BOUNDARY_CODE } // 0x6EB3
```

- 文末は `tone = last.tone_class%10 = 4` → `tone<=1` が false なので `BOUNDARY_CODE` が選ばれる。`marker` が0でも結果は同一。
- `prev` も `level<2` のみ前コードを参照し、文頭では `BOUNDARY_CODE`。
- `is_real_phoneme(cur>>10, next&0x1f)` は `next=BOUNDARY(=0x6EB3, low5=0x13!=除外集合)` で true のまま。二重化ロジックは文末でも発火し得るが欠落ではない。
- `scan` fallback (`marker_base 10000/20000`) でも最終音素は `req.cur` がハングル範囲のため hit し欠落しない。

### 2.5 `render.rs` — 全unitを描画

```rust
for u in units {
    let n = data.read_unit(u.record.woff, u.record.wlen, &mut scratch)?;
    out.extend_from_slice(&scratch[..n]); // 常に1回
    if is_real_phoneme(..) && class_i8%10<2 && extra.pitch!=0 { out.extend(..n2); }
    if u.record.pause>0 { out.resize(out.len()+pause*2, 0); }
}
```

- ループは `units.len()` まで例外なく実行。最終unitの `pause` は `unit_select` で `+1000/+1500` され WAV末尾の無音として保持。
- `ChunkRing` + `produce_chunks` は `per_chunk` バッチで分割するのみ。

### 2.6 `wav.rs` — 旧truncate破壊は解消

- `2d45d8d` で `File::create(&self.path)` → `parent.join(format!("{}_{:03}.{}", stem, split_index+1, ext))` に修正。
- `split()` はヘッダ書込後 `data_size=0` リセットのみ。サンプルは捨てない。
- 残存Low: `finish()` は最終セグメントの `data_size` のみ返す、巨大 `pcm.len()>threshold` は単一ファイルで閾値超過を許容。いずれも欠落ではない。

### 2.7 `voice_dict.rs` / `voice_info.rs`

- `Rec6::from_bytes` は `b6=b[5]` 複製。`MiraeDict` は `diff==0` 検証専用でランタイムは `Dict` を使用するため出力に影響なし。
- docの `[u8 b2][b3][b4][b5][b6]` 5バイト表記は誤記（実際は `phoneme_id 2B + 4B =6B`, `b6` は `b5` 複製）。`VoiceInfo` 28Bエントリは `count*28` 検証で欠落なし。

---

## 3. 再現・検証ログ

- `tone::build_sentence(&[0x0100], &[0]).marker == 1` で `init_from_marker` の文末付与は確認。
- `record_to_prosody` 単体テスト (`g2p_dict_test.rs:239,323`) では `marker==0` のまま `tone_class` のみ検証 — `marker==1` を期待しない。
- `lib::sentence_to_records` の最終 `tone_class%10==4` は `01_truncate_and_segment.md` で手動再現済み。
- `cargo test` 155 tests passed（`02_postprocess_stages.md` 記載）で segment/tone/unit_select/render/wav いずれも欠落なし。

---

## 4. 推奨対応

| # | 位置 | 提案 | Severity |
|---|------|------|----------|
| R-1 | `g2p.rs:record_to_prosody` / `lib.rs:442` | ランタイムでも文末 `marker=1` を立てるか、仕様として「`marker` は `tone` で代替し `1` は立てない」と `lib.rs` docに明記。原典binary完全一致を謳うなら `p.marker=1` を復活し `tone::apply_sandhi` の `MARKER_SPECIAL` 経路を生かす | Medium |
| R-2 | `tone::apply_sandhi` | `marker==1` が never の現状をコメント化、または `debug_assert!(buf[ac-1].marker==0)` で意図を固定 | Low |
| R-3 | `render.rs` / `unit_select.rs` `is_real_phoneme` | 共通化（`unit_select::is_real_phoneme` を単一真実源に）。`hi10` → `hi6` リネーム | Low |
| W-1 | `wav.rs` | `finish()` docに「分割時は最終セグメントサイズのみ返す」旨追記、または `total_bytes` と `Vec<PathBuf>` を返すAPI拡張 | Low |
| D-1 | `voice_dict.rs:28-32` | `6-byte records` 内訳を ` [u16 phoneme_id][u8 b2..b5]; b6 duplicates b[5]` と訂正 | Low |

---

## 5. 判定

- **PCM欠落:** なし。tone→unit→render→WAV いずれも最終サンプルを保持。
- **マーカー欠落:** あり（`marker=1/2` がランタイムで付与されない）。ただし `tone_class` による代替で可聴欠落には至らず、既存テストも `tone` ベースで固定されているためリグレッションなし。原典binaryとの `marker` 完全一致を目的とする場合のみ要修正。
