# mirae-tts

Rust implementation of the 《미래》2.0 TTS : command-line tool, optional HTTP API, and library crate.

## CLI (`mirae-tts-cli`)

The workspace provides a CLI package named `mirae-tts-cli`. The CLI reads Korean (etc.) text, runs synthesis with fixed engine settings (`sample_rate` 22050, `sentence_pause` 4000, `log_progress: true` in `main.rs`), and writes audio to **stdout** unless `-o` / `--output` is set.

**Invocation**

```text
cargo run -p mirae-tts-cli --release -- [OPTIONS] <VOICE_DIR>
```

`VOICE_DIR` is the path to the voice resources the engine loads. Text is taken from **stdin** (full read, then trimmed) unless `-t` / `--text` is given. Empty text after trim is an error.


| Option                    | Description                                  |
| ------------------------- | -------------------------------------------- |
| `-t`, `--text <TEXT>`     | Input text (omit to read stdin).             |
| `-o`, `--output <PATH>`   | Write output to this file (default: stdout). |
| `-f`, `--format <FORMAT>` | Output encoding (default: `wav`). See below. |


**`--format` values** (`mirae-tts-cli --help` lists aliases)


| Value        | Aliases  | Output                                                                                                                                                                      |
| ------------ | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `wav`        | `wave`   | WAV container, mono PCM16 LE.                                                                                                                                               |
| `pcm`        | `raw`    | One raw mono PCM16 LE buffer (no header); same layout as `GET /api/synthesize_raw`.                                                                                         |

| `pcm-stream` | `stream` | Raw mono PCM16 LE written **incrementally** as segments complete; chunking matches the HTTP `audio/l16` stream. With stdout, a closed pipe (e.g. player exited) is ignored. |


With `cargo run`, pass program arguments after `--` (everything before `--` is for Cargo). To run the produced binary after building use `./target/release/mirae-tts-cli`.

### Examples (`cargo run`)

```bash
echo "안녕하십니까?" | cargo run -p mirae-tts-cli --release -- ./Voice > output.wav
echo "안녕하십니까?" | cargo run -p mirae-tts-cli --release -- ./Voice -f pcm | aplay -t raw -f S16_LE -c 1 -r 22050
cargo run -p mirae-tts-cli --release -- ./Voice -t "안녕하십니까?" -f pcm-stream -o output.pcm
cargo run -p mirae-tts-cli --release -- ./Voice -t "안녕하십니까?" -o output.wav
```

After `cargo build -p mirae-tts-cli --release`, run `./target/release/mirae-tts-cli` with the same arguments.

## HTTP server (`mirae-tts-server`)

Run the server package with the workspace package name `mirae-tts-server`.

```bash
cargo run -p mirae-tts-server --release -- --dic ./Voice
```

`mirae-tts-server` can be configured with command-line flags or the corresponding environment variables:


| Flag               | Environment variable | Default        | Description                                                |
| ------------------ | -------------------- | -------------- | ---------------------------------------------------------- |
| `--listen`         | `LISTEN`             | `0.0.0.0:3000` | Host and port to bind (`host:port`).                       |
| `--dic`            | `DIC`                | `./Voice`      | Path to the voice data directory.                          |
| `--maximum-length` | `MAXIMUM_LENGTH`     | `0`            | Input length limit in Unicode scalars; `0` means no limit. |


### Docker

The container image runs `mirae-tts-server` with the same flags/env vars as above (see `Dockerfile` / `compose.yaml` for `LISTEN`, `DIC`, `MAXIMUM_LENGTH`).

```bash
docker build -t mirae-tts-server .
docker run -p 3000:3000 mirae-tts-server
docker compose up --build
```

### HTTP API (`mirae-tts-server`)

CORS is permissive (browsers may call the API from any origin).

**Input**

- **GET** — query parameter `text` (required).
- **POST** — JSON body `{"text":"…"}` (`Content-Type: application/json`).

**Endpoints**


| Method(s)     | Path(s)                                        | Response                                                                                                               |
| ------------- | ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `GET`         | `/`                                            | Bundled HTML UI (`text/html`).                                                                                         |
| `GET`, `POST` | `/api/synthesize`                               | WAV (`Content-Type: audio/wav`).                                                                                       |
| `GET`, `POST` | `/api/synthesize_stream`                        | Streaming PCM16 LE (`Content-Type: audio/l16; rate=<Hz>; channels=1`).                                                 |
| `GET`, `POST` | `/api/synthesize_raw`                           | Raw PCM16 LE body (`Content-Type: application/octet-stream`; `Content-Disposition: attachment; filename="synth.pcm"`). |


**Errors**

JSON object `{"error":"<message>"}` with `400` (empty/whitespace text), `413` (text longer than `--maximum-length` when set), or `500` (synthesis failure / task error).

## Library

Add `mirae-tts-engine` to your `Cargo.toml` (path or git as needed). The **stable API** is [`TtsEngine`](src/synthesizer.rs), [`TtsConfig`](src/synthesizer.rs), [`encode_wav_vec`](src/wave_render.rs), [`pcm_i16le_to_bytes`](src/wave_render.rs), and [`DEFAULT_SAMPLE_RATE`](src/wave_render.rs), all exported from the crate root; the same set is available as `mirae_tts_engine::prelude`. (Cargo maps the package name `mirae-tts-engine` to the Rust crate name `mirae_tts_engine`.) Other modules stay crate-private.

**`TtsConfig`** (`Clone`, `Default`)


| Field            | Default | Description                                                                                                   |
| ---------------- | ------- | ------------------------------------------------------------------------------------------------------------- |
| `sample_rate`    | `22050` | Logical output sample rate (Hz); `effective_sample_rate()` returns this value.                                |
| `sentence_pause` | `4000`  | Silence length in samples (`i16`, clamped ≥ 0) for sentence-boundary pause phonemes when no unit is selected. |
| `log_progress`   | `false` | When `true`, progress and warnings go to stderr (`eprintln!`).                                                |


**`TtsEngine`**


| Method                                                          | Summary                                                                                  |
| --------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `new(voice_dir, config) -> io::Result<Self>`                    | Initialize the engine from `voice_dir`.                                                  |
| `synthesize(&self, text) -> io::Result<Vec<i16>>`               | Full utterance as mono PCM samples (`i16`).                                              |
| `synthesize_streaming(&self, text, on_chunk) -> io::Result<()>` | Invokes `on_chunk(Vec<i16>)` per segment; return `false` from the closure to stop early. |
| `effective_sample_rate(&self) -> u32`                           | Pass to `encode_wav_vec` when wrapping PCM as WAV.                                       |
| `voice_entry_count(&self) -> usize`                             | Number of loaded voice index entries.                                                    |
| `config(&self) / set_config(&mut self, …)`                      | Inspect or replace runtime config.                                                       |


### Examples

Full buffer and WAV:

```rust
use mirae_tts_engine::{encode_wav_vec, TtsConfig, TtsEngine};

fn main() -> std::io::Result<()> {
    let engine = TtsEngine::new("./Voice", TtsConfig::default())?;
    let pcm = engine.synthesize("안녕하십니까?")?;
    let wav = encode_wav_vec(&pcm, engine.effective_sample_rate())?;
    std::fs::write("out.wav", wav)?;
    Ok(())
}
```

Streaming chunks (e.g. lower latency or incremental I/O):

```rust
use mirae_tts_engine::{pcm_i16le_to_bytes, TtsConfig, TtsEngine};

fn main() -> std::io::Result<()> {
    let engine = TtsEngine::new("./Voice", TtsConfig::default())?;
    let mut all = Vec::new();
    engine.synthesize_streaming("안녕하십니까?", |chunk| {
        all.extend_from_slice(&pcm_i16le_to_bytes(&chunk));
        true
    })?;
    Ok(())
}
```

