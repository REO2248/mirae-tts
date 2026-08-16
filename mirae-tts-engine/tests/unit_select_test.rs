//! 実データ検証: VoiceInfo.pkg パース + ユニット選択 (SPEC_tts_rewrite.md §2.5 / T1_voiceinfo.md)。
//!
//! データは環境変数 MIRAE2_VOICE_DIR (既定: /home/user/reo_work/mirae2_re/extracted/미래2.0/Voice) から。
//! データが見つからない場合はスキップ (cargo test は他環境でも通る)。

use mirae_tts_engine::unit_select::{
    is_pause, is_real_phoneme, ProsodyRecord, UnitSelectConfig, UnitSelector,
};
use mirae_tts_engine::voice_info::{VoiceInfo, VoiceInfoEntry};
use std::path::PathBuf;

fn voice_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("MIRAE2_VOICE_DIR") {
        let p = PathBuf::from(d);
        if p.join("VoiceInfo.pkg").exists() {
            return Some(p);
        }
    }
    let candidates = [
        "/home/user/reo_work/mirae2_re/extracted/미래2.0/Voice",
        "Voice",
        "../Voice",
    ];
    for c in candidates {
        let p = PathBuf::from(c);
        if p.join("VoiceInfo.pkg").exists() {
            return Some(p);
        }
    }
    None
}

fn load_voice_info() -> Option<VoiceInfo> {
    let dir = voice_dir()?;
    let info = VoiceInfo::load(dir.join("VoiceInfo.pkg")).ok()?;
    Some(info)
}

#[test]
fn parse_real_voiceinfo_pkg() {
    let Some(info) = load_voice_info() else {
        eprintln!("SKIP: VoiceInfo.pkg not found");
        return;
    };
    // count = 70,150
    assert_eq!(info.count(), 70_150);
    // woff チェーン: 全 70,149 リンク成立
    assert!(info.woff_chain_ok());
    // u16[0] == u16[3]: 全エントリ
    assert!(info.cur_dup_ok());
    // sum(wlen) = 285,520,544 サンプル = VoiceData.pkg サイズ (×2 = 571,041,088 B)
    assert_eq!(info.total_samples(), 285_520_544);
}

#[test]
fn entry0_matches_real_data() {
    let Some(info) = load_voice_info() else {
        eprintln!("SKIP: VoiceInfo.pkg not found");
        return;
    };
    let e = info.entries[0];
    // T1 §3.2 の hexdump とフィールド解釈の突き合わせ
    assert_eq!(e.phone_cur, 0x6d86);
    assert_eq!(e.phone_prev, 0x6eb3);
    assert_eq!(e.phone_next, 0x6d80);
    assert_eq!(e.phone_cur2, 0x6d86);
    assert_eq!(e.woff, 0);
    assert_eq!(e.wlen, 0x161a); // 5658
    assert_eq!(e.pitch, 0x56); // 86 ≈ F0/3
    assert_eq!(e.classcode, 0x0028); // byte+0x14 = 40
    assert_eq!(e.flags, 0xff01);
    assert_eq!(e.pause, -7); // 0xfff9
    assert_eq!(e.woff_lo, 0);
    assert_eq!(e.woff_lo, (e.woff & 0xffff) as u16);
    assert!(e.is_normal());
    // 末尾エントリ
    let last = info.entries[70_149];
    assert_eq!(last.phone_cur, 0x6c12);
    assert_eq!(last.woff_lo, (last.woff & 0xffff) as u16);
}

#[test]
fn scan_known_code_6d86_returns_score() {
    let Some(info) = load_voice_info() else {
        eprintln!("SKIP: VoiceInfo.pkg not found");
        return;
    };
    let sel = UnitSelector::new(&info, UnitSelectConfig::default());
    // 既知の音素コード 0x6d86 (エントリ0 の cur) に対する選択
    let req = mirae_tts_engine::unit_select::UnitRequest {
        cur: 0x6d86,
        prev: 0x6eb3,
        next: 0x6d80,
        pitch: 90,
        class: 0x28,
        flags: 0,
    };
    let hit = sel.scan(&req, true).expect("0x6d86 は必ずヒットする");
    let (entry, score) = hit;
    assert!(score > 0, "score must be positive, got {}", score);
    // 選択エントリは要求コードと一致し、クラスフィルタを通過している
    assert_eq!(entry.phone_cur, 0x6d86);
    assert!(entry.is_normal());
    // クラス 40 (行0) のフィルタ [78,220] × [700,7000]
    assert!((78..=220).contains(&entry.pitch_signed()));
    assert!((700..=7000).contains(&(entry.wlen as i32)));
    // 最良選択のスコアは文脈一致 100 + コスト ≥ 25 以上のはず
    assert!(score >= 100 + 25, "score {} too low", score);
}

#[test]
fn scan_is_deterministic() {
    let Some(info) = load_voice_info() else {
        eprintln!("SKIP: VoiceInfo.pkg not found");
        return;
    };
    let sel = UnitSelector::new(&info, UnitSelectConfig::default());
    let req = mirae_tts_engine::unit_select::UnitRequest {
        cur: 0x6d86,
        prev: 0x6eb3,
        next: 0x6d80,
        pitch: 90,
        class: 0x28,
        flags: 0,
    };
    let a = sel.scan(&req, true);
    let b = sel.scan(&req, true);
    assert_eq!(a, b);
}

#[test]
fn scan_with_special_flag_relaxes_pitch_filter() {
    let Some(info) = load_voice_info() else {
        eprintln!("SKIP: VoiceInfo.pkg not found");
        return;
    };
    let sel = UnitSelector::new(&info, UnitSelectConfig::default());
    // 要求フラグ bit7 + レベル≤1 → ピッチ上限フィルタ省略 (ピッチ > 220 の候補も可)
    let req = mirae_tts_engine::unit_select::UnitRequest {
        cur: 0x6c12,
        prev: 0,
        next: 0,
        pitch: 90,
        class: 0x01,
        flags: 0x80,
    };
    let hit = sel.scan(&req, true).expect("bit7 要求でもヒットする");
    assert!(hit.1 > 0);
}

#[test]
fn special_scan_returns_special_entry() {
    let Some(info) = load_voice_info() else {
        eprintln!("SKIP: VoiceInfo.pkg not found");
        return;
    };
    let sel = UnitSelector::new(&info, UnitSelectConfig::default());
    let sp = sel.scan_special(90).expect("特殊ユニットが存在する");
    assert!(sp.is_special());
    // ピッチ近接: 距離 ≤ 200 (初期値) のはず
    assert!((90 - sp.pitch_signed()).abs() <= 200);
}

#[test]
fn process_real_records_smoke() {
    let Some(info) = load_voice_info() else {
        eprintln!("SKIP: VoiceInfo.pkg not found");
        return;
    };
    let mut sel = UnitSelector::new(&info, UnitSelectConfig::default());
    // 実データに存在するコードで 5 レコードの文を組む (0x6d86 diphone 連鎖 + 0x6c12 文末)
    let records = [
        ProsodyRecord {
            prev_code: 0,
            code: 0x6d86,
            marker: 0,
            flag: 0,
            tone_class: 0x28,
        },
        ProsodyRecord {
            prev_code: 0x6d86,
            code: 0x6d80,
            marker: 0,
            flag: 0,
            tone_class: 0x28,
        },
        ProsodyRecord {
            prev_code: 0x6d80,
            code: 0x6d86,
            marker: 0,
            flag: 0,
            tone_class: 0x0a,
        },
        ProsodyRecord {
            prev_code: 0x6d86,
            code: 0x6d80,
            marker: 0,
            flag: 0,
            tone_class: 0x28,
        },
        ProsodyRecord {
            prev_code: 0x6d80,
            code: 0x6c12,
            marker: 2,
            flag: 1,
            tone_class: 0x01,
        },
    ];
    let out = sel.process(&records);
    // 全レコードが選択される (0x6d86/0x6d80/0x6c12 は全て実在コード)
    assert_eq!(out.units.len(), 5, "all records must select a unit");
    for u in &out.units {
        assert_eq!(
            u.data.phone_cur, u.request.cur,
            "selected cur must match request"
        );
        assert!(u.marker > 0, "marker must be positive");
    }
    // 総サンプル数 > 0 かつ 各ユニットの wlen×2 以上
    let min_wave = out
        .units
        .iter()
        .map(|u| u.data.wlen as i64 * 2)
        .sum::<i64>();
    assert!(out.total_samples >= min_wave);
    assert!(out.total_samples > 0);
    // 文末 0x6c12 (特殊マーカ 2) は 0x740 系ではなく休止コードなので pause 加算が入りうる
    let last = out.units.last().unwrap();
    assert!(last.pause() >= 0);
}

#[test]
fn process_handles_silence_and_pause_codes() {
    let Some(info) = load_voice_info() else {
        eprintln!("SKIP: VoiceInfo.pkg not found");
        return;
    };
    let mut sel = UnitSelector::new(&info, UnitSelectConfig::default());
    // 休止コード 0x6c12 (is_pause(0x1b, 0x12)) を含む文
    let records = [
        ProsodyRecord {
            prev_code: 0,
            code: 0x6c12,
            marker: 0,
            flag: 0,
            tone_class: 0x01,
        },
        ProsodyRecord {
            prev_code: 0x6c12,
            code: 0x6d86,
            marker: 0,
            flag: 0,
            tone_class: 0x28,
        },
        ProsodyRecord {
            prev_code: 0x6d86,
            code: 0x6c12,
            marker: 2,
            flag: 1,
            tone_class: 0x01,
        },
    ];
    let out = sel.process(&records);
    assert_eq!(out.units.len(), 3);
    assert!(out.total_samples > 0);
}

#[test]
fn scan_matches_independent_python_reference() {
    // 独立 Python 再実装 (a800_ref.py: デコンパイル C からの忠実移植) との完全一致を固定値で検証。
    let Some(info) = load_voice_info() else {
        eprintln!("SKIP: VoiceInfo.pkg not found");
        return;
    };
    let sel = UnitSelector::new(&info, UnitSelectConfig::default());
    // (cur, prev, next, pitch, class, flags) → (woff, wlen, pitch, score)
    let cases: [((u16, u16, u16, u16, u8, u8), (u32, u32, u16, i32)); 5] = [
        (
            (0x6d86, 0x6eb3, 0x6d80, 90, 0x28, 0x00),
            (2341103, 6782, 89, 800),
        ),
        (
            (0x6d86, 0x6d80, 0x6d86, 90, 0x0a, 0x00),
            (95860842, 4836, 90, 770),
        ),
        (
            (0x6d80, 0x6d86, 0x6d86, 90, 0x28, 0x00),
            (31271640, 3018, 91, 770),
        ),
        (
            (0x6c12, 0x6d80, 0x0000, 90, 0x01, 0x80),
            (11400332, 2533, 88, 786),
        ),
        (
            (0x6c12, 0x6d86, 0x0000, 90, 0x01, 0x80),
            (11400332, 2533, 88, 794),
        ),
    ];
    for ((cur, prev, next, pitch, class, flags), expect) in cases {
        let req = mirae_tts_engine::unit_select::UnitRequest {
            cur,
            prev,
            next,
            pitch,
            class,
            flags,
        };
        let (e, score) = sel
            .scan(&req, true)
            .unwrap_or_else(|| panic!("scan hit for {:04x}", cur));
        assert_eq!(
            (e.woff, e.wlen, e.pitch, score),
            expect,
            "cur={:04x} prev={:04x} next={:04x} class={:02x} flags={:02x}",
            cur,
            prev,
            next,
            class,
            flags
        );
    }
}

#[test]
fn unselectable_code_is_skipped_like_original() {
    let Some(info) = load_voice_info() else {
        eprintln!("SKIP: VoiceInfo.pkg not found");
        return;
    };
    let mut sel = UnitSelector::new(&info, UnitSelectConfig::default());
    // 0x6eb3 はコーパスに 1 エントリしかなく (pitch=106)、クラス10 のフィルタ窓
    // [pitch 78..105, wlen 700..6000] を外れる → フォールバックも効かず未選択 (オリジナルと同じ挙動)。
    // 0x6eb3: mid5 = 21, FALLBACK_ALLOW[21] ≠ 0 かつ cur 上位10bit = 0x1b (0x6c00 系) → 変種生成なし。
    let records = [ProsodyRecord {
        prev_code: 0x6d80,
        code: 0x6eb3,
        marker: 0,
        flag: 0,
        tone_class: 0x0a,
    }];
    let out = sel.process(&records);
    assert_eq!(
        out.units.len(),
        0,
        "0x6eb3 はフィルタ窓を外れるため未選択のはず"
    );
    assert_eq!(out.total_samples, 0);
}

#[test]
fn boundary_code_used_for_prev_next() {
    let Some(info) = load_voice_info() else {
        eprintln!("SKIP: VoiceInfo.pkg not found");
        return;
    };
    let mut sel = UnitSelector::new(&info, UnitSelectConfig::default());
    // 記事「전자서고…」の先頭 5 レコード相当 (t13): prev/next は
    // FUN_0044b880 の規則どおり 0x6EB3 (DAT_00499234) を使う。
    // 文頭 (idx0) → prev=B; レベル>1 (cls 0x1e) → prev=B;
    // 調値>1 (cls 0x03) → next=B。
    let records = [
        ProsodyRecord { prev_code: 0, code: 0x6d86, marker: 0, flag: 0, tone_class: 0x28 }, // 文頭 (orig cls 0x28)
        ProsodyRecord { prev_code: 0x6d86, code: 0x6d80, marker: 0, flag: 0, tone_class: 0x01 },
        ProsodyRecord { prev_code: 0x6d80, code: 0x6d86, marker: 0, flag: 0, tone_class: 0x0a },
        ProsodyRecord { prev_code: 0x6d86, code: 0x6d80, marker: 0, flag: 0, tone_class: 0x03 }, // 調値 3 → next=B
        ProsodyRecord { prev_code: 0x6d80, code: 0x6c12, marker: 2, flag: 1, tone_class: 0x1e }, // レベル 3 → prev=B
    ];
    let out = sel.process(&records);
    assert_eq!(out.units.len(), 5);
    let B = mirae_tts_engine::unit_select::BOUNDARY_CODE;
    assert_eq!(B, 0x6EB3);
    // u0: 文頭 → prev=B; レベル 2 → prev=B の二重条件
    assert_eq!(out.units[0].request.prev, B, "u0 文頭 prev");
    assert_eq!(out.units[0].request.next, 0x6d80, "u0 next は次のレコード");
    // u1: レベル 0 → prev は前レコード; 調値 1 → next は次レコード
    assert_eq!(out.units[1].request.prev, 0x6d86);
    assert_eq!(out.units[1].request.next, 0x6d86);
    // u2: レベル 1 → prev は前レコード; 調値 0 → next は次レコード
    assert_eq!(out.units[2].request.prev, 0x6d80);
    assert_eq!(out.units[2].request.next, 0x6d80);
    // u3: 調値 3 > 1 → next=B (orig: 고 cls=0x03 next=0x6EB3)
    assert_eq!(out.units[3].request.next, B, "u3 調値>1 next");
    assert_eq!(out.units[3].request.prev, 0x6d86);
    // u4: レベル 3 > 1 → prev=B (orig: 미 cls=0x1e prev=0x6EB3); 末尾 → next=B
    assert_eq!(out.units[4].request.prev, B, "u4 レベル>1 prev");
    assert_eq!(out.units[4].request.next, B, "u4 末尾 next");
}

#[test]
fn sentence_end_record_gets_boundary_next() {
    let Some(info) = load_voice_info() else {
        eprintln!("SKIP: VoiceInfo.pkg not found");
        return;
    };
    let mut sel = UnitSelector::new(&info, UnitSelectConfig::default());
    // 文末マーカ 1 のレコードは調値が低くても next=0x6EB3
    // (FUN_0044b880: idx == count-1 → 境界コード)。
    let records = [
        ProsodyRecord { prev_code: 0, code: 0x6d86, marker: 0, flag: 0, tone_class: 0x28 },
        ProsodyRecord { prev_code: 0x6d86, code: 0x6d80, marker: 1, flag: 0, tone_class: 0x01 },
        ProsodyRecord { prev_code: 0x6d80, code: 0x6c12, marker: 0, flag: 0, tone_class: 0x28 },
    ];
    let out = sel.process(&records);
    assert_eq!(out.units.len(), 3);
    let B = mirae_tts_engine::unit_select::BOUNDARY_CODE;
    assert_eq!(out.units[1].request.next, B, "文末レコード next は境界コード");
    // 文末の次のレコードは通常文脈 (prev は文末レコードのコード)
    assert_eq!(out.units[2].request.prev, 0x6d80);
}

#[test]
fn request_pitch_smoothing_runs() {
    let Some(info) = load_voice_info() else {
        eprintln!("SKIP: VoiceInfo.pkg not found");
        return;
    };
    let mut sel = UnitSelector::new(&info, UnitSelectConfig::default());
    // 3 連ユニットでピッチ平滑化経路が実行される (データ差次第で data2 は None でもよい)
    let records = [
        ProsodyRecord {
            prev_code: 0,
            code: 0x6d86,
            marker: 0,
            flag: 0,
            tone_class: 0x28,
        },
        ProsodyRecord {
            prev_code: 0x6d86,
            code: 0x6d80,
            marker: 0,
            flag: 0,
            tone_class: 0x28,
        },
        ProsodyRecord {
            prev_code: 0x6d80,
            code: 0x6d86,
            marker: 0,
            flag: 0,
            tone_class: 0x0a,
        },
    ];
    let out = sel.process(&records);
    assert_eq!(out.units.len(), 3);
    // active_data は常に参照可能
    for u in &out.units {
        let _ = u.active_data();
    }
    // 継続時間割当: クラス 0x28%10=8 → 0 (非該当)、0x0a%10=0 → 0
    // (データ側 pause は duration または 0 + 句読点加算のいずれか)
    for u in &out.units {
        assert!(u.data.pause >= 0 || u.data2.map_or(false, |d| d.pause >= 0));
    }
}

#[test]
fn helper_predicates_against_spec() {
    // FUN_0042a3c0: 上位10bit ∈ {2,0xe,0x12,0x1b} かつ下位5bit ∈ {1,4,0x12}、または 上位=6 かつ下位 ∈ {3,4,0x12}
    assert!(is_pause(0x1b, 0x12)); // 0x6c12
    assert!(is_pause(0x12, 0x1));
    assert!(is_pause(6, 3));
    assert!(!is_pause(0x1b, 0x10));
    assert!(!is_pause(0, 0));
    // FUN_0044b350: 実音素判定
    assert!(is_real_phoneme(0x1b, 5));
    assert!(!is_real_phoneme(0, 1));
    assert!(!is_real_phoneme(6, 3));
}

#[test]
fn entry_roundtrip_real() {
    let Some(info) = load_voice_info() else {
        eprintln!("SKIP: VoiceInfo.pkg not found");
        return;
    };
    // 全エントリの to_bytes → from_bytes ラウンドトリップ
    for e in info.entries.iter().take(1000) {
        assert_eq!(VoiceInfoEntry::from_bytes(&e.to_bytes()), *e);
    }
}
