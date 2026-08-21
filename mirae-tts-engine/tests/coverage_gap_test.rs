//! Coverage for previously-untested modules: alphabet / postprocess_tables / stage9 / wav.
use mirae_tts_engine::alphabet::{
    ascii_letter_reading, is_letter_reading_type, letter_reading_dispatch,
};
use mirae_tts_engine::g2p::g2p_dict::{WordRecord, stage9_post_loop_propagation};
use mirae_tts_engine::postprocess_tables::{
    DAT_0047D6B4, DAT_0047DF14, STAGE3_PAIR_TABLE, STAGE3_SENTENCE_TABLE, STAGE3_TYPE_A_TABLE,
    stage3_pair_matches, stage3_sentence_matches, stage3_type_a_matches,
};
use mirae_tts_engine::wav::{SAMPLE_RATE, WAV_HEADER_SIZE, write_wav_header};

#[test]
fn alphabet_ascii_readings() {
    assert_eq!(
        ascii_letter_reading(b'a').unwrap(),
        &[0xcb, 0xe6, 0xcb, 0xcb]
    );
    assert_eq!(
        ascii_letter_reading(b'A').unwrap(),
        &[0xcb, 0xe6, 0xcb, 0xcb]
    );
    assert_eq!(
        ascii_letter_reading(b'z').unwrap(),
        &[0xbd, 0xa3, 0xc0, 0xe2]
    );
    assert!(ascii_letter_reading(b'0').is_none());
}

#[test]
fn alphabet_type_gate() {
    assert!(is_letter_reading_type(0x1f));
    assert!(is_letter_reading_type(0x22));
    assert!(!is_letter_reading_type(0x21));
    assert!(!is_letter_reading_type(0x14));
}

#[test]
fn alphabet_dispatch_single_and_jamo() {
    let r = letter_reading_dispatch(b"a");
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].bytes, vec![0xcb, 0xe6, 0xcb, 0xcb]);
    let r2 = letter_reading_dispatch(&[0xA4, 0xA1]);
    assert_eq!(r2.len(), 1);
    assert!(!r2[0].bytes.is_empty());
}

#[test]
fn postprocess_tables_constants() {
    assert_eq!(DAT_0047DF14, &[0xc3, 0xa8, 0x00]);
    assert_eq!(DAT_0047D6B4, &[0xcc, 0xa9, 0x00]);
    assert!(!STAGE3_PAIR_TABLE.is_empty());
    assert!(!STAGE3_TYPE_A_TABLE.is_empty());
    assert!(!STAGE3_SENTENCE_TABLE.is_empty());
}

#[test]
fn postprocess_tables_helpers() {
    // pair lookup should hit a known entry (first element is a valid pair)
    let (a, b) = STAGE3_PAIR_TABLE[0];
    assert!(stage3_pair_matches(a, b));
    assert!(!stage3_pair_matches(b"not", b"found"));
    // type-a / sentence helpers at least return bool without panic
    let _ = stage3_type_a_matches(STAGE3_TYPE_A_TABLE[0]);
    let _ = stage3_sentence_matches(STAGE3_SENTENCE_TABLE[0]);
}

#[test]
fn stage9_propagates_across_boundary() {
    let r1 = WordRecord {
        final_marker: 3,
        phoneme_markers: vec![0x01, 0x02],
        ..WordRecord::default()
    };
    let r2 = WordRecord {
        phoneme_markers: vec![0x03, 0x04],
        ..WordRecord::default()
    };
    let mut records = vec![r1, r2];
    stage9_post_loop_propagation(&mut records);
    assert!(records[1].phoneme_markers.iter().all(|m| m & 0x80 != 0));
}

#[test]
fn wav_header_byte_exact() {
    let mut buf = std::io::Cursor::new(vec![0u8; WAV_HEADER_SIZE]);
    write_wav_header(&mut buf, 8).unwrap();
    let wav = buf.into_inner();
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(&wav[8..12], b"WAVE");
    assert_eq!(&wav[12..16], b"fmt ");
    assert_eq!(
        u32::from_le_bytes(wav[24..28].try_into().unwrap()),
        SAMPLE_RATE
    );
    assert_eq!(u32::from_le_bytes(wav[42..46].try_into().unwrap()), 8u32);
}

#[test]
fn voice_dir_env_helper() {
    // default_voice_dir falls back to DEFAULT_VOICE_DIR when env not set; constant wiring check
    assert_eq!(mirae_tts_engine::VOICE_DIR_ENV, "MIRAE_VOICE_DIR");
    assert_eq!(mirae_tts_engine::DEFAULT_VOICE_DIR, "Voice");
    // when env is absent (common in CI), default_voice_dir == DEFAULT_VOICE_DIR
    // Do not mutate process env here — crate forbids unsafe blocks in tests
    let p = mirae_tts_engine::default_voice_dir();
    assert!(p.ends_with("Voice") || p.ends_with("test_voice"));
}
