//! Needs `VoiceInfo.pkg` + `VoiceData.pkg` under `voice_dir`. stdin or `-t` → WAV or raw PCM to stdout or `-o`.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use mirae_tts_engine::{encode_wav_vec, pcm_i16le_to_bytes, TtsConfig, TtsEngine};

#[derive(Parser)]
#[command(
    name = "mirae-tts",
    about = "Mirae TTS: stdin text → stdout (WAV or mono s16le PCM)"
)]
struct Cli {
    /// Directory with VoiceInfo.pkg and VoiceData.pkg
    voice_dir: PathBuf,
    /// Text to synthesize (default: read stdin)
    #[arg(short = 't', long = "text")]
    text: Option<String>,
    /// Write output here (default: stdout)
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,
    /// Output container / encoding
    #[arg(
        short = 'f',
        long = "format",
        value_enum,
        default_value_t = OutputFormat::Wav
    )]
    format: OutputFormat,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum OutputFormat {
    /// WAV (mono s16le), default
    #[default]
    #[value(alias = "wave")]
    Wav,
    /// Raw mono s16le PCM (no header); same bytes as `GET /synthesize/raw`
    #[value(alias = "raw")]
    Pcm,
    /// Raw PCM written per segment as it is synthesized; same chunking as streaming `audio/l16`
    #[value(alias = "stream")]
    PcmStream,
}

fn main() -> io::Result<()> {
    tracing_subscriber::fmt().init();

    let cli = Cli::parse();

    let input = match &cli.text {
        Some(t) => t.clone(),
        None => {
            // If stdin is a TTY, don't block waiting for input — require -t/--text or a pipe.
            if atty::is(atty::Stream::Stdin) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "no text provided; use -t / --text when running interactively or pipe text to stdin",
                ));
            }

            let mut s = String::new();
            io::stdin().read_to_string(&mut s)?;
            s
        }
    };
    let text = input.trim();
    if text.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "empty input (pipe stdin or use -t / --text)",
        ));
    }

    let engine = TtsEngine::new(
        &cli.voice_dir,
        TtsConfig {
            sample_rate: 22050,
            sentence_pause: 4000,
            log_progress: true,
        },
    )?;

    let rate = engine.effective_sample_rate();

    match cli.format {
        OutputFormat::Wav => {
            let pcm = engine.synthesize(text)?;
            if pcm.is_empty() {
                return Err(io::Error::other("no audio generated"));
            }
            let wav = encode_wav_vec(&pcm, rate)?;
            write_bytes(&cli.output, &wav)?;
        }
        OutputFormat::Pcm => {
            let pcm = engine.synthesize(text)?;
            if pcm.is_empty() {
                return Err(io::Error::other("no audio generated"));
            }
            let bytes = pcm_i16le_to_bytes(&pcm);
            write_bytes(&cli.output, &bytes)?;
        }
        OutputFormat::PcmStream => {
            if let Some(path) = &cli.output {
                let mut f = File::create(path)?;
                synthesize_pcm_stream(&engine, text, &mut f, false)?;
            } else {
                let mut out = io::stdout().lock();
                synthesize_pcm_stream(&engine, text, &mut out, true)?;
            }
        }
    }

    Ok(())
}

fn write_bytes(path: &Option<PathBuf>, data: &[u8]) -> io::Result<()> {
    if let Some(p) = path {
        std::fs::write(p, data)
    } else {
        match io::stdout().lock().write_all(data) {
            Ok(()) => Ok(()),
            // ffplay/ffmpeg closed stdin after enough data — not a failure for a pipe source
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
            Err(e) => Err(e),
        }
    }
}

/// `reader_may_hang_up`: stdout piped to a player that exits early (BrokenPipe = OK).
fn synthesize_pcm_stream<W: Write>(
    engine: &TtsEngine,
    text: &str,
    w: &mut W,
    reader_may_hang_up: bool,
) -> io::Result<()> {
    let mut wrote = false;
    let mut reader_disconnected = false;
    engine.synthesize_streaming(text, |chunk| {
        if chunk.is_empty() {
            return true;
        }
        let bytes = pcm_i16le_to_bytes(&chunk);
        match w.write_all(&bytes) {
            Ok(()) => {}
            Err(e) if reader_may_hang_up && e.kind() == io::ErrorKind::BrokenPipe => {
                reader_disconnected = true;
                return false;
            }
            Err(_) => return false,
        }
        wrote = true;
        if let Err(e) = w.flush() && reader_may_hang_up && e.kind() == io::ErrorKind::BrokenPipe {
            reader_disconnected = true;
            return false;
        }
        true
    })?;
    if !wrote && !reader_disconnected {
        return Err(io::Error::other("no audio generated"));
    }
    Ok(())
}
