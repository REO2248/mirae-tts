//! Real-data verification: VoiceInfo.pkg / VoiceData.pkg -> waveform concat -> WAV.
//! Data from the real Voice directory (MIRAE2_VOICE_DIR overridable).
use std::path::{Path, PathBuf};

use mirae_tts_engine::render::{
    Chunk, ChunkRing, RenderUnit, UnitRecord, is_real_phoneme, render_units,
};
use mirae_tts_engine::voice_data::VoiceData;
use mirae_tts_engine::wav::{WAV_HEADER_SIZE, WavWriter, write_wav_file, write_wav_header};
use mirae_tts_engine::{RING_MAX_BYTES, RING_SLOTS};

fn voice_dir() -> PathBuf {
    match std::env::var("MIRAE2_VOICE_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(_) => PathBuf::from("/home/user/reo_work/mirae2_re/extracted/미래2.0/Voice"),
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
struct ViEntry {
    phone_cur: u16,
    phone_prev: u16,
    phone_next: u16,
    woff: u32,
    wlen: u32,
    pitch: i16,
    classcode: u8,
    pause: i16,
}

fn read_voice_info(dir: &Path) -> Vec<ViEntry> {
    let bytes = std::fs::read(dir.join("VoiceInfo.pkg"))
        .unwrap_or_else(|e| panic!("VoiceInfo.pkg を開けない: {e} ({})", dir.display()));
    let count = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    assert_eq!(count, 70150, "VoiceInfo エントリ数は 70,150 のはず");
    assert_eq!(bytes.len(), 4 + count * 28, "VoiceInfo.pkg サイズ不整合");
    bytes[4..]
        .chunks_exact(28)
        .map(|e| ViEntry {
            phone_cur: u16::from_le_bytes([e[0], e[1]]),
            phone_prev: u16::from_le_bytes([e[2], e[3]]),
            phone_next: u16::from_le_bytes([e[4], e[5]]),
            woff: u32::from_le_bytes(e[8..12].try_into().unwrap()),
            wlen: u32::from_le_bytes(e[12..16].try_into().unwrap()),
            pitch: i16::from_le_bytes(e[0x12..0x14].try_into().unwrap()),
            classcode: e[0x14],
            pause: i16::from_le_bytes(e[0x18..0x1a].try_into().unwrap()),
        })
        .collect()
}

fn open_voice_data(dir: &Path) -> VoiceData {
    VoiceData::open(dir)
        .unwrap_or_else(|e| panic!("VoiceData.pkg を開けない: {e} ({})", dir.display()))
}

#[test]
fn voiceinfo_entry0_and_chain() {
    let dir = voice_dir();
    let vi = read_voice_info(&dir);
    assert_eq!(vi[0].woff, 0);
    assert_eq!(vi[0].wlen, 5658);
    assert_eq!(vi[0].phone_cur, 0x6d86);
    assert_eq!(vi[1].woff, 5658);
    assert_eq!(vi[1].wlen, 3909);
    for i in 0..vi.len() - 1 {
        assert_eq!(
            vi[i + 1].woff,
            vi[i].woff + vi[i].wlen,
            "woff チェーンが i={i} で不連続"
        );
    }
    let sum_wlen: u64 = vi.iter().map(|e| u64::from(e.wlen)).sum();
    let vd = open_voice_data(&dir);
    assert_eq!(
        sum_wlen * 2,
        vd.len().unwrap(),
        "sum(wlen)×2 != VoiceData.pkg サイズ"
    );
}

#[test]
fn voice_data_entry0_first_bytes() {
    let dir = voice_dir();
    let mut vd = open_voice_data(&dir);
    let mut buf = vec![0u8; 5658 * 2];
    vd.read_unit(0, 5658, &mut buf).unwrap();
    assert_eq!(
        &buf[..8],
        &[0x0c, 0xff, 0x17, 0x01, 0x12, 0xff, 0x6e, 0x00],
        "エントリ 0 先頭バイトが T1 §4 の実測と不一致"
    );
    assert_ne!(&buf[..2], &[0, 0]);
}

#[test]
fn voice_data_entry1_follows_entry0() {
    let dir = voice_dir();
    let mut vd = open_voice_data(&dir);
    let e0 = vd.read_unit_vec(0, 5658).unwrap();
    let e1 = vd.read_unit_vec(5658, 3909).unwrap();
    let mut raw = vec![0u8; (5658 + 3909) * 2];
    vd.read_unit(0, 5658 + 3909, &mut raw).unwrap();
    assert_eq!(&raw[5658 * 2..5658 * 2 + 8], &e1[..8]);
    assert_eq!(&raw[5658 * 2 - 8..5658 * 2], &e0[5658 * 2 - 8..]);
    assert_eq!(&raw[..5658 * 2], &e0[..]);
    assert_eq!(&e1[..2], &raw[11316..11318]);
}

#[test]
fn render_entry0_with_doubling() {
    let dir = voice_dir();
    let vi = read_voice_info(&dir);
    let mut vd = open_voice_data(&dir);

    let rec0 = UnitRecord {
        woff: vi[0].woff,
        wlen: vi[0].wlen,
        pitch: vi[0].pitch,
        classcode: vi[0].classcode,
        pause: vi[0].pause,
    };
    let rec1 = UnitRecord {
        woff: vi[1].woff,
        wlen: vi[1].wlen,
        pitch: vi[1].pitch,
        classcode: vi[1].classcode,
        pause: vi[1].pause,
    };
    assert!(is_real_phoneme(
        vi[0].phone_cur >> 10,
        vi[0].phone_next & 0x1f
    ));
    assert_eq!((rec0.classcode as i8) % 10, 0);

    let unit = RenderUnit {
        code_cur: vi[0].phone_cur,
        code_next: vi[0].phone_next,
        record: rec0,
        extra: Some(rec1),
    };

    let mut out = Vec::new();
    let total = render_units(&mut vd, &[unit], &mut out, false).unwrap();

    assert_eq!(total, (5658 + 3909) * 2);
    assert_eq!(out.len(), total);
    let e0 = vd.read_unit_vec(0, 5658).unwrap();
    let e1 = vd.read_unit_vec(5658, 3909).unwrap();
    assert_eq!(&out[..5658 * 2], &e0[..], "主ユニット波形不一致");
    assert_eq!(&out[5658 * 2..], &e1[..], "追加ユニット (2重化) 波形不一致");
}

#[test]
fn render_no_doubling_when_tone_ge_2() {
    let dir = voice_dir();
    let vi = read_voice_info(&dir);
    let mut vd = open_voice_data(&dir);
    let idx = vi
        .iter()
        .position(|e| {
            (e.classcode as i8) % 10 >= 2
                && e.wlen <= 12000
                && is_real_phoneme(e.phone_cur >> 10, e.phone_next & 0x1f)
        })
        .expect("調値>=2 かつ実音素のエントリが無い");
    let e = vi[idx];
    let rec = UnitRecord {
        woff: e.woff,
        wlen: e.wlen,
        pitch: e.pitch,
        classcode: e.classcode,
        pause: 0,
    };
    let unit = RenderUnit {
        code_cur: e.phone_cur,
        code_next: e.phone_next,
        record: rec,
        extra: Some(UnitRecord {
            woff: 0,
            wlen: 100,
            pitch: 80,
            classcode: 0,
            pause: 0,
        }),
    };
    let mut out = Vec::new();
    let total = render_units(&mut vd, &[unit], &mut out, false).unwrap();
    assert_eq!(total, e.wlen as usize * 2, "調値>=2 では 2重化されないはず");
}

#[test]
fn render_pause_inserts_silence() {
    let dir = voice_dir();
    let vi = read_voice_info(&dir);
    let mut vd = open_voice_data(&dir);
    let e = vi[0];
    let rec = UnitRecord {
        woff: e.woff,
        wlen: e.wlen,
        pitch: e.pitch,
        classcode: e.classcode,
        pause: 1000,
    };
    let unit = RenderUnit {
        code_cur: e.phone_cur,
        code_next: e.phone_next,
        record: rec,
        extra: None,
    };
    let mut out = Vec::new();
    let total = render_units(&mut vd, &[unit], &mut out, false).unwrap();
    assert_eq!(total, 5658 * 2 + 1000 * 2);
    assert_eq!(out.len(), total);
    assert!(
        out[5658 * 2..].iter().all(|&b| b == 0),
        "pause 分はゼロ (無音)"
    );
    let mut out2 = Vec::new();
    let rec2 = UnitRecord {
        pause: vi[0].pause,
        ..rec
    };
    let unit2 = RenderUnit {
        record: rec2,
        extra: None,
        ..unit
    };
    render_units(&mut vd, &[unit2], &mut out2, false).unwrap();
    assert_eq!(out2.len(), 5658 * 2, "pause<0 では無音挿入されない");
}

#[test]
fn is_real_phoneme_table() {
    assert!(!is_real_phoneme(0, 1));
    assert!(!is_real_phoneme(0, 4));
    assert!(!is_real_phoneme(0, 6));
    assert!(!is_real_phoneme(0, 0x10));
    assert!(!is_real_phoneme(0, 0xc));
    assert!(!is_real_phoneme(0, 0x12));
    assert!(!is_real_phoneme(0, 8));
    assert!(!is_real_phoneme(0, 9));
    assert!(!is_real_phoneme(0, 10));
    assert!(!is_real_phoneme(0, 0xb));
    assert!(!is_real_phoneme(0, 0xd));
    assert!(!is_real_phoneme(0, 0xe));
    assert!(!is_real_phoneme(0, 0x11));
    assert!(!is_real_phoneme(6, 3));
    assert!(is_real_phoneme(7, 3));
    assert!(is_real_phoneme(0, 0));
    assert!(is_real_phoneme(0x1b, 0));
    assert!(is_real_phoneme(0, 2));
    assert!(is_real_phoneme(0, 5));
}

#[test]
fn chunk_ring_20_slots_and_1mb_limit() {
    let mut ring = ChunkRing::new();
    assert!(ring.is_empty());
    assert_eq!(ring.len(), 0);

    for i in 0..RING_SLOTS - 1 {
        assert!(ring.push(Chunk::new(vec![i as u8; 16])));
    }
    assert_eq!(ring.len(), RING_SLOTS - 1);
    assert!(!ring.can_push(16), "リング full では push できない");
    assert!(!ring.push(Chunk::new(vec![0xff; 16])));

    let mut ring2 = ChunkRing::new();
    assert!(ring2.push(Chunk::new(vec![0u8; 700_000])));
    assert!(ring2.push(Chunk::new(vec![0u8; 300_000])));
    assert_eq!(ring2.total_bytes(), 1_000_000);
    assert!(
        ring2.can_push(48_575),
        "総量+size = 1,048,575 はぎりぎり許容 (<= 0xFFFFF)"
    );
    assert!(
        !ring2.can_push(48_576),
        "総量 > 1MB (0xFFFFF) では push できない"
    );
    assert!(!ring2.push(Chunk::new(vec![0u8; 48_576])));
    assert_eq!(ring2.total_bytes(), 1_000_000);
    assert!(ring2.total_bytes() <= RING_MAX_BYTES);

    let popped: Vec<usize> = std::iter::from_fn(|| ring.pop().map(|c| c.data.len())).collect();
    assert_eq!(popped.len(), RING_SLOTS - 1);
    assert!(ring.is_empty());
    assert_eq!(ring.total_bytes(), 0);
    assert!(ring.pop().is_none());
}

#[test]
fn produce_chunks_streaming() {
    let dir = voice_dir();
    let vi = read_voice_info(&dir);
    let mut vd = open_voice_data(&dir);

    let units: Vec<RenderUnit> = vi[..10]
        .iter()
        .map(|e| RenderUnit {
            code_cur: e.phone_cur,
            code_next: e.phone_next,
            record: UnitRecord {
                woff: e.woff,
                wlen: e.wlen,
                pitch: e.pitch,
                classcode: e.classcode,
                pause: e.pause,
            },
            extra: None,
        })
        .collect();

    let mut expected = Vec::new();
    let expected_total = render_units(&mut vd, &units, &mut expected, false).unwrap();

    let mut ring = ChunkRing::new();
    let mut consumed: Vec<Vec<u8>> = Vec::new();
    let produced =
        mirae_tts_engine::render::produce_chunks(&mut vd, &units, &mut ring, 3, false, |ring| {
            while let Some(c) = ring.pop() {
                consumed.push(c.data);
            }
        })
        .unwrap();
    assert_eq!(produced, 4, "10 ユニット / 3 個ずつ = 4 チャンク");
    while let Some(c) = ring.pop() {
        consumed.push(c.data);
    }
    let total: usize = consumed.iter().map(|c| c.len()).sum();
    assert_eq!(total, expected_total);
    let mut joined = Vec::new();
    for c in &consumed {
        joined.extend_from_slice(c);
    }
    assert_eq!(joined, expected, "チャンク連結結果が一括 render と不一致");
}

#[test]
fn wav_header_46_bytes_exact() {
    let mut buf: Vec<u8> = vec![0u8; 100];
    write_wav_header(&mut std::io::Cursor::new(&mut buf), 4).unwrap();
    let h = &buf[..WAV_HEADER_SIZE];
    assert_eq!(&h[0..4], b"RIFF");
    assert_eq!(
        u32::from_le_bytes(h[4..8].try_into().unwrap()),
        4 + 0x30,
        "RIFF サイズ = data+0x30 (癖)"
    );
    assert_eq!(&h[8..12], b"WAVE");
    assert_eq!(&h[12..16], b"fmt ");
    assert_eq!(
        u32::from_le_bytes(h[16..20].try_into().unwrap()),
        0x12,
        "fmt チャンクサイズ 18 (WAVEFORMATEX)"
    );
    assert_eq!(u16::from_le_bytes(h[20..22].try_into().unwrap()), 1);
    assert_eq!(u16::from_le_bytes(h[22..24].try_into().unwrap()), 1);
    assert_eq!(u32::from_le_bytes(h[24..28].try_into().unwrap()), 22050);
    assert_eq!(u32::from_le_bytes(h[28..32].try_into().unwrap()), 44100);
    assert_eq!(u16::from_le_bytes(h[32..34].try_into().unwrap()), 2);
    assert_eq!(u16::from_le_bytes(h[34..36].try_into().unwrap()), 16);
    assert_eq!(u16::from_le_bytes(h[36..38].try_into().unwrap()), 0);
    assert_eq!(&h[38..42], b"data");
    assert_eq!(u32::from_le_bytes(h[42..46].try_into().unwrap()), 4);
}

#[test]
fn wav_writer_header_at_finish_and_split() {
    let dir = std::env::temp_dir();
    let path = dir.join("mirae2_wav_test.wav");
    let _ = std::fs::remove_file(&path);

    {
        let mut w = WavWriter::create(&path).unwrap();
        w.append(&[1, 2, 3, 4]).unwrap();
        w.append(&[5, 6]).unwrap();
        let n = w.finish().unwrap();
        assert_eq!(n, 6);
    }
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes.len(), WAV_HEADER_SIZE + 6, "46B ヘッダ + PCM");
    assert_eq!(&bytes[WAV_HEADER_SIZE..], &[1, 2, 3, 4, 5, 6]);
    assert_eq!(u32::from_le_bytes(bytes[42..46].try_into().unwrap()), 6);
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        6 + 0x30
    );

    {
        let mut w = WavWriter::create_with_threshold(&path, 100).unwrap();
        w.append(&vec![0x11; 60]).unwrap();
        w.append(&vec![0x22; 60]).unwrap();
        // first 120 bytes (60+60) finalized into the initial file before split, data_size reset
        assert_eq!(w.data_size(), 0);
        w.append(&vec![0x33; 10]).unwrap();
        let n = w.finish().unwrap();
        assert_eq!(n, 10);
    }
    // After split the initial file keeps the first 120 bytes, the final 10 bytes go to the numbered continuation
    {
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(
            bytes.len(),
            WAV_HEADER_SIZE + 120,
            "先頭ファイルは分割前の 120 バイトを保持"
        );
        assert_eq!(
            &bytes[WAV_HEADER_SIZE..WAV_HEADER_SIZE + 60],
            &vec![0x11; 60][..]
        );
        assert_eq!(u32::from_le_bytes(bytes[42..46].try_into().unwrap()), 120);
    }
    let path2 = {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap();
        let ext = path.extension().and_then(|s| s.to_str()).unwrap();
        path.parent()
            .unwrap()
            .join(format!("{}_{:03}.{}", stem, 1, ext))
    };
    let bytes2 = std::fs::read(&path2).unwrap();
    assert_eq!(
        bytes2.len(),
        WAV_HEADER_SIZE + 10,
        "継続ファイルは最終 10 バイト"
    );
    assert_eq!(&bytes2[WAV_HEADER_SIZE..], &vec![0x33; 10][..]);
    assert_eq!(u32::from_le_bytes(bytes2[42..46].try_into().unwrap()), 10);
    let _ = std::fs::remove_file(&path2);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn wav_ffprobe_readable() {
    let dir = voice_dir();
    let vi = read_voice_info(&dir);
    let mut vd = open_voice_data(&dir);

    let units: Vec<RenderUnit> = vi[..20]
        .iter()
        .map(|e| RenderUnit {
            code_cur: e.phone_cur,
            code_next: e.phone_next,
            record: UnitRecord {
                woff: e.woff,
                wlen: e.wlen,
                pitch: e.pitch,
                classcode: e.classcode,
                pause: e.pause,
            },
            extra: None,
        })
        .collect();
    let mut pcm = Vec::new();
    render_units(&mut vd, &units, &mut pcm, false).unwrap();
    assert!(pcm.len() > 10_000, "PCM が小さすぎる");

    let path = std::env::temp_dir().join("mirae2_render.wav");
    write_wav_file(&path, &pcm).unwrap();

    let out = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name,sample_rate,channels",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(&path)
        .output()
        .expect("ffprobe を実行できない");
    assert!(
        out.status.success(),
        "ffprobe 失敗: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("codec_name=pcm_s16le"), "stdout: {stdout}");
    assert!(stdout.contains("sample_rate=22050"), "stdout: {stdout}");
    assert!(stdout.contains("channels=1"), "stdout: {stdout}");
    let _ = std::fs::remove_file(&path);
}
