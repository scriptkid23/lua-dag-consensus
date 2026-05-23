# lua-dag-consensus

## Run the devnet

The default `docker compose` stack brings up a 4-node prod-like devnet running
the `node` binary against the `devnet` profile (spec
[`devnet-prodlike-design`](docs/superpowers/specs/2026-05-15-devnet-prodlike-design.md)):

```bash
docker compose up --build -d

# Each node exposes admin on host port 9100..9103 (see docker-compose.yml).
curl -fsS http://127.0.0.1:9100/readyz
curl -fsS http://127.0.0.1:9101/readyz
curl -fsS http://127.0.0.1:9102/readyz
curl -fsS http://127.0.0.1:9103/readyz

# E2E: poll node0 RPC until macro finality (may take a few minutes).
chmod +x scripts/devnet_e2e_smoke.sh
./scripts/devnet_e2e_smoke.sh

docker compose down -v
```

`/readyz` returns `200 OK` only after the live gossipsub swarm has bound its
listen socket; `/healthz` reports process-liveness only.

Bare-metal (single node):

```bash
cargo run -p node --release -- --profile devnet --config-dir config
```

## Configuration

Configuration is loaded as layered TOML, last-write-wins. The merge rule is
**field-wise for tables, wholesale-replace for arrays** (spec §3.1):

1. `config/default.toml`         — consensus tables (whitepaper Table 17.1)
2. `config/profiles/<profile>.toml` — `[node]`, `[net]`, `[rocksdb]`
3. `config/local.toml`           — optional, gitignored
4. `--override-config <path>`    — repeatable

A small set of environment variables overrides specific fields after the file
merge (spec §3.2). Every other env var is ignored to keep the surface auditable.

| Var | Purpose |
|-----|---------|
| `LUA_DAG_PROFILE` | profile name (default `devnet`) |
| `LUA_DAG_CONFIG_DIR` | config root dir (default `config`) |
| `LUA_DAG_NODE_IDENTITY_LABEL` | per-container identity label |
| `LUA_DAG_BOOTSTRAP_PEERS` | comma-separated multiaddrs; replaces merged `[net].bootstrap` |
| `STORAGE_PATH` | RocksDB path (via `--data-dir`) |

## Devnet PeerIDs

The four devnet nodes use deterministic libp2p keys derived from the labels
`node0`..`node3` via BLAKE3 + the `DEVNET_PEER_IDENTITY` DST. The resulting
PeerIDs are pinned by
[`crates/net/tests/devnet_identity_golden.rs`](crates/net/tests/devnet_identity_golden.rs);
if they ever drift, regenerate the four lines and the matching
`LUA_DAG_BOOTSTRAP_PEERS` entries in `docker-compose.yml`:

```bash
cargo run -p node --bin print_devnet_peer_ids --locked
```
