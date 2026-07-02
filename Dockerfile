ARG RUST_VERSION=1.95.0
FROM rust:${RUST_VERSION}-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libssl-dev \
    ca-certificates \
    git \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY .cargo ./.cargo
COPY crates/mocktioneer-core/Cargo.toml crates/mocktioneer-core/Cargo.toml
COPY crates/mocktioneer-adapter-axum/Cargo.toml crates/mocktioneer-adapter-axum/Cargo.toml
COPY crates/mocktioneer-adapter-cloudflare/Cargo.toml crates/mocktioneer-adapter-cloudflare/Cargo.toml
COPY crates/mocktioneer-adapter-fastly/Cargo.toml crates/mocktioneer-adapter-fastly/Cargo.toml
COPY crates/mocktioneer-adapter-spin/Cargo.toml crates/mocktioneer-adapter-spin/Cargo.toml
COPY crates/mocktioneer-cli/Cargo.toml crates/mocktioneer-cli/Cargo.toml

COPY crates ./crates
COPY edgezero.toml ./edgezero.toml
# `mocktioneer.toml` is gitignored (per-env); ship the committed template as the
# config the image seeds from.
COPY mocktioneer.toml.example ./mocktioneer.toml

RUN cargo fetch --locked
RUN cargo build --locked --release -p mocktioneer-adapter-axum -p mocktioneer-cli

# Seed the default typed-config blob into `.edgezero/` so the OpenRTB/APS
# endpoints serve out-of-the-box: under edgezero #269 they read `bid_cpm`
# through the fail-loud `AppConfig` extractor and 503 until a blob is pushed.
# (Override at deploy time by mounting your own
# `/app/.edgezero/local-config-mocktioneer_config.json`.)
RUN ./target/release/mocktioneer-cli config push --adapter axum --yes

FROM debian:stable-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
  && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --uid 10001 appuser

WORKDIR /app

COPY --from=builder /app/target/release/mocktioneer-adapter-axum /usr/local/bin/mocktioneer-adapter-axum
# The Axum config store reads `./.edgezero/local-config-<id>.json` relative to
# the working directory, so the seeded blob must sit under the runtime WORKDIR.
COPY --from=builder --chown=10001:10001 /app/.edgezero /app/.edgezero

USER appuser

EXPOSE 8787

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:8787/ || exit 1

ENTRYPOINT ["/usr/local/bin/mocktioneer-adapter-axum"]
