FROM rust:1.97.1-trixie AS build-env
LABEL maintainer="yanorei32"

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

WORKDIR /usr/src/mirae-tts

COPY LICENSE Cargo.toml Cargo.lock ./

COPY mirae-tts-engine/Cargo.toml mirae-tts-engine/
COPY mirae-tts-server/Cargo.toml mirae-tts-server/
COPY mirae-tts-cli/Cargo.toml mirae-tts-cli/

RUN mkdir -p mirae-tts-engine/src mirae-tts-server/src mirae-tts-cli/src \
	&& printf '// docker deps cache\n' > mirae-tts-engine/src/lib.rs \
	&& printf 'fn main() {}\n' > mirae-tts-server/src/main.rs \
	&& printf 'fn main() {}\n' > mirae-tts-cli/src/main.rs \
	&& cargo build --release -p mirae-tts-server

RUN	cargo install cargo-license && cargo license \
	--authors \
	--do-not-bundle \
	--avoid-dev-deps \
	--avoid-build-deps \
	--filter-platform "$(rustc -vV | sed -n 's|host: ||p')" \
	> CREDITS

COPY mirae-tts-engine/src/ mirae-tts-engine/src/
COPY mirae-tts-server/src/ mirae-tts-server/src/
COPY mirae-tts-server/assets/ mirae-tts-server/assets/

RUN touch mirae-tts-engine/src/lib.rs mirae-tts-server/src/main.rs \
	&& cargo build --release -p mirae-tts-server

FROM debian:trixie-slim

WORKDIR /

COPY --chown=root:root ./Voice /var/mirae-tts/Voice

COPY --chown=root:root --from=build-env \
	/usr/src/mirae-tts/CREDITS \
	/usr/src/mirae-tts/LICENSE \
	/usr/share/licenses/mirae-tts/

COPY --chown=root:root --from=build-env \
	/usr/src/mirae-tts/target/release/mirae-tts-server \
	/usr/bin/mirae-tts-server

CMD ["/usr/bin/mirae-tts-server"]
