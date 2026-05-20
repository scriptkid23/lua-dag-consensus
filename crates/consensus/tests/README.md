# Consensus integration tests

Golden vectors and seeds used by `crates/consensus/tests/` and related sim
scenarios. Regenerate deliberately when sortition, hashing, or Bullshark rules
change — never update goldens to silence a failing test without understanding
the protocol delta.

## Anchor sortition (`bullshark_anchor.rs`)

| Input | Golden |
|-------|--------|
| Beacon `Hash32([7u8; 32])` | |
| Wave `WaveId(0)` | |
| 4 equal-stake validators (indices 0–3) | Anchor author = validator index **0** |

Test: `vrf_sortition_is_deterministic_for_seed`

To regenerate: run the test with a temporary `eprintln!("{:?}", a.author);`,
read the winning `ValidatorId`, then update the `assert_eq!(a.author, …)` line.

## Sim `happy_path` (`apps/sim/tests/basic_4node.rs`)

| Input | Notes |
|-------|--------|
| Seed `"0x01"` (32-byte hex) | Deterministic replay in `sim::replay::assert_deterministic` |
| 4 validators, 16 rounds | Expect `safety_ok` and `liveness_ok` |

Changing vertex factory hashing, Bullshark commit wiring, or network jitter
policy requires re-running `cargo test -p sim happy_path` and confirming replay
still bit-identical.

## Related plans

- Milestone spec: `docs/superpowers/specs/2026-05-19-l2-sim-milestone-a-design.md`
- Implementation plan: `docs/superpowers/plans/2026-05-19-03b2-l2-bullshark-full.md`
