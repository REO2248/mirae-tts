use mirae_tts_engine::g2p::g2p_dict::Reading;
use mirae_tts_engine::g2p::g2p_dict::{apply_phoneme_sandhi, stage1_phoneme_codes, word_record_from_readings};
use mirae_tts_engine::keypad::KeyPad;

fn sandhi_of(text: &str) -> Vec<u16> {
    let kp = KeyPad::load("/home/user/reo_work/mirae2_re/extracted/미래2.0/Data/Dictionary/KeyPad.Ebd")
        .expect("real KeyPad.Ebd must load");
    let bytes = kp.convert_str(text);
    let readings = vec![mirae_tts_engine::g2p::g2p_dict::Reading::fallback(&bytes)];
    let mut rec = word_record_from_readings(&readings);
    stage1_phoneme_codes(&mut rec);
    apply_phoneme_sandhi(&mut rec);
    rec.phoneme_codes
}

#[test]
fn sandhi_hyuksinul() {
    let got = sandhi_of("혁신을");
    println!("혁신을: {:04x?}", got);
    assert_eq!(got, vec![0x006c, 0x6d30, 0x1901]); // 혁 씨 늘
}

#[test]
fn sandhi_munhak() {
    let got = sandhi_of("문학유산의");
    println!("문학유산의: {:04x?}", got);
    assert_eq!(got, vec![0x6cc4, 0x0001, 0x6cf2, 0x6c06, 0x6e01]); // 무 낙 유 사 늬
}

#[test]
fn sandhi_irukago() {
    let got = sandhi_of("이룩하고");
    println!("이룩하고: {:04x?}", got);
    assert_eq!(got, vec![0x6d32, 0x00c3, 0x6c09, 0x6c80]); // 이 룩 카 고
}

#[test]
fn sandhi_swipge() {
    let got = sandhi_of("쉽게");
    println!("쉽게: {:04x?}", got);
    assert_eq!(got, vec![0x3de6, 0x6d8d]); // 쇄 께
}

#[test]
fn sandhi_jejak() {
    let got = sandhi_of("제작되였습니다");
    println!("제작되였습니다: {:04x?}", got);
    assert_eq!(got, vec![0x6d87, 0x0007, 0x6dce, 0x1472, 0x3910, 0x6d21, 0x6c02]); // 제 작 뙤 였 씀 니 다
}

#[test]
fn sandhi_jungga() {
    let got = sandhi_of("증가");
    println!("증가: {:04x?}", got);
    assert_eq!(got, vec![0x4907, 0x6c00]); // 증 가
}

#[test]
fn sandhi_neureo() {
    let got = sandhi_of("늘어나는");
    println!("늘어나는: {:04x?}", got);
    assert_eq!(got, vec![0x6d01, 0x6c43, 0x6c01, 0x0901]); // 느 러 나 는
}

#[test]
fn sandhi_jipil() {
    let got = sandhi_of("집필활동을");
    println!("집필활동을: {:04x?}", got);
    assert_eq!(got, vec![0x3d27, 0x192b, 0x1a2c, 0x488e, 0x1912]); // 집 필 활 똥 을
}

#[test]
fn sandhi_munhwa() {
    let got = sandhi_of("문화적수요를");
    println!("문화적수요를: {:04x?}", got);
    assert_eq!(got, vec![0x6cc4, 0x6e21, 0x0047, 0x6cc6, 0x6cb2, 0x1903]); // 무 놔 적 수 요 를
}

#[test]
fn sandhi_spelling_override() {
    // 辞書読み (지피화동을) と綴り (집필활동을) が異なる場合も綴りで規則適用する。
    let kp = KeyPad::load("/home/user/reo_work/mirae2_re/extracted/미래2.0/Data/Dictionary/KeyPad.Ebd")
        .expect("real KeyPad.Ebd must load");
    let bytes = kp.convert_str("지피화동을");
    let readings = vec![mirae_tts_engine::g2p::g2p_dict::Reading::fallback(&bytes)];
    let mut rec = word_record_from_readings(&readings);
    rec.spelling = kp.convert_str("집필활동을");
    stage1_phoneme_codes(&mut rec);
    apply_phoneme_sandhi(&mut rec);
    println!("지피화동을+spelling=집필활동을: {:04x?}", rec.phoneme_codes);
    assert_eq!(rec.phoneme_codes, vec![0x3d27, 0x192b, 0x1a2c, 0x488e, 0x1912]);
}

#[test]
fn sandhi_noraejip() {
    // 読み = 조서노래지문학연구 (辞書: 받침落ち), 綴り = 조선노래집문학연구
    let kp = KeyPad::load("/home/user/reo_work/mirae2_re/extracted/미래2.0/Data/Dictionary/KeyPad.Ebd").unwrap();
    let kps = mirae_tts_engine::kps9566::Kps9566::builtin();
    let sp = kp.convert_str("조선노래집문학연구");
    let dc: Vec<char> = kps.decode(&sp).chars().collect();
    println!("decoded spelling: {:?}", dc);
    let reading = kp.convert_str("조서노래지문학연구");
    let mut rec = word_record_from_readings(&[Reading::fallback(&reading)]);
    rec.spelling = kp.convert_str("조선노래집문학연구");
    stage1_phoneme_codes(&mut rec);
    apply_phoneme_sandhi(&mut rec);
    let got: Vec<String> = rec.phoneme_codes.iter().map(|c| format!("{:04x}", c)).collect();
    println!("noraejip: {}", got.join(" "));
    assert_eq!(rec.phoneme_codes, vec![0x6c87, 0x0846, 0x6c81, 0x6d43, 0x3d27, 0x6cc4, 0x1, 0x0872, 0x6cc0]);
}

#[test]
fn sandhi_gojeon() {
    let got = sandhi_of("고전적명작문학작품");
    println!("고전적명작문학작품: {:04x?}", got);
    assert_eq!(got, vec![0x6c80, 0x0847, 0x0051, 0x4864, 0x0007, 0x6cc4, 0x0001, 0x0011, 0x38cb]);
}

#[test]
fn sandhi_joksi() {
    let got = sandhi_of("충족시키며");
    println!("충족시키며: {:04x?}", got);
    assert_eq!(got, vec![0x48c8, 0x0087, 0x0126, 0x6d29, 0x6c64]); // 충 족 싯 키 며
}

// --- t12 追加: 語間規則 (쓰기→쓰긷, 数字語+次語の連音・3 音節濃音化) ---

#[test]
fn sandhi_ssugi_gi_gets_d() {
    // 글쓰기 → [글, 쓰, 긷]: 名詞化 -기 は ㄷ 終声クラス (cls5, 0x1520)。
    let got = sandhi_of("글쓰기");
    println!("글쓰기: {:04x?}", got);
    assert_eq!(got, vec![0x1900, 0x6d10, 0x1520]);
}

#[test]
fn sandhi_hagi_gi_keeps_open() {
    // 방조하기 の 기 は開音節のまま (cls27) — 쓰기 限定の規則。
    let got = sandhi_of("방조하기");
    println!("방조하기: {:04x?}", got);
    assert_eq!(got, vec![0x4805, 0x6c87, 0x6c0c, 0x6d20]); // 방 조 하 기
}

#[test]
fn sandhi_number_word_linking() {
    // 1500여권의 → [천,오,배,겨,꿰,늬]:
    // 数字語末 ㄱ (백) + 次語頭 ㅇ開音節 (여) の連音 (백여→배겨) と、
    // 3 音節濃音化 (권→꿰)。数字内部 (천,오) には適用しない。
    let kp = KeyPad::load("/home/user/reo_work/mirae2_re/extracted/미래2.0/Data/Dictionary/KeyPad.Ebd")
        .expect("real KeyPad.Ebd must load");
    let mut rec = word_record_from_readings(&[Reading::fallback(&kp.convert_str("여권의"))]);
    stage1_phoneme_codes(&mut rec);
    let num_codes = mirae_tts_engine::g2p::g2p_dict::sino_integer_codes(&[1, 5, 0, 0]);
    let mut all = num_codes.clone();
    all.extend(rec.phoneme_codes.iter().copied());
    rec.phoneme_codes = all;
    rec.phoneme_markers = vec![1; rec.phoneme_codes.len()];
    // 綴り: 数字読み (천오백) + 次語 (여권의)
    let mut sp = Vec::new();
    for k in mirae_tts_engine::g2p::g2p_dict::sino_integer_kps_syllables(&[1, 5, 0, 0]) {
        sp.push((k >> 8) as u8);
        sp.push((k & 0xff) as u8);
    }
    sp.extend_from_slice(&kp.convert_str("여권의"));
    rec.spelling = sp;
    apply_phoneme_sandhi(&mut rec);
    // start_pair 保護 (数字内部ペア不変) を検証するため範囲指定版で適用し直す。
    let num_codes = mirae_tts_engine::g2p::g2p_dict::sino_integer_codes(&[1, 5, 0, 0]);
    let mut rec2 = word_record_from_readings(&[Reading::fallback(&kp.convert_str("여권의"))]);
    stage1_phoneme_codes(&mut rec2);
    let mut all = num_codes.clone();
    all.extend(rec2.phoneme_codes.iter().copied());
    rec2.phoneme_codes = all;
    rec2.phoneme_markers = vec![1; rec2.phoneme_codes.len()];
    let mut sp = Vec::new();
    for k in mirae_tts_engine::g2p::g2p_dict::sino_integer_kps_syllables(&[1, 5, 0, 0]) {
        sp.push((k >> 8) as u8);
        sp.push((k & 0xff) as u8);
    }
    sp.extend_from_slice(&kp.convert_str("여권의"));
    rec2.spelling = sp;
    mirae_tts_engine::g2p::g2p_dict::apply_phoneme_sandhi_from(&mut rec2, num_codes.len() - 1);
    println!("1500여권의: {:04x?}", rec2.phoneme_codes);
    assert_eq!(rec2.phoneme_codes, vec![0x0848, 0x6c92, 0x6d45, 0x6c60, 0x6e4d, 0x6e01]);
}
