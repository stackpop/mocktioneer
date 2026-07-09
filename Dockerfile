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

RUN cargo fetch --locked
RUN cargo build --locked --release -p mocktioneer-adapter-axum -p mocktioneer-cli

# `mocktioneer.toml` is gitignored (per-env); ship the committed template as the
# config the image seeds from. Copied here (after the build layers) so that
# editing `bid_cpm` in the template doesn't invalidate the dependency-fetch and
# compile caches — only the cheap `config push` layer below re-runs.
COPY mocktioneer.toml.example ./mocktioneer.toml

# Seed the default typed-config blob into `.edgezero/` so the OpenRTB/APS
# endpoints serve out-of-the-box: under edgezero #269 they read `bid_cpm`
# through the fail-loud `AppConfig` extractor and 503 until a blob is pushed.
# (Override at deploy time by mounting your own
# `/app/.edgezero/local-config-mocktioneer_config.json`.)
RUN ./target/release/mocktioneer-cli config push --adapter axum --yes

# Pin the runtime base to the same Debian release as the builder
# (`rust:1.95.0-slim-bookworm`) so the runtime glibc can't drift out from under
# the build env on a later rebuild.
FROM debian:bookworm-slim AS runtime

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

# Bind to all interfaces so `docker run -p <host>:8787` (and k8s) can reach the
# service — EdgeZero's Axum dev server otherwise defaults to 127.0.0.1:8787,
# which is only reachable from inside the container.
ENV EDGEZERO__ADAPTER__HOST=0.0.0.0 \
    EDGEZERO__ADAPTER__PORT=8787

EXPOSE 8787

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:8787/ || exit 1

ENTRYPOINT ["/usr/local/bin/mocktioneer-adapter-axum"]
