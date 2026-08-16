//! Real-data verification of the g2p_dict (dictionary lookup pipeline).
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


#[test]
fn kps_bytes_to_codes_known_syllables() {
    let kp = keypad();
    assert_eq!(kps_bytes_to_codes(&kp.convert_str("가")).unwrap(), vec![0x0420]);
    assert_eq!(
        kps_bytes_to_codes(&kp.convert_str("조건")).unwrap(),
        vec![0x3520, 0x04A2]
    );
    assert_eq!(kps_bytes_to_codes(&kp.convert_str("돈")).unwrap(), vec![0x1122]);
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
    assert_eq!(codes_to_kps_bytes(&[0x8030]).unwrap(), b"0");
    assert_eq!(codes_to_kps_bytes(&[0x802d]).unwrap(), b"-");
    assert_eq!(codes_to_kps_bytes(&[0x802e]).unwrap(), b".");
}

#[test]
fn key_str_to_codes_conversion() {
    assert_eq!(key_str_to_codes(&[0x01, 0x14]).unwrap(), vec![0x0420]);
    assert_eq!(
        key_str_to_codes(&[0x0d, 0x1c, 0x01, 0x18, 0x2c]).unwrap(),
        vec![0x3520, 0x04A4]
    );
    assert_eq!(key_str_to_codes(&[0x46]).unwrap(), vec![0x8030]);
    assert_eq!(key_str_to_codes(&[0x45]).unwrap(), vec![0x802d]);
    assert_eq!(key_str_to_codes(&[0x44]).unwrap(), vec![0x802e]);
    assert!(key_str_to_codes(&[0x00]).is_none());
}


#[test]
fn split_and_merge_finals() {
    let split = split_finals(&[0x1127]);
    assert_eq!(split, vec![0x1120, 0x0007]);
    assert_eq!(merge_finals(&split), vec![0x1127]);
    assert_eq!(split_finals(&[0x0420]), vec![0x0420]);
    assert_eq!(split_finals(&[0x1124]), vec![0x1124]);
}

#[test]
fn classify_candidate_kinds() {
    assert_eq!(super_classify(&[0x8030, 0x8031]), 1);
    assert_eq!(super_classify(&[0x8000 | 0x2d]), 2);
    assert_eq!(super_classify(&[0x8030, 0x8000 | 0x2d]), 3);
    assert_eq!(super_classify(&[0x0420]), 0x10);
}

fn super_classify(codes: &[u16]) -> u8 {
    mirae_tts_engine::g2p::g2p_dict::classify_candidate(codes)
}


#[test]
fn word_to_readings_ga_colligation_hit() {
    let kp = keypad();
    let d = dicts();
    let readings = word_to_readings(&d, &kp.convert_str("가"));
    assert_eq!(readings.len(), 1);
    assert_eq!(readings[0].bytes, kp.convert_str("가"));
    assert_eq!(readings[0].marker, 0x1b);
    assert!(readings[0].packed.is_none());
}

#[test]
fn word_g2p_ga_full_pipeline() {
    let kp = keypad();
    let d = dicts();
    let readings = word_g2p(&d, &kp.convert_str("가"));
    assert_eq!(readings.len(), 1);
    assert_eq!(readings[0].bytes, kp.convert_str("가"));
    assert_eq!(readings[0].marker, 0x1b);
}

#[test]
fn word_g2p_jogeon_fallback() {
    let kp = keypad();
    let d = dicts();
    let readings = word_g2p(&d, &kp.convert_str("조건"));
    assert_eq!(readings.len(), 1);
    assert_eq!(readings[0].bytes, kp.convert_str("조건"));
    assert_eq!(readings[0].marker, 0x11);
}

#[test]
fn word_to_readings_digit_packed() {
    let kp = keypad();
    let d = dicts();
    let readings = word_to_readings(&d, &kp.convert_str("3"));
    assert!(!readings.is_empty());
    assert!(readings.iter().all(|r| r.packed.is_some() && r.marker == 1));
}


#[test]
fn nonreg_ga_hit() {
    let kp = keypad();
    let d = dicts();
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
    assert!(nonreg_lookup(&d, &kp.convert_str("조건")).is_none());
    assert!(nonreg_lookup(&d, b"abc").is_none());
}


#[test]
fn conjects_verify_real_pairs() {
    let d = dicts();
    let hyo = kps_bytes_to_codes(&keypad().convert_str("효")).unwrap();
    let hyu = kps_bytes_to_codes(&keypad().convert_str("휴")).unwrap();
    assert!(conjects_verify(&d, &hyo, 0x14, &hyu, 0x14));
    let ga = kps_bytes_to_codes(&keypad().convert_str("가")).unwrap();
    let kka = kps_bytes_to_codes(&keypad().convert_str("까")).unwrap();
    assert!(!conjects_verify(&d, &ga, 0x14, &kka, 0x16));
}

#[test]
fn morph_type_code_mapping() {
    assert_eq!(morph_type_code(0x14), Some(0x8030));
    assert_eq!(morph_type_code(0x16), Some(0x8032));
    assert_eq!(morph_type_code(0x1f), Some(0x803b));
    assert_eq!(morph_type_code(0x13), None);
    assert_eq!(morph_type_code(0x20), None);
}


#[test]
fn to_phoneme_code_voiceinfo_verified() {
    assert_eq!(to_phoneme_code(0x0420), 0x6c00);
    assert_eq!(to_phoneme_code(0x1122), 0x0882);
    assert_eq!(to_phoneme_code(0x1125), 0x2082);
    assert_eq!(to_phoneme_code(0x04A2), 0x0840);
}


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
    assert_eq!(rec.phoneme_codes, vec![0x6c00]);
    assert_eq!(rec.phoneme_count, 1);
    assert_eq!(rec.phoneme_markers.len(), 1);
    let prosody = record_to_prosody(&rec);
    assert_eq!(prosody.len(), 1);
    assert_eq!(prosody[0].code, 0x6c00);
    let bytes = prosody[0].to_bytes();
    assert_eq!(bytes.len(), 12);
    assert_eq!(&bytes[2..4], &[0x00, 0x6c]);
}

#[test]
fn stage4_last_record_marker_9() {
    let mut recs = vec![ga_record(), ga_record(), ga_record()];
    stage4_cross_word_sandhi(&mut recs);
    assert_eq!(recs[2].rule_marker, 9);
    assert_eq!(recs[0].rule_flags, [0, 0, 0, 0]);
}

#[test]
fn stage7_prosody_weighted_average() {
    let mut recs = vec![ga_record(), ga_record(), ga_record()];
    recs[0].rule_marker = 2;
    recs[1].rule_marker = 0;
    recs[2].rule_marker = 4;
    stage7_prosody(&mut recs);
    assert_eq!(recs[0].accent, 3);
    assert_eq!(recs[1].accent, 0);
    assert_eq!(recs[2].accent, 4);
    assert_eq!(recs[2].accent, recs[2].rule_marker);
}

#[test]
fn stage8_chunk_boundary_at_60() {
    let mut mk = |count: usize, accent: u8| -> WordRecord {
        let mut r = WordRecord::default();
        r.phoneme_count = count;
        r.accent = accent;
        r.phoneme_markers = vec![0; count];
        r
    };
    let mut recs = vec![mk(30, 0), mk(30, 0), mk(10, 0)];
    stage8_final_markers(&mut recs);
    assert_eq!(recs[0].final_marker, 1);
    assert_eq!(recs[1].final_marker, 5);
    assert_eq!(recs[2].final_marker, 1);
    assert!(recs.iter().all(|r| r.phoneme_markers.iter().all(|&m| m & 0x80 == 0)));
}

#[test]
fn stage8_marker_case_mapping() {
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
    assert_eq!(recs[2].phoneme_markers[0] & 0x80, 0x80);
    assert_eq!(recs[3].final_marker, 7);
    assert_eq!(recs[4].final_marker, 5);
}


#[test]
fn word_to_phoneme_integration() {
    let kp = keypad();
    let d = dicts();
    let mut recs = vec![word_record_from_readings(&word_g2p(&d, &kp.convert_str("가")))];
    postprocess(&mut recs);
    assert_eq!(recs[0].phoneme_codes, vec![0x6c00]);
    let mut recs = vec![word_record_from_readings(&word_g2p(&d, &kp.convert_str("조건")))];
    postprocess(&mut recs);
    assert_eq!(recs[0].phoneme_codes, vec![0x6C87, 0x0840]);
    let prosody = record_to_prosody(&recs[0]);
    assert_eq!(prosody.len(), 2);
    assert_eq!(prosody[0].code, 0x6C87);
    assert_eq!(prosody[1].code, 0x0840);
}

