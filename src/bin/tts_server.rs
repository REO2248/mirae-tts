//! 미래 2.0 HTTP API. `GET /` = bundled UI. WAV / stream (`audio/l16`) / raw PCM; POST JSON `{"text":"…"}` or GET `?text=`.
//! `Arc<TtsEngine>` only — VoiceData is pread/`Send+Sync`, no mutex on synthesize.

use std::sync::Arc;
use std::time::Instant;

use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, MethodRouter},
    Json, Router,
};
use bytes::Bytes;
use clap::Parser;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tower_http::cors::CorsLayer;
use tracing::info;

use mirae_tts::{encode_wav_vec, pcm_i16le_to_bytes, TtsConfig, TtsEngine};

#[derive(Parser)]
#[command(name = "tts_server")]
#[command(about = "미래 2.0 TTS — Web API server")]
struct Cli {
    /// Socket address to bind to.
    #[arg(long, env = "LISTEN", default_value = "0.0.0.0:3000")]
    listen: String,

    /// Path to the dictionary directory (VoiceInfo.pkg, VoiceData.pkg, …).
    #[arg(long = "dic", env = "DIC", default_value = "./Voice")]
    dic: String,

    /// Maximum length of text to synthesize (Unicode scalar count; 0 = unlimited).
    #[arg(long = "maximum-length", env = "MAXIMUM_LENGTH", default_value_t = 0)]
    maximum_length: usize,
}

#[derive(Deserialize)]
struct SynthRequest {
    text: String,
}

#[derive(Deserialize)]
struct SynthQuery {
    text: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

struct AppState {
    engine: TtsEngine,
    /// `maximum-length` 0 = unlimited (Unicode scalars).
    maximum_length: usize,
}

fn json_err(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(ErrorResponse { error: msg.into() })).into_response()
}

fn validate_text(state: &AppState, text: &str) -> Result<(), Box<Response>> {
    if text.trim().is_empty() {
        return Err(Box::new(json_err(
            StatusCode::BAD_REQUEST,
            "text is empty or whitespace only",
        )));
    }
    let n = text.chars().count();
    if state.maximum_length > 0 && n > state.maximum_length {
        return Err(Box::new(json_err(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("text too long: {n} chars (max {})", state.maximum_length),
        )));
    }
    Ok(())
}

fn map_join_io<T>(
    result: Result<Result<T, std::io::Error>, tokio::task::JoinError>,
    ok: impl FnOnce(T) -> Response,
) -> Response {
    match result {
        Ok(Ok(v)) => ok(v),
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "synthesis failed");
            json_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("synthesis failed: {e}"),
            )
        }
        Err(e) => {
            tracing::error!(error = %e, "synthesis task panic");
            json_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("task panic: {e}"),
            )
        }
    }
}

async fn synthesize_wav(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SynthRequest>,
) -> impl IntoResponse {
    synthesize_wav_impl(state, req.text).await
}

async fn synthesize_wav_get(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SynthQuery>,
) -> impl IntoResponse {
    synthesize_wav_impl(state, query.text).await
}

async fn synthesize_wav_impl(state: Arc<AppState>, text: String) -> Response {
    if let Err(r) = validate_text(&state, &text) {
        return *r;
    }

    let engine = Arc::clone(&state);
    let started = Instant::now();
    let result = tokio::task::spawn_blocking(move || {
        let pcm = engine.engine.synthesize(&text)?;
        let rate = engine.engine.effective_sample_rate();
        encode_wav_vec(&pcm, rate)
    })
    .await;

    if let Ok(Ok(_)) = &result {
        info!("Synthesis: {:?}", started.elapsed());
    }

    map_join_io(result, |wav_bytes| {
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "audio/wav")],
            wav_bytes,
        )
            .into_response()
    })
}

async fn synthesize_raw(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SynthRequest>,
) -> impl IntoResponse {
    synthesize_raw_impl(state, req.text).await
}

async fn synthesize_raw_get(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SynthQuery>,
) -> impl IntoResponse {
    synthesize_raw_impl(state, query.text).await
}

async fn synthesize_raw_impl(state: Arc<AppState>, text: String) -> Response {
    if let Err(r) = validate_text(&state, &text) {
        return *r;
    }

    let engine = Arc::clone(&state);
    let started = Instant::now();
    let result = tokio::task::spawn_blocking(move || engine.engine.synthesize(&text)).await;

    if let Ok(Ok(_)) = &result {
        info!("Synthesis: {:?}", started.elapsed());
    }

    map_join_io(result, |pcm| {
        (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/octet-stream"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"synth.pcm\"",
                ),
            ],
            pcm_i16le_to_bytes(&pcm),
        )
            .into_response()
    })
}

async fn synthesize_stream_get(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SynthQuery>,
) -> impl IntoResponse {
    synthesize_stream_impl(state, query.text).await
}

async fn synthesize_stream(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SynthRequest>,
) -> impl IntoResponse {
    synthesize_stream_impl(state, req.text).await
}

async fn synthesize_stream_impl(state: Arc<AppState>, text: String) -> Response {
    if let Err(r) = validate_text(&state, &text) {
        return *r;
    }

    let sample_rate = state.engine.effective_sample_rate();
    let content_type = format!("audio/l16; rate={sample_rate}; channels=1");
    let (tx, rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(8);

    tokio::task::spawn_blocking(move || {
        let started = Instant::now();
        let result = state.engine.synthesize_streaming(&text, |chunk| {
            let bytes = pcm_i16le_to_bytes(&chunk);
            tx.blocking_send(Ok(Bytes::from(bytes))).is_ok()
        });
        info!("Synthesis: {:?}", started.elapsed());
        if let Err(e) = result {
            let _ = tx.blocking_send(Err(e));
        }
    });

    let body = Body::from_stream(ReceiverStream::new(rx));
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&content_type).expect("audio/l16 rate header"),
        )
        .body(body)
        .expect("response build")
        .into_response()
}

async fn index(State(_): State<Arc<AppState>>) -> Html<&'static str> {
    Html(include_str!("../../assets/index.html"))
}

fn routes() -> MethodRouter<Arc<AppState>> {
    get(synthesize_wav_get).post(synthesize_wav)
}

fn routes_stream() -> MethodRouter<Arc<AppState>> {
    get(synthesize_stream_get).post(synthesize_stream)
}

fn routes_raw() -> MethodRouter<Arc<AppState>> {
    get(synthesize_raw_get).post(synthesize_raw)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let cli = Cli::parse();
    info!("Loading voice data from {:?}...", cli.dic);

    let engine = TtsEngine::new(
        &cli.dic,
        TtsConfig {
            log_progress: true,
            ..Default::default()
        },
    )
    .expect("Failed to initialize TTS engine");
    info!(
        "Engine ready — {} voice entries loaded",
        engine.voice_entry_count()
    );

    let state = Arc::new(AppState {
        engine,
        maximum_length: cli.maximum_length,
    });

    let mut app = Router::new().route("/", get(index));
    for path in ["/synthesize", "/synthesize/wav", "/synth.wav"] {
        app = app.route(path, routes());
    }
    for path in ["/synthesize/stream", "/stream.pcm"] {
        app = app.route(path, routes_stream());
    }
    for path in ["/synthesize/raw", "/synth.pcm"] {
        app = app.route(path, routes_raw());
    }

    let app = app.layer(CorsLayer::permissive()).with_state(state);

    let listener = tokio::net::TcpListener::bind(&cli.listen)
        .await
        .expect("Failed to bind");
    info!(
        "Listening on: {}",
        listener.local_addr().expect("local_addr")
    );
    axum::serve(listener, app).await.expect("Server error");
}
