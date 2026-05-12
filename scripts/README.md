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
