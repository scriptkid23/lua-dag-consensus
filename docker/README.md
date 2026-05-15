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
|---------|----------------------|
| node0   | 40000–40002 → 40000–40002 |
| node1   | 40100–40102 → 40000–40002 |
| node2   | 40200–40202 … |
| node3   | 40300–40302 … |

Wire real libp2p multiaddrs inside **`BOOTSTRAP_PEERS`** env when **`apps/node`** exists (Phase B).

## Secrets

Never commit genesis keys — use **`*.example`** only.

## Phase B checklist

Once **`apps/node`** ships per `2026-05-12-06-node-binary.md`:

1. Ensure **`apps/node`** stays in workspace `members`.
2. Replace **`cargo build -p lua_dag_smoke`** builder line with **`cargo build --release -p node`**.
3. **`COPY`** **`target/release/node`** to `/usr/local/bin/node`; adjust **`ENTRYPOINT`**.
4. Adjust **`docker-compose.yml`** service blocks (or anchor) to run **`node`**; pass **`BOOTSTRAP_PEERS`** / **`RUST_LOG`**.
