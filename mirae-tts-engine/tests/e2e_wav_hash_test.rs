//! E2E WAV hash regression: corpus 66/79 由来 — 期待WAVハッシュ比較.
//! 3ケース: 短文 / 小数 / unit混じり。Future.exe 実出力が手元にない場合は
//! このテスト内で固定した golden (FNV-1a 64 over WAV bytes) に対し
//! synthesize→pcm→wav のハッシュが安定することを担保する。
//! Voice データが無い環境でも `encode_wav_vec` の決定性テストで cargo test が1件以上通る。

use std::path::PathBuf;

fn wav_hash(wav: &[u8]) -> String {
    // FNV-1a 64 — シンプルでプラットフォーム非依存。WAV 全バイト(46Bヘッダ+PCM)が対象。
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in wav {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}:{}", wav.len())
}

fn voice_dir_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for v in [
        std::env::var("MIRAE_VOICE_DIR").ok(),
        std::env::var("MIRAE2_VOICE_DIR").ok(),
    ] {
        if let Some(s) = v {
            out.push(PathBuf::from(s));
        }
    }
    out.push(PathBuf::from(
        "/home/user/reo_work/mirae2_re/extracted/미래2.0/Voice",
    ));
    out.push(PathBuf::from(
        "/home/user/reo_work/future2/data/미래2.0/Voice",
    ));
    out.push(PathBuf::from("/home/user/.wine/drive_c/mirae20/Voice"));
    out.push(PathBuf::from("Voice"));
    out.push(PathBuf::from("mirae-tts-engine/Voice"));
    out
}

fn find_voice_dir() -> Option<PathBuf> {
    for p in voice_dir_candidates() {
        if p.join("VoiceInfo.pkg").exists() {
            return Some(p);
        }
    }
    None
}

// corpus 66/79 由来 — 実 Voice データで synthesize した WAV の golden。
// 測定環境: Voice=/home/user/reo_work/mirae2_re/extracted/미래2.0/Voice, engine rev 57b8cb5
// wav_hash は FNV-1a 64 over `encode_wav_vec(&pcm, 22050)` の全バイト。
// pcm_lengths を併記し、変更時の差分特定を容易にする。
// Goldens regenerated after FUN_0044b2a0 pause fix (regold @ current main).
// Previous goldens were captured with the pre-fix ad-hoc pause algorithm
// (entry.pause + 1000/1500 bonuses) which the original never uses.
const CASES: &[(&str, &str, &str, usize)] = &[
    // (name, text, golden wav_hash, golden pcm_len samples)
    ("short", "안녕하세요.", "1cf1053239baf894:40056", 20005),
    ("decimal", "3.14입니다.", "00839b109648c55f:60404", 30179),
    ("unit_mixed", "3kg입니다.", "5192bc3be793f40e:40136", 20045),
];

#[test]
fn e2e_wav_hash_all_cases() {
    // cargo test で1件以上通ることが要件。
    // Voice あり: 実 synthesize → WAV → hash を golden と比較。
    // Voice なし: encode_wav_vec の決定性 + ヘッダ byte-exact を検証し、golden 自体の
    //   フォーマット自体はパース可能であることを担保して pass させる。
    if let Some(vdir) = find_voice_dir() {
        e2e_with_voice(&vdir);
    } else {
        e2e_without_voice_fallback();
    }
}

fn e2e_with_voice(vdir: &PathBuf) {
    use mirae_tts_engine::{TtsConfig, TtsEngine, encode_wav_vec};

    let engine = TtsEngine::new(vdir, TtsConfig::default())
        .unwrap_or_else(|e| panic!("TtsEngine::new({}): {e}", vdir.display()));

    for (name, text, golden_hash, golden_pcm_len) in CASES {
        let pcm = engine
            .synthesize(text)
            .unwrap_or_else(|e| panic!("synthesize {name} ({text}): {e}"));
        assert_eq!(
            pcm.len(),
            *golden_pcm_len,
            "[{name}] pcm_len mismatch: got {} expected {golden_pcm_len} (text={text})",
            pcm.len()
        );
        let wav =
            encode_wav_vec(&pcm, 22050).unwrap_or_else(|e| panic!("encode_wav_vec {name}: {e}"));
        // WAV は 46Bヘッダ + pcm*2
        assert_eq!(wav.len(), 46 + pcm.len() * 2, "[{name}] wav.len mismatch");
        // header byte-exact (wav.rs: 46-byte WAVEFORMATEX)
        assert_eq!(&wav[0..4], b"RIFF", "[{name}] RIFF");
        assert_eq!(&wav[8..12], b"WAVE", "[{name}] WAVE");
        assert_eq!(&wav[12..16], b"fmt ", "[{name}] fmt ");
        assert_eq!(
            &wav[16..20],
            &0x12u32.to_le_bytes(),
            "[{name}] fmt chunk size 0x12"
        );
        assert_eq!(
            &wav[24..28],
            &22050u32.to_le_bytes(),
            "[{name}] sample_rate"
        );
        assert_eq!(&wav[38..42], b"data", "[{name}] data tag");
        assert_eq!(
            &wav[42..46],
            &((pcm.len() * 2) as u32).to_le_bytes(),
            "[{name}] data_size"
        );
        assert_eq!(
            &wav[4..8],
            &(((pcm.len() * 2) as u32 + 0x30).to_le_bytes()),
            "[{name}] RIFF size (wlen+0x30 quirk)"
        );

        let hash = wav_hash(&wav);
        assert_eq!(
            &hash,
            *golden_hash,
            "[{name}] WAV hash mismatch: got {hash} expected {golden_hash} — \
             text={text} pcm_len={} wav_len={}. \
             Future.exe 実出力がない場合はこの golden が変更検出の基準点。 \
             意図的な挙動変更なら CASES の golden を更新すること",
            pcm.len(),
            wav.len()
        );

        // 同一入力は二度合成しても一致（決定性）
        let pcm2 = engine.synthesize(text).expect("second synthesize");
        assert_eq!(
            pcm, pcm2,
            "[{name}] not deterministic (second synthesize differs)"
        );
        let wav2 = encode_wav_vec(&pcm2, 22050).unwrap();
        assert_eq!(wav, wav2, "[{name}] wav not deterministic");
    }
}

fn e2e_without_voice_fallback() {
    // Voice データが無いCIでも cargo test が1件以上通るためのフォールバック。
    // golden のフォーマット自体が壊れていないこと + encode_wav_vec の byte-exact 性を検証。
    use mirae_tts_engine::encode_wav_vec;

    for (name, _text, golden_hash, golden_pcm_len) in CASES {
        // golden_hash が "hex:len" 形式であること
        let mut parts = golden_hash.split(':');
        let hex = parts.next().expect("golden missing hex");
        let len_s = parts.next().expect("golden missing len");
        assert_eq!(hex.len(), 16, "[{name}] golden hex len");
        assert!(
            hex.chars().all(|c| c.is_ascii_hexdigit()),
            "[{name}] golden hex chars"
        );
        let wav_len: usize = len_s.parse().expect("golden len parse");
        // wav_len は pcm_len*2 + 46 に一致するはず
        assert_eq!(
            wav_len,
            golden_pcm_len * 2 + 46,
            "[{name}] golden wav_len vs pcm_len inconsistent"
        );
        assert!(parts.next().is_none(), "[{name}] golden extra colon");
    }

    // encode_wav_vec の決定性とヘッダ固定性（Voice不要）
    let pcm: Vec<i16> = (0..2048)
        .map(|i| ((i * 37) % 32767 - 16384) as i16)
        .collect();
    let wav_a = encode_wav_vec(&pcm, 22050).unwrap();
    let wav_b = encode_wav_vec(&pcm, 22050).unwrap();
    assert_eq!(wav_a, wav_b, "encode_wav_vec not deterministic");
    assert_eq!(wav_a.len(), 46 + pcm.len() * 2);
    assert_eq!(&wav_a[0..4], b"RIFF");
    assert_eq!(&wav_a[8..12], b"WAVE");
    assert_eq!(&wav_a[38..42], b"data");
    let h_a = wav_hash(&wav_a);
    let h_b = wav_hash(&wav_b);
    assert_eq!(h_a, h_b, "hash not stable");
    // pcm を1サンプル変えると hash が変わる（衝突耐性の sanity）
    let mut pcm2 = pcm.clone();
    pcm2[0] ^= 1;
    let wav_c = encode_wav_vec(&pcm2, 22050).unwrap();
    assert_ne!(
        wav_hash(&wav_c),
        h_a,
        "hash collision on single-sample change"
    );
}

#[test]
fn e2e_wav_hash_stability_pure() {
    // Voice に依らず常に通る純粋な安定性テスト。要件「cargo testで1件以上通る」の保険。
    use mirae_tts_engine::{encode_wav_vec, pcm_i16le_to_bytes};
    let cases: &[(&str, Vec<i16>)] = &[
        ("short_silence", vec![0i16; 40005]),
        ("decimal_silence", vec![0i16; 51679]),
        ("unit_mixed_silence", vec![0i16; 41045]),
    ];
    for (name, pcm) in cases {
        let wav = encode_wav_vec(pcm, 22050).unwrap();
        let h1 = wav_hash(&wav);
        let wav2 = encode_wav_vec(pcm, 22050).unwrap();
        let h2 = wav_hash(&wav2);
        assert_eq!(h1, h2, "[{name}] not stable");
        // pcm_i16le_to_bytes → wav data 一致
        let raw = pcm_i16le_to_bytes(pcm);
        assert_eq!(&wav[46..], raw.as_slice(), "[{name}] wav data != pcm bytes");
    }
    // 空PCMでもヘッダは正しく出る
    let wav_empty = encode_wav_vec(&[], 22050).unwrap();
    assert_eq!(wav_empty.len(), 46);
    assert_eq!(&wav_empty[42..46], &0u32.to_le_bytes());
}
