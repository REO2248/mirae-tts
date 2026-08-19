//! Real-data verification: VoiceInfo.pkg parse + unit selection (SPEC 2.5 / T1_voiceinfo.md).
//! Data from MIRAE2_VOICE_DIR; skipped when missing.
use mirae_tts_engine::unit_select::{
    ProsodyRecord, UnitSelectConfig, UnitSelector, is_pause, is_real_phoneme,
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
    assert!(info.woff_chain_ok());
    assert!(info.cur_dup_ok());
    assert_eq!(info.total_samples(), 285_520_544);
}

#[test]
fn entry0_matches_real_data() {
    let Some(info) = load_voice_info() else {
        eprintln!("SKIP: VoiceInfo.pkg not found");
        return;
    };
    let e = info.entries[0];
    assert_eq!(e.phone_cur, 0x6d86);
    assert_eq!(e.phone_prev, 0x6eb3);
    assert_eq!(e.phone_next, 0x6d80);
    assert_eq!(e.phone_cur2, 0x6d86);
    assert_eq!(e.woff, 0);
    assert_eq!(e.wlen, 0x161a);
    assert_eq!(e.pitch, 0x56);
    assert_eq!(e.classcode, 0x0028);
    assert_eq!(e.flags, 0xff01);
    assert_eq!(e.pause, -7);
    assert_eq!(e.woff_lo, 0);
    assert_eq!(e.woff_lo, (e.woff & 0xffff) as u16);
    assert!(e.is_normal());
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
    assert_eq!(entry.phone_cur, 0x6d86);
    assert!(entry.is_normal());
    assert!((78..=220).contains(&entry.pitch_signed()));
    assert!((700..=7000).contains(&(entry.wlen as i32)));
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
    assert!((90 - sp.pitch_signed()).abs() <= 200);
}

#[test]
fn process_real_records_smoke() {
    let Some(info) = load_voice_info() else {
        eprintln!("SKIP: VoiceInfo.pkg not found");
        return;
    };
    let mut sel = UnitSelector::new(&info, UnitSelectConfig::default());
    let records = [
        ProsodyRecord {
            prev_code: 0,
            code: 0x6d86,
            marker: 0,
            flags: 0,
            tone_class: 0x28,
        },
        ProsodyRecord {
            prev_code: 0x6d86,
            code: 0x6d80,
            marker: 0,
            flags: 0,
            tone_class: 0x28,
        },
        ProsodyRecord {
            prev_code: 0x6d80,
            code: 0x6d86,
            marker: 0,
            flags: 0,
            tone_class: 0x0a,
        },
        ProsodyRecord {
            prev_code: 0x6d86,
            code: 0x6d80,
            marker: 0,
            flags: 0,
            tone_class: 0x28,
        },
        ProsodyRecord {
            prev_code: 0x6d80,
            code: 0x6c12,
            marker: 2,
            flags: 1,
            tone_class: 0x01,
        },
    ];
    let out = sel.process(&records);
    assert_eq!(out.units.len(), 5, "all records must select a unit");
    for u in &out.units {
        assert_eq!(
            u.data.phone_cur, u.request.cur,
            "selected cur must match request"
        );
        assert!(u.marker > 0, "marker must be positive");
    }
    let min_wave = out
        .units
        .iter()
        .map(|u| u.data.wlen as i64 * 2)
        .sum::<i64>();
    assert!(out.total_samples >= min_wave);
    assert!(out.total_samples > 0);
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
    let records = [
        ProsodyRecord {
            prev_code: 0,
            code: 0x6c12,
            marker: 0,
            flags: 0,
            tone_class: 0x01,
        },
        ProsodyRecord {
            prev_code: 0x6c12,
            code: 0x6d86,
            marker: 0,
            flags: 0,
            tone_class: 0x28,
        },
        ProsodyRecord {
            prev_code: 0x6d86,
            code: 0x6c12,
            marker: 2,
            flags: 1,
            tone_class: 0x01,
        },
    ];
    let out = sel.process(&records);
    assert_eq!(out.units.len(), 3);
    assert!(out.total_samples > 0);
}

#[test]
fn scan_matches_independent_python_reference() {
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
    let records = [ProsodyRecord {
        prev_code: 0x6d80,
        code: 0x6eb3,
        marker: 0,
        flags: 0,
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
    let records = [
        ProsodyRecord {
            prev_code: 0,
            code: 0x6d86,
            marker: 0,
            flags: 0,
            tone_class: 0x28,
        },
        ProsodyRecord {
            prev_code: 0x6d86,
            code: 0x6d80,
            marker: 0,
            flags: 0,
            tone_class: 0x01,
        },
        ProsodyRecord {
            prev_code: 0x6d80,
            code: 0x6d86,
            marker: 0,
            flags: 0,
            tone_class: 0x0a,
        },
        ProsodyRecord {
            prev_code: 0x6d86,
            code: 0x6d80,
            marker: 0,
            flags: 0,
            tone_class: 0x03,
        },
        ProsodyRecord {
            prev_code: 0x6d80,
            code: 0x6c12,
            marker: 2,
            flags: 1,
            tone_class: 0x1e,
        },
    ];
    let out = sel.process(&records);
    assert_eq!(out.units.len(), 5);
    let B = mirae_tts_engine::unit_select::BOUNDARY_CODE;
    assert_eq!(B, 0x6EB3);
    assert_eq!(out.units[0].request.prev, B, "u0 文頭 prev");
    assert_eq!(out.units[0].request.next, 0x6d80, "u0 next は次のレコード");
    assert_eq!(out.units[1].request.prev, 0x6d86);
    assert_eq!(out.units[1].request.next, 0x6d86);
    assert_eq!(out.units[2].request.prev, 0x6d80);
    assert_eq!(out.units[2].request.next, 0x6d80);
    assert_eq!(out.units[3].request.next, B, "u3 調値>1 next");
    assert_eq!(out.units[3].request.prev, 0x6d86);
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
    let records = [
        ProsodyRecord {
            prev_code: 0,
            code: 0x6d86,
            marker: 0,
            flags: 0,
            tone_class: 0x28,
        },
        ProsodyRecord {
            prev_code: 0x6d86,
            code: 0x6d80,
            marker: 1,
            flags: 0,
            tone_class: 0x01,
        },
        ProsodyRecord {
            prev_code: 0x6d80,
            code: 0x6c12,
            marker: 0,
            flags: 0,
            tone_class: 0x28,
        },
    ];
    let out = sel.process(&records);
    assert_eq!(out.units.len(), 3);
    let B = mirae_tts_engine::unit_select::BOUNDARY_CODE;
    assert_eq!(
        out.units[1].request.next, B,
        "文末レコード next は境界コード"
    );
    assert_eq!(out.units[2].request.prev, 0x6d80);
}

#[test]
fn request_pitch_smoothing_runs() {
    let Some(info) = load_voice_info() else {
        eprintln!("SKIP: VoiceInfo.pkg not found");
        return;
    };
    let mut sel = UnitSelector::new(&info, UnitSelectConfig::default());
    let records = [
        ProsodyRecord {
            prev_code: 0,
            code: 0x6d86,
            marker: 0,
            flags: 0,
            tone_class: 0x28,
        },
        ProsodyRecord {
            prev_code: 0x6d86,
            code: 0x6d80,
            marker: 0,
            flags: 0,
            tone_class: 0x28,
        },
        ProsodyRecord {
            prev_code: 0x6d80,
            code: 0x6d86,
            marker: 0,
            flags: 0,
            tone_class: 0x0a,
        },
    ];
    let out = sel.process(&records);
    assert_eq!(out.units.len(), 3);
    for u in &out.units {
        let _ = u.active_data();
    }
    for u in &out.units {
        assert!(u.data.pause >= 0 || u.data2.map_or(false, |d| d.pause >= 0));
    }
}

#[test]
fn helper_predicates_against_spec() {
    assert!(is_pause(0x1b, 0x12));
    assert!(is_pause(0x12, 0x1));
    assert!(is_pause(6, 3));
    assert!(!is_pause(0x1b, 0x10));
    assert!(!is_pause(0, 0));
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
    for e in info.entries.iter().take(1000) {
        assert_eq!(VoiceInfoEntry::from_bytes(&e.to_bytes()), *e);
    }
}
