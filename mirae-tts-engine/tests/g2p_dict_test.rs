//! g2p_dict (辞書引きパイプライン) の実データ検証。
//!
//! 期待値は全て G2P_detail.md / SPEC_tts_rewrite.md と実データ
//! (Voice/*.pkg, KeyPad.Ebd, VoiceInfo.pkg) から導出:
//! - 「가」: colligation.pkg ヒット (X=13, 先頭レコード種別 0x1b=27)
//! - 「조건」: colligation/User/NonReg 全てミス → フォールバック (マーカ 0x11)
//! - NonReg「가」: 反転キー [14 01] のプレフィクス完全一致 (matched=2, 種別 0x11)
//! - Conjects: 효+種別F → X=9 / 휴+F → X=23 (Connect[9][23]=1 で接続可)
//!   가+F → X=0 / 까+H → X=91 (Connect[0][91]=0 で接続不可)
//! - 音素コード: 가(0x0420) → 0x6c00 (VoiceInfo 699 units), 돈(0x1124) → 0x1903
//!   (874 units) — G2P §11.5 の実データ例と完全一致

use std::path::Path;

use mirae_tts_engine::connect::ConnectMatrix;
use mirae_tts_engine::dict::Dict;
use mirae_tts_engine::g2p::g2p_dict::{
    codes_to_kps_bytes, conjects_verify, kps_bytes_to_codes, key_str_to_codes, merge_finals,
    morph_type_code, nonreg_lookup, postprocess, record_to_prosody, split_finals,
    stage4_cross_word_sandhi, stage7_prosody, stage8_final_markers, to_phoneme_code,
    word_g2p, word_record_from_readings, word_to_readings, G2pDicts, WordRecord,
};
use mirae_tts_engine::keypad::KeyPad;

const VOICE_DIR: &str = "/home/user/reo_work/mirae2_re/extracted/미래2.0/Voice";

fn dicts() -> G2pDicts<'static> {
    fn load(name: &str) -> Dict {
        Dict::load(Path::new(VOICE_DIR).join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
    }
    // リークで 'static 化 (テスト専用)
    let colligation: &'static Dict = Box::leak(Box::new(load("colligation.pkg")));
    let user: &'static Dict = Box::leak(Box::new(load("User.pkg")));
    let nonreg: &'static Dict = Box::leak(Box::new(load("NonReg.pkg")));
    let conjects: &'static Dict = Box::leak(Box::new(load("Conjects.pkg")));
    let connect: &'static ConnectMatrix =
        Box::leak(Box::new(ConnectMatrix::load(Path::new(VOICE_DIR).join("Connect.pkg")).unwrap()));
    G2pDicts {
        colligation,
        user,
        nonreg,
        conjects,
        connect,
    }
}

fn keypad() -> KeyPad {
    KeyPad::load("/home/user/reo_work/mirae2_re/extracted/미래2.0/Data/Dictionary/KeyPad.Ebd")
        .expect("real KeyPad.Ebd must load")
}

// ---------------------------------------------------------------------------
// 1. コード変換 (KeyPad バイト列 ↔ u16 音節コード ↔ キー文字列)
// ---------------------------------------------------------------------------

#[test]
fn kps_bytes_to_codes_known_syllables() {
    let kp = keypad();
    // 가 = b0 a1 → 初声1<<10 | 中声1<<5 = 0x0420
    assert_eq!(kps_bytes_to_codes(&kp.convert_str("가")).unwrap(), vec![0x0420]);
    // 조건 → 조(0x3520) 건(0x04A2) (init/med は Unicode 順、fin は北朝鮮順列内番号)
    assert_eq!(
        kps_bytes_to_codes(&kp.convert_str("조건")).unwrap(),
        vec![0x3520, 0x04A2]
    );
    // 돈 = ㄷ(4)ㅗ(9)ㄴ(1) = 0x1122
    assert_eq!(kps_bytes_to_codes(&kp.convert_str("돈")).unwrap(), vec![0x1122]);
    // 変換不能 (孤立バイト) → None
    assert!(kps_bytes_to_codes(&[0xa1]).is_none());
}

#[test]
fn codes_to_kps_bytes_roundtrip() {
    let kp = keypad();
    for w in ["가", "조건", "돈", "좋", "효", "휴", "흐", "나라", "사람"] {
        let bytes = kp.convert_str(w);
        let codes = kps_bytes_to_codes(&bytes).unwrap();
        assert_eq!(codes_to_kps_bytes(&codes).unwrap(), bytes, "roundtrip {w}");
    }
    // 特殊コード: 数字 0x8030 → '0'
    assert_eq!(codes_to_kps_bytes(&[0x8030]).unwrap(), b"0");
    assert_eq!(codes_to_kps_bytes(&[0x802d]).unwrap(), b"-");
    assert_eq!(codes_to_kps_bytes(&[0x802e]).unwrap(), b".");
}

#[test]
fn key_str_to_codes_conversion() {
    // 「가」のキー文字列 [01 14] → 0x0420
    assert_eq!(key_str_to_codes(&[0x01, 0x14]).unwrap(), vec![0x0420]);
    // 「조건」= [0d 1c 01 18 2c]
    assert_eq!(
        key_str_to_codes(&[0x0d, 0x1c, 0x01, 0x18, 0x2c]).unwrap(),
        vec![0x3520, 0x04A4]
    );
    // 特殊キー文字: 'F'(0x46)=数字0, 'E'(0x45)='-', 'D'(0x44)='.'
    assert_eq!(key_str_to_codes(&[0x46]).unwrap(), vec![0x8030]);
    assert_eq!(key_str_to_codes(&[0x45]).unwrap(), vec![0x802d]);
    assert_eq!(key_str_to_codes(&[0x44]).unwrap(), vec![0x802e]);
    // 未知文字 → None
    assert!(key_str_to_codes(&[0x00]).is_none());
}

// ---------------------------------------------------------------------------
// 2. 終声分離/再統合・候補分類 (FUN_0040a290 / FUN_0040a370 / FUN_0041f0a0)
// ---------------------------------------------------------------------------

#[test]
fn split_and_merge_finals() {
    // 돋 = 0x1127 (終声 ㄷ = 7 ∈ 分離対象)
    let split = split_finals(&[0x1127]);
    assert_eq!(split, vec![0x1120, 0x0007]);
    assert_eq!(merge_finals(&split), vec![0x1127]);
    // 가 = 0x0420 (終声なし) → そのまま
    assert_eq!(split_finals(&[0x0420]), vec![0x0420]);
    // 終声 ㄴ (4) は分離対象外
    assert_eq!(split_finals(&[0x1124]), vec![0x1124]);
}

#[test]
fn classify_candidate_kinds() {
    // 数字のみ → 1
    assert_eq!(super_classify(&[0x8030, 0x8031]), 1);
    // 記号のみ (bit15 付きで数字以外) → 2
    assert_eq!(super_classify(&[0x8000 | 0x2d]), 2);
    // 数字+記号混合 → 3
    assert_eq!(super_classify(&[0x8030, 0x8000 | 0x2d]), 3);
    // 通常 (音節を含む) → 0x10
    assert_eq!(super_classify(&[0x0420]), 0x10);
}

fn super_classify(codes: &[u16]) -> u8 {
    mirae_tts_engine::g2p::g2p_dict::classify_candidate(codes)
}

// ---------------------------------------------------------------------------
// 3. 単語→読みの主経路 (FUN_0041f320 相当)
// ---------------------------------------------------------------------------

#[test]
fn word_to_readings_ga_colligation_hit() {
    let kp = keypad();
    let d = dicts();
    let readings = word_to_readings(&d, &kp.convert_str("가"));
    assert_eq!(readings.len(), 1);
    // 読み = 가 (KeyPad バイト列), マーカ = colligation 6B レコード先頭種別 0x1b
    assert_eq!(readings[0].bytes, kp.convert_str("가"));
    assert_eq!(readings[0].marker, 0x1b);
    assert!(readings[0].packed.is_none());
}

#[test]
fn word_g2p_ga_full_pipeline() {
    let kp = keypad();
    let d = dicts();
    // FUN_00444fb0 相当の全体フロー: 文脈チェック → 形態素解析 → NonReg → フォールバック
    let readings = word_g2p(&d, &kp.convert_str("가"));
    assert_eq!(readings.len(), 1);
    assert_eq!(readings[0].bytes, kp.convert_str("가"));
    assert_eq!(readings[0].marker, 0x1b);
}

#[test]
fn word_g2p_jogeon_fallback() {
    let kp = keypad();
    let d = dicts();
    // 条件: colligation にも User にも NonReg にも無い → 単語そのまま + マーカ 0x11
    let readings = word_g2p(&d, &kp.convert_str("조건"));
    assert_eq!(readings.len(), 1);
    assert_eq!(readings[0].bytes, kp.convert_str("조건"));
    assert_eq!(readings[0].marker, 0x11);
}

#[test]
fn word_to_readings_digit_packed() {
    let kp = keypad();
    let d = dicts();
    // 数字のみ候補 → パック出力 0x152D|idx, マーカ 1 (FUN_0041f020)
    let readings = word_to_readings(&d, &kp.convert_str("3"));
    assert!(!readings.is_empty());
    assert!(readings.iter().all(|r| r.packed.is_some() && r.marker == 1));
}

// ---------------------------------------------------------------------------
// 4. NonReg 検索 (FUN_00444fb0 相当, G2P §5)
// ---------------------------------------------------------------------------

#[test]
fn nonreg_ga_hit() {
    let kp = keypad();
    let d = dicts();
    // 反転キー [14 01] でプレフィクス完全一致 → 読み = 가, マーカ = レコード種別 0x11
    let hit = nonreg_lookup(&d, &kp.convert_str("가")).expect("NonReg must hit 가");
    assert_eq!(hit.reading, kp.convert_str("가"));
    assert_eq!(hit.marker, 0x11);
    assert_eq!(hit.matched, 2);
    assert!(!hit.records.is_empty());
    assert_eq!(hit.records[0].kind, 0x11);
}

#[test]
fn nonreg_other_hits() {
    let kp = keypad();
    let d = dicts();
    for w in ["거"] {
        let hit = nonreg_lookup(&d, &kp.convert_str(w)).unwrap_or_else(|| panic!("{w} must hit"));
        assert_eq!(hit.reading, kp.convert_str(w), "{w} reading");
        assert_eq!(hit.marker, 0x11, "{w} marker");
    }
}

#[test]
fn nonreg_miss() {
    let kp = keypad();
    let d = dicts();
    // 条件: どのエントリにもマッチしない → None (呼び出し側がフォールバック)
    assert!(nonreg_lookup(&d, &kp.convert_str("조건")).is_none());
    assert!(nonreg_lookup(&d, b"abc").is_none());
}

// ---------------------------------------------------------------------------
// 5. Conjects 検証 (FUN_0044e670 相当, G2P §8)
// ---------------------------------------------------------------------------

#[test]
fn conjects_verify_real_pairs() {
    let d = dicts();
    // 接続可: 효+種別0x14 → X=9, 휴+種別0x14 → X=23, Connect[9][23] = 1
    let hyo = kps_bytes_to_codes(&keypad().convert_str("효")).unwrap();
    let hyu = kps_bytes_to_codes(&keypad().convert_str("휴")).unwrap();
    assert!(conjects_verify(&d, &hyo, 0x14, &hyu, 0x14));
    // 接続不可: 가+0x14 → X=0, 까+0x16 → X=91, Connect[0][91] = 0
    let ga = kps_bytes_to_codes(&keypad().convert_str("가")).unwrap();
    let kka = kps_bytes_to_codes(&keypad().convert_str("까")).unwrap();
    assert!(!conjects_verify(&d, &ga, 0x14, &kka, 0x16));
}

#[test]
fn morph_type_code_mapping() {
    // 種別 0x14..0x1f → 0x8030..0x803f (FUN_0044e4a0)
    assert_eq!(morph_type_code(0x14), Some(0x8030));
    assert_eq!(morph_type_code(0x16), Some(0x8032));
    assert_eq!(morph_type_code(0x1f), Some(0x803b));
    assert_eq!(morph_type_code(0x13), None);
    assert_eq!(morph_type_code(0x20), None);
}

// ---------------------------------------------------------------------------
// 6. 音素コード (DAT_00489214 クラス置換, G2P §11)
// ---------------------------------------------------------------------------

#[test]
fn to_phoneme_code_voiceinfo_verified() {
    // 実データ検証 (VoiceInfo.pkg のユニット数):  가 → 0x6c00、돈 → 0x1882
    assert_eq!(to_phoneme_code(0x0420), 0x6c00); // 가
    assert_eq!(to_phoneme_code(0x1122), 0x0882); // 돈 (ㄷ+ㅗ+ㄴ, cls 2)
    // 終声のクラスは列マスクのビット位置 (FUN_004280a0 相当, t11 で修正):
    // 돋(0x1125, (ㄷ,ㅗ) 列の 5 番目の終声) → クラス 8 (列マスクの 5 番目のセットビット位置)
    assert_eq!(to_phoneme_code(0x1125), 0x2082);
    // 終声 ㄴ → クラス 2: 건(0x04A2) → 2<<10 | 4<<5 | 0 = 0x0840
    assert_eq!(to_phoneme_code(0x04A2), 0x0840);
}

// ---------------------------------------------------------------------------
// 7. 語レコード + 9 段階後処理チェーン (G2P §9)
// ---------------------------------------------------------------------------

fn ga_record() -> WordRecord {
    let kp = keypad();
    let d = dicts();
    let readings = word_g2p(&d, &kp.convert_str("가"));
    word_record_from_readings(&readings)
}

#[test]
fn stage1_phoneme_codes_and_12b_conversion() {
    let mut rec = ga_record();
    postprocess(std::slice::from_mut(&mut rec));
    // 段階 1: 読み 가 → 音素コード列 [0x6c00], 音素数 1
    assert_eq!(rec.phoneme_codes, vec![0x6c00]);
    assert_eq!(rec.phoneme_count, 1);
    // 段階 8 まで通した最終マーカ列が音素ごとに付く
    assert_eq!(rec.phoneme_markers.len(), 1);
    // 12B レコード化: +2 = 音素コード
    let prosody = record_to_prosody(&rec);
    assert_eq!(prosody.len(), 1);
    assert_eq!(prosody[0].code, 0x6c00);
    // バイト列も 12B 構造
    let bytes = prosody[0].to_bytes();
    assert_eq!(bytes.len(), 12);
    assert_eq!(&bytes[2..4], &[0x00, 0x6c]);
}

#[test]
fn stage4_last_record_marker_9() {
    // 段階 4 (FUN_004407c0): 語末レコードへマーカ 9 を付与
    let mut recs = vec![ga_record(), ga_record(), ga_record()];
    stage4_cross_word_sandhi(&mut recs);
    assert_eq!(recs[2].rule_marker, 9);
    // フックは TODO (常に 0) なのでフラグは立たない
    assert_eq!(recs[0].rule_flags, [0, 0, 0, 0]);
}

#[test]
fn stage7_prosody_weighted_average() {
    // 段階 7 (FUN_00440470): 前後マーカの重み付き平均 (0.5) + 2 段平滑化 (0.99)
    let mut recs = vec![ga_record(), ga_record(), ga_record()];
    recs[0].rule_marker = 2;
    recs[1].rule_marker = 0;
    recs[2].rule_marker = 4;
    stage7_prosody(&mut recs);
    // rec1: (0 + 0) * 0.5 → s1 = 0, s2 = 0 → マーカ 2 (< 4) かつ [1.86, 2.9] 外 → アクセント 3
    assert_eq!(recs[0].accent, 3);
    // rec2: (2 + 4) * 0.5 = 3.0 → マーカ 0 なのでアクセントは 0 のまま
    assert_eq!(recs[1].accent, 0);
    // rec3: マーカ 4 → 伝播 (アクセント 4)
    assert_eq!(recs[2].accent, 4);
    // 最終語は +0xb5c6 = +0xb5c5
    assert_eq!(recs[2].accent, recs[2].rule_marker);
}

#[test]
fn stage8_chunk_boundary_at_60() {
    // 段階 8 (FUN_004425c0): 音素累積 60 (DAT_00489170) でチャンク境界 (マーカ 5)
    let mut mk = |count: usize, accent: u8| -> WordRecord {
        let mut r = WordRecord::default();
        r.phoneme_count = count;
        r.accent = accent;
        r.phoneme_markers = vec![0; count];
        r
    };
    let mut recs = vec![mk(30, 0), mk(30, 0), mk(10, 0)];
    stage8_final_markers(&mut recs);
    // rec1: 累積 30 < 60 → ケース 0: マーカ = (flag_link == 0) = 1
    assert_eq!(recs[0].final_marker, 1);
    // rec2: 累積 60 >= 60 → チャンク境界マーカ 5
    assert_eq!(recs[1].final_marker, 5);
    // rec3: 累積リセット後 10 → マーカ 1
    assert_eq!(recs[2].final_marker, 1);
    // bit7 伝搬は語単位では行わない (t21 確定: オリジナル FUN_004425c0 の
    // 後方伝搬は文単位で実行 — sentence_to_records のグループ処理が担当)。
    // 語単位で実行すると全語に bit7 が付き、キャプチャ (各行末尾語群のみ
    // bit7) と不一致になる。
    assert!(recs.iter().all(|r| r.phoneme_markers.iter().all(|&m| m & 0x80 == 0)));
}

#[test]
fn stage8_marker_case_mapping() {
    // +0xb5c6 → 最終マーカ変換: 3→3, 6→2, 8→6, 9→7, 4/5→5
    let mut mk = |accent: u8| -> WordRecord {
        let mut r = WordRecord::default();
        r.phoneme_count = 1;
        r.accent = accent;
        r.phoneme_markers = vec![0; 1];
        r
    };
    let mut recs = vec![mk(3), mk(6), mk(8), mk(9), mk(4)];
    stage8_final_markers(&mut recs);
    assert_eq!(recs[0].final_marker, 3);
    assert_eq!(recs[1].final_marker, 2);
    assert_eq!(recs[2].final_marker, 6);
    // ケース 8 は自身の音素マーカに bit7
    assert_eq!(recs[2].phoneme_markers[0] & 0x80, 0x80);
    assert_eq!(recs[3].final_marker, 7);
    assert_eq!(recs[4].final_marker, 5);
}

// ---------------------------------------------------------------------------
// 8. 統合: 単語 → 音素コード列 → 12B レコード
// ---------------------------------------------------------------------------

#[test]
fn word_to_phoneme_integration() {
    let kp = keypad();
    let d = dicts();
    // 가: colligation ヒット → 読み 가 → 音素 0x6c00
    let mut recs = vec![word_record_from_readings(&word_g2p(&d, &kp.convert_str("가")))];
    postprocess(&mut recs);
    assert_eq!(recs[0].phoneme_codes, vec![0x6c00]);
    // 조건: フォールバック → 読み 조건 → 音素 [0x6C87, 0x0840]
    let mut recs = vec![word_record_from_readings(&word_g2p(&d, &kp.convert_str("조건")))];
    postprocess(&mut recs);
    assert_eq!(recs[0].phoneme_codes, vec![0x6C87, 0x0840]);
    let prosody = record_to_prosody(&recs[0]);
    assert_eq!(prosody.len(), 2);
    assert_eq!(prosody[0].code, 0x6C87);
    assert_eq!(prosody[1].code, 0x0840);
}
