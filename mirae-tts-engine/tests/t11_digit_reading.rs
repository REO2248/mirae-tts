//! t11: 数字読みサブシステム (FUN_0040afb0/FUN_0043c230 相当) の検証。
//! 期待値はすべてオリジナル実測 (/tmp/orig_capture.json, wine キャプチャ) に基づく。
//! Usage: cargo test --release --test t11_digit_reading -- --nocapture
use mirae_tts_engine::g2p::g2p_dict::{kps_bytes_to_codes, sino_integer_codes, to_phoneme_code, decimal_codes};

fn kpsbe(u: u16) -> [u8; 2] {
    u.to_be_bytes()
}

fn ph(kps: u16) -> u16 {
    let codes = kps_bytes_to_codes(&kpsbe(kps)).unwrap_or_default();
    codes.iter().map(|&c| to_phoneme_code(c)).next().unwrap_or(0)
}

/// マスクベースのクラス計算 (FUN_004280a0 相当) の実測一致:
/// キャプチャ 287/287 レコードで検証済み (t11_digit_reading.md)。
#[test]
fn class_from_mask_capture_anchors() {
    // (KPS, 実測音素コード)
    let anchors: &[(u16, u16)] = &[
        (0xBCAD, 0x0847), // 전
        (0xBBF4, 0x6C07), // 자
        (0xBAB7, 0x6C46), // 서
        (0xB0D6, 0x6C80), // 고
        (0xCBCE, 0x1932), // 일 (通常変換 — 数字読みの 0x1532 とは別)
        (0xBCB3, 0x4847), // 정 (ㅇ → クラス 18)
        (0xBAC2, 0x4846), // 성 (ㅇ → クラス 18)
        (0xBAA9, 0x3806), // 삼 (ㅁ → クラス 14)
        (0xBBAB, 0x3D26), // 십 (ㅂ → クラス 15)
        (0xB5DF, 0x4863), // 령 (ㅇ → クラス 18)
        (0xCBA6, 0x48B2), // 용 (ㅇ → クラス 18)
    ];
    for &(kps, expect) in anchors {
        let got = ph(kps);
        assert_eq!(got, expect, "KPS {kps:#06x} → phoneme {got:#06x}, expect {expect:#06x}");
    }
}

/// 「2.0」→ 0x1532 (2) 0x3851 (.) 0x4863 (0) — 実測一致。
#[test]
fn decimal_2_0_capture() {
    let codes = decimal_codes(&[2], &[0]);
    assert_eq!(codes, vec![0x1532, 0x3851, 0x4863]);
}

/// 「1500」→ 천오백: 천(0x0848) 오(0x6C92) 백(0x0145) — 3 レコード。
/// 注: 백 は PE 実値 0xB9CA (終声ㄱ付き) → 0x0145。実測 REQ 220 の 0x6D45 は
/// 「백여권의」の連音後レコード (배+겨) であり、sino_integer_codes 単体の出力ではない
/// (t11_digit_reading.md / digit_tables.rs コメント参照)。
#[test]
fn integer_1500_capture() {
    let codes = sino_integer_codes(&[1, 5, 0, 0]);
    assert_eq!(codes, vec![0x0848, 0x6C92, 0x0145]);
    assert_eq!(codes.len(), 3);
}

/// 「35」→ 삼십오: 삼(0x3806) 십(0x3D26) 오(0x6C92) — 3 レコード (実測一致)。
#[test]
fn integer_35_capture() {
    let codes = sino_integer_codes(&[3, 5]);
    assert_eq!(codes, vec![0x3806, 0x3D26, 0x6C92]);
    assert_eq!(codes.len(), 3);
}

/// 位取り規則: 1 省略 (10→십, 100→백, 1000→천, 10000→만), 0 読み飛ばし。
#[test]
fn integer_place_value_rules() {
    assert_eq!(ph(0xBBAB), 0x3D26); // 십
    assert_eq!(sino_integer_codes(&[1, 0]), vec![0x3D26]); // 10 → 십
    assert_eq!(sino_integer_codes(&[1, 1]), vec![0x3D26, 0x1932]); // 11 → 십일
    assert_eq!(sino_integer_codes(&[1, 0, 0]), vec![ph(0xB9CA)]); // 100 → 백 (PE 実値)
    assert_eq!(sino_integer_codes(&[1, 0, 0, 0]), vec![ph(0xBDE7)]); // 1000 → 천
    assert_eq!(sino_integer_codes(&[1, 0, 0, 0, 0]), vec![ph(0xB6ED)]); // 10000 → 만
    assert_eq!(sino_integer_codes(&[0]), vec![0x4863]); // 0 → 령
    assert_eq!(sino_integer_codes(&[2, 3, 0, 5]), vec![
        ph(0xCBCB), ph(0xBDE7), ph(0xBAA9), ph(0xB9CA), ph(0xCAEF),
    ]); // 2305 → 이천삼백오
}

/// 漢数詞テーブル (0x489190) の通常変換。
#[test]
fn sino_digit_phonemes() {
    // 0→령 1→일 2→이 3→삼 4→사 5→오 6→육 7→칠 8→팔 9→구
    assert_eq!(ph(0xB5DF), 0x4863); // 령
    assert_eq!(ph(0xCBCE), 0x1932); // 일
    assert_eq!(ph(0xCBCB), 0x6D32); // 이
    assert_eq!(ph(0xBAA9), 0x3806); // 삼
    assert_eq!(ph(0xBAA1), 0x6C06); // 사
    assert_eq!(ph(0xCAEF), 0x6C92); // 오
    assert_eq!(ph(0xB5FA), 0x00E3); // 육
    assert_eq!(ph(0xBEBB), 0x1928); // 칠
    assert_eq!(ph(0xC1C7), 0x180B); // 팔
    assert_eq!(ph(0xB0E9), 0x6CC0); // 구
}
