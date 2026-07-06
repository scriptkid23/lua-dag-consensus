# Remove `devnet_factory` — distributed vertex certification as the only L1 production path

**Date:** 2026-07-06
**Status:** Approved
**Supersedes the rollout-flag posture of:** `2026-06-04-distributed-vertex-certificate-design.md` (the flag was a safe-rollout measure; this spec removes it)

## Goal

Delete the legacy `vertex_protocol = "devnet_factory"` path (host-side `L1Driver`
fabricating all 2f+1 quorum certs with devnet BLS keys) so the distributed
`vertex_cert` protocol (propose → partials → ≥2f+1 aggregate) is the only way a
live node produces certified vertices.

## Decisions (confirmed with user)

1. **Full flag cleanup.** Remove `VertexProtocol` enum, `vertex_protocol`,
   `l1_driver_enabled`, and `l1_real_vertex_certs` from node config. The
   orchestrator always verifies certificates; the `[0xAB]` fixture path is never
   accepted by a node.
2. **Skeleton mode becomes ingress-only.** With no gossip swarm the node boots
   but never proposes (no `genesis_propose`). With a swarm, distributed
   certification is unconditionally active. No new flags.

## Design

### Config — `apps/node/src/config_layers.rs`, `config/profiles/devnet.toml`

- Delete `VertexProtocol` enum and the `vertex_protocol`, `l1_driver_enabled`,
  `l1_real_vertex_certs` fields from `NodeSection`.
- Delete the three corresponding lines (and their comments) from
  `config/profiles/devnet.toml`.
- `NodeSection` does not use `deny_unknown_fields`, so stale keys in existing
  config files are silently ignored. Accepted for devnet; no migration code.

### Runtime — `apps/node/src/runtime.rs`

- Delete the whole `match cfg.node.vertex_protocol` block (both arms, including
  the `L1Driver` spawn and all `ensure!` guards).
- Replace with swarm-derived gating:
  - `gossip_publish_tx.is_some()` → log
    `"L1 distributed vertex certification active"`, pass
    `propose_enabled = true` to the orchestrator.
  - No swarm (skeleton) → `propose_enabled = false`; node runs ingress-only.
- No startup failure modes remain for L1 configuration.

### Orchestrator — `apps/node/src/orchestrator.rs`

- Drop the `l1_real_vertex_certs` parameter; always run
  `dag::cert::verify_certified_vertex` before ingesting a certified vertex.
- Rename `vertex_protocol_distributed` → `propose_enabled`; it gates only the
  startup `genesis_propose` kick. Event-driven protocol handling is unchanged.

### Delete `apps/node/src/l1/` entirely

`driver.rs`, `vertex_builder.rs`, `parent.rs`, `mod.rs`. `parent.rs` is only
used by the driver — the `vertex_cert` protocol in `crates/consensus` selects
parents itself (≥2f+1 certified vertices at round−1). `apps/sim` keeps its own
`vertex_factory.rs`: it is a sim-internal fixture, consistent with the 06-11
invariant that only the node runtime must not fabricate quorum certs.

### Tests — `apps/node/tests/`

- Delete `l1_driver_smoke.rs`.
- Port the two tests importing `vertex_builder`:
  - `vertex_cert_reject.rs` — fabricate valid certs via
    `dag::cert::build_quorum_cert` (real devnet-key signatures); build invalid
    certs by hand-constructing `CertifiedVertex`.
  - `l1_gossip_roundtrip.rs` — same: real quorum certs via `dag::cert`, which
    pass the now-unconditional verify gate.
- `l1_distributed_smoke.rs` already covers the distributed path; unchanged.
- Considered and rejected: keeping `vertex_builder.rs` as a `#[cfg(test)]`
  helper — it preserves exactly the fabrication code this change retires.

### Docs

- Update `docs/architecture/layer-1.md`: replace the "L1 Driver / Proposer" box
  with the `vertex_cert` proposer (propose → partials → 2f+1); drop the
  round-robin annotation (pending blobs attach only to the node's own
  proposal).
- Add superseded notes to the legacy plan docs that describe `devnet_factory`
  as the default (`2026-05-23-06b-l1-vertex-driver.md`,
  `2026-06-11-distributed-vertex-certificate.md` §Task 11 default).

## Consequences

- The 4-node docker devnet runs the real distributed protocol: rounds advance
  cert-driven instead of per-tick fabrication. Any latent liveness bug in the
  distributed path will now surface on devnet — intended.
- A node without a swarm can still boot for lightweight dev/test, but produces
  no vertices.

## Verification

- `cargo test --workspace --locked` green.
- `rg "devnet_factory|DevnetFactory|l1_driver_enabled|l1_real_vertex_certs|VertexProtocol" apps/ crates/ config/` returns no hits outside `docs/`.
- Devnet compose smoke: 4 nodes produce certified vertices and advance rounds
  with `vertex_protocol` absent from config.
