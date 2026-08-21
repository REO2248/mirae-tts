# 静的検証 — unit_select / render duration (wine GUI不要の範囲)

## 結論: 静的解析だけで大部分は検証可能。追加の静的差分は1件も見つからず。

## 検証手法
Future.exe を capstone で逆アセンブルし、unit選択系関数を Rust ポートと突合。
(wine GUI 実行は不使用)

## エンジン ctor (0x4c7xx) の既定値 — 全て一致

| offset | 値 | 意味 | 我々のポート |
|---|---|---|---|
| +0xdc | 0 | pitchスケール無効 | random_mode=false ✓ |
| +0xe0 | 120 | 基準ピッチ | render factor 120/pitch ✓ |
| +0xe4 | 90 | 既定リクエストpitch | request_pitch_default=90 ✓ |
| +0xe8 | 15 | smoothing許容 | pitch_smoothing_tolerance=15 ✓ |
| +0xec | 3 | 文末tone閾値 | end_tone_threshold=3 ✓ |
| +0xd4 | 20000 | marker_base | unit_select marker_base=20000 ✓ |

## FUN_0044a800 (正規化+分類) — 一致
- normalize: c/10==2→%10+0x1e; %10==2→+3; %10==5→+4 = `normalize_target_class` と同一
- pause判定 hi10∈{2,0xe,0x12,0x1b}×low5∈{1,4,0x12}, hi10==6×{3,4,0x12} = `is_pause` 同一
- PHON_CLASS_FLAG_D@0x48bc90 / C@0x48bb70 ゲートも同一テーブル参照

## FUN_0044b880 (走査) — フィルタ一致
0x4aa7a-0x4ab09 の filter table (0x48c644) 検査:
- bit7なし: pitch>=f[0] && wlen>=f[2] && wlen<=f[3]
- bit7あり: pitch>=f[0] && pitch<=f[1] && wlen>=f[2] (<=f[3] は後段)
→ 我々の scan の `f[0]<=pitch && pitch<=f[1] && f[2]<=wlen && wlen<=f[3]` と同一。

## FUN_00440470 (duration) — 重要な確認
0x4c135: `[ebp+0xdc]!=0` なら duration = fild(+0xe0=120) / pitch × wlen。
しかし ctor で **+0xdc=0 (既定OFF)**。GUIの表情モードでのみ有効化。
→ 我々のポートが random_mode のみでスケールを適用するのは**既定動作として正しい**。

## 残る差分の所在（静的に見つからなかった領域）

E2E長不一致 (42517 vs 33815 samples) は上記のどこにも説明がなく、残る候補は:
1. FUN_0044b880 スコアリング本体 (~795命令) の分岐レベル照合 — 静的継続可
2. stage7 pitch smoothing (0x40540-0x40630) の浮動小数演算順序 — 部分確認済み、要精査
3. GT wav の入力テキスト未記録問題（wine側メタデータ不足）— 静的解析では解決不可

## 結論
「wine GUIなしで静的解析だけで完結できるか」→ **ユニット選択・レンダリングの
ロジック検証は静的で可能かつ現時点で差分ゼロ**。E2E長差の最終切り分けには
(1) b880スコアリングの全分岐照合（静的継続）か、(2) 入力を固定した wine 再キャプチャ
のどちらかが必要。
