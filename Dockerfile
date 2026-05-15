# syntax=docker/dockerfile:1

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
COPY tools ./tools
COPY apps ./apps

RUN cargo build --release -p lua_dag_smoke --bin lua-dag-smoke

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends libstdc++6 ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --uid 1000 node

COPY --from=builder /build/target/release/lua-dag-smoke /usr/local/bin/lua-dag-smoke

ENV STORAGE_PATH=/data/rocksdb
USER node
WORKDIR /home/node

ENTRYPOINT ["/usr/local/bin/lua-dag-smoke"]
