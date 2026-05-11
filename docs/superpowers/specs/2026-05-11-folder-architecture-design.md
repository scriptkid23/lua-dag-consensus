# LUA-DAG — Rust Folder Architecture Design

- **Ngày**: 2026-05-11
- **Trạng thái**: Draft (chờ review)
- **Tài liệu nguồn**: `docs/whitepaper.pdf` (LUA-DAG: Modular Data Availability and Accountable Finality on a Directed Acyclic Graph, 38 trang)
- **Ngôn ngữ triển khai**: Rust (edition 2024, stable channel)
- **Code comments**: tiếng Anh (theo user rule); spec doc viết tiếng Việt cho readability nội bộ

---

## 1. Tóm tắt

Document này cố định **kiến trúc thư mục và ranh giới crate** cho phần triển khai Rust của LUA-DAG ở giai đoạn đầu, tập trung vào **L2 (Bullshark micro-ordering)** và **L3 (Casper-FFG macro-finality + 2-chain rule)**. Layout được thiết kế để mở rộng tự nhiên về sau cho L1 (Availability DAG) và L4 (Bitcoin Sovereign Anchor) mà không cần refactor cấu trúc gốc.

Quyết định cốt lõi:

1. **Cargo workspace, ~5 library crate + 3 binary crate**, đặt tên ngắn không prefix (`types`, `crypto`, `consensus`, `net`, `storage`, `node`, `sim`, `cli`).
2. **Pure deterministic state machine** cho consensus core — pattern Event/Action, không tokio/libp2p/rocksdb trong crate `consensus`.
3. **libp2p (gossipsub + QUIC) + RocksDB** cho production node; **in-memory adapter** cho simulator.
4. **5 trait ports** trong `consensus::ports` là DI seam duy nhất ra ngoài: `DagView`, `Clock`, `RandomnessBeacon`, `ValidatorSet`, `Persistence`.
5. **3 binary tách biệt**: `node` (validator production), `sim` (deterministic adversarial simulator), `cli` (dev/inspect/ops).

---

## 2. Bối cảnh & phạm vi

### 2.1 Phạm vi giai đoạn này

Theo whitepaper Chương 15.1, lộ trình triển khai chia thành nhiều giai đoạn. Document này khoá phạm vi vào:

- **L2 — Bullshark micro-ordering** (Chương 8): waves 4 round, anchor selection bằng ECVRF, shortcut + slow path commit, linearization, MicroQC.
- **L3 — Macro-Finality** (Chương 9): MacroCheckpoint cadence `W=8` micro-slot, adaptive BLS aggregation Modes 0 / A / B, 2-chain finality rule.
- **Leader election & beacon** (Chương 8 + 11.3): ECVRF sortition, Shoal reputation `[0.8, 1.2]`, beacon chaining `R_w = H(R_{w-1} ‖ MacroQC)`.
- **Slashing & accountability** (Chương 9.4, 13): equivocation 100%, surround vote, double-vote 50%, inactivity leak.
- **Cross-layer invariant** `lock_macro` (§13.5).
- **API tier** (§5.3 + Appendix A): `accepted` → `soft_confirmed` → `justified` → `finalized` (lifecycle `epoch_finalized` để placeholder L4).

### 2.2 Ngoài phạm vi (giai đoạn này, nhưng layout có chỗ chừa)

- **L1 — Availability DAG** (Narwhal-class certified DAG, erasure coding, custody): consensus consume `DagView` trait; impl sẽ là crate mới `crates/dag/` sau.
- **L4 — Bitcoin Sovereign Anchor** (Babylon/Pikachu-style Taproot checkpoint): hook sẽ là crate mới `crates/anchor/`.
- **Light client + sync committee** (Chương 10): `MacroHeader` types đã có trong `types/macros/header.rs` để forward-compat; logic sync committee về sau.
- **Anti-Sybil tooling, DKG ceremony, custody challenges**, MEV fairness mode (Fino), DAS — đều ngoài phạm vi giai đoạn này.

### 2.3 Bối cảnh repo

Khi viết doc này, repo gốc chỉ chứa:
- `docs/whitepaper.pdf` + `docs/whitepaper.tex`
- `README.md` (1 dòng)
- Chưa có code Rust, chưa có `Cargo.toml`.

Không có legacy code để cân nhắc — design là greenfield.

---

## 3. Các quyết định cốt lõi (Q&A summary)

Brainstorming đi qua 5 câu hỏi đa lựa chọn. Kết quả:

| # | Quyết định | Lựa chọn |
|---|------------|----------|
| 1 | Phạm vi triển khai | L2 + L3 core consensus (option D) |
| 2 | Phong cách Cargo | Workspace với 3–5 crate trung bình (option B) |
| 3 | Binary + simulation | 3 binary: `node` + `sim` + `cli` (option A) |
| 4 | Network + storage | libp2p + RocksDB (option A) |
| 5 | Core pattern | Pure deterministic state machine (skip → đề xuất mặc định, option A) |

Bổ sung: bỏ prefix `lua-dag-` khỏi tên crate; folder workspace giữ tên repo `lua-dag-consensus`.

---

## 4. Các phương án đã cân nhắc

### 4.1 Hướng A — Layer-aligned + apps/crates split (chọn)

Một workspace, library trong `crates/`, binary trong `apps/`. Consensus chứa cả L2 + L3 trong cùng crate, chia submodule. Vừa budget 3–5 library crate, ranh giới rõ, mở rộng L1/L4 chỉ là thêm crate.

### 4.2 Hướng B — Concern-aligned (loại)

Tách Bullshark (L2) và macro-finality (L3) thành 2 crate riêng. Rõ biên giới layer hơn, nhưng cross-layer invariant (`lock_macro`, beacon chaining `R_w = H(R_{w-1} ‖ MacroQC)`, vote_book surround detection) buộc 2 crate phụ thuộc 2 chiều hoặc phải tạo crate cầu nối — vượt budget và làm khó cross-layer property test.

### 4.3 Hướng C — Hexagonal (domain/ports/adapters tách hoàn toàn) (loại)

Maximum testability nhưng cần 6–8 crate. Vượt budget. Đáng cân nhắc lại nếu mở rộng full stack về sau.

---

## 5. Kiến trúc top-level

```
lua-dag-consensus/                          # workspace root
├── Cargo.toml                              # [workspace], resolver = "2"
├── Cargo.lock
├── rust-toolchain.toml                     # pin stable channel
├── rustfmt.toml
├── clippy.toml
├── deny.toml                               # cargo-deny: licenses + advisories
├── README.md
├── LICENSE-APACHE
├── LICENSE-MIT
├── .github/workflows/                      # ci.yml, audit.yml, release.yml
├── docs/
│   ├── whitepaper.pdf
│   ├── whitepaper.tex
│   ├── superpowers/specs/                  # design docs (this file lives here)
│   └── architecture/                       # ADRs, sequence diagrams
├── crates/                                 # library crates
│   ├── types/                              # shared structs + canonical codec
│   ├── crypto/                             # BLS12-381, ECVRF, hash, PoP
│   ├── consensus/                          # pure state machine (L2 + L3)
│   ├── net/                                # libp2p gossip + RPC + peers
│   └── storage/                            # RocksDB adapter + WAL
├── apps/                                   # binary crates
│   ├── node/                               # validator production binary
│   ├── sim/                                # deterministic adversarial simulator
│   └── cli/                                # dev / inspect / ops tool
├── tests/                                  # workspace-level integration tests
│   ├── e2e_two_chain_finality.rs
│   ├── e2e_anchor_dos_recovery.rs
│   └── common/                             # shared fixtures
├── benches/                                # criterion benches cross-crate
├── fuzz/                                   # cargo-fuzz targets (nightly toolchain)
├── scripts/                                # local devnet, lint, release scripts
└── config/                                 # default TOML params (Table 17.1)
```

Quyết định kèm theo:
- `crates/` (library) vs `apps/` (binary) tách rõ — nhìn cây biết ngay đâu là code reusable.
- `tests/` ở workspace root chỉ chứa **integration cross-crate**; mỗi crate vẫn có `tests/` riêng.
- `fuzz/` ở root vì cargo-fuzz cần nightly toolchain — không ép cả workspace nightly.
- `config/` chứa default params từ Bảng 17.1 (`ROUND_DURATION=250ms`, `MACRO_WINDOW_W=8`, `MICRO_COMMITTEE_SIZE=256`, `T_MACROPROPOSE=4s`, `T_SUBNET=2s`, `T_CANONICALIZE=8s`, `SUBNET_FLAT_THRESHOLD=500`, `SUBNET_FULL_THRESHOLD=1000`, `BTC_CONFIRMATIONS_FOR_FINAL=6`, v.v.) — `node`, `sim`, `cli` cùng đọc một nguồn → không lệch.
- Không có `examples/` — mọi demo đi qua `apps/sim` để không drift khỏi protocol thật.
- Crate name không prefix; `package.publish = false` ở workspace level. Nếu sau muốn publish, rename lúc đó hoặc đổi qua private registry.

---

## 6. Nội bộ `crates/consensus/`

Phần phức tạp nhất. Pattern: pure deterministic state machine.

```
crates/consensus/
├── Cargo.toml
├── README.md                          # crate overview + sequence diagrams
├── src/
│   ├── lib.rs                         # public surface, re-export qua prelude
│   ├── prelude.rs
│   ├── config.rs                      # NGUỒN SỰ THẬT cho Table 17.1 params
│   ├── event.rs                       # Event enum: mọi input vào SM
│   │                                  #   CertifiedVertexReceived, MicroQcAssembled,
│   │                                  #   MacroProposalReceived, BlsPartialReceived,
│   │                                  #   SubnetAggregateReceived, TimerFired(id),
│   │                                  #   ValidatorSetUpdated, SlashEvidenceFound
│   ├── action.rs                      # Action enum: mọi output ra ngoài
│   │                                  #   BroadcastMicroVote, BroadcastMacroProposal,
│   │                                  #   BroadcastBlsPartial, ScheduleTimer,
│   │                                  #   CancelTimer, PersistMacroQc,
│   │                                  #   EmitSlashEvidence, UpdateBlobStatus
│   ├── state_machine.rs               # struct StateMachine + fn step(Event) -> Vec<Action>
│   ├── lock_macro.rs                  # invariant §13.5
│   ├── bullshark/                     # L2 micro-ordering
│   │   ├── mod.rs
│   │   ├── wave.rs                    # rounds 4w..4w+3
│   │   ├── anchor.rs                  # anchor selection (gọi leader::vrf_sortition)
│   │   ├── commit.rs                  # shortcut (2 rounds) + slow path (4 rounds)
│   │   ├── linearize.rs               # Closure(Aw) BFS, tie-break by vertex hash
│   │   ├── micro_qc.rs                # MicroQC aggregation ≥ ⌈2/3·C⌉
│   │   └── tests.rs
│   ├── macro_fin/                     # L3 macro-finality
│   │   ├── mod.rs
│   │   ├── window.rs                  # W=8 micro-slot cadence
│   │   ├── proposer.rs                # primary + backup; T_macropropose=4s
│   │   ├── checkpoint.rs              # MacroCheckpoint build/verify
│   │   ├── aggregation/
│   │   │   ├── mod.rs                 # AggregationMode + Ke selector (Eq. 9.1)
│   │   │   ├── mode0_flat.rs          # Ne<500
│   │   │   ├── mode_a_subnet.rs       # Ne≥500, subnet rotation per epoch
│   │   │   ├── mode_b_leaderless.rs   # fallback
│   │   │   └── subnet.rs              # subnet(vi,e) = H(pubkey ‖ R_macro) mod Ke
│   │   ├── macro_qc.rs                # MacroQC verify, signed-stake calc, Mode B tie-break
│   │   ├── two_chain.rs               # Casper FFG 2-chain rule
│   │   ├── vote_book.rs               # per-validator vote history (epoch-indexed)
│   │   └── tests.rs
│   ├── leader/                        # election + timing (dùng chéo L2 + L3)
│   │   ├── mod.rs
│   │   ├── beacon.rs                  # R_w + R_macro_h chaining (Eq. 8.1)
│   │   ├── vrf_sortition.rs           # gọi crypto::ecvrf; tính y_i · W/(w_i · rep_i)
│   │   ├── reputation.rs              # Shoal reputation [0.8, 1.2]
│   │   └── timeout.rs                 # timers centralised here
│   ├── slashing/
│   │   ├── mod.rs
│   │   ├── evidence.rs                # SlashEvidence verifier (pure)
│   │   ├── equivocation.rs            # macro equivocation 100% slash
│   │   ├── surround.rs                # Casper surround, quét vote_book
│   │   ├── inactivity_leak.rs         # 0.5%/window sau 4 unfinalized windows
│   │   └── penalty.rs                 # double-vote 50%, DA 5%/incident, cap 50%
│   ├── api/                           # external query surface
│   │   ├── mod.rs
│   │   ├── tier.rs                    # BlobStatus enum (Appendix A)
│   │   └── query.rs                   # latest_finalized(), micro_head(), blob_status()
│   └── ports/                         # outbound traits — DI seam
│       ├── mod.rs
│       ├── dag_view.rs                # DagView trait — L1 plug-in point
│       ├── clock.rs                   # Clock trait
│       ├── rng_beacon.rs              # RandomnessBeacon trait
│       ├── validator_set.rs           # ValidatorSet trait
│       └── persistence.rs             # Persistence trait
└── tests/                             # crate-level integration tests
    ├── bullshark_shortcut_commit.rs
    ├── bullshark_slow_path_commit.rs
    ├── two_chain_finality.rs
    ├── adaptive_aggregation_modes.rs  # quét Ne = 100/500/1000/5000
    ├── lock_macro_invariant.rs
    ├── slashing_evidence.rs
    └── beacon_chaining.rs
```

Nguyên tắc:

1. `state_machine.rs` là **entrypoint duy nhất**. Method công khai: `fn step(&mut self, ev: Event) -> SmallVec<[Action; 8]>`. Không tokio, không libp2p, không rocksdb in scope.
2. `event.rs` + `action.rs` là **contract** giữa consensus và thế giới ngoài. Mọi tương tác với consensus pass qua hai enum này → fuzz target dễ viết, codec stable.
3. `config.rs` là **single source of truth** cho Table 17.1. Mỗi field có doc comment trích Chương/Section trong whitepaper. Test/sim override qua builder.
4. `ports/` là **DI seam**. 5 trait: `DagView` (L1 plug-in point), `Clock`, `RandomnessBeacon`, `ValidatorSet`, `Persistence`. Consensus chỉ depend `types` + `crypto`; không bao giờ depend ngược lên `net`/`storage`.
5. L2 + L3 **cohabit chứ không tách crate** — vì `lock_macro`, beacon chaining, vote_book surround detection đều cross-layer. Submodule biên đủ rõ; `bullshark/` không biết `macro_fin/` và ngược lại (giao tiếp qua `state_machine.rs` và `lock_macro.rs`).
6. `vote_book.rs` tách file riêng để surround-vote detection fuzz/optimise độc lập.

---

## 7. Các crate library còn lại

### 7.1 `crates/types/` — shared data structures

```
crates/types/
├── src/
│   ├── lib.rs
│   ├── primitives.rs                  # Round, Height, Epoch, ValidatorId, StakeWeight, BlobId
│   ├── crypto_types.rs                # BlsPubkey, BlsSig, BlsAggSig, VrfProof, Hash32, PoP
│   ├── dag/                           # input types từ L1 (consensus consume read-only)
│   │   ├── mod.rs
│   │   ├── vertex.rs
│   │   ├── certified.rs
│   │   └── refs.rs                    # BlobRef, ChunkRef — opaque
│   ├── micro/
│   │   ├── mod.rs
│   │   ├── checkpoint.rs
│   │   └── qc.rs
│   ├── macros/                        # 'macro' là keyword Rust → folder số nhiều
│   │   ├── mod.rs
│   │   ├── checkpoint.rs
│   │   ├── qc.rs
│   │   ├── header.rs                  # MacroHeader cho light client (forward-compat)
│   │   └── proposal.rs
│   ├── validator/
│   │   ├── mod.rs
│   │   ├── identity.rs                # ValidatorIdentity (ASN, cloud, region)
│   │   ├── set.rs                     # ValidatorSet snapshot, epoch-indexed
│   │   └── dkg.rs                     # DKGCommitment skeleton (opt-in)
│   ├── slashing.rs                    # SlashEvidence enum + variants
│   ├── codec/
│   │   ├── mod.rs                     # canonical (deterministic) serialization
│   │   └── borsh_impl.rs              # Borsh — gọn, deterministic, no schema drift
│   └── error.rs
└── tests/
    ├── codec_roundtrip.rs
    └── canonical_hash.rs              # hash determinism across versions
```

Nguyên tắc: chỉ struct + serde + hash. Không verify, không aggregate, không business logic.

### 7.2 `crates/crypto/` — primitive wrappers

```
crates/crypto/
├── src/
│   ├── lib.rs
│   ├── hash.rs                        # Blake3 + Sha256 + domain separation tags
│   ├── bls/
│   │   ├── mod.rs
│   │   ├── keys.rs                    # SecretKey, PublicKey, PoP
│   │   ├── sign.rs
│   │   ├── aggregate.rs
│   │   └── bitmap.rs                  # validator/subnet bitmap helpers
│   ├── vrf/
│   │   ├── mod.rs
│   │   ├── ecvrf.rs                   # ECVRF Edwards25519 RFC 9381
│   │   └── sortition.rs               # stake-weighted sortition
│   ├── kdf.rs                         # HKDF cho beacon chaining + subnet assign
│   ├── dkg/
│   │   ├── mod.rs
│   │   └── fingerprint.rs             # DKG commitment skeleton
│   └── error.rs
├── benches/
│   ├── bls_verify.rs
│   ├── bls_aggregate.rs               # 100 / 500 / 1000 partials
│   └── vrf_verify.rs
└── tests/
    ├── bls_pop.rs                     # rogue-key resistance
    └── vrf_determinism.rs
```

Library backing đề xuất: `blst` (Supranational) cho BLS12-381, `vrf` crate hoặc `ecvrf-rs` cho ECVRF. Public API ẩn lib cụ thể sau trait alias mỏng → swap dễ khi cần PQ migration.

### 7.3 `crates/net/` — libp2p adapter

```
crates/net/
├── src/
│   ├── lib.rs
│   ├── config.rs                      # NetConfig: listen, bootstrap, gossip params
│   ├── transport.rs                   # QUIC + TCP, noise xx, yamux
│   ├── identity.rs                    # libp2p PeerId ↔ ValidatorId; epoch rotate
│   ├── gossip/
│   │   ├── mod.rs
│   │   ├── topics.rs                  # enum Topic { CertifiedVertex, MicroQc,
│   │   │                              #   MacroProposal, BlsPartial(SubnetId),
│   │   │                              #   SubnetAggregate, SlashEvidence }
│   │   ├── codec.rs
│   │   └── publisher.rs               # publish + de-dup ring
│   ├── rpc/
│   │   ├── mod.rs
│   │   ├── causal_set.rs              # L1 sync placeholder
│   │   └── checkpoint_sync.rs         # late-joining validator fast-sync
│   ├── peers/
│   │   ├── mod.rs                     # PeerManager
│   │   ├── scoring.rs                 # gossipsub score + ban
│   │   └── discovery.rs               # kad-dht (optional) + bootstrap
│   ├── bridge.rs                      # ★ ADAPTER DUY NHẤT ★
│   │                                  # libp2p::Event → consensus::Event
│   │                                  # consensus::Action → libp2p publish/RPC
│   └── error.rs
└── tests/
    ├── gossip_roundtrip.rs
    └── peer_ban.rs
```

Nguyên tắc: `bridge.rs` là cổng duy nhất giữa consensus và libp2p. Consensus crate không bao giờ `use libp2p::*`.

### 7.4 `crates/storage/` — RocksDB adapter

```
crates/storage/
├── src/
│   ├── lib.rs
│   ├── config.rs
│   ├── db.rs                          # RocksDB wrapper + column family bootstrap
│   ├── columns.rs                     # enum ColumnFamily + key schema docs
│   ├── keys.rs                        # big-endian encoding → monotonic prefix scan
│   ├── stores/
│   │   ├── mod.rs
│   │   ├── vertex_store.rs            # CertifiedVertex by (round, author)
│   │   ├── micro_store.rs             # MicroCheckpoint + MicroQC by slot
│   │   ├── macro_store.rs             # MacroCheckpoint + MacroQC + 2-chain pointers
│   │   ├── valset_store.rs            # ValidatorSet snapshot per epoch
│   │   ├── slash_store.rs             # SlashEvidence (append-only)
│   │   └── vote_book_store.rs         # vote history per-validator
│   ├── wal.rs                         # WAL cho atomic batch
│   ├── gc.rs                          # hot (200 rounds) / warm (10k rounds) / cold
│   ├── snapshot.rs                    # state snapshot cho fast-sync + WS bootstrap
│   ├── persistence_impl.rs            # ★ impl consensus::ports::Persistence ★
│   └── error.rs
└── tests/
    ├── crash_recovery.rs              # kill -9 mid-write → WAL replay correct
    ├── pruning.rs
    └── snapshot_roundtrip.rs
```

Nguyên tắc: storage cài đặt **trait** từ `consensus::ports::Persistence`, không depend logic consensus.

---

## 8. Binary crates

### 8.1 `apps/node/` — validator production

```
apps/node/
├── src/
│   ├── main.rs
│   ├── args.rs                        # clap CLI
│   ├── config.rs                      # NodeConfig (load TOML từ ../config/)
│   ├── runtime.rs                     # tokio multi-thread runtime
│   ├── orchestrator.rs                # ★ glue ★: drives StateMachine via bridge + storage + timer
│   ├── timer.rs                       # impl Clock; emit Event::TimerFired
│   ├── validator_set_loader.rs        # bootstrap + epoch transition
│   ├── observability/
│   │   ├── mod.rs
│   │   ├── metrics.rs                 # prometheus exporter
│   │   ├── tracing.rs                 # tracing-subscriber + structured log
│   │   └── health.rs                  # readiness + liveness
│   ├── rpc_server.rs                  # external query (JSON-RPC or gRPC)
│   └── shutdown.rs                    # graceful drain + persist
└── tests/
    └── node_smoke.rs                  # spawn 4 nodes localhost
```

### 8.2 `apps/sim/` — deterministic adversarial simulator

```
apps/sim/
├── src/
│   ├── main.rs
│   ├── args.rs                        # --validators N --rounds R --seed S
│   ├── world.rs                       # World owns N × StateMachine
│   ├── virtual_clock.rs               # impl Clock (virtual time)
│   ├── virtual_net.rs                 # message bus + adversary scheduler
│   ├── virtual_dag.rs                 # impl DagView in-memory (L1 placeholder)
│   ├── virtual_beacon.rs              # impl RandomnessBeacon
│   ├── adversary/
│   │   ├── mod.rs
│   │   ├── byzantine.rs               # equivocate, withhold, surround
│   │   └── network.rs                 # drop/delay/duplicate; partition
│   ├── scenarios/
│   │   ├── mod.rs
│   │   ├── happy_path.rs
│   │   ├── anchor_dos.rs              # 1/3 stake offline
│   │   ├── mode_b_fallback.rs         # macro proposer drop liên tiếp
│   │   ├── equivocation_inject.rs
│   │   ├── byzantine_split.rs
│   │   └── network_partition.rs
│   ├── checker/                       # property invariants
│   │   ├── mod.rs
│   │   ├── safety.rs                  # no two finalized conflicting macros
│   │   ├── liveness.rs                # finality progress under healthy stake
│   │   └── lock_macro.rs              # §13.5 invariant
│   ├── metrics.rs                     # finality latency p50/p95
│   └── replay.rs                      # --seed → bit-identical replay
└── tests/
    └── basic_4node.rs
```

Nguyên tắc: `sim` **không depend `net` hay `storage`**. Chỉ depend `consensus` + `types` + `crypto` + in-memory impls. Deterministic được nhờ không có async I/O, không có thread sched.

### 8.3 `apps/cli/` — dev/inspect/ops

```
apps/cli/
└── src/
    ├── main.rs
    ├── args.rs
    └── commands/
        ├── mod.rs
        ├── inspect.rs                 # decode + dump MacroCheckpoint/QC từ rocksdb
        ├── keygen.rs                  # generate BLS key + PoP + VRF key
        ├── verify.rs                  # verify SlashEvidence offline (cho watcher)
        ├── replay_log.rs              # replay event log từ prod
        └── bench_aggregate.rs         # ad-hoc BLS aggregate throughput
```

---

## 9. Đồ thị phụ thuộc

```
                   types
                     ↑
                   crypto
                     ↑
                 consensus           (depend types + crypto only)
                  ↑     ↑
              net          storage   (depend consensus cho Event/Action/Persistence trait)
                ↑     ↑      ↑
              node ───┴──────┘
              
              sim ────→ consensus + types + crypto       (KHÔNG net, KHÔNG storage)
              cli ────→ types + crypto + storage         (đọc raw db offline)
```

Tính chất:
- `consensus` không depend `net`/`storage` → vẫn build & test được khi 2 crate kia đang dở dang.
- `net`/`storage` depend ngược lên `consensus` qua **trait**, không qua impl cụ thể → swap dễ.
- `sim` không kéo libp2p / rocksdb → compile nhanh, deterministic.

Acyclic. Tooling enforce bằng `cargo deny` + check `[dependencies]` trong CI.

---

## 10. Lộ trình mở rộng (giai đoạn sau)

Thêm crate, **không** refactor cấu trúc gốc:

| Giai đoạn | Crate / module mới | Hook tích hợp |
|-----------|--------------------|--------------------|
| L1 Availability DAG | `crates/dag/` (Narwhal-class certified DAG, erasure coding, custody, challenges) | impl `consensus::ports::DagView`; gossip topics mới trong `net::gossip::topics` |
| L4 Bitcoin Anchor | `crates/anchor/` (Taproot checkpoint, vigilante relay, BTC SPV) | event mới `Event::BitcoinAnchorConfirmed`; action mới `Action::EmitBitcoinCheckpoint` |
| Light client | `crates/light_client/` (sync committee verifier) | re-export `types::macros::MacroHeader`; sync committee logic |
| DAS / 2D RS | mở rộng `crates/dag/das/` | thêm `kzg_commitment` vào Vertex schema (backward compat) |
| DKG ceremony | mở rộng `crates/crypto/dkg/` | mandatory cho new activations qua governance |
| MEV fairness (Fino) | crate ngoài hoặc module trong `consensus/bullshark/fairness/` | opt-in flag |

---

## 11. Convention & tooling

- **Edition**: 2024.
- **Tên crate**: ngắn, không prefix. `package.publish = false` workspace-wide.
- **Tên binary**: trùng crate name (`node`, `sim`, `cli`). Override khi đóng release stand-alone.
- **Module thay vì keyword**: `macros/` (không phải `macro`), `macro_fin/` (không phải `macro`).
- **Code comments**: tiếng Anh (theo user rule). Spec doc tiếng Việt.
- **Codec**: Borsh cho on-wire + on-disk; canonical, deterministic, no schema drift.
- **Lint**: `clippy::all` + `clippy::pedantic` (chọn lọc), `unsafe_code = "forbid"` trừ `crypto`.
- **CI**: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo deny check`, `cargo test --workspace`, `cargo +nightly fuzz check` (smoke), `apps/sim` chạy 1 scenario nhanh.

---

## 12. Open questions (cần quyết khi sang plan)

1. **JSON-RPC vs gRPC** cho `node::rpc_server`? — Default đề xuất JSON-RPC (đơn giản, công cụ đầy đủ); gRPC nếu cần throughput cao.
2. **Codec on-wire**: Borsh hay SSZ? Đề xuất Borsh (gọn, deterministic, Rust-first). SSZ có Ethereum precedent nhưng tooling kém hơn.
3. **VRF library**: `vrf` crate, `ecvrf-rs`, hay tự wrap `curve25519-dalek` + RFC 9381? Cần khảo sát audit status.
4. **TLA+ companion**: có viết spec TLA+ song song cho commit rule + 2-chain rule không (whitepaper Chương 15.1 gợi ý)?
5. **Property test framework**: `proptest` (default) hay `bolero` (kết hợp fuzz + property)?
6. **Workspace dependency unification**: dùng `[workspace.dependencies]` (Cargo 1.64+) để pin version một chỗ — đề xuất bật.

Các open question này không cản triển khai folder layout; trả lời khi sang giai đoạn writing-plans.

---

## 13. Tham chiếu whitepaper

| Module trong code | Section whitepaper |
|--------------------|--------------------|
| `consensus::bullshark::wave` + `anchor` | Ch. 8.1 |
| `consensus::bullshark::commit` | Ch. 8.2 (shortcut + slow path) |
| `consensus::bullshark::linearize` | Ch. 8.3 |
| `consensus::bullshark::micro_qc` | Ch. 8.4 |
| `consensus::macro_fin::window` + `proposer` | Ch. 9.1 |
| `consensus::macro_fin::aggregation::*` | Ch. 9.2, Eq. 9.1/9.2 |
| `consensus::macro_fin::two_chain` | Ch. 9.3 |
| `consensus::leader::beacon` | Eq. 8.1, Ch. 11.3 |
| `consensus::leader::vrf_sortition` | Ch. 8.1 (private sortition) |
| `consensus::leader::reputation` | Ch. 7.1 (Shoal) |
| `consensus::slashing::*` | Ch. 9.4, Ch. 13 |
| `consensus::lock_macro` | §13.5 |
| `consensus::api::tier` | Appendix A |
| `consensus::config` | Table 17.1 |
| `storage::gc` | Ch. 7.4 (`GC_HOT_HORIZON`, `GC_WARM_HORIZON`) |
| `types::macros::header` (forward-compat) | Ch. 10.1 |

---

## 14. Self-review notes

Tự review ngay sau khi viết:

- [x] **Placeholder scan**: không có TBD / TODO không phân giải; mọi open question đã liệt kê ở §12.
- [x] **Internal consistency**: dependency graph (§9) khớp với mô tả từng crate (§6–8); naming `macros/` (số nhiều, types crate) vs `macro_fin/` (consensus crate) giải thích rõ.
- [x] **Scope check**: chỉ cover folder/crate layout cho L2+L3 + skeleton mở rộng; không lấn vào API design chi tiết hay plan triển khai từng module — đúng phạm vi 1 spec.
- [x] **Ambiguity check**: 
  - "Pure deterministic state machine" định nghĩa rõ ở §6 (Event → Action, không tokio/I/O).
  - "DI seam" đã liệt kê đúng 5 trait ports.
  - Folder name `macros/` lý do explicit (keyword conflict).
  - Binary name vs crate name discrepancy đã ghi (§5, §11).
- [x] **Mở rộng**: §10 đã chỉ rõ cách add L1/L4/light client mà không refactor.

Tự review thông qua — không phát hiện vấn đề cần fix inline.
