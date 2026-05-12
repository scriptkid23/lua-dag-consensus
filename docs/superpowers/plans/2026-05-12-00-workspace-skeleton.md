# LUA-DAG Workspace Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the Cargo workspace root (no crates yet) with toolchain pin, lint configs, dependency policy, default protocol params, CI scaffolding, and a green `cargo --version` baseline that subsequent plans (`01`..`08`) can extend without touching workspace files.

**Architecture:** A `[workspace]` Cargo manifest with `resolver = "2"`, empty `members = []` initially (each subsequent plan adds its crate to the list), `[workspace.dependencies]` table for unified version pinning, and `[workspace.lints]` for shared lint policy. Plus repo-wide config files (`rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml`), placeholder dual licenses, ADR folder, default protocol-param TOML (Table 17.1), and a minimal GitHub Actions CI workflow that runs `cargo fmt --check` + `cargo deny check` (no `cargo build` yet — workspace is empty).

**Tech Stack:** Cargo workspace (edition 2024), `rust-toolchain.toml` pinning stable 1.85+, `cargo-deny`, `rustfmt`, `clippy`, GitHub Actions.

---

## File Structure

This plan creates only **workspace-root-level** files. All crate directories (`crates/types/`, `crates/crypto/`, etc.) are created by their respective plans (01–08).

```
lua-dag-consensus/
├── Cargo.toml                         # NEW — [workspace] manifest, members = []
├── rust-toolchain.toml                # NEW — pin stable
├── rustfmt.toml                       # NEW
├── clippy.toml                        # NEW
├── deny.toml                          # NEW — cargo-deny config
├── .gitignore                         # NEW — Rust + workspace ignores
├── LICENSE-APACHE                     # NEW — Apache-2.0 text
├── LICENSE-MIT                        # NEW — MIT text
├── .github/workflows/ci.yml           # NEW — fmt + deny only at this stage
├── docs/architecture/                 # NEW — empty dir + README
│   └── README.md
├── config/                            # NEW — default protocol params
│   ├── default.toml                   # Table 17.1 params
│   └── README.md
└── scripts/                           # NEW — devnet/lint/release helpers
    └── README.md
```

**Not touched here:** `crates/`, `apps/`, `tests/`, `benches/`, `fuzz/` — those directories are created on demand by plans 01–08 and 03 respectively.

---

## Task 1: Workspace Cargo manifest

**Files:**
- Create: `Cargo.toml`

- [ ] **Step 1: Write `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = []

[workspace.package]
edition      = "2024"
rust-version = "1.85"
license      = "Apache-2.0 OR MIT"
publish      = false
repository   = "https://github.com/1hoodlabs/lua-dag-consensus"
authors      = ["LUA-DAG contributors"]

[workspace.dependencies]
# --- core ---
borsh        = { version = "1.5", default-features = false, features = ["derive", "std"] }
serde        = { version = "1", features = ["derive"] }
serde_json   = "1"
toml         = "0.8"
thiserror    = "2"
anyhow       = "1"
hex          = "0.4"
smallvec     = { version = "1", features = ["serde", "union"] }
bytes        = "1"

# --- async / runtime (only used by `net`, `node`) ---
tokio        = { version = "1", features = ["macros", "rt-multi-thread", "sync", "time", "signal"] }
futures      = "0.3"

# --- crypto (only used by `crypto`) ---
blst         = "0.3"
sha2         = "0.10"
blake3       = "1"
curve25519-dalek = "4"

# --- network (only used by `net`, `node`) ---
libp2p       = { version = "0.55", default-features = false, features = ["gossipsub", "quic", "tcp", "tls", "noise", "yamux", "macros", "tokio", "kad", "identify"] }

# --- storage (only used by `storage`, `cli`) ---
rocksdb      = { version = "0.22", default-features = false, features = ["snappy"] }

# --- observability (only used by `node`) ---
tracing            = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
prometheus         = "0.13"

# --- cli (only used by binaries) ---
clap         = { version = "4", features = ["derive"] }

# --- test / dev ---
proptest     = "1"
rand         = "0.8"
rand_chacha  = "0.3"
tempfile     = "3"

[workspace.lints.rust]
unsafe_code            = "forbid"
unreachable_pub        = "warn"
missing_debug_implementations = "warn"

[workspace.lints.clippy]
all      = { level = "deny",  priority = -1 }
pedantic = { level = "warn",  priority = -1 }
# pedantic exceptions we accept
module_name_repetitions = "allow"
missing_errors_doc      = "allow"
missing_panics_doc      = "allow"
must_use_candidate      = "allow"
return_self_not_must_use = "allow"

[profile.release]
opt-level     = 3
lto           = "fat"
codegen-units = 1
strip         = "symbols"
panic         = "abort"

[profile.bench]
opt-level     = 3
lto           = "thin"
debug         = true

[profile.dev]
opt-level = 0
debug     = true

[profile.test]
opt-level = 1
```

- [ ] **Step 2: Verify it parses**

Run: `cargo metadata --format-version 1 --no-deps`
Expected: JSON output starts with `{"packages":[],...` and exit code 0 (members list empty is fine).

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "chore: add Cargo workspace manifest with shared deps and lints"
```

---

## Task 2: Toolchain + formatter + clippy config

**Files:**
- Create: `rust-toolchain.toml`
- Create: `rustfmt.toml`
- Create: `clippy.toml`

- [ ] **Step 1: Write `rust-toolchain.toml`**

```toml
[toolchain]
channel    = "1.85"
components = ["rustfmt", "clippy", "rust-src"]
profile    = "minimal"
```

- [ ] **Step 2: Write `rustfmt.toml`**

```toml
edition          = "2024"
max_width        = 100
hard_tabs        = false
tab_spaces       = 4
newline_style    = "Unix"
use_field_init_shorthand = true
use_try_shorthand        = true
imports_granularity      = "Crate"
group_imports            = "StdExternalCrate"
reorder_imports          = true
```

> Note: `imports_granularity` and `group_imports` are nightly-only formatting options. With stable 1.85 they are ignored silently — keep them so contributors using nightly get consistent output.

- [ ] **Step 3: Write `clippy.toml`**

```toml
msrv = "1.85"
avoid-breaking-exported-api = false
cognitive-complexity-threshold = 30
too-many-arguments-threshold   = 8
type-complexity-threshold      = 250
```

- [ ] **Step 4: Verify**

Run: `cargo fmt --all --check`
Expected: exit 0 (no files to format yet, but config must parse).

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: exit 0 (no crates to lint yet, but config must parse).

- [ ] **Step 5: Commit**

```bash
git add rust-toolchain.toml rustfmt.toml clippy.toml
git commit -m "chore: pin Rust 1.85 stable, add rustfmt and clippy configs"
```

---

## Task 3: Dependency policy (`cargo-deny`)

**Files:**
- Create: `deny.toml`

- [ ] **Step 1: Write `deny.toml`**

```toml
[graph]
all-features    = false
no-default-features = false

[advisories]
db-path     = "~/.cargo/advisory-db"
db-urls     = ["https://github.com/rustsec/advisory-db"]
yanked      = "deny"
ignore      = []

[licenses]
allow = [
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "MIT",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-3.0",
    "Unicode-DFS-2016",
    "MPL-2.0",
    "Zlib",
    "CC0-1.0",
    "OpenSSL",
]
confidence-threshold = 0.93
exceptions = []

[[licenses.clarify]]
name = "ring"
expression = "MIT AND ISC AND OpenSSL"
license-files = [{ path = "LICENSE", hash = 0xbd0eed23 }]

[bans]
multiple-versions       = "warn"
wildcards               = "deny"
highlight               = "all"
workspace-default-features  = "allow"
external-default-features   = "allow"
allow                   = []
deny                    = []
skip                    = []
skip-tree               = []

[sources]
unknown-registry = "deny"
unknown-git      = "deny"
allow-registry   = ["https://github.com/rust-lang/crates.io-index"]
allow-git        = []
```

- [ ] **Step 2: Install and verify `cargo-deny`**

Run: `cargo install --locked cargo-deny --version 0.16.4 2>&1 | tail -3`
Expected: exit 0 (or "already installed").

Run: `cargo deny check`
Expected: All 4 sections (advisories, bans, licenses, sources) pass — workspace has no deps yet so this is essentially a config syntax check.

- [ ] **Step 3: Commit**

```bash
git add deny.toml
git commit -m "chore: add cargo-deny policy for licenses, advisories, sources"
```

---

## Task 4: `.gitignore`

**Files:**
- Create: `.gitignore`

- [ ] **Step 1: Write `.gitignore`**

```gitignore
# Rust build artifacts
/target/
**/*.rs.bk

# Local cargo lock for binaries lives in workspace root — keep it
# (Cargo.lock is committed; rule above only ignores target/)

# RocksDB local data + WAL (devnet, sim runs)
/data/
*.rdb
*.log
*.sst
*.wal

# Editor / OS
.vscode/*
!.vscode/extensions.json
.idea/
*.swp
.DS_Store
Thumbs.db

# Coverage / profiling
*.profraw
/coverage/
/tarpaulin-report.html

# Local env overrides
.env
.env.local
config/local.toml
```

- [ ] **Step 2: Commit**

```bash
git add .gitignore
git commit -m "chore: add .gitignore for Rust artifacts and local data"
```

---

## Task 5: Dual licenses (Apache-2.0 + MIT)

**Files:**
- Create: `LICENSE-APACHE`
- Create: `LICENSE-MIT`

- [ ] **Step 1: Write `LICENSE-APACHE`**

Use the standard Apache-2.0 license text verbatim from <https://www.apache.org/licenses/LICENSE-2.0.txt>. Copy the full text (about 11 KB). Do **not** modify the boilerplate "APPENDIX: How to apply" section. Owner line (last paragraph) is left as `Copyright 2026 LUA-DAG contributors`.

- [ ] **Step 2: Write `LICENSE-MIT`**

```
MIT License

Copyright (c) 2026 LUA-DAG contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

- [ ] **Step 3: Commit**

```bash
git add LICENSE-APACHE LICENSE-MIT
git commit -m "chore: add Apache-2.0 and MIT dual license"
```

---

## Task 6: Architecture docs folder

**Files:**
- Create: `docs/architecture/README.md`

- [ ] **Step 1: Write `docs/architecture/README.md`**

```markdown
# Architecture Decision Records

This folder stores Architecture Decision Records (ADRs) and sequence diagrams
for the LUA-DAG Rust implementation.

The folder architecture spec lives in
[`../superpowers/specs/2026-05-11-folder-architecture-design.md`](../superpowers/specs/2026-05-11-folder-architecture-design.md).

ADRs are numbered sequentially (`0001-...md`, `0002-...md`) and follow the
Michael Nygard template: Context → Decision → Status → Consequences.
```

- [ ] **Step 2: Commit**

```bash
git add docs/architecture/README.md
git commit -m "docs: add architecture/ADR folder placeholder"
```

---

## Task 7: Default protocol parameters (Table 17.1)

**Files:**
- Create: `config/default.toml`
- Create: `config/README.md`

- [ ] **Step 1: Write `config/default.toml`**

Every value is sourced from whitepaper Chapter 17 Table 17.1. Field names use the same identifiers as `crates/consensus/src/config.rs` (defined in plan 03).

```toml
# LUA-DAG default protocol parameters (whitepaper Table 17.1).
# This file is the single source of truth for node / sim / cli.
# DO NOT edit per-environment values here — copy to config/local.toml
# (gitignored) and override there.

schema_version = 1

[timing]
round_duration_ms       = 250    # micro round length
t_macropropose_ms       = 4000   # macro proposer slot
t_subnet_ms             = 2000   # subnet aggregation window
t_canonicalize_ms       = 8000   # canonical macro publish window

[bullshark]
micro_committee_size    = 256
shortcut_round_count    = 2
slow_path_round_count   = 4
wave_round_count        = 4

[macro_fin]
macro_window_w          = 8      # micro-slots per macro window
two_chain_depth         = 2
inactivity_leak_bps_per_window = 50  # 0.5 % per window
inactivity_leak_trigger_windows = 4

[aggregation]
subnet_flat_threshold   = 500    # Ne < 500  → Mode 0
subnet_full_threshold   = 1000   # Ne ≥ 1000 → Mode A target
# Mode B (leaderless fallback) triggers when proposer misses both primary and backup slots.

[leader]
reputation_floor        = 0.8
reputation_ceiling      = 1.2
reputation_decay        = 0.95

[slashing]
equivocation_bps        = 10000  # 100 %
double_vote_bps         = 5000   # 50 %
da_incident_bps         = 500    # 5 % per incident
slashing_cap_bps        = 5000   # 50 % per epoch cap

[anchor_l4]
btc_confirmations_for_final = 6   # placeholder until L4 lands

[storage]
gc_hot_horizon_rounds   = 200
gc_warm_horizon_rounds  = 10000
snapshot_interval_macros = 256
```

- [ ] **Step 2: Write `config/README.md`**

```markdown
# Protocol Parameters

`default.toml` mirrors whitepaper Table 17.1.

`node`, `sim`, and `cli` all load this file as their base configuration; any
override file (e.g. `config/local.toml`, gitignored) is merged on top.

When changing a parameter:
1. Update `default.toml` here.
2. Update `crates/consensus/src/config.rs` so the typed constant matches.
3. Note the change in `docs/architecture/` if it has cross-component effects.

Loaders **must reject** unknown `schema_version` values — bump the version
when the schema (not just values) changes.
```

- [ ] **Step 3: Verify TOML parses**

Run: `cargo install --locked taplo-cli --version 0.9.3 2>&1 | tail -2; taplo check config/default.toml`
Expected: exit 0 (or skip taplo if install fails — manual `cargo run -q --example -- 'toml::from_str(...)'` is fine; the value parses in plan 03 too).

- [ ] **Step 4: Commit**

```bash
git add config/default.toml config/README.md
git commit -m "chore: add default protocol params from whitepaper Table 17.1"
```

---

## Task 8: Scripts folder placeholder

**Files:**
- Create: `scripts/README.md`

- [ ] **Step 1: Write `scripts/README.md`**

```markdown
# Scripts

Local helper scripts. Each script is independently executable on Windows
(PowerShell) and Linux/macOS (bash) where possible.

Planned scripts (filled in by later plans):

- `devnet/up.sh` — spin up a 4-node localnet using `apps/node` + a shared
  `config/devnet.toml`.
- `devnet/down.sh` — tear devnet down and clean `data/`.
- `lint.sh` / `lint.ps1` — run `cargo fmt --check`, `cargo clippy -D warnings`,
  `cargo deny check`.
- `release.sh` — tag + build release binaries.

Scripts must not embed secrets. Read overrides from `config/local.toml`.
```

- [ ] **Step 2: Commit**

```bash
git add scripts/README.md
git commit -m "chore: add scripts/ folder placeholder"
```

---

## Task 9: CI workflow (fmt + deny only at this stage)

**Files:**
- Create: `.github/workflows/ci.yml`

The workspace has no crates yet, so `cargo build` would succeed trivially. We still want CI live so plans 01–08 can extend it.

- [ ] **Step 1: Write `.github/workflows/ci.yml`**

```yaml
name: ci

on:
  push:
    branches: [main]
  pull_request:

concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: "-D warnings"

jobs:
  fmt:
    name: rustfmt
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: 1.85
          components: rustfmt
      - run: cargo fmt --all --check

  deny:
    name: cargo-deny
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v2
        with:
          command: check
          arguments: --all-features

  build:
    name: cargo build (workspace)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: 1.85
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - name: cargo build
        run: cargo build --workspace --all-targets --locked
      - name: cargo clippy
        run: cargo clippy --workspace --all-targets --locked -- -D warnings
      - name: cargo test
        run: cargo test --workspace --locked
```

- [ ] **Step 2: Verify YAML parses locally**

Run: `python -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml')); print('ok')"`
Expected: `ok`

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add fmt, deny, build, clippy, test workflow"
```

---

## Task 10: Generate `Cargo.lock` and end-to-end verification

**Files:**
- Create: `Cargo.lock` (auto-generated)

- [ ] **Step 1: Trigger lockfile generation**

Run: `cargo generate-lockfile`
Expected: exit 0; `Cargo.lock` created (will be small — no members).

- [ ] **Step 2: Full local check**

Run these commands sequentially:

```bash
cargo fmt --all --check
cargo deny check
cargo metadata --format-version 1 --no-deps
```

Expected: all three exit 0.

- [ ] **Step 3: Commit lockfile**

```bash
git add Cargo.lock
git commit -m "chore: commit initial empty Cargo.lock"
```

---

## Self-Review

Spec coverage check against the parts of `2026-05-11-folder-architecture-design.md` that touch the **workspace root**:

- §5 top-level tree: ✅ `Cargo.toml`, `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml`, dual licenses, `.github/workflows/`, `docs/architecture/`, `config/`, `scripts/` all created. `crates/`, `apps/`, `tests/`, `benches/`, `fuzz/` deferred to their owning plans — explicitly noted in File Structure.
- §5 "default TOML params (Table 17.1)": ✅ Task 7 — every field name pinned, schema_version field added.
- §11 conventions: ✅ Edition 2024 in workspace package; `package.publish = false`; `unsafe_code = forbid`; clippy::all + pedantic; ci runs fmt/clippy/deny/build/test.
- §12 open question #6 ("workspace dependency unification"): ✅ Task 1 includes `[workspace.dependencies]` table.
- Cross-platform: CI runs on `ubuntu-latest` only at this stage — Windows runner can be added in plan 06 (`node`) where libp2p+rocksdb might need cross-OS validation. Acceptable scope cut.

No placeholders, no TBDs, no "implement later". Types/method names referenced in later plans (`consensus::config`, `crates/consensus/src/config.rs`) are documented as forward references, not used at runtime in this plan.
