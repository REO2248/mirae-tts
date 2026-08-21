//! t11: digit-reading subsystem verification (FUN_0040afb0 / FUN_0043c230).
//! Usage: cargo test --release --test t11_digit_reading -- --nocapture
use mirae_tts_engine::g2p::g2p_dict::{
    decimal_codes, kps_bytes_to_codes, sino_integer_codes, to_phoneme_code,
};

fn kpsbe(u: u16) -> [u8; 2] {
    u.to_be_bytes()
}

fn ph(kps: u16) -> u16 {
    let codes = kps_bytes_to_codes(&kpsbe(kps)).unwrap_or_default();
    codes
        .iter()
        .map(|&c| to_phoneme_code(c))
        .next()
        .unwrap_or(0)
}

#[test]
fn class_from_mask_capture_anchors() {
    let anchors: &[(u16, u16)] = &[
        (0xBCAD, 0x0847),
        (0xBBF4, 0x6C07),
        (0xBAB7, 0x6C46),
        (0xB0D6, 0x6C80),
        (0xCBCE, 0x1932),
        (0xBCB3, 0x4847),
        (0xBAC2, 0x4846),
        (0xBAA9, 0x3806),
        (0xBBAB, 0x3D26),
        (0xB5DF, 0x4863),
        (0xCBA6, 0x48B2),
    ];
    for &(kps, expect) in anchors {
        let got = ph(kps);
        assert_eq!(
            got, expect,
            "KPS {kps:#06x} → phoneme {got:#06x}, expect {expect:#06x}"
        );
    }
}

#[test]
fn decimal_2_0_capture() {
    let codes = decimal_codes(&[2], &[0]);
    assert_eq!(codes, vec![0x1532, 0x3851, 0x4863]);
}

#[test]
fn integer_1500_capture() {
    let codes = sino_integer_codes(&[1, 5, 0, 0]);
    assert_eq!(codes, vec![0x0848, 0x6C92, 0x0145]);
    assert_eq!(codes.len(), 3);
}

#[test]
fn integer_35_capture() {
    let codes = sino_integer_codes(&[3, 5]);
    assert_eq!(codes, vec![0x3806, 0x3D26, 0x6C92]);
    assert_eq!(codes.len(), 3);
}

#[test]
fn integer_place_value_rules() {
    assert_eq!(ph(0xBBAB), 0x3D26);
    assert_eq!(sino_integer_codes(&[1, 0]), vec![0x3D26]);
    assert_eq!(sino_integer_codes(&[1, 1]), vec![0x3D26, 0x1932]);
    assert_eq!(sino_integer_codes(&[1, 0, 0]), vec![ph(0xB9CA)]);
    assert_eq!(sino_integer_codes(&[1, 0, 0, 0]), vec![ph(0xBDE7)]);
    assert_eq!(sino_integer_codes(&[1, 0, 0, 0, 0]), vec![ph(0xB6ED)]);
    assert_eq!(sino_integer_codes(&[0]), vec![0x4863]);
    assert_eq!(
        sino_integer_codes(&[2, 3, 0, 5]),
        vec![ph(0xCBCB), ph(0xBDE7), ph(0xBAA9), ph(0xB9CA), ph(0xCAEF),]
    );
}

#[test]
fn sino_digit_phonemes() {
    assert_eq!(ph(0xB5DF), 0x4863);
    assert_eq!(ph(0xCBCE), 0x1932);
    assert_eq!(ph(0xCBCB), 0x6D32);
    assert_eq!(ph(0xBAA9), 0x3806);
    assert_eq!(ph(0xBAA1), 0x6C06);
    assert_eq!(ph(0xCAEF), 0x6C92);
    assert_eq!(ph(0xB5FA), 0x00E3);
    assert_eq!(ph(0xBEBB), 0x1928);
    assert_eq!(ph(0xC1C7), 0x180B);
    assert_eq!(ph(0xB0E9), 0x6CC0);
}
