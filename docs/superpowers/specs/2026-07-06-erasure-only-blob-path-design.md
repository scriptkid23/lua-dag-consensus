# Erasure-only blob path — RS 4/8 (rate 1/2), 32 KiB shards, sequential mode removed

**Date:** 2026-07-06
**Status:** Approved
**Context:** Reconciles the last data-plane mismatch between `docs/architecture/layer-1.md`
("RS Rate 1/2, 32KB") and the code (RS 4/6 default + sequential 64 KiB fallback), in the
direction of the diagram: the code changes.

## Goal

Erasure coding becomes the only blob custody/gossip path. Reed–Solomon parameters move
to `k=4, n=8` (rate 1/2: 4 data + 4 parity shards, 32 KiB each). The sequential
chunk mode (plain 64 KiB splitting, no parity) is deleted end-to-end: node config,
node custody, and the `crates/dag` blob library.

## Decisions (confirmed with user)

1. **RS 4/8, 32 KiB shards** — matches the architecture diagram's "Rate 1/2, 32KB".
2. **Hard blob-size cap accepted:** max payload = `k × data_shard_size` = **128 KiB**
   with devnet defaults. Oversized `lua_submitBlob` is rejected with an explicit error.
   The cap remains configurable per deployment via `erasure_k` /
   `erasure_data_shard_size_bytes`. Automatic striping of larger payloads is out of
   scope (separate project if ever needed).
3. **Full removal** of sequential support, including the `crates/dag` library layer —
   not just the node wiring.

## Design

### Erasure parameters — rate 1/2

- `config/profiles/devnet.toml`: `erasure_n = 6` → `8`.
- `crates/dag/src/erasure/config.rs` — `ErasureConfig::devnet_default()`: `n: 6` → `8`;
  update the doc comment ("4 data + 4 parity shards, 32 KiB each").
- `apps/node/src/config_layers.rs` — `default_erasure_n()`: `6` → `8`.

### Node custody — erasure mandatory (`apps/node/src/blob/mod.rs`)

- `BlobCustodyConfig` becomes `{ erasure: ErasureConfig }` — the `chunk_size` field and
  the `Option` wrapper are removed.
- `publish_payload`: only the `encode_shards` → `erasure_chunks` path. An oversized
  payload surfaces `encode_shards`' error to the caller.
- `unit_count_for`: always `cfg.n`. `blob_ref_commitment`: always
  `rs_merkle_commitment`. `register_chunk_meta` / `register_chunk_in_ledger`:
  erasure-only.
- `apps/node/src/blob/rocks_store.rs`: match arm for `ChunkPayload::Sequential` removed.

### Node config (`apps/node/src/config_layers.rs`, `config/profiles/devnet.toml`)

- Delete `l1_erasure_enabled` and `blob_chunk_size_bytes` (+ `default_blob_chunk_size`).
- Keep `erasure_k`, `erasure_n`, `erasure_data_shard_size_bytes`.
- `apps/node/src/runtime.rs` — `blob_custody_config()` builds the plain `ErasureConfig`
  unconditionally.

### RPC — explicit oversize rejection (`apps/node/src/rpc_server.rs`)

`submit_blob` returns a structured error instead of silent `null` when publish fails:

```json
{ "error": "payload exceeds max blob size (131072 bytes)" }
```

The max is computed from the custody handle's config (`k × data_shard_size`), not
hard-coded. Other failure modes keep the current `null` behavior. The success response
is unchanged (`chunk_count` now always equals `n`).

### `crates/dag` — delete sequential support

- `blob/chunk.rs`: remove `ChunkPayload::Sequential`, `split_payload`, `chunk_count`,
  and the `total_chunks()` accessor. `ChunkPayload` stays an enum with the single
  `Erasure` variant (wire extensibility).
- `blob/custody.rs`: remove `CustodyKind::Sequential`, `register_sequential`, the
  `register_meta` back-compat wrapper, and `sequential_complete`.
- `blob/commit.rs`: remove `blob_commitment` (sequential whole-payload commitment).
  `blob_id_from_payload` stays (mode-independent id derivation).
- **Wire-format note:** removing the first enum variant shifts the borsh tag of
  `Erasure` from 1 to 0. Accepted: pre-production, all devnet nodes upgrade together;
  no rolling upgrade against old binaries. The gossip topic string
  `lua-dag/v1/blob-chunk` is unchanged.

### Dependent tests and comments

- `crates/dag/tests/blob_chunk_roundtrip.rs`: drop sequential-mode tests; keep/port
  erasure ones (including custody-availability coverage).
- `crates/net/tests/blob_gossip_roundtrip.rs` and the `gossip_wire.rs` unit test:
  build sample chunks via `encode_shards` + `erasure_chunks` instead of
  `split_payload`.
- `crates/net/src/gossip/topics.rs`: comment wording "Sequential blob payload chunk
  stream" → "Blob shard stream" (comment only; wire string untouched).
- `apps/node` tests (`blob_custody_smoke`, `blob_gossip_roundtrip`, `blob_status_rpc`,
  `l1_distributed_smoke`, `erasure_recovery`, `blob/mod.rs` unit tests): replace
  `BlobCustodyConfig { chunk_size: …, erasure: None }` with a small test
  `ErasureConfig` (`k=4, n=8, data_shard_size=1024` → 4 KiB cap, fits the existing
  ~1.5 KiB test payloads).
- New coverage: `lua_submitBlob` oversize rejection test (payload > k × shard_size →
  `error` field present).

### Docs

`docs/architecture/layer-1.md` data-plane box becomes accurate:
"Erasure Coding<br/>RS 4/8 (Rate 1/2), 32KB shards<br/>max blob 128KB".

## Consequences

- Every blob costs 2× its payload in gossip bandwidth and storage (was 1.5× at 4/6);
  tolerates loss of any 4 of 8 shards (was 2 of 6).
- Blobs over 128 KiB are rejected at the RPC; clients must split larger payloads
  themselves.
- `apps/sim` is untouched (it does not use blob chunking).

## Verification

- `cargo test --workspace --locked` — green except the 4 known pre-existing failures
  (timer cancel, node blob_gossip_roundtrip, l1_distributed_smoke genesis,
  consensus vertex_cert_distributed).
- `rg "Sequential|split_payload|chunk_count|blob_commitment|l1_erasure_enabled|blob_chunk_size_bytes|chunk_size" apps crates config`
  → no functional hits outside `docs/` (allow: unrelated identifiers such as
  RocksDB WAL settings; verify each residual hit is not blob-sequential logic).
- `lua_submitBlob` with a 200 KiB payload returns the explicit oversize error.
