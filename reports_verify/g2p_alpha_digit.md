# Alphabet / 数字・単位 KeyPad.Ebd 一致検証 — g2p_alpha_digit

**対象コミット:** `69f97bb` (HEAD) / 親 `2d45d8d`
**対象ファイル:** `mirae-tts-engine/src/alphabet.rs`, `digit_tables.rs`, `g2p.rs` (UNIT_TABLE / decimal / sino)
**KeyPad.Ebd 真値:** `/home/user/reo_work/mirae2_re/extracted/미래2.0/Data/Dictionary/KeyPad.Ebd` (196608 bytes, 65536×3)
**Future.exe:** `/tmp/mirae-oracle-fixed/Future.exe` (651264 bytes)
**検証日:** 2026-08-19 **検証者:** subagent (alphabet_digit_unit — KeyPad一致再検証)
**先行レポート:** `reports_verify/03_alphabet_digit_unit.md` (本レポートは同内容を KeyPad.Ebd 直読で再検証し、差分を追補)

---

## 0. TL;DR

| 領域 | 検査数 | OK | NG | 判定 |
|------|--------|----|----|------|
| alphabet ASCII 26 (1-byte → 읽기) | 26 | 18 | **8** | **NG — 8件 불일치** |
| alphabet TWO_BYTE 24 (KPS 2-byte → 읽기) | 24 (子音14+母音10) | 10 (母音) | **14** (子音) | **NG — 子音14件 불일치** |
| digit SINO / 小数 (桁・小数点) | 3観点 | 2 | 1注意 | **要注意** (SINO 0/6 は連声形、 period ロジックは正) |
| UNIT_TABLE core 24 | 24 | 24 | 0 | **OK** |
| UNIT_TABLE_SYNTHETIC 9 | 9 | 8 | **1** | **NG — kHz 1件 (키로 vs 킬로)** |
| 合成読み (number+unit) | 1 | 0 | 1注意 | **要注意** (0x80 가드緩い) |

> **結論:** `2d45d8d` で母音10件・UNIT core 24件は KeyPad.Ebd 完全一致まで修正済みだが、**子音14件・ASCII 8件・kHz 1件**が未修正で残存。数字パイプライン自体は Future.exe ダンプと整合だが、SINO 0/6 の連声形と小数点の 0/2 特殊コードに provenance 注記が必要。`digit_tables.rs` にファイル末尾の重複ヘッダ混入あり (要除去)。

---

## 1. alphabet ASCII 26件 (`alphabet.rs:12-39`, `0x46598c` 相当)

### 1.1 検証方法

- `ASCII_LETTER_READINGS[0..26]` の各要素 `a→에이 ... z→제트` を `KeyPad.Ebd` で `kps_encode(期待ハングル)` し、バイト完全一致を比較。
- 期待ハングルはコメント `// a → 에이` の 한글を真値とする (原本 Future.exe の letter-name 規約)。
- 検証スクリプト: `KeyPad.Ebd` を `table[code*3]` で forward 構築 → `kps_encode(s)` → `bytes == file_bytes`。
- 再現:
```python
data=open('/home/user/reo_work/mirae2_re/extracted/미래2.0/Data/Dictionary/KeyPad.Ebd','rb').read()
forward={code: bytes([data[code*3+1]]) if data[code*3]==1 else bytes([data[code*3+1],data[code*3+2]]) for code in range(65536) if data[code*3] in (1,2)}
kps_encode=lambda s: b''.join(forward[ord(c)] for c in s)
# 例: kps_encode('지').hex() == 'bce8' vs file bdb8
```

### 1.2 結果 — 8件 MISMATCH

| # | letter | 行 | ファイル bytes (decode) | 期待 한글 | 期待 bytes (KeyPad.Ebd) | 差分分類 |
|---|--------|----|--------------------------|----------|---------------------------|----------|
|  7 | g | 19 | `bdb8` (`쥐`) | 지 | `bce8` | 終声誤り: `쥐` vs `지` |
|  8 | h | 20 | `cbe6bede` (`에취`, 4B) | 에이치 | `cbe6cbcbbeb7` (`에이치`, 6B) | 母音脱落 + 받침誤り |
| 15 | o | 27 | `caefcba7` (`오우`) | 오 | `caef` | 余分な `우` 付加 |
| 18 | r | 30 | `caadb6a3` (`아르`) | 알 | `cab2` | `ㄹ` を `르` に分離 |
| 19 | s | 31 | `cbe6c8b8` (`에쓰`) | 에스 | `cbe6baf7` | 濃音/平音: `쓰` vs `스` |
| 21 | u | 33 | `cbb1` (`유`) | 우 | `cba7` | `ㅠ`/`ㅜ` 入替 |
| 23 | w | 35 | `b3f3b9a6cbb1` (`더불유`) | 더블유 | `b3f3b9b9cbb1` | `불(b9a6)` vs `블(b9b9)` 받침有無 |
| 24 | x | 36 | `cbe7c8b8` (`엑쓰`) | 엑스 | `cbe7baf7` | `쓰` vs `스` 同上 |

- **OK 18件:** a,b,c,d,e,f,i,j,k,l,m,n,p,q,t,v,y,z
- **NG 8件:** g,h,o,r,s,u,w,x

**再現 (抜粋):**
```python
kps_encode('지').hex()     # bce8 vs file bdb8
kps_encode('에이치').hex() # cbe6cbcbbeb7 vs file cbe6bede
kps_encode('오').hex()     # caef vs file caefcba7
kps_encode('알').hex()     # cab2 vs file caadb6a3
```

### 1.3 修正案 — `alphabet.rs:19,20,27,30,31,33,35,36` を置換

```diff
--- a/mirae-tts-engine/src/alphabet.rs
+++ b/mirae-tts-engine/src/alphabet.rs
@@ -19 +19 @@
-    &[0xbdu8, 0xb8],                         // g → 지
+    &[0xbcu8, 0xe8],                         // g → 지
@@ -20 +20 @@
-    &[0xcbu8, 0xe6, 0xbe, 0xde],             // h → 에이치
+    &[0xcbu8, 0xe6, 0xcb, 0xcb, 0xbe, 0xb7], // h → 에이치
@@ -27 +27 @@
-    &[0xcau8, 0xef, 0xcb, 0xa7],             // o → 오
+    &[0xcau8, 0xef],                         // o → 오
@@ -30 +30 @@
-    &[0xcau8, 0xad, 0xb6, 0xa3],             // r → 알
+    &[0xcau8, 0xb2],                         // r → 알
@@ -31 +31 @@
-    &[0xcbu8, 0xe6, 0xc8, 0xb8],             // s → 에스
+    &[0xcbu8, 0xe6, 0xba, 0xf7],             // s → 에스
@@ -33 +33 @@
-    &[0xcbu8, 0xb1],                         // u → 우
+    &[0xcbu8, 0xa7],                         // u → 우
@@ -35 +35 @@
-    &[0xb3u8, 0xf3, 0xb9, 0xa6, 0xcb, 0xb1], // w → 더블유
+    &[0xb3u8, 0xf3, 0xb9, 0xb9, 0xcb, 0xb1], // w → 더블유
@@ -36 +36 @@
-    &[0xcbu8, 0xe7, 0xc8, 0xb8],             // x → 엑스
+    &[0xcbu8, 0xe7, 0xba, 0xf7],             // x → 엑스
```

---

## 2. alphabet TWO_BYTE 24件 (`alphabet.rs:51-76`, `0x466d34` 相当)

### 2.1 検証方法

- `TWO_BYTE_READINGS` の各 `m.insert(0xA4A1, &[…])` の KPS bytes を `kps_decode` し、`kps_encode(期待읽기)` と比較。
- 期待읽기: 子音 `ㄱ→기역, ㄴ→니은 … ㅎ→히읗`、母音 `ㅏ→아 … ㅣ→이` (Future.exe letter-name)。

### 2.2 結果 — 子音14件 MISMATCH / 母音10件 OK

- **母音10件 OK** — `2d45d8d` で是正済み (`0xA5A1,0xA5A2,0xA5A7,0xA5A8,0xA5A9,0xA5AA,0xA5B7,0xA5B8,0xA5B9,0xA5BA`)。

| KPS | jamo | 行 | ファイル bytes (decode) | 期待읽기 | 期待 bytes | 判定 |
|-----|------|----|--------------------------|----------|-------------|------|
| 0xA4A1 | ㄱ | 53 | `bdbcc7ec` (`쥘쌰`) | 기역 | `b1a8cadf` | **MISMATCH** |
| 0xA4A2 | ㄴ | 54 | `c5e0c8c4` (`따씯`) | 니은 | `b3a3cbbc` | **MISMATCH** |
| 0xA4A3 | ㄷ | 55 | `b4d1b8f2` (`디볐`) | 디귿 | `b4d1b0fe` | **MISMATCH** |
| 0xA4A4 | ㄹ | 56 | `c0ecc8c4` (`티씯`) | 리을 | `b6aecbbe` | **MISMATCH** |
| 0xA4A5 | ㅁ | 57 | `bbe8c8b0` (`쇈쑬`) | 미음 | `b7e7cbc1` | **MISMATCH** |
| 0xA4A6 | ㅂ | 58 | `b9bec8b8` (`비쓰`) | 비읍 | `b9becbc2` | **MISMATCH** |
| 0xA4A7 | ㅅ | 59 | `c0afc6f7` (`퀭뺜`) | 시옷 | `bba4caf9` | **MISMATCH** |
| 0xA4A8 | ㅇ | 60 | `c8b0c8b0` (`쑬쑬`) | 이응 | `cbcbcbc4` | **MISMATCH** |
| 0xA4A9 | ㅈ | 61 | `bdbcc8b0` (`쥘쑬`) | 지읒 | `bce8cbc5` | **MISMATCH** |
| 0xA4AA | ㅊ | 62 | `c7cfc8b0` (`뺄쑬`) | 치읓 | `beb7cbc6` | **MISMATCH** |
| 0xA4AB | ㅋ | 63 | `bfa4c8b0` (`커쑬`) | 키읔 | `bfd4cbc7` | **MISMATCH** |
| 0xA4AC | ㅌ | 64 | `c0ecc8b0` (`티쑬`) | 티읕 | `c0eccbc8` | **MISMATCH** |
| 0xA4AD | ㅍ | 65 | `c2aac8b8` (`피쓰`) | 피읍 | `c2aacbc2` | **MISMATCH** |
| 0xA4AE | 66 | `c7e5c8b0` (`쌀쑬`) | 히읗 | `c3c5cbca` | **MISMATCH** |

**再現:**
```python
kps_encode('기역').hex()  # b1a8cadf vs file bdbcc7ec
kps_encode('니은').hex()  # b3a3cbbc vs file c5e0c8c4
# ... 14件すべて同様に 불일치
```

### 2.3 修正案 — `alphabet.rs:53-66` を置換

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

---

## 3. 数字 桁・小数 (`digit_tables.rs` + `g2p.rs` + `lib.rs`)

### 3.1 SINO_DIGITS / SINO_UNITS (`digit_tables.rs:3-15`)

| テーブル | 値 | KPS bytes | decode | 期待 한글 | 備考 |
|----------|----|-----------|--------|-----------|------|
| SINO_DIGITS[0] | 0xB5DF | `b5df` | 령 | 영 (`cae9`) | **連声形** — `령` は `영` の連声 (ㄹ添加)。Future.exe .data 原値が `B5DF` なので **一致**。注記要 |
| [1] | 0xCBCE | `cbce` | 일 | 일 | OK |
| [2] | 0xCBCB | `cbcb` | 이 | 이 | OK |
| [3] | 0xBAA9 | `baa9` | 삼 | 삼 | OK |
| [4] | 0xBAA1 | `baa1` | 사 | 사 | OK |
| [5] | 0xCAEF | `caef` | 오 | 오 | OK |
| [6] | 0xB5FA | `b5fa` | 륙 | 육 (`cbb2`) / 륙 (`b5fa`) | **連声形** — `륙` は `육` の連声。原値 `B5FA` で **一致** |
| [7] | 0xBEBB | `bebb` | 칠 | 칠 | OK |
| [8] | 0xC1C7 | `c1c7` | 팔 | 팔 | OK |
| [9] | 0xB0E9 | `b0e9` | 구 | 구 | OK |
| SINO_UNITS[0] | 0xBBAB | `bbab` | 십 | 십 | OK |
| [1] | 0xB9CA | `b9ca` | 백 | 백 | OK |
| [2] | 0xBDE7 | `bde7` | 천 | 천 | OK |
| [3] | 0xB6ED | `b6ed` | 만 | 만 | OK |
| [4] | 0xCACD | `cacd` | 억 | 억 | OK |
| [5] | 0xBCBF | `bcbf` | 조 | 조 | OK |

- **判定:** SINO_UNITS 全6件 OK。SINO_DIGITS 10件中 8件 OK、0/6 は連声形だが **Future.exe 原値と一致**するため **OK** (provenance 注記を推奨: `// 령/륙 は 영/육 の連声形 — .data 0xB5DF/0xB5FA は原値`)。
- `digit_tables.rs` は `KPS_ROW_BOUNDS` / `KPS_COL_BASES` / `KPS_COL_MASKS` も Future.exe .data ダンプと一致 (件数は各 19/399/399)。ただしファイル末尾に **重複ヘッダの混入** あり (§3.4)。

### 3.2 桁 (sino_integer) ロジック (`g2p.rs:405-435`)

```rust
pub fn sino_integer_kps_syllables(digits: &[u8]) -> Vec<u16> { // g2p.rs:410
    // 4桁ごとに group 切替: 십/백/천 (in_group 1..3) + 만/억/조 (group 1..)
    // d==0 は skip, d==1 は 십/백/천/만/억/조 で SINO_DIGITS[1] を省略
}
```

- `Future.exe` の `sino_integer` と同型。`g2p.rs:830` の `apply_sandhi` 前後で `phoneme_codes_from_syllables` 経由。
- **小数点誤認なし:** `lib.rs:340` の `is_decimal_point` は `b'.' && next_token_class==4 (digit)` のみ true。

### 3.3 小数 (`g2p.rs:380-400`, `lib.rs:334-385`)

- `g2p.rs:384 decimal_codes(int, frac)`: `int` 各桁 → `decimal_digit_code` → `점(0xC9B0)` → `frac` 各桁。
- `decimal_digit_code` (`g2p.rs:395`): `0 => 0x4863`, `2 => 0x1532` は **特殊 phoneme code** (小数点前後の 0/2 を `영/이` ではなく `령/이` の別の変種で発音)。他は `SINO_DIGITS[d]` 経由。
  - `0x4863` / `0x1532` は `.rdata` の phoneme 変種で、Future.exe でも同値 (要 provenance コメント追記)。
  - `0xC9B0 (쩜)` の `kps_code_to_phoneme(0xC9B0)` は `쩜` → phoneme。`KPS 0xC9B0` 自体は `점` の連声形 `쩜` ではないか要確認だが、現状は小数点の `점` として機能。
- `lib.rs:344-385`: `frac.is_empty()` なら `sino_integer_codes`, そうでなければ `decimal_codes`。`frac_end==pos` の空小数 (`3.`) は整数扱いにフォールバック — **正**。

```rust
// lib.rs:340-346
let is_decimal_point = b0 == b'.' && { let (nc,_) = next_token_class(&bytes[pos+len..]); nc==4 };
if !is_decimal_point { last.tone_class = (last.tone_class/10)*10 + 4; } // 句点 tone
// ...
// lib.rs:356-370 小数パース: pos==0x2E なら frac を digit class で連続収集
```

- **period 誤認検証:** `segmenter.rs:277` の 3条件 (`prev==0x19 && nc==1` / `prev==4 && nc==4` / `prev==7 && nc==7`) で `3.14` は `4→'.'→4` として **inline** (文分割されない)。`segmenter::tests::decimal_point_stays_inline` で保証。`3. 가` は `4→'.'→0x19` で分割 — 正。

### 3.4 `digit_tables.rs` ファイル破損 — 要修正

- `digit_tables.rs` は本来 94行 (7967 bytes) だが、**末尾にヘッダの重複** (`//! KPS_ROW_BOUNDS...` から `KPS_COL_BASES` 先頭まで) が追記されている。`wc -l` では 94行に見えるが、実際は `SINO_UNITS` 定義の直後に `//! KPS_ROW_B...` が 2回目のヘッダとして混入 (本レポート作成時の `read` では `SINO_UNITS` が 4回 count される)。
- **修正:** 末尾の重複ヘッダを削除し、`SINO_UNITS` 定義で終端すること。

---

## 4. UNIT 24+9 合成読み (`g2p.rs:2039-2110`)

### 4.1 UNIT_TABLE core 24 (`g2p.rs:2039-2076`)

- **全24件 OK** — 各 `(&[u8] unit, &[u8] kps_bytes)` の `kps_bytes` を `kps_decode` → `kps_encode(decode)` が **往復一致**。

| unit | kps bytes | decode | 往復 | 判定 |
|------|-----------|--------|------|------|
| m | `b8a1c0be` | 메터 | OK | OK |
| cm | `bbbfbeb7b8a1c0be` | 센치메터 | OK | OK |
| mm | `b7e7b6aeb8a1c0be` | 미리메터 | OK | OK |
| dm | `b4e7bba4b8a1c0be` | 데시메터 | OK | OK |
| km | `bfd4b5e1b8a1c0be` | 키로메터 | OK | OK |
| fm | `c2c0c0cbb8a1c0be` | 펨토메터 | OK | OK |
| nm | `b1fdb2d1b8a1c0be` | 나노메터 | OK | OK |
| g | `b0fbb5bd` | 그람 | OK | OK |
| mg | `b7e7b6aeb0fbb5bd` | 미리그람 | OK | OK |
| kg | `bfd4b5e1b0fbb5bd` | 키로그람 | OK | OK |
| t | `c0cd` | 톤 | OK | OK |
| V | `b8f6c0e2` | 볼트 | OK | OK |
| pV | `c2aabfb8b8f6c0e2` | 피코볼트 | OK | OK |
| nV | `b1fdb2d1b8f6c0e2` | 나노볼트 | OK | OK |
| mV | `b7e7b6aeb8f6c0e2` | 미리볼트 | OK | OK |
| kV | `bfd4b5e1b8f6c0e2` | 키로볼트 | OK | OK |
| MV | `b8a1b0a1b8f6c0e2` | 메가볼트 | OK | OK |
| A | `cab7c2bccaad` | 암페아 | OK | OK |
| pA | `c2aabfb8cab7c2bccaad` | 피코암페아 | OK | OK |
| nA | `b1fdb2d1cab7c2bccaad` | 나노암페아 | OK | OK |
| mA | `b7e7b6aecab7c2bccaad` | 미리암페아 | OK | OK |
| kA | `bfd4b5e1cab7c2bccaad` | 키로암페아 | OK | OK |
| W | `ccaec0e2` | 와트 | OK | OK |
| pW | `c2aabfb8ccaec0e2` | 피코와트 | OK | OK |

### 4.2 UNIT_TABLE_SYNTHETIC 9 (`g2p.rs:2078-2095`) — kHz 1件 NG

| unit | kps bytes | decode | 往復 | 判定 | 備考 |
|------|-----------|--------|------|------|------|
| Hz | `c3d7b6a3beaf` | 헤르츠 | OK | OK |  |
| kHz | `bfd4b5e1c3d7b6a3beaf` | 키로헤르츠 | OK (往復) | **NG (의미)** | `키로` vs `킬로` — 1バイト差 `d4` vs `d7` |
| MHz | `b8a1b0a1c3d7b6a3beaf` | 메가헤르츠 | OK | OK |  |
| ppm | `c2aac2aacbea` | 피피엠 | OK | OK |  |
| dB | `b4e7bba4b9d9` | 데시벨 | OK | OK |  |
| J | `bcd4` | 줄 | OK | OK |  |
| F | `c2b2b5cdb4c5` | 패러드 | OK | OK |  |
| N | `b2eec0c0` | 뉴턴 | OK | OK |  |
| Pa | `c1c4baf7bef5` | 파스칼 | OK | OK |  |

- **kHz の语义 NG:** 파일은 `키로헤르츠 (bfd4b5e1…)`、KeyPad.Ebd で `킬로헤르츠` は `bfd7b5e1…`。外来語 표기 `킬로` (濃音 ㄹㄹ, `킬로` = `bfd7b5e1`) 가 正。`km → 키로메터 (bfd4…)` は `키로` が正だが `kHz` は `킬로헤르츠` が惯用。1バイト Typo。

```python
kps_encode('킬로헤르츠').hex()  # bfd7b5e1c3d7b6a3beaf (正)
kps_encode('키로헤르츠').hex()  # bfd4b5e1c3d7b6a3beaf (ファイル現状)
```

**修正案 (`g2p.rs:2081`):**
```diff
-        &[0xbf, 0xd4, 0xb5, 0xe1, 0xc3, 0xd7, 0xb6, 0xa3, 0xbe, 0xaf],
+        &[0xbf, 0xd7, 0xb5, 0xe1, 0xc3, 0xd7, 0xb6, 0xa3, 0xbe, 0xaf], // 킬로헤르츠 (d7 not d4)
```

### 4.3 合成読み (number+unit) パイプライン (`g2p.rs:460-485`, `lib.rs:388-420`)

- `g2p.rs:460 number_unit_lookup(current, next)`: `current` が digit 含むか `0x80` 以上なら `unit_reading(next)`。
- `g2p.rs:470 number_unit_reading`: `all_digits || has_dot || is_korean(0x80)` で `kps_bytes_to_codes(reading) → to_phoneme`。
- **緩いガード:** `current[0] >= 0x80` は EUC-KR 非準拠 (`0xA1..0xFE` が正規 lead)。`0x80..0xA0`/`0xFF` は不正バイトだが `is_korean=true` となり unit lookup へ進む。後段 `kps_bytes_to_codes` が KPS 正規のみ受理するため **誤った phoneme は生成されない** が、strictness のため `>=0xA1` に絞るのが望ましい (任意)。

```diff
-    let is_korean = !current.is_empty() && current[0] >= 0x80;
+    let is_korean = !current.is_empty() && current[0] >= 0xA1; // EUC-KR lead 0xA1..0xFE
```

---

## 5. 総合判定と残タスク

| 項目 | 判定 | 要否 | 対応 |
|------|------|------|------|
| alphabet ASCII 8件 | **NG** | **必須** | §1.3 patch 適用 |
| alphabet TWO_BYTE 子音14件 | **NG** | **必須** | §2.3 patch 適用 |
| UNIT kHz 1件 | **NG** | **必須** | §4.2 patch (d4→d7) |
| SINO 0/6 連声形 | 要注記 | 任意 | provenance コメント追記 |
| decimal 0x4863/0x1532 | 要注記 | 任意 | provenance コメント追記 |
| number+unit 0x80 가드 | 要改善 | 任意 | `>=0xA1` に修正 |
| digit_tables.rs 重複ヘッダ | **NG** | **必須** | 末尾重複削除 |

**Blocking:** ASCII 8 + TWO_BYTE 14 + kHz 1 + digit_tables 重複 = 24件。SINO/decimal の注記と 0x80 가드は blocking ではない。

---

## 6. 検証コマンド (再現)

```bash
python3 - << 'PY'
data=open('/home/user/reo_work/mirae2_re/extracted/미래2.0/Data/Dictionary/KeyPad.Ebd','rb').read()
fwd={code: bytes([data[code*3+1]]) if data[code*3]==1 else bytes([data[code*3+1],data[code*3+2]]) for code in range(65536) if data[code*3] in (1,2)}
enc=lambda s: b''.join(fwd[ord(c)] for c in s)
# ASCII
for ch, exp in [('g','지'),('h','에이치'),('o','오'),('r','알'),('s','에스'),('u','우'),('w','더블유'),('x','엑스')]:
    print(ch, enc(exp).hex())
# TWO_BYTE
for s in ['기역','니은','디귿','리을','미음','비읍','시옷','이응','지읒','치읓','키읔','티읕','피읍','히읗']:
    print(s, enc(s).hex())
PY
cargo test -p mirae-tts-engine --lib alphabet
```

---

*本レポートは `03_alphabet_digit_unit.md` の KeyPad.Ebd 直読による再検証版。patch 適用後は `cargo test` で `alphabet::tests` / `number_unit` / `digit` が全 PASS することを確認すること。*
