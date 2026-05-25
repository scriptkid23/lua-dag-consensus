# Scripts

Local helper scripts. Each script is independently executable on Windows
(PowerShell) and Linux/macOS (bash) where possible.

- `devnet_e2e_smoke.sh` / `devnet_e2e_smoke.ps1` — after `docker compose up`,
  poll `lua_getLatestFinalized` on node0 (`http://127.0.0.1:9200/`) until L3
  macro finality is observable. Used by CI (`.github/workflows/docker-smoke.yml`).

Planned scripts (filled in by later plans):

- `devnet/up.sh` — spin up a 4-node localnet using `apps/node` + a shared
  `config/devnet.toml`.
- `devnet/down.sh` — tear devnet down and clean `data/`.
- `lint.sh` / `lint.ps1` — run `cargo fmt --check`, `cargo clippy -D warnings`,
  `cargo deny check`.
- `release.sh` — tag + build release binaries.

Scripts must not embed secrets. Read overrides from `config/local.toml`.
