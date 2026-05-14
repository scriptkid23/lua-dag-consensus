# Docker local devnet (Phase A smoke) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a **`Dockerfile` (multi-stage)** and **`docker-compose.yml`** that build and run **four isolated dev containers** each opening **RocksDB** at **`/data/rocksdb`** on a persisted host bind mount — **without requiring `apps/node`**. Phase B swaps the runtime binary for **`node`** per `docs/superpowers/plans/2026-05-12-06-node-binary.md`. Spec: `docs/superpowers/specs/2026-05-14-docker-node-dev-design.md`.

**Architecture:** A workspace member **`lua_dag_smoke`** ships a **`[[bin]]`** that dependency-links **`storage`**, opens `Database::open` from **`STORAGE_PATH`** (default `/data/rocksdb`), prints one line on **stderr**, then **`std::thread::park`** so the container stays up for Compose. Docker **builder** uses **`rust:1.88-bookworm`** + distro packages sufficient for **`librocksdb-sys`**/`bindgen`. **Runtime** **`debian:bookworm-slim`** + **`libstdc++6`** + **non-root** user **`node`** (uid **1000**). Compose publishes **distinct host ports** per replica for future P2P; Phase A binds **placeholder listens** documented in **`docker/README.md`** only — no real libp2p yet.

**Tech Stack:** **Docker Compose v2**, **Rust 1.88**, Debian **bookworm**, **clang/llvm/cmake** (builder), crates **`storage`**, **`tempfile`** (unused in Phase A smoke if we skip temp tests — omitted from smoke binary deps).

---

## File structure map

```
tools/lua_dag_smoke/
├── Cargo.toml
└── src/
    └── main.rs
Cargo.toml                              # workspace members += tools/lua_dag_smoke
.dockerignore                           # excludes target/, devnet-data/, .git/, large junk
Dockerfile                               # repo root multi-stage build
docker-compose.yml                       # services node0–node3
docker/README.md                         # bootstrap/ports/secrets placeholders + Phase B hints
.gitignore                               # lines for devnet-data/
```

Phase B edits (not executed in Phase A checklist): Dockerfile **`cargo build --release -p node`**, copy **`node`**, Compose **`command`/env wiring** documented at end.

---

### Task 1: Workspace crate `lua_dag_smoke` (RocksDB open + park)

**Files:**

- Create: `tools/lua_dag_smoke/Cargo.toml`
- Create: `tools/lua_dag_smoke/src/main.rs`
- Modify: `Cargo.toml` (root workspace `members` array append)

- [ ] **Step 1: Append workspace member**

In root `Cargo.toml`, extend `members`:

```toml
members = [
    "crates/types",
    "crates/crypto",
    "crates/consensus",
    "crates/net",
    "crates/storage",
    "tools/lua_dag_smoke",
]
```

- [ ] **Step 2: Write `tools/lua_dag_smoke/Cargo.toml`**

```toml
[package]
name         = "lua_dag_smoke"
version      = "0.1.0"
edition.workspace      = true
rust-version.workspace = true
license.workspace      = true
publish.workspace      = true
repository.workspace   = true
authors.workspace      = true

[[bin]]
name = "lua-dag-smoke"
path = "src/main.rs"

[lints]
workspace = true

[dependencies]
storage = { path = "../../crates/storage" }
```

- [ ] **Step 3: Write `tools/lua_dag_smoke/src/main.rs`**

```rust
//! Dev-only binary: proves RocksDB + `storage::Database::open` in Linux containers.

use std::{path::PathBuf, thread, time::Duration};

fn main() {
    let path = env_path();
    let cfg = storage::StorageConfig {
        path,
        create_if_missing: true,
        max_total_wal_size_mb: 256,
    };

    match storage::Database::open(&cfg) {
        Ok(_) => {
            eprintln!(
                "lua-dag-smoke: opened RocksDB at {}",
                cfg.path.display()
            );
        }
        Err(e) => {
            eprintln!("lua-dag-smoke: FATAL {:?}", e);
            std::process::exit(1);
        }
    }

    // Stay alive indefinitely for Docker Compose smoke (SIGTERM terminates the process).
    loop {
        thread::sleep(Duration::from_secs(86400));
    }
}

fn env_path() -> PathBuf {
    let key = "STORAGE_PATH";
    match std::env::var_os(key) {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        Some(_) => PathBuf::from("/data/rocksdb"),
        None => PathBuf::from("/data/rocksdb"),
    }
}
```

- [ ] **Step 4: Build smoke binary locally (Linux parity check or WSL/GitHub)**

Run:

```bash
cargo build -p lua_dag_smoke --bin lua-dag-smoke
```

Expected: **PASS** (`Finished dev` …). On Windows MSVC without full RocksDB toolchain this may fail — that is acceptable; primary target is Docker Linux build.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml tools/lua_dag_smoke/
git commit -m "feat(smoke): add lua_dag_smoke binary for Docker RocksDB sanity"
```

---

### Task 2: `.dockerignore` for fast context

**Files:**

- Create: `.dockerignore`

- [ ] **Step 1: Write `.dockerignore`**

```
target
devnet-data
.git
.github
**/target
.idea
.vscode
*.md
!docker/README.md
```

- [ ] **Step 2: Commit**

```bash
git add .dockerignore
git commit -m "chore(docker): add dockerignore trimming build context"
```

---

### Task 3: Multi-stage **`Dockerfile`**

**Files:**

- Create: `Dockerfile`

- [ ] **Step 1: Write `Dockerfile`**

```dockerfile
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
```

- [ ] **Step 2: Build**

Run:

```bash
docker build -t lua-dag-smoke:test .
```

Expected: **`Successfully tagged lua-dag-smoke:test`** (first run may take ~10–20 minutes compiling RocksDB). If linker errors list missing **`libfoo.so`** at **`RUN ldd /usr/local/bin/lua-dag-smoke`** in an interactive debug shell, augment **runtime `apt-get install`** with the Debian packages providing those SONAMEs (**document each addition inline in commit message**, not placeholders).

- [ ] **Step 3: Smoke-run single container**

Run:

```bash
docker run --rm -e STORAGE_PATH=/data/rocksdb -v lua-smokeVol:/data lua-dag-smoke:test 2>&1 | head -n 1
```

Expected: **`lua-dag-smoke: opened RocksDB at`** + path. **`Ctrl+C`** or **`docker kill`** terminates.

- [ ] **Step 4: Commit**

```bash
git add Dockerfile
git commit -m "feat(docker): phase-a Dockerfile for lua-dag-smoke RocksDB sanity"
```

---

### Task 4: **`docker-compose.yml`** × 4 replicas

**Files:**

- Create: `docker-compose.yml`

- [ ] **Step 1: Write `docker-compose.yml`**

Use a YAML anchor so every service shares **build/image/env** without deprecated **`extends`** quirks across Compose versions:

```yaml
x-dev-smoke: &dev-smoke
  build:
    context: .
    dockerfile: Dockerfile
  image: lua-dag-dev:latest
  environment:
    STORAGE_PATH: /data/rocksdb

services:
  node0:
    <<: *dev-smoke
    volumes:
      - ./devnet-data/node0:/data
    ports:
      - "40000:40000"
      - "40001:40001"
      - "40002:40002"
    hostname: node0

  node1:
    <<: *dev-smoke
    volumes:
      - ./devnet-data/node1:/data
    ports:
      - "40100:40000"
      - "40101:40001"
      - "40102:40002"
    hostname: node1

  node2:
    <<: *dev-smoke
    volumes:
      - ./devnet-data/node2:/data
    ports:
      - "40200:40000"
      - "40201:40001"
      - "40202:40002"
    hostname: node2

  node3:
    <<: *dev-smoke
    volumes:
      - ./devnet-data/node3:/data
    ports:
      - "40300:40000"
      - "40301:40001"
      - "40302:40002"
    hostname: node3
```

**Note:** Placeholder **`40000–40002`** map nowhere for **`lua-dag-smoke`** (no sockets yet) until **`apps/node`** occupies them.

- [ ] **Step 2: Gitignore **`devnet-data/`**

Append to `.gitignore` (top or bottom):

```
devnet-data/
```

Then:

```bash
git add docker-compose.yml .gitignore
git commit -m "feat(docker): compose devnet skeleton with per-node volumes"
```

- [ ] **Step 3: Compose up sanity**

Run:

```bash
docker compose up --build -d && sleep 10 && docker compose ps
```

Expected: **`node0`**–**`node3`** state **running**/`healthy` (Compose v2 prints **running**).

```bash
docker compose logs node0 --tail 3
```

Expected: **`lua-dag-smoke: opened RocksDB`** line.

Cleanup:

```bash
docker compose down
```

(Host directories **`./devnet-data/*/rocksdb`** internals appear after first run.)

---

### Task 5: **`docker/README.md` developer doc**

**Files:**

- Create: `docker/README.md`

- [ ] **Step 1: Write `docker/README.md`**

```markdown
# Docker local devnet (Phase A)

## What runs

Compose starts **four** containers from `Dockerfile` running **`lua-dag-smoke`**: each opens RocksDB under env **`STORAGE_PATH=/data/rocksdb`** (bind-mounted to `./devnet-data/nodeN`). This validates **Linux build + RocksDB linkage** independent of **`apps/node`**.

## Prerequisites

- Docker Compose v2
- Repo root **`docker-compose.yml`** and **`Dockerfile`**

## Commands

```bash
docker compose up --build
```

**Wiping state:** Delete **`./devnet-data/`** entirely or **`docker compose down -v`** (only removes Compose-defined volumes — our bind mounts remain on disk unless you delete dirs manually).

## Bootstrap / listeners (future)

Containers resolve each other via Compose DNS (**`node0`**, **`node1`**, …).

Host port mapping placeholders:

| Replica | Published base block |
|---------|-----------------------|
| node0   | 40000–40002 → 40000–40002 |
| node1   | 40100–40102 → 40000–40002 |
| node2   | 40200–40202 … |
| node3   | 40300–40302 … |

Wire real libp2p multiaddrs inside **`BOOTSTRAP_PEERS`** env when **`apps/node`** exists (Phase B).

## Secrets

Never commit genesis keys — use **`*.example`** only.

## Phase B checklist

Once **`apps/node`** ships per `2026-05-12-06-node-binary.md`:

1. Add **`apps/node`** to workspace `members`.
2. Replace **`cargo build -p lua_dag_smoke`** builder line with **`cargo build --release -p node`**.
3. **`COPY`** **`target/release/node`** to `/usr/local/bin/node`; adjust **`ENTRYPOINT`**.
4. Adjust **`docker-compose.yml`** service blocks (or anchor) to run **`node`**; pass **`BOOTSTRAP_PEERS`** / **`RUST_LOG`**.

```

- [ ] **Step 2: Commit**

```bash
git add docker/README.md
git commit -m "docs(docker): explain phase-a compose and phase-b wiring"
```

---

### Task 6: Optional CI — **`docker/build-smoke`** workflow

**Files:**

- Create: `.github/workflows/docker-smoke.yml`

- [ ] **Step 1: Write `.github/workflows/docker-smoke.yml`**

```yaml
name: Docker build smoke

on:
  pull_request:
  push:
    branches: [main]

jobs:
  build:
    runs-on: ubuntu-latest
    timeout-minutes: 60
    steps:
      - uses: actions/checkout@v4

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v3

      - name: Build Phase A image
        uses: docker/build-push-action@v6
        with:
          context: .
          tags: lua-dag-smoke:ci
          load: true
          cache-from: type=gha
          cache-to: type=gha,mode=max

      - name: Run container sanity (exit if binary fails)
        run: |
          set -euo pipefail
          cid=$(docker run -d -e STORAGE_PATH=/data/rocksdb -v "$RUNNER_TEMP/rock:/data" lua-dag-smoke:ci)
          sleep 5
          docker logs "$cid" 2>&1 | grep -q 'opened RocksDB' || { docker logs "$cid"; exit 1; }
          docker rm -f "$cid" >/dev/null
```

Adjust **`docker-compose`** omission here intentionally — single-container faster on free CI minutes.

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/docker-smoke.yml
git commit -m "ci(docker): build Phase A docker image smoke"
```

---

### Task 7: Final verification + meta commit guard

**Files:** none  

- [ ] **Step 1:** From clean tree run **`docker compose up --build -d`** (Task 4) again.

- [ ] **Step 2:** If optional CI added, **`act`**/`push` verifies workflow green.

- [ ] **Step 3 (optional rollup commit)**

If granular commits undesirable, **`git squash`** — otherwise skip.

---

## Self-review checklist

**Spec coverage:**

| Area (spec §) | Implemented by Task |
|---|---|
| Goal – Linux multi-stage Dockerfile | Task 3 |
| Goal – compose dev replicas | Task 4 |
| Fixed `/data/rocksdb` path | `STORAGE_PATH` default + smoke main |
| Placeholder when no `node` | `lua_dag_smoke` + README Phase B |
| Non-root runtime | Task 3 `USER node` |
| Secrets never in compose | README only |
| CI sketch optional | Task 6 |
| Phase B handoff | README + plan tail |

**Placeholder scan:** None — each step names concrete files / commands.

**Type consistency:** Single env var **`STORAGE_PATH`** end-to-end; ports table matches compose.

**Known gap:** **`depends_on`** ordering omitted for smoke; add when **`node`** bootstraps require peer startup ordering.

---

## Execution hand-off

Plan complete — saved **`docs/superpowers/plans/2026-05-14-docker-devnet-phase-a.md`**.

Execution options:

1. **Subagent-driven (recommended)** — fresh subagent per task, checkpoint reviews.
2. **Inline execution** — run tasks sequentially in-session with checkpoints.

Tell me **`1`** or **`2`** when starting implementation after plan approval.

