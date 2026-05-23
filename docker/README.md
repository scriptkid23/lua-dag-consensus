# Docker prod-like devnet (Phase B)

## What runs

Compose starts **four `node` containers** built from the workspace
`Dockerfile`, each running `node` against the `devnet` profile
(`config/profiles/devnet.toml`). Inside the container every node listens on
`9000/tcp` (gossip) and `9100/tcp` (admin); the host-side ports are remapped
in `docker-compose.yml`:

| Service | Host gossip | Host admin |
|---------|-------------|------------|
| node0   | `9000`      | `9100`     |
| node1   | `9001`      | `9101`     |
| node2   | `9002`      | `9102`     |
| node3   | `9003`      | `9103`     |

RocksDB lives under `/data/rocksdb` inside the container, bind-mounted to
`./devnet-data/nodeN` on the host so state survives container restarts.

**Schema upgrade:** adding the `blob_status` column family requires a fresh
RocksDB directory. After pulling a build that introduces blob persistence, run
`docker compose down -v` once to wipe `./devnet-data/` before bringing the
devnet back up.

## Healthcheck strategy

The chosen healthcheck path is the **in-binary `node --health-probe`**
subcommand:

```dockerfile
HEALTHCHECK --interval=5s --timeout=2s --retries=12 \
    CMD ["/usr/local/bin/node", "--health-probe"]
```

`--health-probe` opens a short-lived TCP connection to `127.0.0.1:9100` and
sends `GET /readyz`. Exit code `0` means the admin endpoint returned `200`;
non-zero means anything else (no listener, `503`, parse error, etc.).

This avoids installing `curl`/`wget` in the runtime image while keeping the
probe path identical to what an external monitoring system would use. If the
strategy ever changes, update this section before introducing competing
checks (we don't want both `--health-probe` and a `curl` fallback in tree).

## Commands

```bash
docker compose up --build -d

# Each node exposes admin on host port 9100..9103.
curl -fsS http://127.0.0.1:9100/readyz

# Stop + remove containers (bind-mounted RocksDB dirs survive).
docker compose down

# Stop + drop named volumes (host bind mounts under ./devnet-data still
# need manual removal).
docker compose down -v
```

## Identity + bootstrap

Each container gets:

- `LUA_DAG_NODE_IDENTITY_LABEL=nodeN` — selects the BLAKE3-derived
  deterministic libp2p key for `nodeN`.
- `LUA_DAG_BOOTSTRAP_PEERS=…` — the multiaddrs of the **other three** nodes,
  using `/dns4/<hostname>/tcp/9000/p2p/<PeerId>`. The PeerIDs are pinned in
  [`crates/net/tests/devnet_identity_golden.rs`](../crates/net/tests/devnet_identity_golden.rs)
  and must match the strings inside `docker-compose.yml`.

To regenerate the PeerIDs after an intentional DST or label change:

```bash
cargo run -p node --bin print_devnet_peer_ids --locked
```

…and paste the four lines into both the golden test and
`docker-compose.yml`.

## Secrets

Never commit real validator keypairs. The `devnet_seed` identity kind is for
local devnet **only** — testnet/prod profiles must mount real keys (spec
§3.4 option 2).
