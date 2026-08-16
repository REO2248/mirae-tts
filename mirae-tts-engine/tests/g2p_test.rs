//! g2p.rs の検証テスト — 音素コード変換基盤 + 静的例外語テーブル + 数字/単位読み。
//!
//! 期待値は全て G2P_detail.md / out_g2p_exc*.txt / dump_g2p_consts.txt の
//! オリジナル Future.exe 実データから採取。

use mirae_tts_engine::g2p::*;

// ============================================================================
// §1. 16bit 音素コード
// ============================================================================

#[test]
fn phoneme_code_decomposition() {
    // G2P §11.5 の実データ例 (VoiceInfo.pkg 上位コード)
    assert_eq!(split_phoneme(0x6c0c), (27, 0, 12)); // (27,0,12) = 아 系
    assert_eq!(split_phoneme(0x1903), (6, 8, 3)); // (6,8,3) = 돈 系
    // 0x0924 = (2,9,4): クラス 2 × 下位 4 → 休止コード。
    // ※ G2P §11.5 の表記 (2/18/4) は算術誤り (18<<5|4 = 0xA44 ≠ 0x924)
    assert_eq!(split_phoneme(0x0924), (2, 9, 4));
    assert_eq!(split_phoneme(0x14c7), (5, 6, 7));
    assert_eq!(split_phoneme(0x4848), (18, 2, 8));
}

#[test]
fn phoneme_code_synthesis() {
    // make_phoneme は分解の逆
    assert_eq!(make_phoneme(27, 0, 12), 0x6c0c);
    assert_eq!(make_phoneme(6, 8, 3), 0x1903);
    assert_eq!(make_phoneme(2, 9, 4), 0x0924);
    assert_eq!(make_phoneme(5, 6, 7), 0x14c7);
    assert_eq!(make_phoneme(18, 2, 8), 0x4848);
    // フィールドのマスク (クラス 6bit / 母音 5bit / 初声 5bit)
    assert_eq!(make_phoneme(0x3f, 0x1f, 0x1f), 0xffff);
    assert_eq!(make_phoneme(0x40, 0x20, 0x20), 0x0000);
}

#[test]
fn final_class_table_matches_report() {
    // G2P §11.2 の DAT_00489214 実データ
    let expected = [0, 2, 2, 5, 6, 0, 15, 14, 15, 6, 6, 15, 15, 14, 5, 15, 5, 18, 0, 5, 15, 5, 0, 5, 27, 5, 5, 0];
    assert_eq!(FINAL_TO_CLASS, expected);
    // クラス集合 {0,2,5,6,14,15,18,27} (VoiceInfo の分布と一致)
    let mut classes: Vec<u8> = FINAL_TO_CLASS.to_vec();
    classes.sort_unstable();
    classes.dedup();
    assert_eq!(classes, vec![0, 2, 5, 6, 14, 15, 18, 27]);
}

#[test]
fn apply_final_class_replaces_upper_bits() {
    // FUN_00406c10: (クラス置換[コード>>10] << 10) | (コード & 0x3ff)
    // 終声 index 6 (ㄷ) → クラス 15; 0x1903 & 0x3ff = 0x103
    assert_eq!(apply_final_class(0x1903), 0x3d03); // (15, 8, 3)
    // 終声 index 4 (ㄴ) → クラス 6
    assert_eq!(apply_final_class(0x1123), 0x1923); // (6, 9, 3)
    // 終声 index 2 (ㄲ) → クラス 2; 0x080c & 0x3ff = 0x00c
    assert_eq!(apply_final_class(0x080c), 0x080c); // (2, 0, 12)
    // 終声なし (index 0) → クラス 0 (中位/下位は保持)
    assert_eq!(apply_final_class(0x0005), 0x0005);
    // 中位 5bit (母音)・下位 5bit (初声) は不変
    assert_eq!(apply_final_class(0x0fff) & 0x3ff, 0x3ff);
}

#[test]
fn synthesize_uses_class_27_base() {
    // FUN_00428620: 0x6c00 | (母音&0x1f)<<5 | (初声&0x1f)
    assert_eq!(synthesize(0, 0), 0x6c00);
    assert_eq!(synthesize(0, 12), 0x6c0c);
    assert_eq!(synthesize(9, 18), 0x6d32);
    assert_eq!(synthesize(21, 19), 0x6eb3);
    assert_eq!(synthesize(12, 0), 0x6d80);
}

#[test]
fn syllable_phoneme_conversion() {
    // 音節コード (KS X 1001: 初声<<10 | 中声<<5 | 終声) → 中間コード [終声][中声][初声]
    // 돈 = 初声ㄷ(3) 中声ㅗ(9) 終声ㄴ(4) → 0x0d24
    assert_eq!(syllable_to_intermediate(0x0d24), 0x1123);
    // 音素コード: クラス = FINAL_TO_CLASS[終声4=ㄴ] = 6 → (6, 9, 3)
    assert_eq!(syllable_to_phoneme(0x0d24), 0x1923);
    assert_eq!(split_phoneme(syllable_to_phoneme(0x0d24)), (6, 9, 3));
    // 終声なし音節: 가 = 初声ㄱ(1) 中声ㅏ(1) 終声0 → 中間 0x21 → クラス 0 → (0, 1, 1)
    assert_eq!(syllable_to_phoneme(0x0420), 0x0021);
    // 逆変換: クラス 27 (母音終わり基底) のみ完全に戻せる
    assert_eq!(phoneme_to_syllable(0x6c0c), Some(0x3000)); // (27,0,12) → 初声12 中声0 終声0
    assert_eq!(phoneme_to_syllable(0x6d32), Some(0x4920)); // (27,9,18) → 初声18 中声9
    // 他のクラスは終声が一意でないため逆変換不可
    assert_eq!(phoneme_to_syllable(0x1903), None);
    assert_eq!(phoneme_to_syllable(0x0924), None);
}

#[test]
fn pause_and_real_phoneme_predicates() {
    // FUN_0042a3c0 (G2P §11.4): クラス∈{2,0xe,0x12,0x1b} × 下位∈{1,4,0x12}、
    // または クラス==6 × 下位∈{3,4,0x12}
    assert!(is_pause(2, 4));
    assert!(is_pause(2, 1));
    assert!(is_pause(2, 0x12));
    assert!(is_pause(0x0e, 4));
    assert!(is_pause(0x12, 0x12));
    assert!(is_pause(0x1b, 1));
    assert!(is_pause(6, 3));
    assert!(is_pause(6, 4));
    assert!(!is_pause(27, 12));
    assert!(!is_pause(2, 3));
    assert!(!is_pause(6, 5));
    // 0x0924 = (2,9,4) → 休止コード (T4 の pause 検出と一致)
    assert!(is_pause_code(0x0924));
    // 0x1903 = (6,8,3): クラス 6 × 下位 3 は FUN_0042a3c0 の休止条件に一致
    assert!(is_pause_code(0x1903));
    assert!(!is_pause_code(0x6c0c));

    // FUN_0044b350 (G2P §11.4): 下位 ∉ {1,4,6,8..14,16,17,18} かつ
    // (下位 != 3 または クラス != 6)
    assert!(!is_real_phoneme(27, 12)); // 下位 12 は除外集合
    assert!(!is_real_phoneme(2, 18)); // 下位 18 は除外集合
    assert!(!is_real_phoneme(6, 8));
    assert!(!is_real_phoneme(6, 3)); // クラス 6 × 下位 3 は除外
    assert!(!is_real_phoneme(2, 1)); // 休止マーカ
    assert!(is_real_phoneme(2, 3)); // 下位 3 はクラス 6 以外なら実音素
    assert!(is_real_phoneme(6, 5));
    assert!(is_real_phoneme(27, 19));
    assert!(is_real_phoneme(6, 2));
    // コード版
    assert!(!is_real_phoneme_code(0x6c0c));
    assert!(!is_real_phoneme_code(0x0924));
    // 0x6d33 = (27,9,19): 下位 19 は除外集合外 → 実音素
    assert!(is_real_phoneme_code(0x6d33));
}

// ============================================================================
// §2. 静的例外語テーブル (FUN_0043b010)
// ============================================================================

/// KeyPad 内部バイト列のデコード用 (KPS9566)。
fn dec(bytes: &[u8]) -> String {
    let kps = mirae_tts_engine::kps9566::Kps9566::builtin();
    kps.decode(bytes)
}

#[test]
fn exception_table_size() {
    // FUN_0043b010 の比較チェーンは 61 検査 (うち 해도 が重複検査のため
    // 実質 60 エントリ) — 本テーブルは重複を除いた 60 エントリ。
    assert_eq!(EXCEPTION_TABLE.len(), 60);
}

#[test]
fn exception_lookup_words() {
    // 辞書引き型 (Lookup): 正規形を FUN_00444fb0 に渡す
    let cases: &[(&[u8], &str, &str)] = &[
        (&[0xb1, 0xfd, 0xb0, 0xa1], "나가", "나가아"),
        (&[0xb4, 0xdd, 0xb0, 0xd6], "대고", "대이고"),
        (&[0xc3, 0xcd, 0xba, 0xb7], "해서", "하여서"),
        (&[0xcb, 0xce, 0xb8, 0xc9, 0xc3, 0xf9, 0xc3, 0xcd, 0xba, 0xb7], "일반화해서", "일반화하여서"),
        (&[0xbd, 0xdb, 0xbc, 0xbf, 0xc3, 0xcd], "창조해", "창조하여"),
        (&[0xbc, 0xad, 0xc3, 0xcd, 0xbc, 0xec], "전해질", "전하여질"),
        (&[0xb6, 0xed, 0xb1, 0xfd], "만나", "만나아"),
        (&[0xb9, 0xbe, 0xc3, 0xcd], "비해", "비하여"),
        (&[0xb4, 0xdd, 0xc3, 0xcd, 0xba, 0xb7, 0xb4, 0xc7, 0xbc, 0xe8], "대해서든지", "대하여서든지"),
        (&[0xb8, 0xf5, 0xbc, 0xac, 0xcb, 0xcb], "본적이", "보는적이"),
        (&[0xb1, 0xfd, 0xcc, 0xae], "나와", "나오아"),
        (&[0xb4, 0xae, 0xb5, 0xd8, 0xca, 0xbf], "돌려야", "돌리여야"),
        (&[0xc0, 0xb2], "탄", "타는"),
        (&[0xca, 0xf1], "온", "오는"),
        (&[0xc3, 0xcd, 0xca, 0xbf], "해야", "하여야"),
        (&[0xc3, 0xcd, 0xb4, 0xaa], "해도", "하여도"),
        (&[0xb4, 0xdd, 0xc3, 0xcd, 0xca, 0xbf], "대해야", "대하여야"),
        (&[0xb4, 0xdd, 0xc3, 0xcd, 0xba, 0xb7], "대해서", "대하여서"),
        (&[0xcc, 0xae, 0xba, 0xb7], "와서", "오아서"),
        (&[0xcc, 0xae, 0xb4, 0xaa], "와도", "오아도"),
    ];
    for (input, word, expected) in cases {
        let rule = lookup_exception(input).unwrap_or_else(|| panic!("no rule for {word}"));
        match rule.out {
            ExceptionOutcome::Lookup(form) => {
                assert_eq!(dec(form), *expected, "word {word}");
            }
            _ => panic!("{word}: expected Lookup"),
        }
    }
}

#[test]
fn exception_hard_words() {
    // ハードコード型 (Hard): main + sub 断片
    let cases: &[(&[u8], &str, &str, &str, u8)] = &[
        (&[0xbb, 0xf4, 0xb4, 0xaa], "자도", "자", "도", 0x04),
        (&[0xb0, 0xa1, 0xbc, 0xea], "가진", "가지", "는", 0x04),
        (&[0xb0, 0xa1, 0xbc, 0xe8, 0xb2, 0xf7], "가지는", "가지", "는", 0x04),
        (&[0xb3, 0xad, 0xb6, 0xb0], "내린", "내리", "ㄴ", 0x04),
        (&[0xb0, 0xa1, 0xb7, 0xb2], "가면", "가", "면", 0x04),
        (&[0xbc, 0xc2, 0xb5, 0xb9], "졸라", "졸", "라", 0x04),
        (&[0xb0, 0xa1, 0xb7, 0xb2, 0xba, 0xb7], "가면서", "가", "면서", 0x04),
        (&[0xb0, 0xa1, 0xb4, 0xaa], "가도", "가", "도", 0x04),
        (&[0xcb, 0xcb, 0xb5, 0xcf], "이런", "이렇", "ㄴ", 0x05),
        (&[0xc2, 0xd9, 0xb4, 0xe7], "한데", "하", "ㄴ데", 0x04),
        (&[0xc2, 0xd7, 0xca, 0xde], "하여", "하", "여", 0x04),
        (&[0xb8, 0xf6], "볼", "보", "ㄹ", 0x04),
        (&[0xb0, 0xa5], "갈", "가", "ㄹ", 0x04),
        (&[0xb2, 0xa4], "날", "나", "ㄹ", 0x04),
        (&[0xbd, 0xd5], "찬", "차", "ㄴ", 0x04),
        (&[0xb8, 0xf3, 0xbb, 0xa6], "보신", "보시", "ㄴ", 0x04),
        (&[0xb0, 0xa1, 0xbc, 0xe8], "가지", "가", "지", 0x04),
        (&[0xb3, 0xad, 0xb0, 0xa1], "내가", "내", "가", 0x02),
        (&[0xb4, 0xdd, 0xbc, 0xe8, 0xb6, 0xa6], "대지를", "대지", "를", 0x01),
        (&[0xba, 0xa8, 0xb1, 0xe1], "삶과", "삶", "과", 0x01),
        (&[0xb0, 0xfb, 0xb6, 0xa6], "그를", "그", "를", 0x02),
    ];
    for (input, word, main, sub, marker) in cases {
        let rule = lookup_exception(input).unwrap_or_else(|| panic!("no rule for {word}"));
        match rule.out {
            ExceptionOutcome::Hard(h) => {
                assert_eq!(dec(h.main), *main, "word {word} main");
                assert_eq!(dec(h.sub), *sub, "word {word} sub");
                assert_eq!(h.marker, *marker, "word {word} marker");
                assert_eq!(h.morphemes, 2, "word {word} morphemes");
            }
            _ => panic!("{word}: expected Hard"),
        }
    }
}

#[test]
fn exception_daethaeneun_three_morphemes() {
    // 대해서는 → 대하 + 여서 + 는 (3 形態素, sub2 あり)
    let rule = lookup_exception(&[0xb4, 0xdd, 0xc3, 0xcd, 0xba, 0xb7, 0xb2, 0xf7]).unwrap();
    match rule.out {
        ExceptionOutcome::Hard(h) => {
            assert_eq!(dec(h.main), "대하");
            assert_eq!(dec(h.sub), "여서");
            assert_eq!(h.sub2.map(dec).as_deref(), Some("는"));
            assert_eq!(h.morphemes, 3);
            assert_eq!(h.f1389, 0x15);
            assert_eq!(h.f1400, 0x91);
        }
        _ => panic!("expected Hard"),
    }
}

#[test]
fn exception_no_match_returns_none() {
    // テーブル外の単語は None (→ 既定経路 FUN_00444fb0 相当へ)
    assert!(lookup_exception(b"abc").is_none());
    assert!(lookup_exception(&[0xb0, 0xa1]).is_none()); // 가 単独
    assert!(lookup_exception(&[0xc2, 0xd7]).is_none()); // 하 単独
    assert!(lookup_exception(&[]).is_none());
}

#[test]
fn exception_first_match_wins() {
    // 先頭一致 (FUN_0043b010 の if/else 順序)
    // 전해 (0x47e224) は 전해질 (0x47e238) より先に検査される
    let rule = lookup_exception(&[0xbc, 0xad, 0xc3, 0xcd]).unwrap();
    assert!(matches!(rule.out, ExceptionOutcome::Lookup(f) if dec(f) == "전하여"));
}

// ============================================================================
// §3. 数字/単位読み
// ============================================================================

#[test]
fn unit_table_size_and_content() {
    assert_eq!(UNIT_TABLE.len(), 24);
    // 代表 5 例 + 先頭/末尾
    let cases: &[(&[u8], &str)] = &[
        (b"m", "메터"),
        (b"cm", "센치메터"),
        (b"kg", "키로그람"),
        (b"V", "볼트"),
        (b"A", "암페아"),
        (b"pW", "피코와트"),
        (b"MV", "메가볼트"),
        (b"W", "와트"),
    ];
    for (unit, expected) in cases {
        let r = unit_reading(unit).unwrap_or_else(|| panic!("no unit {:?}", unit));
        assert_eq!(dec(r), *expected, "unit {:?}", unit);
    }
    assert!(unit_reading(b"X").is_none());
    assert!(unit_reading(b"kilogram").is_none());
}

#[test]
fn unit_match_words() {
    // PTR_DAT_0048a478 (FUN_0040aef0 用)
    assert!(unit_match(b"m"));
    assert!(unit_match(b"cm"));
    assert!(unit_match(b">"));
    assert!(unit_match(b"g"));
    assert!(!unit_match(b"mmx"));
    assert!(!unit_match(b"kg")); // kg は 0x48a478 に無い (0x48a490 側)
    assert!(!unit_match(b""));
}

#[test]
fn digit_words_table() {
    // PTR_DAT_0048a6b0: 40 エントリ (sentinel/NULL は空)
    assert_eq!(DIGIT_WORDS.len(), 40);
    assert_eq!(dec(DIGIT_WORDS[0]), "한");
    assert_eq!(dec(DIGIT_WORDS[6]), "조");
    assert_eq!(dec(DIGIT_WORDS[7]), "여러");
    assert_eq!(dec(DIGIT_WORDS[12]), "ㄴ");
    assert!(DIGIT_WORDS[11].is_empty()); // sentinel
    assert!(DIGIT_WORDS[17].is_empty()); // NULL
    assert_eq!(dec(DIGIT_WORDS[18]), "개");
    assert_eq!(dec(DIGIT_WORDS[29]), "자루");
    assert_eq!(dec(DIGIT_WORDS[35]), "꼴");
    assert_eq!(dec(DIGIT_WORDS[39]), "가지");

    // PTR_DAT_0048a6e0: 40 エントリ
    assert_eq!(DIGIT_PREFIXES.len(), 40);
    assert_eq!(dec(DIGIT_PREFIXES[0]), "ㄴ");
    assert_eq!(dec(DIGIT_PREFIXES[31]), "번째");
    assert_eq!(dec(DIGIT_PREFIXES[39]), "매");
}

#[test]
fn digit_word_hit_and_prefix_len() {
    // strstr 相当: 数字語が部分文字列として現れるか
    assert_eq!(digit_word_hit(&[0xc2, 0xd9]), Some(0)); // 한
    // "수백" = 수(bae3) + 백(b9ca): テーブル順で 백(2) が 수(8) より先にヒット
    let soobaek = [0xba, 0xe3, 0xb9, 0xca];
    assert_eq!(digit_word_hit(&soobaek), Some(2)); // 백
    assert_eq!(digit_prefix_len(&soobaek), 0); // 백 はプレフィクス表に無い
    // "몇개" = 몇(b7b8) + 개(b1b6)
    let myeotgae = [0xb7, 0xb8, 0xb1, 0xb6];
    assert_eq!(digit_word_hit(&myeotgae), Some(9)); // 몇
    assert_eq!(digit_prefix_len(&myeotgae), 3); // 개 が位置 2 でヒット → 2+1
    // ヒットなし
    assert_eq!(digit_word_hit(b"abc"), None);
    assert_eq!(digit_prefix_len(b"abc"), 0);
}

#[test]
fn special_to_key_char_conversion() {
    // 数字 0x30..0x39 → 0x46..0x4F ('F'..'O'), '-' → 'E', '.' → 'D'
    assert_eq!(special_to_key_char(0x30), Some(0x46));
    assert_eq!(special_to_key_char(0x39), Some(0x4f));
    assert_eq!(special_to_key_char(0x35), Some(0x4b));
    assert_eq!(special_to_key_char(0x2d), Some(0x45));
    assert_eq!(special_to_key_char(0x2e), Some(0x44));
    // bit15 付き (0x8000|値) でも同じ
    assert_eq!(special_to_key_char(0x8030), Some(0x46));
    assert_eq!(special_to_key_char(0x8039), Some(0x4f));
    assert_eq!(special_to_key_char(0x802d), Some(0x45));
    assert_eq!(special_to_key_char(0x802e), Some(0x44));
    // 非対応
    assert_eq!(special_to_key_char(0x41), None);
    assert_eq!(special_to_key_char(0x00), None);
    assert_eq!(special_to_key_char(0x3a), None);
}

// ============================================================================
// §4. アルファベット/字母読み
// ============================================================================

#[test]
fn digraph_tables() {
    // 28 種のダイグラフ規則 (0x4768c8..0x4769a0)
    assert_eq!(DIGRAPHS.len(), 28);
    assert_eq!(DIGRAPHS[0], b"es");
    assert_eq!(DIGRAPHS[5], b"oo");
    assert_eq!(DIGRAPHS[27], b"aff");
    // 22 種の読み断片 (0x4767a0..0x476898)
    assert_eq!(DIGRAPH_READINGS.len(), 22);
    assert_eq!(dec(DIGRAPH_READINGS[0]), "이");
    assert_eq!(dec(DIGRAPH_READINGS[1]), "오이");
    assert_eq!(dec(DIGRAPH_READINGS[2]), "에이");
    assert_eq!(dec(DIGRAPH_READINGS[5]), "쉬오우");
    assert_eq!(dec(DIGRAPH_READINGS[7]), "오울ㄷ");
    assert_eq!(dec(DIGRAPH_READINGS[8]), "오우스ㅌ");
    assert_eq!(dec(DIGRAPH_READINGS[12]), "유어");
    assert_eq!(dec(DIGRAPH_READINGS[19]), "아ㅎ");
    assert_eq!(dec(DIGRAPH_READINGS[21]), "전");
}

#[test]
fn jamo_readings() {
    // 単独字母の静的読み
    for (jamo, reading) in JAMO_READINGS {
        assert_eq!(dec(jamo), dec(reading), "jamo {:?}", jamo);
    }
    assert_eq!(dec(JAMO_READINGS[0].0), "ㅍ");
    assert_eq!(dec(JAMO_READINGS[1].0), "ㄴ");
    assert_eq!(dec(JAMO_READINGS[5].0), "ㅂ");
}
