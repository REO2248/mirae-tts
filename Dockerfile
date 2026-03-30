FROM rust:1.91-slim AS builder
WORKDIR /usr/src/mirae-tts

COPY rust-toolchain.toml ./
RUN rustup show \
 && rustup target add x86_64-unknown-linux-musl

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
    pkg-config \
    musl-tools \
    git \
    ca-certificates \
 && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY mirae-tts-engine/Cargo.toml mirae-tts-engine/
COPY mirae-tts-server/Cargo.toml mirae-tts-server/
COPY mirae-tts-cli/Cargo.toml mirae-tts-cli/
RUN mkdir -p mirae-tts-engine/src mirae-tts-server/src mirae-tts-cli/src \
 && printf '// docker deps cache\n' > mirae-tts-engine/src/lib.rs \
 && printf 'fn main() {}\n' > mirae-tts-server/src/main.rs \
   && printf 'fn main() {}\n' > mirae-tts-cli/src/main.rs \
 && cargo build --release -p mirae-tts-server --target x86_64-unknown-linux-musl

COPY mirae-tts-engine/src/ mirae-tts-engine/src/
COPY mirae-tts-server/src/ mirae-tts-server/src/
COPY mirae-tts-server/assets/ mirae-tts-server/assets/
RUN touch mirae-tts-engine/src/lib.rs mirae-tts-server/src/main.rs \
 && cargo build --release --target x86_64-unknown-linux-musl -p mirae-tts-server

FROM scratch AS runtime

COPY ./Voice /data/Voice
COPY --from=builder /usr/src/mirae-tts/target/x86_64-unknown-linux-musl/release/mirae-tts-server /usr/local/bin/mirae-tts-server

ENV LISTEN=0.0.0.0:3000
ENV DIC=/data/Voice
ENV MAXIMUM_LENGTH=0

EXPOSE 3000
VOLUME ["/data/Voice"]

ENTRYPOINT ["/usr/local/bin/mirae-tts-server"]
