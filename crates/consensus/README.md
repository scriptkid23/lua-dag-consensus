# `consensus`

Pure deterministic state machine for the LUA-DAG protocol (L2 Bullshark
micro-ordering + L3 Casper-FFG macro-finality, with cross-layer
invariants).

Public entry point: [`StateMachine::step`].

This crate **must not** depend on `tokio`, `libp2p`, or `rocksdb`. Side
effects are emitted as `Action`s; the host binary executes them.

See [`docs/superpowers/specs/2026-05-11-folder-architecture-design.md`](../../docs/superpowers/specs/2026-05-11-folder-architecture-design.md)
§6 for the design rationale.
