# mirae-tts: static musl `tts_server` + slim Alpine runtime (non-root, healthcheck).
# Rust compiler version: `rust-toolchain.toml` (downloaded via rustup on first `rustup show`).

FROM rust:1.91-slim AS builder
WORKDIR /usr/src/mirae-tts

COPY rust-toolchain.toml ./
RUN rustup show

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
    pkg-config \
    musl-tools \
    git \
    ca-certificates \
 && rm -rf /var/lib/apt/lists/*

RUN rustup target add x86_64-unknown-linux-musl

# Dependency layer (default features only — skips `tts_server` bin)
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src/bin \
 && printf '// docker deps cache\n' > src/lib.rs \
 && printf 'fn main() {}\n' > src/main.rs \
 && cargo build --release --target x86_64-unknown-linux-musl

COPY . .
RUN touch src/lib.rs src/main.rs src/bin/tts_server.rs \
 && cargo build --release --target x86_64-unknown-linux-musl --bin tts_server --features web

FROM alpine:3.21 AS runtime
RUN apk add --no-cache ca-certificates

RUN adduser -D -u 1000 app

COPY --from=builder /usr/src/mirae-tts/target/x86_64-unknown-linux-musl/release/tts_server /usr/local/bin/tts_server
RUN chown app:app /usr/local/bin/tts_server && chmod +x /usr/local/bin/tts_server

USER app
WORKDIR /home/app

ENV LISTEN=0.0.0.0:3000
ENV DIC=/data/Voice
ENV MAXIMUM_LENGTH=0

EXPOSE 3000
VOLUME ["/data/Voice"]

LABEL org.opencontainers.image.title="mirae-tts"
LABEL org.opencontainers.image.description="미래 2.0 TTS HTTP server (tts_server)"

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD wget -qO- http://127.0.0.1:3000/ >/dev/null || exit 1

ENTRYPOINT ["/usr/local/bin/tts_server"]
