use mirae_tts_engine::g2p::number_unit_lookup;

#[allow(unused_imports)]
use kps9566::kps9566 as kps;

fn dec(bytes: &[u8]) -> String {
    let mut s = String::new();
    kps::Decoder::new().decode_to_string(bytes, &mut s, true);
    s
}

#[test]
fn number_unit_lookup_basic() {
    // Stage 6: digit in current + unit_match in next
    // "3" + "m" → should return m's reading (메터)
    let r = number_unit_lookup(b"3", b"m");
    assert!(r.is_some(), "3 + m should match");
    assert_eq!(dec(r.unwrap()), "메터");

    // "3" + "km" → should return km's reading
    let r = number_unit_lookup(b"3", b"km");
    assert!(r.is_some(), "3 + km should match");

    // "3" + "g" → grams (그람)
    let r = number_unit_lookup(b"3", b"g");
    assert!(r.is_some(), "3 + g should match");
    assert_eq!(dec(r.unwrap()), "그람");

    // "3" + "kg" → kilograms (킬로그람)
    let r = number_unit_lookup(b"3", b"kg");
    assert!(r.is_some(), "3 + kg should match");
    assert_eq!(dec(r.unwrap()), "키로그람");

    // "3" + "V" → volts (볼트)
    let r = number_unit_lookup(b"3", b"V");
    assert!(r.is_some(), "3 + V should match");
    assert_eq!(dec(r.unwrap()), "볼트");

    // "3" + "W" → watts (와트)
    let r = number_unit_lookup(b"3", b"W");
    assert!(r.is_some(), "3 + W should match");
    assert_eq!(dec(r.unwrap()), "와트");

    // Non-unit next token → should not match
    let r = number_unit_lookup(b"3", b"hello");
    assert!(r.is_none(), "3 + hello should not match");

    // Non-numeric current + unit next → should not match (stage 6 guard)
    let r = number_unit_lookup(b"abc", b"m");
    assert!(r.is_none(), "abc + m should not match");

    // Empty tokens → should not match
    let r = number_unit_lookup(b"", b"m");
    assert!(r.is_none(), "empty + m should not match");
    let r = number_unit_lookup(b"3", b"");
    assert!(r.is_none(), "3 + empty should not match");
}

#[test]
fn number_unit_reading_stage1_all_digit() {
    // Stage 1 & 6: all-digit token + unit
    // "3" + "m" → 삼(3) + 미터(meter) phoneme codes
    let r = mirae_tts_engine::g2p::number_unit_reading(b"3", b"m");
    assert!(r.is_some(), "3 + m should produce reading");
    let codes = r.unwrap();
    assert!(!codes.is_empty(), "should produce non-empty codes");

    // "100" + "km" → 백 + 킬로미터
    let r = mirae_tts_engine::g2p::number_unit_reading(b"100", b"km");
    assert!(r.is_some(), "100 + km should produce reading");
}

#[test]
fn number_unit_reading_stage2_decimal() {
    // Stage 2: decimal token + unit
    // "2.5" + "km" → decimal reading + 킬로미터
    let r = mirae_tts_engine::g2p::number_unit_reading(b"2.5", b"km");
    assert!(r.is_some(), "2.5 + km should produce reading");
    let codes = r.unwrap();
    assert!(
        !codes.is_empty(),
        "should produce non-empty codes for decimal"
    );

    // "3.14" + "m" → 삼점일사 + 미터
    let r = mirae_tts_engine::g2p::number_unit_reading(b"3.14", b"m");
    assert!(r.is_some(), "3.14 + m should produce reading");

    // Decimal with only fractional part
    let r = mirae_tts_engine::g2p::number_unit_reading(b".5", b"km");
    assert!(r.is_some(), ".5 + km should produce reading");
}

#[test]
fn number_unit_reading_stage3_korean_number_word() {
    // Stage 3: Korean number word + unit → unit reading only
    // "한" (one) + "m" → unit codes only (Korean word reading from caller)
    let r = mirae_tts_engine::g2p::number_unit_reading(b"\xc2\xd9", b"m");
    assert!(r.is_some(), "한 + m should produce unit reading");
    let codes = r.unwrap();
    assert!(
        !codes.is_empty(),
        "should produce codes for Korean word + unit"
    );
}

#[test]
fn number_unit_reading_stage7_no_match() {
    // Stage 7: no match → None
    let r = mirae_tts_engine::g2p::number_unit_reading(b"hello", b"m");
    assert!(r.is_none(), "hello + m should not match");

    let r = mirae_tts_engine::g2p::number_unit_reading(b"3", b"xyz");
    assert!(r.is_none(), "3 + xyz should not match");

    let r = mirae_tts_engine::g2p::number_unit_reading(b"", b"m");
    assert!(r.is_none(), "empty + m should not match");
}

#[test]
fn number_unit_reading_various_units() {
    // Test various unit types through the full pipeline
    let test_cases: &[(&[u8], &[u8])] = &[
        (b"5", b"g"),
        (b"10", b"V"),
        (b"100", b"W"),
        (b"1000", b"A"),
        (b"2", b"Hz"),
        (b"3", b"Hz"),
        (b"5", b"ppm"),
        (b"10", b"dB"),
        (b"25", b"J"),
        (b"37", b"F"),
    ];
    for (num, unit) in test_cases {
        let r = mirae_tts_engine::g2p::number_unit_reading(num, unit);
        assert!(
            r.is_some(),
            "{} + {} should produce reading",
            String::from_utf8_lossy(num),
            String::from_utf8_lossy(unit)
        );
        let codes = r.unwrap();
        assert!(
            !codes.is_empty(),
            "{} + {} should produce non-empty codes",
            String::from_utf8_lossy(num),
            String::from_utf8_lossy(unit)
        );
    }
}

#[test]
fn number_unit_reading_empty_and_edge_cases() {
    // Edge cases
    let r = mirae_tts_engine::g2p::number_unit_reading(b"0", b"m");
    assert!(r.is_some(), "0 + m should produce reading");

    let r = mirae_tts_engine::g2p::number_unit_reading(b"999999", b"km");
    assert!(r.is_some(), "999999 + km should produce reading");

    // Token with only dot
    let r = mirae_tts_engine::g2p::number_unit_reading(b".", b"m");
    // This should fall through all stages since "." is not a recognized number
    // The is_all_digits check fails, decimal check finds dot but no digits around it
}
