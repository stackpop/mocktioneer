ARG RUST_VERSION=1.91.1
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

COPY crates ./crates
COPY edgezero.toml ./edgezero.toml

RUN cargo fetch --locked
RUN cargo build --locked --release -p mocktioneer-adapter-axum

FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
  && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --uid 10001 appuser

COPY --from=builder /app/target/release/mocktioneer-adapter-axum /usr/local/bin/mocktioneer-adapter-axum

USER appuser

EXPOSE 8787

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:8787/ || exit 1

ENTRYPOINT ["/usr/local/bin/mocktioneer-adapter-axum"]
