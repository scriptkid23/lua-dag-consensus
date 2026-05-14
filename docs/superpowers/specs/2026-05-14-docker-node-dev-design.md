# Design: Docker-based local development node (`lua-dag`)

**Date:** 2026-05-14  
**Status:** Draft approved for specification (brainstorm rounds 1–3)  
**Audience:** Agents and contributors running a multi-node devnet locally.

## 1. Goals and non-goals

### Goals

- Provide a **`Dockerfile` (multi-stage)** that builds the project’s **`node`** binary inside Linux, including **native dependencies required by `rocksdb` / `storage`** (e.g. `clang`, `llvm-dev`, C++ toolchain), avoiding Windows-specific toolchain pain for day-to-day dev.
- Provide a **`docker-compose.yml`** geared at **scenario A — local development**: multiple containers on a shared bridge network, isolated data dirs, simple bootstrap addressing between services.
- Use a **fixed in-container RocksDB root** (e.g. `/data/rocksdb`) so persistence does not depend on process **`cwd`** ambiguity.
- Document **explicit placeholders** if the **`apps/node` binary (per plan `2026-05-12-06-node-binary.md`) does not yet exist** in the tree.

### Non-goals

- Production hardening (rate limits, full observability stack, distroless minimal attack surface audits).
- Guaranteed **bit-identical** runs across hosts (reserved for **`sim`**; see plan `2026-05-12-07-sim-binary.md`).
- One-click mainnet deployment or key ceremony automation.

## 2. Repo context

- **`Dockerfile*`** is not currently present under the repository root searched at spec time.
- **`apps/`** tree **may not** be present yet; **node orchestration depends on introducing the `node` package/binary** matching `docs/superpowers/plans/2026-05-12-06-node-binary.md`.
- **`sim`** deliberately avoids RocksDB/network; Docker node path is orthogonal and should **not** pull `sim` into the runtime image.

## 3. Architectural approach

### Recommended: multi-stage build (primary)

**Builder stage**

- Base: Debian- or Ubuntu-based image with **`build-essential`**, **`clang`**, **`libclang-dev`**, **`cmake`**, **`pkg-config`** (exact list refined at implementation).
- Install **Rust toolchain** matching workspace **`rust-version`** (currently **1.88**).
- `cargo build --release -p node` once the package exists.

**Runtime stage**

- Copy **released `node`** binary + required shared libs OR fully static linkage if the linkage story allows (implementation resolves).
- **Non-root** user `node`, **`WORKDIR`** explicit, **`USER node`**.

### Optional profile: bind-mount source + `cargo run` (later)

For rapid edit cycles, provide an **alternate compose override** (separate YAML) that mounts the workspace and caches `registry`/`target` in Docker volumes; not required for the first deliverable.

## 4. `docker-compose` layout (development)

### Topology

- **Services:** `node0`, `node1`, … (default **four** replicas; configurable via Compose scale or YAML duplication per team taste).
- **Network:** Default Compose bridge; inter-container addressing via **DNS names** (`node0`, `node1`).
- **Bootstrap peers:** Pass **internal** multiaddrs (not `127.0.0.1` from another container’s viewpoint), via environment (`BOOTSTRAP_PEERS`) or mounted config templating.

### Ports

- **Publish** overlapping P2P / RPC ports to the host with **offsets** (`base + index * stride`) when multiple nodes expose to localhost.
- Exact port map follows **`net`** / **`node`** configuration once stabilized; compose should stay a **thin wrapper**.

### Persistence

- **Container path:** e.g. `STORAGE_PATH=/data/rocksdb` aligning with **`StorageConfig.path`** wired by `node` startup.
- **Host mapping:** Prefer **`./devnet-data/<id>/` bind mounts** for easy inspection; **`gitignore`** the directory **and** `.dockerignore` bulky artifacts appropriately.
- **Named volumes** alternative documented for teammates who prefer not to clutter the working tree (`docker compose down -v` wipes state — call out explicitly).

## 5. Security and operational notes (minimal)

### Users and privileges

Run as **non-root** in the runtime image. If bind mounts cause permission friction on Windows/WSL/macOS Docker Desktop, capture **explicit troubleshooting** entries (chmod/uid mapping). Avoid defaulting compose to `--privileged`.

### Secrets

- **Never** commit real keys into `docker-compose.yml`.
- Provide **examples only** (`*.example`, `secrets/README`) and keep real material **gitignored** or inject via `.env.local` omitted from git.

### Healthchecks

- If `node` exposes HTTP metrics/admin, add **`healthcheck`** with `wget`/`curl`.
- Pure P2P without HTTP: **omit** deceptive healthchecks; optional **process-alive** probes only if valuable for CI smoke.

### CI sketch (optional)

- **`docker build`** on default branch pipelines.
- Lightweight smoke: **`docker compose up --abort-on-container-exit`** with deterministic env or **`node --version`** only until networking is deterministic in CI.

## 6. Phased rollout

### Phase A — scaffolding (no runnable consensus yet acceptable)

1. Dockerfile multi-stage compiling workspace default binary **or stub** gated behind feature flags.
2. `docker-compose.yml` with placeholders + documented env contract.
3. `.gitignore` / `.dockerignore` entries for `devnet-data/`.

### Phase B — wire real `node`

1. Align `Cargo` workspace membership for **`apps/node`** once implemented per plan **`2026-05-12-06-node-binary.md`**.
2. Replace placeholders with **`CMD` invoking `node` with flags** wired to Compose env volumes.

## 7. Acceptance criteria

- From a clean Linux CI runner**, `docker build` succeeds** once `node` target exists **or** the agreed stub builds.
- `docker compose up` starts **N** replicas with distinct writable data dirs **without crashing** due to RocksDB/native deps.
- Developer documentation (short **README snippet** pointer from repo root acceptable in implementation task) explains **bootstrap addressing**, **port mapping**, and **data wipes**.

## 8. Self-review (spec quality)

| Check            | Result |
|------------------|--------|
| Placeholder scan | Repo state (missing `apps/node`) called out intentionally; phased rollout resolves ambiguity |
| Internal consistency | `sim`/Rocks separation preserved; persistence path anchored in-container |
| Scope            | Dev-only Docker; CI/deeper ops explicitly optional |
| Ambiguity        | Bootstrap format defers to `node`/net config implementation phase |
