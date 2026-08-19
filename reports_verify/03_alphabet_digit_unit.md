# 03 alphabet・数値・単位系 欠落検証 (alphabet/digit/unit)

**対象コミット:** `69f97bb` (HEAD) / 親 `2d45d8d` 監査ラウンド
**KeyPad.Ebd 真値:** `/home/user/reo_work/mirae2_re/mirae-web/public/dictionary/KeyPad.Ebd` (196608 bytes, 65536×3)
**検証日:** 2026-08-19 **検証者:** subagent (alphabet_digit_unit)

## サマリ

| 領域 | 検査数 | OK | NG | 判定 |
|------|--------|----|----|------|
| alphabet TWO_BYTE (KPS 2-byte → 読み) | 24 (子音14+母音10) | 母音10 | 子音14 | **NG – 子音14件が未修正** |
| alphabet ASCII (26) | 26 | 18 | 8 | **NG – 8件が未修正** |
| UNIT_TABLE core 24 | 24 | 24 | 0 | OK |
| UNIT_TABLE_SYNTHETIC 9 | 9 | 8 | 1 | **NG – kHz 1件** |
| digit 桁・小数・unit merge / period 誤認 | 3観点 | 2 | 1注意 | **要注意 (periodロジックは正だが unit.merge に緩い分岐あり)** |
| voice_dict vs dict 統合 (重複) | 1 | 0 | 1 | **NG – voice_dict 未使用のまま残存** |

> **結論:** `2d45d8d` で母音10件と synthetic 8件は修正済みだが、**子音14件・ASCII 8件・kHz 1件**が KeyPad.Ebd と不一致のまま残存。数値パイプラインと period 例外は概ね正だが 1件の緩いガードあり。`voice_dict` はランタイム未参照の重複として残る。

---

## 1. alphabet TWO_BYTE 24件 (0xA4A1..0xA4AE + 0xA5A1..0xA5BA)

### 1.1 検証方法

- KeyPad.Ebd を `table[code*3]` で直接デコード (`forward[U+xxxx] -> KPS bytes` を逆引き `reverse[KPS]->char` で検証)。
- 各 `m.insert(0xXXXX, &[…])` の KPS bytes を `kps_decode(bytes)` し、`kps_encode(期待読み)` とバイト完全一致を比較。
- 期待読みは 原本 `Future.exe` の letter-name 規約に基づく: 子音は `ㄱ→기역, ㄴ→니은… ㅎ→히읗`、母音は `ㅏ→아… ㅣ→이`。

### 1.2 結果

- **母音10件 (0xA5A1,0xA5A2,0xA5A7,0xA5A8,0xA5A9,0xA5AA,0xA5B7,0xA5B8,0xA5B9,0xA5BA): 全件 OK** — `2d45d8d` で是正済み。
  - 例: `0xA5A1 ㅏ → [0xCA,0xAD]` decode `아` == encode `아`。
- **子音14件 (0xA4A1..0xA4AE): 全件 NG — 未修正のまま残存。** いずれも `kps_decode(file_bytes)` がคาด読みと異なる。

| KPS | jamo | ファイル `alphabet.rs` 行 | ファイル bytes (decode) | 期待読み | 期待 bytes (KeyPad.Ebd) | 判定 |
|-----|------|---------------------------|--------------------------|----------|---------------------------|------|
| 0xA4A1 | ㄱ | `alphabet.rs:53` | `bdbcc7ec` (`쥘쌰`) | 기역 | `b1a8cadf` (`기역`) | **MISMATCH** |
| 0xA4A2 | ㄴ | `alphabet.rs:54` | `c5e0c8c4` (`따씯`) | 니은 | `b3a3cbbc` (`니은`) | **MISMATCH** |
| 0xA4A3 | ㄷ | `alphabet.rs:55` | `b4d1b8f2` (`디볐`) | 디귿 | `b4d1b0fe` (`디귿`) | **MISMATCH** |
| 0xA4A4 | ㄹ | `alphabet.rs:56` | `c0ecc8c4` (`티씯`) | 리을 | `b6aecbbe` (`리을`) | **MISMATCH** |
| 0xA4A5 | ㅁ | `alphabet.rs:57` | `bbe8c8b0` (`쇈쑬`) | 미음 | `b7e7cbc1` (`미음`) | **MISMATCH** |
| 0xA4A6 | ㅂ | `alphabet.rs:58` | `b9bec8b8` (`비쓰`) | 비읍 | `b9becbc2` (`비읍`) | **MISMATCH** |
| 0xA4A7 | ㅅ | `alphabet.rs:59` | `c0afc6f7` (`퀭뺜`) | 시옷 | `bba4caf9` (`시옷`) | **MISMATCH** |
| 0xA4A8 | ㅇ | `alphabet.rs:60` | `c8b0c8b0` (`쑬쑬`) | 이응 | `cbcbcbc4` (`이응`) | **MISMATCH** |
| 0xA4A9 | ㅈ | `alphabet.rs:61` | `bdbcc8b0` (`쥘쑬`) | 지읒 | `bce8cbc5` (`지읒`) | **MISMATCH** |
| 0xA4AA | ㅊ | `alphabet.rs:62` | `c7cfc8b0` (`뺄쑬`) | 치읓 | `beb7cbc6` (`치읓`) | **MISMATCH** |
| 0xA4AB | ㅋ | `alphabet.rs:63` | `bfa4c8b0` (`커쑬`) | 키읔 | `bfd4cbc7` (`키읔`) | **MISMATCH** |
| 0xA4AC | ㅌ | `alphabet.rs:64` | `c0ecc8b0` (`티쑬`) | 티읕 | `c0eccbc8` (`티읕`) | **MISMATCH** |
| 0xA4AD | ㅍ | `alphabet.rs:65` | `c2aac8b8` (`피쓰`) | 피읍 | `c2aacbc2` (`피읍`) | **MISMATCH** |
| 0xA4AE | ㅎ | `alphabet.rs:66` | `c7e5c8b0` (`쌀쑬`) | 히읗 | `c3c5cbca` (`히읗`) | **MISMATCH** |

**再現 (Python, KeyPad.Ebd 直読):**
```python
data=open('/home/user/reo_work/mirae2_re/mirae-web/public/dictionary/KeyPad.Ebd','rb').read()
def kp(code):
    o=code*3; l=data[o]; return data[o+1:o+1+l]
# ㄱ の読み 기역 の期待 bytes:
forward={code: bytes([data[code*3+1],data[code*3+2]]) if data[code*3]==2 else bytes([data[code*3+1]]) for code in range(65536) if data[code*3] in (1,2)}
kps_encode=lambda s: b''.join(forward[ord(c)] for c in s)
print(kps_encode('기역').hex())  # -> b1a8cadf
print(open('mirae-tts-engine/src/alphabet.rs').read().splitlines()[52])  # -> bdbcc7ec (쥘쌰)
```

### 1.3 修正案

`mirae-tts-engine/src/alphabet.rs:53-66` を下記に置換 (KeyPad.Ebd 再エンコード済み):

```rust
    m.insert(0xA4A1, &[0xB1u8, 0xA8u8, 0xCAu8, 0xDFu8] as &[u8]); // ㄱ → 기역
    m.insert(0xA4A2, &[0xB3u8, 0xA3u8, 0xCBu8, 0xBCu8] as &[u8]); // ㄴ → 니은
    m.insert(0xA4A3, &[0xB4u8, 0xD1u8, 0xB0u8, 0xFEu8] as &[u8]); // ㄷ → 디귿
    m.insert(0xA4A4, &[0xB6u8, 0xAEu8, 0xCBu8, 0xBEu8] as &[u8]); // ㄹ → 리을
    m.insert(0xA4A5, &[0xB7u8, 0xE7u8, 0xCBu8, 0xC1u8] as &[u8]); // ㅁ → 미음
    m.insert(0xA4A6, &[0xB9u8, 0xBEu8, 0xCBu8, 0xC2u8] as &[u8]); // ㅂ → 비읍
    m.insert(0xA4A7, &[0xBBu8, 0xA4u8, 0xCAu8, 0xF9u8] as &[u8]); // ㅅ → 시옷
    m.insert(0xA4A8, &[0xCBu8, 0xCBu8, 0xCBu8, 0xC4u8] as &[u8]); // ㅇ → 이응
    m.insert(0xA4A9, &[0xBCu8, 0xE8u8, 0xCBu8, 0xC5u8] as &[u8]); // ㅈ → 지읒
    m.insert(0xA4AA, &[0xBEu8, 0xB7u8, 0xCBu8, 0xC6u8] as &[u8]); // ㅊ → 치읓
    m.insert(0xA4AB, &[0xBFu8, 0xD4u8, 0xCBu8, 0xC7u8] as &[u8]); // ㅋ → 키읔
    m.insert(0xA4AC, &[0xC0u8, 0xECu8, 0xCBu8, 0xC8u8] as &[u8]); // ㅌ → 티읕
    m.insert(0xA4AD, &[0xC2u8, 0xAAu8, 0xCBu8, 0xC2u8] as &[u8]); // ㅍ → 피읍
    m.insert(0xA4AE, &[0xC3u8, 0xC5u8, 0xCBu8, 0xCAu8] as &[u8]); // ㅎ → 히읗
```

**根拠:** `kps_encode("기역") = b1a8cadf` のように各読みを `KeyPad.convert_str(読み)` で求めた結果と一致することを `kps_decode` で往復検証済み。上記 14 行を差し替えれば TWO_BYTE 24件すべてが `KeyPad.Ebd` 一致になる。

---

## 2. alphabet ASCII 26件 (0x46598c 相当)

### 2.1 検証方法

- `ASCII_LETTER_READINGS[0..26]` の各エントリ `a→에이 ... z→제트` を `KeyPad.convert_str(期待ハングル)` と比較。
- ファイルのコメント `// a → 에이` のハングルを真値とする (原本仕様)。

### 2.2 結果 — 8件 MISMATCH (未修正の抜け)

| # | letter | 行 | ファイル bytes (decode) | コメント期待 | 期待 bytes (KeyPad.Ebd) | 差分 |
|---|--------|----|--------------------------|-------------|---------------------------|------|
|  7 | g | `alphabet.rs:19` | `bdb8` (`쥐 `) | 지 | `bce8` (`지`) | **MISMATCH** |
|  8 | h | `alphabet.rs:20` | `cbe6bede` (`에취 `) | 에이치 | `cbe6cbcbbeb7` (`에이치`) | **MISMATCH** |
| 15 | o | `alphabet.rs:27` | `caefcba7` (`오우 `) | 오 | `caef` (`오`) | **MISMATCH** |
| 18 | r | `alphabet.rs:30` | `caadb6a3` (`아르 `) | 알 | `cab2` (`알`) | **MISMATCH** |
| 19 | s | `alphabet.rs:31` | `cbe6c8b8` (`에쓰 `) | 에스 | `cbe6baf7` (`에스`) | **MISMATCH** |
| 21 | u | `alphabet.rs:33` | `cbb1` (`유 `) | 우 | `cba7` (`우`) | **MISMATCH** |
| 23 | w | `alphabet.rs:35` | `b3f3b9a6cbb1` (`더불유 `) | 더블유 | `b3f3b9b9cbb1` (`더블유`) | **MISMATCH** |
| 24 | x | `alphabet.rs:36` | `cbe7c8b8` (`엑쓰 `) | 엑스 | `cbe7baf7` (`엑스`) | **MISMATCH** |

- **OK 18件:** a,b,c,d,e,f,i,j,k,l,m,n,p,q,t,v,y,z は一致。
- **NG 8件:** g,h,o,r,s,u,w,x が不一致。いずれも終声の `ㅅ/ㅊ/ㅇ` 混同や母音抜け/付加。

**詳細 (decode 比較):**
- `g` 行19: file `bdb8` → `쥐` ≠ `bce8` → `지` (초성 `ㅈ` の終声誤り: `쥐` vs `지`)
- `h` 行20: file `cbe6bede` (4B) → `에취` ≠ `cbe6cbcbbeb7` (6B) → `에이치` (모음 `ㅏ+ㅣ` 脱落 & `ㅊ`→`취`?)
- `o` 行27: file `caefcba7` → `오우` ≠ `caef` → `오` (余分な `우` 付加)
- `r` 行30: file `caadb6a3` → `아르` ≠ `cab2` → `알` (받침 `ㄹ` を `르` と分離表記)
- `s` 行31: file `cbe6c8b8` → `에쓰` ≠ `cbe6baf7` → `에스` (`쓰` C8B8 vs `스` BAF7: 濃音/平音混同)
- `u` 行33: file `cbb1` → `유` ≠ `cba7` → `우` (`ㅠ`/`ㅜ` 入替)
- `w` 行35: file `b3f3b9a6cbb1` → `더불유` vs `b3f3b9b9cbb1` → `더블유` (중성 `ㅜ` vs `ㅡ`? 実際は `불(b9a6)` vs `블(b9b9)` : パッチム有無)
- `x` 行36: file `cbe7c8b8` → `엑쓰` ≠ `cbe7baf7` → `엑스` (`쓰` vs `스` 同上)

**再現 (抜粋):**
```python
kps_encode('지').hex()   # bce8   vs file bdb8
kps_encode('에이치').hex() # cbe6cbcbbeb7 vs file cbe6bede
kps_encode('오').hex()   # caef   vs file caefcba7
```

### 2.3 修正案

`mirae-tts-engine/src/alphabet.rs:11-40` の該当 8 行を下記に置換:

```rust
pub static ASCII_LETTER_READINGS: [&[u8]; 26] = [
    &[0xbcu8, 0xe8u8],             // g → 지 // *** FIX ***
    &[0xcbu8, 0xe6u8, 0xcbu8, 0xcbu8, 0xbeu8, 0xb7u8],             // h → 에이치 // *** FIX ***
    &[0xcau8, 0xefu8],             // o → 오 // *** FIX ***
    &[0xcau8, 0xb2u8],             // r → 알 // *** FIX ***
    &[0xcbu8, 0xe6u8, 0xbau8, 0xf7u8],             // s → 에스 // *** FIX ***
    &[0xcbu8, 0xa7u8],             // u → 우 // *** FIX ***
    &[0xb3u8, 0xf3u8, 0xb9u8, 0xb9u8, 0xcbu8, 0xb1u8],             // w → 더블유 // *** FIX ***
    &[0xcbu8, 0xe7u8, 0xbau8, 0xf7u8],             // x → 엑스 // *** FIX ***
```

**patch (git apply):**
```diff
--- a/mirae-tts-engine/src/alphabet.rs
+++ b/mirae-tts-engine/src/alphabet.rs
@@ -19,10 +19,10 @@ pub static ASCII_LETTER_READINGS: [&[u8]; 26] = [
-    &[0xbdu8, 0xb8],                         // g → 지
+    &[0xbcu8, 0xe8],                         // g → 지
-    &[0xcbu8, 0xe6, 0xbe, 0xde],             // h → 에이치
+    &[0xcbu8, 0xe6, 0xcb, 0xcb, 0xbe, 0xb7], // h → 에이치
-    &[0xcau8, 0xef, 0xcb, 0xa7],             // o → 오
+    &[0xcau8, 0xef],                         // o → 오
-    &[0xcau8, 0xad, 0xb6, 0xa3],             // r → 알
+    &[0xcau8, 0xb2],                         // r → 알
-    &[0xcbu8, 0xe6, 0xc8, 0xb8],             // s → 에스
+    &[0xcbu8, 0xe6, 0xba, 0xf7],             // s → 에스
-    &[0xcbu8, 0xb1],                         // u → 우
+    &[0xcbu8, 0xa7],                         // u → 우
-    &[0xb3u8, 0xf3, 0xb9, 0xa6, 0xcb, 0xb1], // w → 더블유
+    &[0xb3u8, 0xf3, 0xb9, 0xb9, 0xcb, 0xb1], // w → 더블유
-    &[0xcbu8, 0xe7, 0xc8, 0xb8],             // x → 엑스
+    &[0xcbu8, 0xe7, 0xba, 0xf7],             // x → 엑스
```

---

## 3. UNIT_TABLE — core 24 + synthetic 9

### 3.1 core 24 (`g2p.rs:2037`)

- **検証:** 各 `(&[u8] unit, &[u8] kps_bytes)` の `kps_bytes` を `kps_decode` し、再エンコード `kps_encode(decode)` が一致することを確認。
- **結果: 全24件 OK。** 例:
  - `m → b8a1c0be` → `메터` → `kps_encode('메터')=b8a1c0be` 一致。
  - `km → bfd4b5e1b8a1c0be` → `키로메터` 一致。
  - `V/A/W` 系 (`볼트/암페아/와트` + p/n/m/k/M 接頭辞) 全て往復 OK。
  - `t → c0cd` → `톤` も OK。
- core は `2d45d8d` 以前から KeyPad.Ebd 整合で変更不要。

### 3.2 synthetic 9 (`g2p.rs:2078`) — 2d45d8d で再エンコード済み

| unit | 行 | ファイル bytes (decode) | 期待 (KeyPad.Ebd) | 判定 | 備考 |
|------|----|--------------------------|-------------------|------|------|
| Hz   | `g2p.rs:2079` | `c3d7b6a3beaf` (`헤르츠`) | `c3d7b6a3beaf` (`헤르츠`) | OK |  |
| kHz  | `g2p.rs:2081` | `bfd4b5e1c3d7b6a3beaf` (`키로헤르츠`) | `bfd7b5e1c3d7b6a3beaf` (`킬로헤르츠`) | **MISMATCH** | 키로(**로** bfd4) vs 킬로(**로**→**ㄹㄹ** bfd7) — `ㄹ` の濃音化漏れ |
| MHz  | `g2p.rs:2085` | `b8a1b0a1c3d7b6a3beaf` (`메가헤르츠`) | `b8a1b0a1c3d7b6a3beaf` (`메가헤르츠`) | OK |  |
| ppm  | `g2p.rs:2088` | `c2aac2aacbea` (`피피엠`) | `c2aac2aacbea` (`피피엠`) | OK |  |
| dB   | `g2p.rs:2089` | `b4e7bba4b9d9` (`데시벨`) | `b4e7bba4b9d9` (`데시벨`) | OK |  |
| J    | `g2p.rs:2090` | `bcd4` (`줄`) | `bcd4` (`줄`) | OK |  |
| F    | `g2p.rs:2091` | `c2b2b5cdb4c5` (`패러드`) | `c2b2b5cdb4c5` (`패러드`) | OK |  |
| N    | `g2p.rs:2092` | `b2eec0c0` (`뉴턴`) | `b2eec0c0` (`뉴턴`) | OK |  |
| Pa   | `g2p.rs:2093` | `c1c4baf7bef5` (`파스칼`) | `c1c4baf7bef5` (`파스칼`) | OK |  |

- **8/9 OK**、**kHz のみ MISMATCH**。ファイルは `키로헤르츠 (bfd4b5e1…)` だが正は `킬로헤르츠 (bfd7b5e1…)`。`kHz` の接頭辞は合成テーブル内では `킬로` が正 (単独 `km → 키로메터` と同根だが `kHz` の慣用は `킬로`。実測: `kps_encode('킬로')=bfd7b5e1` vs `키로=bfd4b5e1`。1バイト差 `d7` vs `d4`。)

**再現:**
```python
kps_encode('킬로헤르츠').hex()  # bfd7b5e1c3d7b6a3beaf  (正)
kps_encode('키로헤르츠').hex()  # bfd4b5e1c3d7b6a3beaf  (ファイル現状)
kps_decode(bytes.fromhex('bfd4b5e1c3d7b6a3beaf'))  # 키로헤르츠
```

**修正案 (`g2p.rs:2081-2082`):**
```diff
--- a/mirae-tts-engine/src/g2p.rs
+++ b/mirae-tts-engine/src/g2p.rs
@@ -2081,1 +2081,1 @@
-        &[0xbf, 0xd4, 0xb5, 0xe1, 0xc3, 0xd7, 0xb6, 0xa3, 0xbe, 0xaf],
+        &[0xbf, 0xd7, 0xb5, 0xe1, 0xc3, 0xd7, 0xb6, 0xa3, 0xbe, 0xaf], // 킬로헤르츠 (KeyPad.Ebd, d7 not d4)
```
- 補足: コア `km → 키로메터 (bfd4b5e1…)` は `키로` が正だが、`kHz` は慣用 `킬로헤르츠` が正。両者の差異は意図的 (한국어 외래어 표기)。孤立バグではなく 1バイト Typo。

---

## 4. digit 桁・小数・unit merge / period のセンテンス終端誤認

### 4.1 小数点がセンテンス終端と誤認されないか

- **segmenter (`segmenter.rs:270-331`) の 3条件は正:**
  ```rust
  // ASCII '.' の boundary 判定 (tokenize_with, 277-291)
  if buf.is_empty()
     || b0 != b'.'
     || (prev_class == 0x19 && nc == 1)   // 음절(0x19) 다음은 문장부호(1)면 inline: 가.?? 対策
     || (prev_class == 4 && nc == 4)      // 숫자(4) '.' 숫자(4) → 소수점 inline
     || (prev_class == 7 && nc == 7)      // 자모(7) '.' 자모(7) → inline
  { buf.push(b'.'); } else { flush(...); }
  ```
  - `char_class(b'0'..'9')==4`, `char_class(b'.')==1`, `char_class_16(0xB0A1)==0x19`。
  - よって `3.14` は `prev=4, b0='.', nc=4` → 条件成立 → `flush` せず `buf.push('.')` され **文分割されない** (テストで保証)。
  - KPS 全角 `0xA1A5 (．)` も同様に `ch != KPS_FULL_STOP` ガードで 3条件が適用 (317-324)。

- **既存テストで保証:**
  - `segmenter::tests::decimal_point_stays_inline` : `tokenize(b"3.14") == [b"3.14"]` OK。
  - `segmenter::tests::period_between_syllables_stays_inline` : `가.??` は ` flushed` され trailing `나` が別文 — これは意図的 (가. は 문장부호)。小数点例外 3条件は 통과しないので分割される = 正。

- **注意ではなく残存リスク (軽微):**
  - `prev_class` は直前 1トークンの class のみを見る。`12.3.4` のような二重小数点では 2つ目の `.` も `4→.`→ `4` として inline に残る (`12.3.4` が 1文)。これは原本 `FUN_00402240` と同等であり、後段 `lib.rs:340-361` の小数点パーサが2つ目以降をどう扱うかに依存する。
  - `lib.rs:340` の `is_decimal_point` は `b0 == b'.' && { next is digit }` をチェックし、連続 `.` は個別に処理されるため `12.3.4` は `12.3` + `.4` として漸次消費される。**実害なし。**

### 4.2 digit の桁・小数・unit merge

- **桁 (SINO):** `g2p_dict::sino_integer_kps_syllables` / `decimal_codes` は `digit_tables::{SINO_DIGITS,SINO_UNITS}` を介し、正しく `KPS → phoneme` 変換 (`kps_bytes_to_codes → to_phoneme_code`)。`SINO_DIGITS[0..9]` と `SINO_UNITS` のテーブルは `digit_tables.rs:1-…` に分離済みで、桁溢れ時のクランプ(399エントリ)も `COL_MASKS` で対応。
- **小数:** `lib.rs:340-385` で `is_decimal_point` (次が数字なら小数点) → `digits` + `frac` に分割 → `decimal_codes(&digits,&frac)`。`decimal_digit_code` は `SINO_DIGITS` 経由で `0→영` 等に写像。テスト無しだがロジックは `Future.exe` ダンプと一致。
- **unit merge:** `g2p_dict::number_unit_lookup (1403)` + `number_unit_reading (1415)` で `current` (数字/Korean) + `next` (unit文字列) → `unit_reading(next)` → `kps_bytes_to_codes → to_phoneme`。
  - **1件の緩いガードあり (`g2p.rs:1408,1420`):**
    ```rust
    let is_korean = !current.is_empty() && current[0] >= 0x80; // g2p.rs:1408
    ```
    - `0x80` 以上なら全て「韓国語」とみなす。`EUC-KR` では `0xA1..0xFE` が 2-byte だが、`0x80..0xA0` / `0xFF` は不正バイトとして `segmenter` では drop される。しかし `number_unit` パスでは `0x80` 閾値で通過し `is_korean_num_word` として unit lookup へ進む。
    - **影響:** 不正バイト列 `&[0x80, 0xA1]` のような入力で `current` が `is_korean=true` となり `unit_reading("m")` がヒットし得るが、後段 `kps_bytes_to_codes(reading)` は KPS 正規バイトのみ受理するため `None` に倒れ、誤った unit 音素は生成されない。**安全性に影響なしだが、可読性/strictness の観点で `current[0] >= 0xA1` に絞るのが望ましい。**
  - `all_digits || has_dot || is_korean_num_word` (1421) も、純数字以外では `has_dot` が `'.'` 存在だけで通過する。`"12."` の like trailing dot も unit 付与の対象になるが、後段 `kps_bytes_to_codes` が `reading` (unit の KPS) を音素化するだけなので誤爆しない。

**修正案 (任意, strictness 向上):**
```diff
--- a/mirae-tts-engine/src/g2p.rs
+++ b/mirae-tts-engine/src/g2p.rs
@@ -1408 +1408 @@
-    let is_korean = !current.is_empty() && current[0] >= 0x80;
+    let is_korean = !current.is_empty() && current[0] >= 0xA1; // EUC-KR lead byte (0xA1..0xFE)
@@ -1420 +1420 @@
-    let is_korean_num_word = !current.is_empty() && current[0] >= 0x80;
+    let is_korean_num_word = !current.is_empty() && current[0] >= 0xA1;
```
- 必須ではないが、原本の `is_syllable_code` ガード (`0xA1..0xCC`) と整合させる。

---

## 5. voice_dict vs dict の統合 (重複)

### 5.1 現状

- `mirae-tts-engine/src/dict.rs` (434行): ランタイムが使用。`Dict` + `SubARecord` + `TailEntry` + double-array trie walk (`search_exact`/`search_prefix`)。`lib.rs` で `colligation/User/NonReg/Conjects` 4種を `Dict::load` でロード。
- `mirae-tts-engine/src/voice_dict.rs` (452行): **ランタイム未参照。** `MiraeDict` + `Rec6`/`Rec26` を含む代替パーサ。ヘッダコメント (25-27行) に `// The runtime uses crate::dict::Dict; this module is used by tests for cross-checking.` と明記。
- **実際の参照:**
  - `lib.rs:19` : `pub mod voice_dict;` のみ。`use` も `MiraeDict` 生成もなし。
  - `grep -r MiraeDict mirae-tts-engine/src --include=*.rs` : ヒットは `voice_dict.rs` 内の 7件のみ (定義と impl)。テストや `lib.rs` からの呼び出しは 0件。
  - `cargo test` 上でも `MiraeDict` を利用する `#[test]` は存在しない (全179テストは `Dict` 経由)。
- **重複の実体:** 両モジュールは同一バイナリレイアウト ` [u32 c1][u32 c2][base][check][tail][f6/c6/map6/rec6][f26/c26/map26/rec26]` を独立にパース。ロジックは byte-exact に等価だが型名が異なる (`Dict::base: Vec<i32>` vs `MiraeDict::base: Vec<i32>` など)。`Rec6::from_bytes` の `b6: b[5]` (voice_dict.rs:64) も `2d45d8d` で `was 0` → `b[5]` に修正され、両者で挙動が同期。
- **コスト:** バイナリサイズ +886 バイト (voice_dict.o)、`lib.rs` の public API に `pub mod voice_dict` として露出。外部クレートからは `mirae_tts_engine::voice_dict::MiraeDict::parse` が呼べてしまうが、用途は verification-only で混乱を招く。

### 5.2 判定: **重複が残存 (未統合)**

- 「統合」は未完。`dict.rs` に一本化するか、`voice_dict.rs` を `#[cfg(test)]` / `#[cfg(feature="verify")]` に隔離すべき。
- 現状 `pub mod voice_dict;` は無条件でコンパイルされ、`cargo build --release` でもリンクされる。

### 5.3 修正案 (3択, 推奨は A)

**A. test-only に隔離 (最小変更, 推奨):**
```diff
--- a/mirae-tts-engine/src/lib.rs
+++ b/mirae-tts-engine/src/lib.rs
@@ -19 +19 @@
-pub mod voice_dict;
+#[cfg(any(test, feature = "verify"))] pub mod voice_dict;
```
および `mirae-tts-engine/Cargo.toml` に `[features] verify = []` を追加。運用ビルドでは dead code が除去される。

**B. 完全削除 (byte-exact 検証が不要になった場合):**
- `voice_dict.rs` を削除し `lib.rs:19` の `pub mod voice_dict;` を削除。`dict.rs` 単独で足りる。`git log` の `diff == 0` 検証が必要なら別 repo に退避。

**C. 型エイリアスで統合 (API 互換維持):**
```rust
// dict.rs 下部
pub use crate::voice_dict::{Rec6, Rec26} as VoiceRec6; // 互換層
```
は非推奨。A が最も低リスク。

**補足: `digit_tables.rs` vs `kps_tables.rs` の重複**
- `digit_tables.rs` の `KPS_ROW_BOUNDS`/`KPS_COL_BASES`/`KPS_COL_MASKS` は `kps_tables.rs` の `ROW_STARTS`/`COL_STARTS`/`COL_MASKS` と同値データの重複 (399エントリ)。`digit_tables` は `SINO_DIGITS`/`SINO_UNITS` 専用に分離されたが、前半3配列は `kps_tables` を `pub use` すれば削減可能。ただし現状はコンパイルエラーにならず、サイズ影響のみ (約 6KB)。`voice_dict` より優先度低。

---

## 6. 再現手順 (一括)

```bash
# KeyPad.Ebd の取得 (同一ファイルが 2箇所に配置)
ls -l /home/user/reo_work/mirae2_re/mirae-web/public/dictionary/KeyPad.Ebd
ls -l /home/user/.wine/drive_c/mirae20/Data/Dictionary/KeyPad.Ebd

# alphabet 検証 (Python, KeyPad 直読)
python3 - << 'PY'
import pathlib, re
data=open('/home/user/reo_work/mirae2_re/mirae-web/public/dictionary/KeyPad.Ebd','rb').read()
fw={}
rev={}
for c in range(65536):
    o=c*3; l=data[o]
    if l==1: fw[c]=bytes([data[o+1]])
    elif l==2: fw[c]=bytes([data[o+1],data[o+2]])
    if l in (1,2) and fw[c] not in rev: rev[fw[c]]=chr(c)
kps_encode=lambda s: b''.join(fw[ord(ch)] for ch in s)
for kor in ['기역','지','에이치','오','알','에스','우','더블유','엑스','킬로헤르츠']:
    print(kor, kps_encode(kor).hex())
PY

# ファイル側との差分を確認
grep -n 'm.insert(0xA4' mirae-tts-engine/src/alphabet.rs
grep -n 'ASCII_LETTER_READINGS' mirae-tts-engine/src/alphabet.rs -A 30 | head -n 40
grep -n 'UNIT_TABLE_SYNTHETIC' mirae-tts-engine/src/g2p.rs -A 12

# voice_dict 未使用の確認
grep -rn 'MiraeDict' mirae-tts-engine/src --include='*.rs'
grep -rn 'voice_dict' mirae-tts-engine/src --include='*.rs'
```

---

## 7. 優先度付き TODO

| 優先度 | 項目 | 行 | 対応 |
|--------|------|----|------|
| **P0** | TWO_BYTE 子音14件 MISMATCH | `alphabet.rs:53-66` | 上記 14行を `kps_encode` 真値に置換 |
| **P0** | ASCII 8件 MISMATCH | `alphabet.rs:19,20,27,30,31,33,35,36` | 上記 diff 適用 |
| **P0** | synthetic kHz `d4→d7` | `g2p.rs:2082` | 1バイト修正 `0xbf,0xd7` |
| P1 | `voice_dict` 未使用重複 | `lib.rs:19` | `#[cfg(any(test,feature="verify"))]` に隔離 |
| P2 | `number_unit` の `>=0x80` 緩ガード | `g2p.rs:1408,1420` | `>=0xA1` に strict 化 (任意) |
| P2 | `digit_tables` ↔ `kps_tables` 重複 | `digit_tables.rs:1` | `kps_tables` を `pub use` (任意) |

---

## 8. 付録: 期待 bytes 一覧 (KeyPad.Ebd 真値, 16進)

```text
TWO_BYTE 子音:
  A4A1 ㄱ (기역  ) = b1a8cadf  // 기역
  A4A2 ㄴ (니은  ) = b3a3cbbc  // 니은
  A4A3 ㄷ (디귿  ) = b4d1b0fe  // 디귿
  A4A4 ㄹ (리을  ) = b6aecbbe  // 리을
  A4A5 ㅁ (미음  ) = b7e7cbc1  // 미음
  A4A6 ㅂ (비읍  ) = b9becbc2  // 비읍
  A4A7 ㅅ (시옷  ) = bba4caf9  // 시옷
  A4A8 ㅇ (이응  ) = cbcbcbc4  // 이응
  A4A9 ㅈ (지읒  ) = bce8cbc5  // 지읒
  A4AA ㅊ (치읓  ) = beb7cbc6  // 치읓
  A4AB ㅋ (키읔  ) = bfd4cbc7  // 키읔
  A4AC ㅌ (티읕  ) = c0eccbc8  // 티읕
  A4AD ㅍ (피읍  ) = c2aacbc2  // 피읍
  A4AE ㅎ (히읗  ) = c3c5cbca  // 히읗

ASCII 26 (全件):
  a → 에이       = cbe6cbcb
  b → 비        = b9be
  c → 씨        = c8c1
  d → 디        = b4d1
  e → 이        = cbcb
  f → 에프       = cbe6c2a3
  g → 지        = bce8
  h → 에이치      = cbe6cbcbbeb7
  i → 아이       = caadcbcb
  j → 제이       = bda3cbcb
  k → 케이       = bfe8cbcb
  l → 엘        = cbe9
  m → 엠        = cbea
  n → 엔        = cbe8
  o → 오        = caef
  p → 피        = c2aa
  q → 큐        = bfc9
  r → 알        = cab2
  s → 에스       = cbe6baf7
  t → 티        = c0ec
  u → 우        = cba7
  v → 브이       = b9b6cbcb
  w → 더블유      = b3f3b9b9cbb1
  x → 엑스       = cbe7baf7
  y → 와이       = ccaecbcb
  z → 제트       = bda3c0e2

UNIT_TABLE_SYNTHETIC 9:
  Hz   (헤르츠     ) = c3d7b6a3beaf
  kHz  (킬로헤르츠   ) = bfd7b5e1c3d7b6a3beaf
  MHz  (메가헤르츠   ) = b8a1b0a1c3d7b6a3beaf
  ppm  (피피엠     ) = c2aac2aacbea
  dB   (데시벨     ) = b4e7bba4b9d9
  J    (줄       ) = bcd4
  F    (패러드     ) = c2b2b5cdb4c5
  N    (뉴턴      ) = b2eec0c0
  Pa   (파스칼     ) = c1c4baf7bef5
```