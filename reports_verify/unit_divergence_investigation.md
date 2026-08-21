# unit selection 分岐点調査 (2026-08-21)

## 目的
REQストリーム一致(281/281)なのに wine GT WAV と音声がズレる原因の特定。

## 確定した事実

### 1. 選択レベルの一致証拠 (記事冒頭)
t8アンカー(オリジナル実機キャプチャ)と我々のポートの選択テイク:

| ユニット | t8 seq | woff | wlen | 一致 |
|---|---|---|---|---|
| 전 | seq0 | 21078045 | 5429 | ✅ |
| 자 | seq2 | 87705763 | 2348 | ✅ |
| 서 | seq3 | 107857333 | 5065 | ✅ |
| 고 | seq5 | 124282834 | 3882 | ✅ |
| 미 | seq6 | 91090519 | 5572 | ✅ |
| 래 | seq8 | 113900837 | 3145 | ✅ |

(t8の375読み取りには extra 読み込みが挟まるため seq番号はズレるが内容一致)

### 2. 音声レベル
- 記事前半: 無音#29 (≈unit175/176境界, サンプル846301) まで **バイト一致**
- 分岐点: **unit176** req=(참,고,\|) pitch=87 — 「조선말대사전상식대사전」直後の
  文末境界ユニット
- 以降ドリフト累積し、総差99339サンプル(4.5秒)/記事全体

### 3. unit176の候補スコア分析 (orig_capture_t9.json req182)
- 我々/mirae2_tts2選択: woff=180553953 wlen=4553 score=4
- 候補最高スコア: woff=188957187 wlen=9269 score=44
- スコア最高が選ばれない仕組み(探索順/フォールバック)は FUN_0044b880 の
  実装詳細に依存 — この要求で我々とオリジナルが別テイクを選んだ可能性が高い

### 4. キャプチャデータの品質限界
- orig_capture_t9.json 要求列: 293件(実281+ノイズ)。LCS整列で52ペアのみ
  完全整列可能。req#52-53に孤立要求2件、以降オフセットが変動
- cands照合: woff単独では281/281がどこかのreqグループに見つかるが、
  要求対応付けが崩れるため「同一要求で同一テイク」の全数検証は不能
- /tmp/orig_unit_seq.json (375件のwoff/wlen系列・最も信頼できる比較データ)
  は消失済み。hwbp_units.log は0バイト化済み

## 結論
- ポート固有の問題ではない: mirae2_tts2 と同じ分岐(REQ一致でも選択テイクが
  中盤から変わる)が存在。ポートはmirae2_tts2と全入力バイト一致しており
  動作としては参照実装と完全同等
- 音声完全一致の次の一歩: FUN_0044b880 のスコア同点時の選択ルール
  (探索順序依存)を逆アセンブルで確定させ、unit176 のような
  「score=4 vs score=44」の要求でオリジナルがどちらを選ぶか再現すること
- 再現手段: hwbp キャプチャの再実行 (t8手順、/tmp/hwbp.c は消失、
  tts_reports2/scripts/t8_hwbp_capture.c にソースあり)

## ファイル
- /tmp/ours_units.txt — 我々の281ユニット要求+選択ダンプ (MIRAE_DEBUG=1)
- /tmp/kps_phoneme_map.txt — KPS9566→音素コード対応表 (2350エントリ)
- /tmp/art_ours.wav == /tmp/art_ref.wav — 記事合成結果 (バイト一致)


## POST-MORTEM (same day)

The divergence investigated above does not exist. The input text used for the
comparison (/tmp/article_s09_1.txt) had all newlines stripped; the original
capture text (/tmp/cap_text.txt) is newline-terminated. With the correct text,
port == mirae2_tts2 == Test.Wav byte-exact (d30a9754943e19b649054181dd6f618e).
The "unit176 branch point" was an artifact of comparing against a
mis-decomposed input. Kept as a record of the analysis method.
