use mirae_tts_engine::{TtsConfig, TtsEngine};
fn main() {
    let vdir = std::path::Path::new("/home/user/reo_work/mirae2_re/extracted/미래2.0/Voice");
    let engine = TtsEngine::new(vdir, TtsConfig::default()).unwrap();
    let pcm = engine.synthesize("안녕하십니까").unwrap();
    println!("samples={}", pcm.len());
}
