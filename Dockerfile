# syntax=docker/dockerfile:1
#
# Multi-stage build for the prod-like devnet (spec §6).
# - Builder uses rust:1.88 (matching `rust-toolchain.toml`).
# - Runtime is `debian:bookworm-slim` with a non-root user.
# - `HEALTHCHECK` calls `node --health-probe`, an in-binary probe that
#   curls `127.0.0.1:9100/readyz`. No `curl` required in the runtime image.

FROM rust:1.88-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        clang \
        libclang-dev \
        llvm-dev \
        cmake \
        ninja-build \
        pkg-config \
        libssl-dev \
        build-essential \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY apps ./apps
COPY config ./config
COPY rust-toolchain.toml ./
COPY rustfmt.toml ./
COPY clippy.toml ./
COPY deny.toml ./

RUN cargo build --release -p node --bin node

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends libstdc++6 ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --create-home --shell /usr/sbin/nologin node \
 && mkdir -p /data/rocksdb && chown node:node /data/rocksdb

USER node
WORKDIR /home/node

COPY --from=builder --chown=node:node /build/target/release/node /usr/local/bin/node
COPY --from=builder --chown=node:node /build/config /home/node/config

EXPOSE 9000 9100 9200

ENV STORAGE_PATH=/data/rocksdb

HEALTHCHECK --interval=5s --timeout=2s --retries=12 \
    CMD ["/usr/local/bin/node", "--health-probe"]

ENTRYPOINT ["/usr/local/bin/node"]
CMD ["--profile", "devnet", "--config-dir", "/home/node/config", "--admin-listen", "0.0.0.0:9100", "--rpc-listen", "0.0.0.0:9200"]
