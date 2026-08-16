//! Integration tests: KeyPad.Ebd -> segmenter -> kps9566, plus tone sandhi over the 12B pipeline.
//! Expectations derived from the original Future.exe analysis (SPEC_tts_rewrite.md 2.1-2.4).
use mirae_tts_engine::keypad::KeyPad;
use mirae_tts_engine::kps9566::Kps9566;
use mirae_tts_engine::record::ProsodyRecord;
use mirae_tts_engine::segmenter::{tokenize, Sentence};
use mirae_tts_engine::tone::{apply_sandhi, build_sentence};

const KEYPAD_EBD: &str =
    "/home/user/reo_work/mirae2_re/extracted/미래2.0/Data/Dictionary/KeyPad.Ebd";

fn keypad() -> KeyPad {
    KeyPad::load(KEYPAD_EBD).expect("KeyPad.Ebd must load")
}

fn pipeline(text: &str) -> Vec<Sentence> {
    let kp = keypad();
    let internal = kp.convert_str(text);
    tokenize(&internal)
}

#[test]
fn real_keypad_has_65536_entries() {
    let kp = keypad();
    for code in [
        0x0000u16, 0x0041, 0xAC00, 0xB098, 0xC870, 0x4E00, 0xD7A3, 0xFFFF,
    ] {
        let (len, payload) = kp.entry(code);
        assert!(len == 1 || len == 2);
        assert_eq!(payload.len(), len as usize);
    }
    assert_eq!(kp.entry('조' as u16), (2, &[0xbc, 0xbf][..]));
    assert_eq!(kp.entry('건' as u16), (2, &[0xb0, 0xbc][..]));
}

#[test]
fn speech_pkg_word_roundtrip() {
    let kp = keypad();
    assert_eq!(kp.convert_str("조건"), &[0xbc, 0xbf, 0xb0, 0xbc]);

    let kps = Kps9566::builtin();
    assert_eq!(kps.decode(&[0xbc, 0xbf, 0xb0, 0xbc]), "조건");

    let internal = kp.convert_str("조건입니다.");
    let back = kps.decode(&internal);
    assert_eq!(back, "조건입니다.");
}

#[test]
fn korean_sentence_split() {
    let sents = pipeline("안녕하세요. 반갑습니다!");
    let kps = Kps9566::builtin();
    let texts: Vec<String> = sents.iter().map(|s| kps.decode(&s.text)).collect();
    assert_eq!(texts, vec!["안녕하세요.", " 반갑습니다!"]);
    assert_eq!(sents[0].start, 0);
    assert_eq!(sents[1].start, 5 * 2 + 1); // 5 syllables × 2B + '.' = 11 bytes
}

#[test]
fn korean_dot_inside_sentence() {
    let sents = pipeline("가나.다라");
    let kps = Kps9566::builtin();
    let texts: Vec<String> = sents.iter().map(|s| kps.decode(&s.text)).collect();
    assert_eq!(texts, vec!["가나.", "다라"]);
}

#[test]
fn decimal_stays_one_sentence() {
    let sents = pipeline("3.14는 원주율");
    let kps = Kps9566::builtin();
    let texts: Vec<String> = sents.iter().map(|s| kps.decode(&s.text)).collect();
    assert_eq!(texts.len(), 1);
    assert_eq!(texts[0], "3.14는 원주율");
}

#[test]
fn sandhi_pipeline_with_built_records() {
    let mut buf: Vec<ProsodyRecord> = Vec::new();
    let mut s1 = build_sentence(&[0x0100, 0x0101], &[0, 0]);
    apply_sandhi(&mut buf, &mut s1);
    let mut s2 = build_sentence(&[0x0200, 0x0201], &[2, 1]);
    apply_sandhi(&mut buf, &mut s2);

    assert_eq!(buf.len(), 4);
    assert_eq!(buf[0].tone_class, 40);
    assert_eq!(buf[1].tone_class, 1);
    assert_eq!(buf[1].marker, 1);
    assert_eq!(buf[2].tone_class, 13);
    assert_eq!(buf[2].marker, 2);
    assert_eq!(buf[3].tone_class, 31);
    assert_eq!(buf[3].marker, 0);
}

#[test]
fn sandhi_case5_chain() {
    let mut buf: Vec<ProsodyRecord> = Vec::new();
    let mut s1 = build_sentence(&[0x0100, 0x0101], &[6, 6]);
    apply_sandhi(&mut buf, &mut s1);
    assert_eq!(buf[0].tone_class, 45);
    assert_eq!(buf[1].tone_class, 5 + 0x1E);

    let mut s2 = build_sentence(&[0x0200, 0x0201], &[1, 1]);
    apply_sandhi(&mut buf, &mut s2);
    assert_eq!(buf[1].tone_class, 5);
    assert_eq!(buf[2].tone_class, 51);
    assert_eq!(buf[3].tone_class, 11);
}

#[test]
fn record_serialization_12_bytes() {
    let r = ProsodyRecord {
        prev_code: 0x0001,
        code: 0x6D86,
        marker: 2,
        flags: 0,
        tone_class: 0x28,
    };
    let b = r.to_bytes();
    assert_eq!(b.len(), 12);
    let r2 = ProsodyRecord::from_bytes(&b).unwrap();
    assert_eq!(r, r2);
    // bytes: +0 prev, +2 cur, +4 marker, +5 flags, +6 class
    assert_eq!(&b[0..2], &[0x01, 0x00]);
    assert_eq!(&b[2..4], &[0x86, 0x6D]);
    assert_eq!(b[4], 2);
    assert_eq!(b[6], 0x28);
}

