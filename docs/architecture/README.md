# Architecture Decision Records

This folder stores Architecture Decision Records (ADRs) and sequence diagrams
for the LUA-DAG Rust implementation.

The folder architecture spec lives in
[`../superpowers/specs/2026-05-11-folder-architecture-design.md`](../superpowers/specs/2026-05-11-folder-architecture-design.md).

ADRs are numbered sequentially (`0001-...md`, `0002-...md`) and follow the
Michael Nygard template: Context → Decision → Status → Consequences.

## Network & gossip

- [Network & Gossip architecture](./network-gossip.md) — transport, gossip layer,
  inbound/outbound flows, `apps/node` wiring (Vietnamese).
- [Gossipsub architecture](./gossipsub-internals.md) — deep dive on the
  Gossipsub *overlay* algorithm (mesh, eager/lazy push, heartbeat, scoring);
  not transport or app wiring (Vietnamese).

## Performance runbooks

- [LiveDag high-traffic scaling](../performance/live-dag-high-traffic.md) —
  when to move beyond `RwLock<HashMap>` + `Arc`, DashMap migration, metrics.
