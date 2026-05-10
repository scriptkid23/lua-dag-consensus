# LUA-DAG v2 — Modular DA + Accountable Finality on a DAG

> **Edition**: English (`2026-05-09-lua-dag-v2-design-en.md`). Source draft: `2026-05-09-lua-dag-v2-design.md`.
> **Version**: v2 design draft (rev. 3 after external critical review)
> **Date**: 2026-05-10
> **Status**: Proposed specification — not implemented, not audited
> **Predecessors**: `docs/luadag.pdf` (LUA-DAG v1), `docs/improve.pdf` (independent review of rev. 2)
> **Purpose**: Redesign LUA-DAG around the positioning “modular DA + finality layer” (Celestia-class), addressing theoretical gaps and design omissions identified in the v1 review, **and resolving rev. 2 architectural mismatches called out by external critique**.
> **Rev. 2 changelog**: Updated per literature review (Consensus MCP, 70 papers): default VRF moved to iVRF (post-quantum, unbiasable), optional Mysticeti-style uncertified DAG, two-mode subnet aggregation (leader + leaderless fallback), optional Babylon-style Bitcoin checkpoint, vocabulary aligned with “ebb-and-flow / 3-slot finality”, differentiation vs Acki Nacki, PQ migration advanced from v3 to v2.1.
> **Rev. 3 changelog (PDF-driven, 2026-05-10)** — addresses five architectural mismatches identified by independent review:
> 1. **Cryptographic stack consistency (resolve PQ paradox)**: downgrade default VRF from iVRF (PQ claim) to **ECVRF (Edwards25519)** in v2.0; remove “post-quantum” labeling from v2.0; full-stack PQ migration (BLS → STARK aggregation, ECVRF → iVRF/lattice-VRF) deferred to **v3** (timeline aligned with Ethereum PQ roadmap, Drake 2025).
> 2. **Realistic throughput target + DAS as hard requirement**: lower v2.0 DA throughput target to **5 MB/s sustained** (comparable to current Celestia) to allow home-internet full nodes; 30–100 MB/s **requires DAS** (2D Reed–Solomon + KZG, or RLNC-DAS) in v2.1 — do **not** ship 100 MB/s without DAS.
> 3. **Adaptive subnetting**: remove hard-coded 8 subnets; introduce three-mode aggregation: **flat gossip** for N<500 (Mode 0), **subnet-based** for N>1000 (Mode A — adaptive count = ⌈N/128⌉), **interpolated** for 500≤N≤1000.
> 4. **Finality boundary clarity**: Bitcoin checkpointing (Babylon-style) moves from **optional → default ON**; explicitly define two tiers: **Fast Execution Finality** (5–10s, MacroQC, for rollup transactions) vs **Sovereign Epoch Finality** (~60 min, 6 BTC blocks, **required** before validator unbond/withdraw or large bridge releases).
> 5. **Multi-dimensional anti-Sybil**: retain 5% stake cap **but** add behavioral + cryptographic layers: (a) IP/ASN/cloud declaration with reward decay by concentration score, (b) DKG-based key-origin fingerprinting, (c) exponential slashing if shared key origin is proven across split “validators”.

---

## 0. Executive summary

LUA-DAG v2 is a **consensus and data availability** protocol for rollups and app-chains. The protocol **has no execution layer** and **does not compete head-on with generic L1s** such as Sui/Aptos/Monad; it competes in the modular DA + shared finality segment, with primary peers Celestia, Avail, and EigenDA.

Compared with v1, v2 makes eight core changes:

1. **Remove execution and generic “block consensus”** — narrow scope to exactly two deliverables: published-and-available data, and accountable hard finality of a header chain.
2. **Replace v1’s vague “deterministic” frontier rule with the Bullshark anchor commit rule** — the most important theoretical fix.
3. **Macro-finality uses adaptive subnet BLS aggregation (rev.3)** — flat gossip for small validator sets (<500), subnet-based for large ones (>1000); instead of hard-coded 8 subnets as in rev.2.
4. **Fully specified permissionless membership** — activation queue, withdrawal delay, churn limit, randomness beacon refresh; v1 only said “permissionless” without defining it.
5. **Clear API contract for soft confirm vs hard finality** — `accepted`, `soft_confirmed`, `finalized`, `epoch_finalized` (rev.3) are four distinct user-visible states, to prevent rollup developers from treating a weaker tier as a stronger one.
6. **Probabilistic safety analysis for committees with concrete numbers** — no hand-waving.
7. **Bitcoin checkpoint default ON (rev.3)** — defines two finality tiers: Fast Execution Finality (5–10s, transaction-grade) vs Sovereign Epoch Finality (~60 min, 6 BTC blocks, validator-rotation-grade).
8. **Multi-dimensional anti-Sybil (rev.3)** — adds behavioral + cryptographic layers on top of the 5% stake cap to resist stake-split Sybil behavior.

What **does not** change vs v1: PoS, partial synchrony, accountable safety under 1/3 Byzantine stake, VRF private sortition, Casper FFG–style accountable finality, light client with sync committee.

Performance targets (evidence-based expectations, not yet empirically proven):


| Metric                          | Target v2.0                              | Target v2.1+ (after DAS is ready)      | Reference comparison                    |
| ------------------------------- | ---------------------------------------- | -------------------------------------- | --------------------------------------- |
| DA throughput sustained         | **5 MB/s** (rev.3, hard-cap)             | 30–100 MB/s with DAS                   | Celestia ~6 MB/s, Avail ~2 MB/s (2024) |
| Soft-confirm latency p95        | 0.5–1.5s                                 | 0.5–1.5s                               | Bullshark ~2s, Mysticeti ~600ms        |
| Fast Execution Finality p95     | 5–10s (MacroQC, rollup tx-grade)         | 5–10s                                  | Celestia ~12s, Ethereum ~15min         |
| Sovereign Epoch Finality p95    | **~60 min** (6 BTC blocks, rev.3)      | ~60 min                                | Babylon ~60 min                        |
| Validator HW requirements (v2.0) | **8 vCPU / 32 GB / 1 TB NVMe / 100 Mbps** (rev.3 lowered via throughput cap) | 16 vCPU / 64 GB / 2 TB NVMe / 500 Mbps (v2.1 with DAS) | v2.0 home-internet friendly; v2.1 datacenter |


**Rev.3 note**: rev.2 targeted 30–100 MB/s in v2.0 **without** DAS — independent review (`docs/improve.pdf`) flagged this as “decentralization suicide” because 100 MB/s ≈ 8.6 TB/day, runnable as a full node only on datacenter-grade hardware. Rev.3 splits targets into two phases: v2.0 ships at a decentralization-preserving level (5 MB/s); v2.1 ships higher throughput **together with** DAS.


---

## 1. Positioning, scope, and success metrics

### 1.1 Positioning

LUA-DAG v2 = **“published, available, and finalized blob data”** as a service. Customers are rollups, app-chains, and bridge protocols — **not** end users of dApps.

Each rollup sends to LUA-DAG:

- Blobs (serialized rollup transaction batches).
- Optionally: state-root commitments / fraud-proof references.

LUA-DAG returns:

- **Availability evidence** (DAG vertex certificates).
- **Ordering evidence** (anchor commit) — blobs are ordered deterministically *within* each rollup namespace.
- **Hard-finality evidence** (macro checkpoint header with BLS aggregate signature).
- **Slashable evidence** when safety is violated.

Rollups handle execution, cross-rollup atomicity (if needed), and encrypted mempools themselves. LUA-DAG **does not sequence in the shared-sequencer sense** — an important scope decision to keep speed and complexity manageable.

### 1.2 What we are NOT

This section exists because v1 tried to do too much in one spec.

- **NOT a generic L1**: no EVM, no Move VM, no SVM, no execution.
- **NOT a shared sequencer**: the rollup keeps its own ordering layer; LUA-DAG only commits blob order within a namespace.
- **OPTIONAL MEV resistance** (revised rev.2): by default MEV is a rollup concern, but the spec provides an **optional fairness mode** based on Fino-style integration (Malkhi et al. 2022) — zero message overhead, no threshold encryption. Rollups opt in at deployment.
- **NOT a light DA at v2.0** (rev.3 clarified): no DA sampling for light nodes in v2.0 — hence v2.0 throughput is **hard-capped at 5 MB/s** (full nodes download all data). DAS is **mandatory** in v2.1 and is a prerequisite to unlock throughput >5 MB/s. v2.0 light clients must trust the sync committee for DA (similar to Celestia full-replication light mode).
- **NOT post-quantum at v2.0** (rev.3 honest labeling): rev.2 marketed iVRF as “post-quantum” while simultaneously using BLS12-381 (not post-quantum) for aggregation — a **security theater** risk. Rev.3 states plainly: **the entire v2.0 crypto stack is classical** (ECVRF + BLS12-381). PQ migration is a **single coordinated event in v3** (BLS → STARK aggregation, ECVRF → lattice/iVRF), not a patchwork.
- **NOT a restaking AVS**: primary safety and liveness rest entirely on native PoS stake — no live consensus trust borrowed from Ethereum/BTC. Bitcoin **does not vote, does not sign blocks, and does not participate in MacroQC safety/liveness paths**.
- **Bitcoin checkpoint = default ON in v2.0** (rev.3, promoted from optional): Babylon-style checkpointing of the latest finalized MacroCheckpoint into Bitcoin Taproot each epoch is **mandatory** on v2.0 mainnet, not optional. Rationale: only with this checkpoint do validator unbonding/rotation get a short safe window (~60 min ≈ 6 BTC blocks) instead of a two-week weak-subjectivity cycle. See §1.4 and §8.6 for the two-tier finality model.

### 1.3 Success metrics (KPIs)

A v2 implementation counts as “successful” on an adversarial testnet if:


| KPI                                                   | Pass threshold v2.0 (rev.3)                                                       |
| ----------------------------------------------------- | --------------------------------------------------------------------------------- |
| Fast Execution Finality latency p95                   | < 10s with 200 validators, WAN 5 regions, 0 Byzantine                               |
| Fast Execution Finality latency p95 (degraded)        | < 20s with 200 validators, 1/4 stake offline + 5% packet loss                     |
| Sovereign Epoch Finality latency p95 (rev.3)        | < 90 minutes (6 BTC confirmations + propagation)                                  |
| DA throughput sustained                               | **> 5 MB/s with 200 validators, 64KB–1MB blob mix** (rev.3 lowered from 30 MB/s)   |
| Soft-confirm latency p95                              | < 2s                                                                              |
| State sync from weak-subjectivity checkpoint        | < 30 minutes on **home internet 100 Mbps** (rev.3 lower bound via throughput cap) |
| Light client header verify                            | < 5ms on mobile (Snapdragon 8–class)                                            |
| Storage growth rate                                   | < 100 GB / validator / month at 5 MB/s sustained (rev.3 lowered from 500 GB)      |
| Slashable evidence detection                          | 100% for equivocation; > 99% for data unavailability in the test scenario         |
| Anti-Sybil correlation alarm rate (rev.3)           | < 1% false positives in baseline; ≥ 95% true positives when injecting a 6-node split |


**Not** v2.0 KPIs:
- Application-level TPS (no execution).
- MEV resistance (default OFF; optional fairness mode is specified but not a v2 KPI — see §1.2).
- Cross-rollup atomic latency (out of scope).
- Throughput >5 MB/s — rev.3 moves this to v2.1+ KPIs after DAS is ready.

### 1.4 Finality tiers (rev.3 addition)

Rev.3 introduces an explicit **two-tier finality model** exposed via API:


| Tier                                | Latency target | Underlying mechanism                                        | **Required** use case                                                                      |
| ----------------------------------- | -------------- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| Soft confirmation                   | 0.5–1.5s       | MicroQC (Bullshark anchor commit + micro committee)         | UI preview, revertible fee charge                                                          |
| **Fast Execution Finality** (rev.3) | 5–10s          | MacroQC × 2 (Casper FFG 2-chain rule), accountable slashable | Rollup transactions, capped cross-rollup messaging, bridge withdrawals **below the value cap** |
| **Sovereign Epoch Finality** (rev.3)| ~60 min        | Bitcoin checkpoint (6 BTC confirmations) on the header      | Validator unbonding/rotation **(mandatory)**, bridge withdrawals **above the value cap**, sovereign settlement |


Why split the two tiers (PDF critique §6.4):
- ~5s “hard finality” is safe for ordinary rollup transactions (accountable safety: if reverted, ≥ W/3 stake is slashed).
- **But** once an attacker has unbonded stake (past `WITHDRAWAL_DELAY`), slashing can no longer execute → an external anchor (Bitcoin) is needed to lock history.
- Therefore: validators **cannot unbond** on Fast Execution Finality alone; they must wait until their MacroCheckpoint hash is committed and 6-block-confirmed on Bitcoin.

The spec defines a bridge “value cap” in §8.6 — each bridge protocol picks its own cap from risk appetite; default `1000 × validator_min_stake` for mainnet starter.

---

## 2. System model and threat model

### 2.1 Validator set and stake

Validator set $\mathcal{V} = v_1, \dots, v_N$ with stake weights $w_i \ge w_{\min}$. Total active stake in epoch $e$ is $W_e = \sum_{i \in \text{active}_e} w_i$.

Stake is capped at $w_{\max} = 0.05 \cdot W_e$ — no validator holds more than 5% voting power. Above the cap, excess is “burned to voting power 0” (stake still exists but does not increase vote weight); validators are encouraged to split.

#### 2.1.1 Anti-Sybil obligations (rev.3 addition)

Independent review (`docs/improve.pdf` §6.5) notes that the 5% cap is only a **vanity metric** in a permissionless environment: an entity with 30% stake can trivially split into six anonymous nodes × 5% on the same cloud instance. For the 5% cap to be technically meaningful, each validator must:

1. **Declare network identity** at registration:
   - ASN (Autonomous System Number) of the primary uplink.
   - Cloud provider (if any): AWS/GCP/Azure/Hetzner/self-hosted.
   - Region code (ISO 3166-2 or cloud-specific zone).
   - This declaration is committed on-chain in the activation transaction; changes require re-attestation.

2. **Submit DKG-fingerprint commitment** (rev.3, §9.7):
   - Validator keys are derived through a deterministic-but-blinded process from a stake-address-bound seed.
   - Foundation/governance may (off-chain) verify that multiple validators were derived from the same seed → slashable evidence.

3. **Accept correlation-based reward decay**:
   - Reward × `(1 - decay_rate × concentration_score)` where `concentration_score` is computed from ASN/cloud/voting-pattern overlap with other validators.
   - Details in §10.6.

Validators who do **not** declare ASN/cloud/region are treated as “all unknown” → maximum `concentration_score` → maximum decay (near-zero reward). This is a strong enough incentive for honest declaration without KYC (attackers may fake declarations but pay the opportunity cost of false declarations).

### 2.2 Network

Classic **partial synchrony** (DLS 1988): there exist unknown $\Delta$ and unknown $\text{GST}$ such that after $\text{GST}$, every message between two correct nodes is delivered within $\Delta$. Before GST the adversary fully controls scheduling.

Topology: gossip-based overlay with eager push for metadata (vertex headers, votes) and pull-on-demand for blob chunks. **No** deterministic relay topology (avoids single-point-of-failure patterns like Solana’s turbine block leader).

### 2.3 Trust assumptions


| Primitive                          | Assumption                                                                                                                |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Hash (SHA-256, BLAKE3)             | Collision-resistant, second-preimage-resistant                                                                          |
| Signatures (BLS12-381)             | EUF-CMA secure **under classical assumptions** (rev.3): pre-quantum DLP-hard. **Not** safe against a quantum computer with sufficient qubits. |
| Aggregate signatures (BLS)         | Rogue-key attacks prevented via proof-of-possession                                                                      |
| VRF (ECVRF Edwards25519 default — rev.3) | Pseudo-random, unpredictable for non-holders; bias-resistant at a level acceptable for consensus (Algorand, Polkadot precedent). **Not** post-quantum. iVRF deferred as a v3 option. |
| Bitcoin PoW (rev.3)                | Hashrate ≫ attacker budget; standard Nakamoto consensus with 6-block confirmation rule                                   |
| Time                               | **No** synchronized clocks; only increasing local timeouts                                                           |


**Crypto stack consistency note (rev.3)**: rev.2 claimed “post-quantum readiness” via iVRF while simultaneously using BLS12-381 for aggregation — creating **security theater** (see `docs/improve.pdf` §6.1). Rev.3 states clearly: **all of v2.0 is classical-secure**. PQ migration is a single coordinated event in v3 (BLS → STARK aggregation per Drake 2025; ECVRF → lattice-VRF/iVRF). No patchwork.


### 2.4 Threat model


| Property                                 | Level                                                                                                             |
| ---------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Maximum Byzantine stake for safety       | $f < W/3$                                                                                                       |
| Minimum online honest stake for liveness | $> 2W/3$ after GST                                                                                              |
| Adaptive corruption                      | Allowed — the adversary may choose validators to corrupt after seeing the beacon, but not faster than one epoch |
| Crash + Byzantine combined               | Total $\le f$                                                                                                   |
| Network partition before GST             | Arbitrary allowed; safety preserved                                                                              |
| Network partition after GST              | Ruled out by definition                                                                                         |
| Long-range attack                        | Mitigated by (a) 1-week weak subjectivity (rev.3 lowered from 2 weeks thanks to BTC anchor), (b) **Bitcoin checkpoint default ON** (§8.4); attacker must re-mine BTC PoW history to bypass |
| Data withholding                         | Mitigated by certified vertex + retrieval challenge (Section 5)                                                |
| Sybil via stake split                    | Mitigated (rev.3) by: (a) 5% cap, (b) ASN/cloud declaration + reward decay, (c) DKG-fingerprint slashing (§9.7) |
| Quantum break of BLS / ECVRF (rev.3)     | **OUT OF SCOPE v2.0**: acknowledge classical crypto openly. PQ migration to v3 is a roadmap commitment.        |


Adaptive corruption is stronger than classical BFT and is why VRF private sortition is required for every leader/collector role.

### 2.5 Reference impossibility bounds

LUA-DAG v2 does not violate any of the following:

- **FLP 1985**: deterministic termination is impossible in asynchronous networks with one fault. ⇒ v2 uses partial synchrony, with *eventual* termination after GST.
- **DLS 1988**: $f < N/3$ is a tight bound for partial-synchrony BFT with signatures. ⇒ v2 chooses the 1/3 baseline.
- **CAP**: under partition, v2 chooses **safety over liveness** (hard-finality stalls rather than forks).

---

## 3. Architecture overview

### 3.0 Place in the taxonomy

LUA-DAG v2 belongs to **ebb-and-flow protocols** (Neu et al., IEEE S&P 2021): combining a synchronous dynamically-available protocol (DAG availability + micro-ordering — always live even when participation fluctuates) with a partially synchronous finality gadget (macro layer — accountable hard finality). This matches Ethereum 3-slot finality (3SF, D’Amato 2024) and RLMD-GHOST + finality gadget.

**Rev.3 addition**: Layer 4 (Sovereign Anchor) borrowed from Babylon/Pikachu (Tas 2022, Azouvi 2022) — placing LUA-DAG at the intersection of **ebb-and-flow** and **PoS-checkpointed-into-PoW**. This hybrid pattern aims for “best of both worlds”: throughput + accountable finality of native PoS, plus Bitcoin PoW immutability for long-range protection. Taxonomy placement helps reviewers verify safety/liveness by reusing known ebb-and-flow lemmas + Babylon checkpointing results.

The two-step architecture and separation of execution-verification vs block-propagation-attestation also resemble **Acki Nacki** (Goroshevsky 2024). LUA-DAG’s main differences: (a) dedicated DAG availability layer to load-balance bandwidth; (b) accountable safety via Casper FFG 2-chain rule, not probabilistic safety; (c) macro-finality via the full validator set, not a random committee per block; (d) Sovereign Anchor tier (rev.3) provides Bitcoin-grade immutability for high-value settlement.

### 3.1 Four layers (rev.3; previously three before Layer 4 promotion)

```
                         ┌─────────────────────────────────┐
                         │  Rollup / app-chain client      │
                         │  (execution + own mempool)      │
                         └──────────────┬──────────────────┘
                                        │ submit blob
                                        ▼
        ┌────────────────────────────────────────────────────────┐
        │                                                         │
        │  Layer 1: AVAILABILITY DAG (Narwhal-class)              │
        │  - blobs broken into chunks, erasure-coded              │
        │  - vertex = (round, author, parents, blob_refs, sig)    │
        │  - certified vertex = vertex + 2f+1 sigs                │
        │  Output: causal DAG of certified vertices                │
        │                                                         │
        ├─────────────────────────────────────────────────────────┤
        │                                                         │
        │  Layer 2: MICRO-ORDERING (Bullshark anchor commit)      │
        │  - ECVRF private sortition picks anchor each wave       │
        │  - anchor commit rule: 2f+1 next-round vertices link    │
        │  - committed sub-DAG → deterministic linearization      │
        │  Output: MicroQC at end of each wave                    │
        │                                                         │
        ├─────────────────────────────────────────────────────────┤
        │                                                         │
        │  Layer 3: MACRO-FINALITY (Casper FFG-class)             │
        │  - every W micro-slots, build MacroCheckpoint           │
        │  - all validators vote MACRO-VOTE                       │
        │  - adaptive aggregation (Mode 0 flat / A subnet, rev.3) │
        │  - 2-chain finality rule → Fast Execution Finality      │
        │  Output: MacroQC + finalized header                     │
        │                                                         │
        ├─────────────────────────────────────────────────────────┤
        │                                                         │
        │  Layer 4: SOVEREIGN ANCHOR (rev.3 default ON)           │
        │  - vigilante relayer batches MacroCheckpoint hash       │
        │  - commit to Bitcoin Taproot tx each epoch              │
        │  - 6-block confirmation → Sovereign Epoch Finality      │
        │  Output: BitcoinAnchorProof for epoch_finalized state   │
        │                                                         │
        └────────────────────────────────────────────────────────┘
                                        │ finalized header + (optional) BTC proof
                                        ▼
                         ┌─────────────────────────────────┐
                         │  Rollup / bridge / light client │
                         └─────────────────────────────────┘
```

### 3.2 Why four layers (rev.3)

Rev.3 adds Sovereign Anchor as Layer 4 (instead of an optional add-on as in rev.2). Each layer solves **exactly one problem that other layers do not solve well**:


| Layer                   | Problem                                           | Why layers cannot be merged                                                              |
| ----------------------- | ------------------------------------------------- | ----------------------------------------------------------------------------------- |
| Availability DAG        | Reliable broadcast of large data                 | Merging with ordering ⇒ the leader becomes a bandwidth bottleneck; Narwhal/Tusk-style evidence |
| Micro-ordering          | Fast linearization of the causal DAG               | Merging with DA ⇒ data may be “named” but not available; merging with macro ⇒ slow |
| Macro-finality          | Fast Execution Finality for rollup transactions   | Merging with micro ⇒ small committee lacks accountability; higher aggregate cost     |
| Sovereign Anchor (rev.3)| Long-range / unbond-window protection             | PoS-only finality is insufficient once the attacker has unbonded; needs external trust (BTC PoW) |


### 3.3 Clear boundaries between layers

Each layer exposes **a single interface** to the layer above:

- L1 → L2: `causal_set(round_cut)` returns the set of `CertifiedVertex` with round ≤ round_cut.
- L2 → L3: `micro_head()` returns `MicroCheckpoint { slot, parent_macro, anchor_vertex, committed_sub_dag_root, ordered_blob_refs_root, micro_qc }` (defined in §4).
- L3 → consumer: `latest_finalized()` returns `(MacroHeader, MacroQC)` for the highest checkpoint in state `finalized` (Fast Execution Finality, §3.5).
- L3 + BTC anchor → consumer (rev.3): `latest_epoch_finalized()` returns `(MacroHeader, MacroQC, BitcoinAnchorProof)` for the highest checkpoint with 6-block confirmation on Bitcoin (Sovereign Epoch Finality).
- L3 → rollup API: `blob_status(blob_id)` returns one of `{submitted, accepted, ordered, soft_confirmed, justified, finalized, epoch_finalized}` per the state machine in §3.5 (rev.3 adds `epoch_finalized`).

Inter-layer communication uses only these interfaces. No layer reach-throughs into another layer’s internal state. This matters for testing (each layer tested independently with mocks) and for proofs (per-layer safety proofs composed via interface contracts).

### 3.4 Typical data flow

```
t=0     client → rollup → blob commitment
t+50ms  ingress validator broadcasts chunks + commitment
t+100ms vertex created at round r, references blob
t+250ms 2f+1 votes ⇒ vertex certified
t+500ms anchor at round r+1 has 2f+1 successors at r+2
t+750ms MicroQC issued (2-round wave shortcut)        ← soft_confirmed
...
t+5s    macro window W=8 micro-slots closes
t+5.5s  MacroCheckpoint proposed; adaptive subnet aggregation (rev.3)
t+6.5s  MacroQC formed                                ← justified
...
t+12s   next MacroCheckpoint also justified           ← FAST EXEC FINALIZED
                                                      (rollup transactions safe)
...
t+~30min  MacroCheckpoint hash committed to BTC Taproot tx (each epoch ~30min)
t+~60min  6 BTC blocks confirm                        ← SOVEREIGN EPOCH FINALIZED
                                                      (validator unbond/withdrawal allowed)
```

### 3.5 Blob lifecycle state machine (rev.3 expanded)

The API exposed to rollup developers is a **monotone state machine with a single revert step** (`accepted → soft_confirmed`). Rev.3 adds terminal state `epoch_finalized` to align with the two-tier finality model (§1.4). Every rollup integration must treat states as follows:

```
        submitted
            │
            │  ingress validator receives blob,
            │  broadcasts chunks + Merkle commitment
            ▼
        accepted                      ← API: accepted=true
            │
            │  blob_ref included in ≥ 1 certified vertex
            │  (vertex references blob_ref + Merkle proof for ≥ 1 chunk)
            ▼
        ordered                       ← API: ordered=true
            │
            │  chunk belongs to Closure(A_w) of an anchor commit
            │  (appears in `ordered_blob_refs_root` of a MicroCheckpoint)
            ▼
        soft_confirmed  ← MAY REVERT  ← API: soft_confirmed=true, finalized=false
            │
            │  MicroCheckpoint lies in the window
            │  of justified MacroCheckpoint C_h
            ▼
        justified       ← MAY REVERT  ← API: justified=true, finalized=false
            │
            │  C_{h+1} also justified with parent_hash = hash(C_h)
            ▼
        finalized       ← IRREVERSIBLE except slashable evidence
                                       ← API: finalized=true (Fast Execution Finality)
            │
            │  (rev.3) MacroCheckpoint hash embedded in Bitcoin Taproot tx +
            │  wait for 6 BTC confirmations
            ▼
        epoch_finalized ← TRULY IRREVERSIBLE (rev.3)
                                       ← API: epoch_finalized=true (Sovereign Epoch Finality)
```

**Triggers (exact):**

| Transition                                  | Trigger                                                                                                                        |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `submitted → accepted`                      | Ingress validator issues signed receipt `IngressReceipt { blob_id, commitment, slot, ingress_sig }`; client verifies signature as evidence that at least one validator committed responsibility for the blob (slashable if availability later fails) |
| `accepted → ordered`                        | There exists ≥ 1 `CertifiedVertex` containing `blob_ref` (2f+1 BLS sigs)                                                                       |
| `ordered → soft_confirmed`                  | `MicroQC` for slot $s$ formed; blob lies in `ordered_blob_refs_root` of MicroCheckpoint $s$                                 |
| `soft_confirmed → justified`                | MacroCheckpoint $C_h$ containing the hash of the MicroCheckpoint that contains the blob is justified (has MacroQC)                                      |
| `justified → finalized`                     | $C_{h+1}$ justified with `parent_height_hash(C_{h+1}) = hash(C_h)` (2-chain rule, §7.5) — **Fast Execution Finality**           |
| `finalized → epoch_finalized` (rev.3)       | MacroCheckpoint hash of $C_h$ embedded in a Bitcoin Taproot OP_RETURN tx (via vigilante relayer); 6 BTC block confirmations — **Sovereign Epoch Finality** |

**Revert semantics:**

All transitions are **monotone in an honest node’s local view** in steady state. “Reverts” occur only in specified failure modes:

- `accepted` and `ordered`: **never** revert (Bullshark anchor commit monotonicity theorem, §6.8).
- `soft_confirmed`: monotone **when the `lock_macro` invariant (§11.5) is honored**. Violating `lock_macro` is treated as a protocol bug (test/audit time), not a runtime case.
- `justified → soft_confirmed`: only if MacroQC is orphaned in a macro fork; macro fork requires ≥ W/3 stake equivocation and **always** yields slashable evidence (§11.1). Rollups must wait for resolution.
- `finalized`: irreversible **within the slashable window** (~`WITHDRAWAL_DELAY` = 6 BTC blocks ~ 60 min, rev.3). Past this window an attacker may have unbonded → need `epoch_finalized` for assurance.
- `epoch_finalized` (rev.3): absolutely irreversible — attacker must re-mine ≥ 6 Bitcoin blocks to bypass, economically infeasible at current hashrate.

**Rollup recommendations (rev.3 updated):**

| Use case                                               | Minimum recommended state |
| ------------------------------------------------------ | ------------------------- |
| UI preview for users                                   | `soft_confirmed`          |
| L2 fee charge (revertible)                             | `soft_confirmed`          |
| Cross-rollup messaging (revertible, same ecosystem)    | `finalized`               |
| Bridge withdrawal **below value cap**                  | `finalized` (Fast Exec)   |
| Bridge withdrawal **above value cap** (rev.3)        | `epoch_finalized` (Sovereign) |
| Validator unbond / set rotation (rev.3, **mandatory**) | `epoch_finalized` (Sovereign) |
| Update L1-anchored state root (DA layer settlement)    | `finalized` sufficient; `epoch_finalized` for high-value chains |
| Light client sync read                                 | `justified` enough for passive view; `finalized` for settlement reads |

API contracts:
- `latest_finalized()` returns the highest `finalized` header (Fast Exec).
- `latest_epoch_finalized()` (rev.3) returns the highest `epoch_finalized` (Sovereign) header; may lag `latest_finalized` by ~30–90 minutes.
- If a rollup queries `soft_confirmed` or `finalized`, the response **must** include `revert_risk` (`revert_risk_local: true` for soft; `revert_risk_long_range: true` for finalized but not yet `epoch_finalized`).

---

## 4. Data structures


| Struct                | Purpose                           | Main fields                                                                                                      |
| --------------------- | --------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `Blob`                | Rollup payload unit               | `namespace_id`, `data` (bytes), `commitment` (Merkle root over chunks)                                         |
| `Chunk`               | Blob fragment after erasure coding | `blob_id`, `index`, `data`, `proof` (Merkle proof against commitment)                                           |
| `IngressReceipt`      | Evidence for `accepted` (§3.5)    | `blob_id`, `commitment`, `slot`, `ingress_validator_id`, `ingress_sig` (slashable if availability later fails)   |
| `Vertex`              | DAG vertex                        | `round`, `author`, `parents` (≥ 2f+1 certified vertex hashes from r-1), `blob_refs` (list of commitments), `signature` |
| `CertifiedVertex`     | Vertex + quorum signatures        | `vertex`, `quorum_sigs` (2f+1 BLS aggregate)                                                                     |
| `MicroCheckpoint`     | Output of one wave                | `slot`, `parent_macro`, `anchor_vertex`, `committed_sub_dag_root`, `ordered_blob_refs_root`, `micro_qc`          |
| `MicroQC`             | Aggregate vote on MicroCheckpoint | `slot`, `committee_bitmap`, `bls_aggregate`                                                                      |
| `MacroCheckpoint`     | Hard-finality unit                | `height`, `parent_height_hash`, `micro_head_hash`, `da_root`, `validator_set_root`, `epoch`, `proposer_id`       |
| `MacroQC`             | Aggregate vote on MacroCheckpoint | `height`, `included_subnets` (8-bit bitmap), `subnet_aggregates` (sparse list, indexed by bitmap), `validator_bitmap`, `bls_aggregate`, `total_signed_stake` |
| `MacroHeader`         | Light header for clients          | `height`, `parent_hash`, `micro_head_hash`, `da_root`, `epoch`, `aggregate_sig`, `validator_bitmap_compressed`  |
| `SyncCommitteeUpdate` | Light-client sync committee update | `epoch`, `next_committee_root`, `aggregate_sig`                                                                 |
| `BitcoinAnchorProof` (rev.3) | Evidence for `epoch_finalized` | `macro_height`, `macro_hash`, `btc_txid`, `btc_block_height`, `btc_block_hash`, `merkle_proof_to_btc_block`, `confirmations` (≥6 ⇒ epoch_finalized) |
| `ValidatorIdentity` (rev.3) | Declared network identity      | `validator_id`, `asn`, `cloud_provider`, `region_code`, `attestation_sig`, `epoch_attested`                      |
| `DKGCommitment` (rev.3)| Key-derivation commitment for fingerprinting | `validator_id`, `commitment_root`, `dkg_session_id`, `proof_of_possession`                                      |
| `SlashEvidence`       | Slashable evidence                | `kind`, `validator_id`, `evidence_a`, `evidence_b` (two conflicting signed messages)                             |


Note: **no** struct is named “block” in v2. This is intentional — “block” is overloaded when data + order + finality are merged into one struct.

---

## 5. Layer 1 — Availability DAG

### 5.0 Decision: certified DAG (default v2) vs uncertified DAG (option v2.1)

LUA-DAG v2 defaults to a **certified** Narwhal/Bullshark-style DAG — a vertex may be used as a parent only after it has 2f+1 signatures. Trade-off: one extra round of latency for a stronger availability guarantee and a simpler spec.

2023–2024 literature shows **uncertified** Mysticeti-style DAGs (Babel 2023) can reach ~0.5s WAN latency at 50k+ TPS — roughly 2× faster than certified DAGs. However uncertified DAGs open new attack vectors that Adelie (Chursin 2024) must address. Deferring uncertified mode to v2.1 is deliberate — it reduces spec/audit risk for the first v2 release.

A **low-effort** enhancement is integrated: Shoal-style **leader reputation + pipelining** (Spiegelman 2023) — eliminates timeouts in the common case (“Prevalent Responsiveness”), cutting latency by 40%+ when there are no failures. Reputation scores come from historical liveness and feed anchor selection (see §6.2).

### 5.1 Vertex creation

Each validator $v_i$ in each round $r$:

1. Wait until there are ≥ 2f+1 certified vertices in round $r-1$ (`parents_ready`).
2. Package the local blob queue into `blob_refs` (constraint: total size ≤ `MAX_VERTEX_PAYLOAD = 1 MB`).
3. Create `Vertex { round: r, author: i, parents: parents_ready, blob_refs, signature: sign(...) }`.
4. Gossip the vertex to every other validator.

Round 0 has a genesis vertex per validator (no parents).

### 5.2 Vertex certification

When validator $v_j$ receives vertex $V$ from $v_i$:

1. Verify the signature.
2. Verify all `parents` are certified vertices from round $r-1$ (seen locally).
3. Verify every `blob_ref` has **available** corresponding chunks: on the fast path verify a Merkle proof for at least one received chunk; full availability for all chunks is deferred to the retrieval-challenge mechanism in §5.5 (lazy verification + slashing on failure).
4. If checks pass, $v_j$ signs the vertex hash and gossips the vote.

When $v_i$ collects ≥ 2f+1 votes, it aggregates them into a `CertifiedVertex` and gossips it again.

**Important**: uncertified vertices must **not** be used as parents. This differs from some DAG variants that allow uncertified parents — here we trade one round of latency for a stronger availability guarantee.

### 5.3 Erasure coding for blobs

Each blob is split into $k$ data chunks and expanded to $n = 2k$ chunks via Reed–Solomon (rate 1/2). Any $k$ chunks suffice to recover the blob.

- $k = \lceil |\text{blob}| / 32\text{KB} \rceil$ (fixed chunk data size).
- Commitment = Merkle root over $n$ chunk hashes.
- When gossiping, validators send chunks in parallel across peers. Per Narwhal, each validator sends only $1/N$ of total bytes ⇒ load-balanced.

**Throughput cap v2.0 (rev.3)**: new bytes committed through the DA layer are hard-capped at `THROUGHPUT_HARD_CAP_V20 = 5 MB/s` at runtime — enforced via (a) `MAX_VERTEX_PAYLOAD = 256 KB`, (b) fee-market base-fee parameters targeting ~50% capacity (~2.5 MB/s normal, spikes up to 5 MB/s). Validators reject vertices whose total `blob_refs` size exceeds the vertex payload cap. Rationale for lowering from rev.2’s 30–100 MB/s: see `docs/improve.pdf` §6.2 — 100 MB/s **without** DAS ≈ 8.6 TB/day, runnable as a full node only in datacenters.

**Why 1D instead of 2D in v2.0**: 2D Reed–Solomon (Celestia-style) enables DA sampling for light nodes but adds ~2–4× overhead. v2.0 chooses 1D because:
1. Light clients trust the sync committee for DA (see Section 8).
2. A 5 MB/s throughput cap is low enough for home-internet full nodes to replicate all data — sampling is unnecessary.

**v2.1 transition (rev.3 commitment)**: 2D Reed–Solomon + KZG polynomial commitments (Hall-Andersen 2025 or RLNC-DAS Grundei 2025) are **mandatory** in v2.1. The vertex schema adds a `kzg_commitment` field; during the transition window, legacy vertices (Merkle-only commitments) remain valid for backwards compatibility.

### 5.4 Garbage collection

Vertices and chunks have a three-state lifecycle:

- **Hot**: round ≤ `current - GC_HOT_HORIZON` (default `GC_HOT_HORIZON = 200`). Full bytes must be kept on every validator.
- **Warm**: round below the latest finalized macro but above `GC_WARM_HORIZON` (default 10,000 rounds). Validators must keep at least commitment + one chunk; they may participate in retrieval challenges.
- **Cold**: round more than `GC_WARM_HORIZON` below the finalized macro. Validators may fully garbage-collect. Archive nodes (opt-in role) retain data indefinitely.

### 5.5 Custody assignment and retrieval challenge

#### 5.5.1 Custody assignment (rev.2 addition)

Each chunk of a blob is assigned deterministically to a set of **custody validators**:

$$\text{custody}(blob\_id, chunk\_idx) = \{ v_i : H(\text{pubkey}_i \,\|\, blob\_id \,\|\, chunk\_idx) \bmod N < K_{\text{custody}} \}$$

with `K_custody = 2f+1` (default) ⇒ each chunk has ≥ 2f+1 custody validators. Because assignment uses the VRF beacon in `blob_id` (randomness from ingress validator + slot hash), an adversary with <1/3 stake cannot guarantee capturing the entire custody set of any chunk.

**Custody validator obligations**:

- Store full chunk bytes in the **hot tier** (round ≤ `current - GC_HOT_HORIZON`).
- Answer retrieval challenges for custodied chunks within $T_{\text{retrieve}} = 30$s.
- When a blob enters the warm tier, still retain at least one chunk + Merkle proof.

Validators **not** in the custody set may garbage-collect chunks earlier (after `GC_HOT_HORIZON`); they only keep commitments + a bitmap of “which chunks I have seen” for certification voting.

#### 5.5.2 Retrieval challenge

Any rollup or full node may issue a retrieval challenge `Challenge { blob_id, chunk_idx, challenger_id, deadline }` to **target set** = `custody(blob_id, chunk_idx)`. Each target validator must respond within $T_{\text{retrieve}} = 30$s with:

- `Response { chunk_data, merkle_proof, signature_over_challenge }`, OR
- `Defense { reason: ColdGCed | NotInCustody, proof }` if valid.

**Slashable evidence form**:

$$\text{UnavailabilityEvidence} = \{\text{Challenge}, \text{TargetSet}, \text{NoResponseProof}_{\ge 2f+1 \text{ witnesses}}\}$$

where `NoResponseProof` is **gossip-level evidence**, not data-level: aggregated signed attestations from ≥ 2f+1 validators (witnesses may be any validators, not necessarily in the custody set) attesting to the proposition

$$\text{Witness}_v = \text{"I observed Challenge}_c\text{ gossiped at } t_0\text{; I waited } T_{\text{retrieve}}\text{; I did NOT observe any valid Response/Defense from } v_j \text{ on the gossip topic"}$$

Witnesses only verify (a) signatures on any `Response` they see, (b) wall-clock measurement of the deadline. Witnesses **do not** need to verify chunk correctness — they only assert **silence on the gossip layer**. Honest validators do not lie about this (lying = signed false statement = slashable if the adversary produces counter-evidence such as “I saw a response”), so gossip-level attestation suffices.

When the evidence is valid:

- Slash each non-responding custody validator per `DATA_UNAVAILABILITY_SLASH = 5%`.
- If the **entire** custody set fails (≥ 2f+1 validators do not respond), mark the blob `unavailable`; in addition to slashing the custody set, slash the ingress validator (first to broadcast the commitment) 5% — mitigating ingress validators that commit blobs without actually pushing chunks to gossip.

#### 5.5.3 Constraints

- Challenges **do not** apply to cold blobs (after `GC_WARM_HORIZON`). Rollups needing longer-lived pins must use opt-in archive nodes.
- The same `(blob_id, chunk_idx, challenger)` cannot issue a new challenge more often than `MIN_CHALLENGE_INTERVAL = 60s` (anti-spam).
- Challenges must pay a small fee floor → refunded if the target fails (asymmetric incentive for honest challenges, DoS mitigation).

---

## 6. Layer 2 — Micro-ordering (Bullshark anchor commit)

This is the **most important section** because v1 hand-waved here. v2 uses Bullshark’s actual commit rule instead of a vague “deterministic frontier.”

### 6.1 Wave structure

The DAG is partitioned into waves of four rounds each (steady state may commit in two rounds via the shortcut path). Wave $w$ comprises rounds $4w, 4w+1, 4w+2, 4w+3$.

### 6.2 Anchor selection (VRF private sortition)

At the start of wave $w$, derive randomness beacon $R_w$:

$$R_w = H(R_{w-1} \,\|\, \text{MacroQC of latest finalized})$$

Each validator $v_i$ computes:

$$y_i = \text{ECVRF}_i(R_w \,\|\, \text{"anchor"})$$

The anchor proposer for wave $w$ is the validator minimizing $y_i \cdot W / (w_i \cdot \text{rep}_i)$ in the wave, where $\text{rep}_i \in [0.5, 1.5]$ is a Shoal-style leader reputation score (rolling average of recent liveness — misbehaving anchors lose reputation; healthy anchors are boosted). **Nobody knows the anchor’s identity until the anchor publishes its vertex in round $4w$ — the key defense against adaptive DoS.**

**Trade-off with unbiasability** (rev.3 narrowed): the `rep_i` coefficient is a **non-cryptographic input** from local liveness measurements ⇒ a small attack surface:

- Reputation is derived from DAG observations; if the adversary can influence “whether my vertex is included in parents,” it may bias other validators’ `rep_i`.
- Rev.3 narrows the range: `rep_i ∈ [0.8, 1.2]` (max 1.5× advantage instead of 3×) to shrink the bias surface, trading ~20% of Shoal’s latency improvement (still better than pure stake weighting by hardware margin).
- Reputation **does not** apply to macro proposer selection (§7.2) — only to micro-layer anchor selection, where safety does not directly depend on leader-rotation fairness.
- If reputation bias remains a practical issue on testnet, fall back to `rep_i = 1` for all validators (pure stake-weighted ECVRF).

When the anchor vertex is certified, it reveals the ECVRF proof. Every validator verifies the proof ⇒ confirms the correct anchor.

**Crypto choice (rev.3 — replaces rev.2)**: the spec defaults to **ECVRF Edwards25519 (RFC 9381)**. Rationale:

1. **Consistency with BLS aggregation**: both anchor selection (ECVRF) and macro vote aggregation (BLS12-381) are **classical elliptic-curve-based**. rev.2’s iVRF (PQ claim) plus classical BLS created **security theater** — if a quantum attacker breaks BLS, ECVRF/iVRF no longer matter; the attacker has already forged MacroQC. See `docs/improve.pdf` §6.1.
2. **Maturity**: ECVRF has production-grade reference implementations (libsodium, Algorand, Polkadot) — suitable for the v2 prototype timeline.
3. **Acceptable bias resistance**: ECVRF lacks iVRF’s formal “unbiasability,” but with private sortition + stake-weighted thresholds the bias surface is bounded by the randomness beacon’s grinding resistance (§9.4) — not by the VRF alone.

**Post-quantum migration timeline (rev.3)**: full crypto-stack migration is a single coordinated event in **v3** (not a v2.1 patchwork):
- ECVRF → lattice-based VRF or iVRF (Esgin 2023) once production-ready.
- BLS12-381 → STARK aggregation (Drake 2025) or hash-based multi-signatures.
- A dedicated migration plan will be specified in the v2.1 → v3 transition document.

Keep the iVRF reference in §17 for future v3 use.

### 6.3 Commit rule (steady-state, 2-round shortcut)

Anchor vertex $A_w$ at round $4w$ is **committed** if:

- There exist ≥ $2f+1$ certified vertices at round $4w+1$ where each vertex links directly to $A_w$ via `parents`.

This is the shortcut path — the happy path commits in two rounds (~500ms with 250ms rounds).

### 6.4 Commit rule (slow path, 4-round)

If the shortcut path does not form (anchor vertex uncertified or insufficient links), the wave enters the slow path:

- Round $4w+2$: validators broadcast `wave_vote(w, support_anchor_or_skip)`.
- Round $4w+3$: if ≥ $2f+1$ votes support the anchor, commit. If ≥ $2f+1$ votes skip, the wave is skipped (no `MicroCheckpoint` for this wave; advance to wave $w+1$).

The slow path costs four rounds (~1s) but preserves liveness even if the anchor crashes or the network bursts.

### 6.5 Linearization

When anchor $A_w$ is committed:

1. Compute causal closure $\text{Closure}(A_w)$ = certified vertices with a path to $A_w$ in the DAG, **excluding** the closure of the previously committed anchor ($A_{w-1}$ if any).
2. Sort $\text{Closure}(A_w)$ in **deterministic topological order**: BFS from $A_w$, tie-break by lexicographic vertex hash. All correct nodes compute the same order.
3. In that order, group `blob_refs` by namespace; within a namespace preserve order of appearance.
4. Build `MicroCheckpoint` with `committed_sub_dag_root = MerkleRoot(ordered vertices)` and `ordered_blob_refs_root = MerkleRoot(per-namespace ordered refs)`.

This is true **deterministic linearization** — unlike v1’s “deterministic frontier” without a concrete function.

### 6.6 MicroQC formation

The anchor proposer broadcasts `MicroCheckpoint`. A micro committee of $C_{\text{micro}}$ validators (default 256, weighted-sampled via VRF) signs the MicroCheckpoint hash. When the anchor collects ≥ $\lceil 2/3 \cdot C_{\text{micro}}\rceil$ signatures, it aggregates them into `MicroQC`.

### 6.7 Soft-confirmation contract

When a `MicroQC` exists for slot $s$:

- Every blob in `ordered_blob_refs_root` for slot $s$ is **soft_confirmed**.
- The API exposes this with `soft_confirmed = true, finalized = false, revert_risk = true`.
- Soft confirmation **may revert** under rare conditions (see §3.5 lifecycle and §11.5 cross-layer attacks). Rollups should **not** use soft confirmation for settlement-grade decisions; see the “use case → minimum state” table in §3.5.

### 6.8 Why the Bullshark anchor rule fixes v1

v1 claimed: “`frontier_root` is deterministic because correct nodes apply the same function to the same DAG snapshot.” Problem: **no global “snapshot”** exists in an asynchronous network.

Bullshark fixes this by defining the commit rule via **causal evidence in the DAG**, not timing. When 2f+1 vertices at round $r+1$ link to the anchor at round $r$, that evidence propagates: any correct node that sees those 2f+1 vertices **must** have seen the anchor and parent certificates. Hence all correct nodes eventually agree on the commit.

Bullshark (Spiegelman et al., CCS 2022) proves this formally; v2 reuses the result directly.

---

## 7. Layer 3 — Macro-finality

### 7.1 Cadence

Every $W = 8$ micro-slots (default), the protocol creates one `MacroCheckpoint`. With 250ms rounds and 4-round waves, each micro-slot ≈1s ⇒ macro window ≈8s.

### 7.2 Proposer selection

The macro proposer at height $h$ is chosen by VRF private sortition analogous to anchors (§6.2), but using beacon $R^{\text{macro}}_h$:

$$R^{\text{macro}}_h = H(R^{\text{macro}}_{h-1} \,\|\, \text{MacroQC}_{h-1})$$

A backup proposer is ranked second; if the primary does not publish within $T_{\text{macropropose}} = 4$s, the backup takes over.

### 7.3 MacroCheckpoint construction

The proposer at height $h$ builds:

```
MacroCheckpoint {
  height: h,
  parent_height_hash: hash(MacroCheckpoint_{h-1}),
  micro_head_hash: hash(MicroCheckpoint at end of window),
  da_root: Merkle root over all certified vertex hashes in window,
  validator_set_root: Merkle root of active validator set at h,
  epoch: epoch_of(h),
  proposer_id: i,
}
```

The proposer broadcasts `MacroProposal`.

### 7.4 Adaptive aggregation: Mode 0 / A / B (rev.3 rewrite)

This scales macro voting — the **key** innovation over v1. rev.3 replaces rev.2’s “static 8 subnets” with **adaptive aggregation** based on active validator set size, after `docs/improve.pdf` §6.3 noted that hard-coding eight subnets for 100–500 validators (LUA-DAG’s target, similar to Sui/Aptos) adds unnecessary hops and latency.

#### 7.4.1 Adaptive subnet count

At the start of each epoch $e$, subnet count follows active-set size $N_e$:

$$
K_e = \begin{cases}
0 & \text{if } N_e < N_{\text{flat}} = 500 & \text{(Mode 0: flat gossip)} \\
\left\lceil N_e / 128 \right\rceil & \text{if } N_{\text{flat}} \le N_e \le N_{\text{full}} = 1000 & \text{(interpolated, 4–8 subnets)} \\
\left\lceil N_e / 128 \right\rceil & \text{if } N_e > N_{\text{full}} & \text{(Mode A: subnets)}
\end{cases}
$$

capped at $K_{\max} = 32$ to avoid excessive fragmentation. `SUBNET_TARGET_SIZE = 128` is chosen so each subnet still carries enough stake for internal 2/3 thresholds (not a strict prerequisite — see Mode A step 2 for deadline-based publishing).

**Subnet partitioning** (when $K_e > 0$): rebuild each epoch with the VRF beacon:

$$\text{subnet}(v_i, e) = H(\text{pubkey}_i \,\|\, R^{\text{macro}}_{\text{epoch\_start}(e)}) \bmod K_e$$

Use the VRF beacon instead of `validator_id mod K_e` to stop adversaries from choosing `validator_id` at deposit time to stack one subnet (Sybil-style subnet capture). Rebuilding partitions each epoch prevents committing stake to a specific subnet before the beacon is known.

With `MAX_STAKE_FRACTION = 5%`, per-subnet stake is bounded in a Hoeffding-style way ~`W/K_e + O(√(W/K_e) · MAX_STAKE_FRACTION)`.

#### 7.4.2 Mode 0 — flat gossip (N < 500)

Activated when $K_e = 0$. No subnet structure; aggregation runs at the macro proposer:

1. Every validator receives `MacroProposal`, verifies, signs the hash, gossips partial sigs.
2. The macro proposer collects partial sigs directly; aggregates once `total_signed_stake ≥ 2W/3` or deadline $T_{\text{macropropose}} = 4$s.
3. If the proposer fails / is DoSed ⇒ fall back to Mode B (gossip aggregation, §7.4.4).

**Cost (Mode 0)**:
- Per validator: one sign + one broadcast.
- Macro proposer: aggregates ~$N$ sigs directly; at N=200 with BLS12-381 batch verify ⇒ ~20ms; acceptable.
- Latency savings vs rev.2’s eight-subnet layout at N=200: drop the subnet hop (~$T_{\text{subnet}} = 2$s) ⇒ total macro round-trip ~3s instead of ~5s.

#### 7.4.3 Mode A — subnet-based (N ≥ 500, default for large sets)

Activated when $K_e \ge 4$:

1. Every validator receives `MacroProposal`, verifies, signs the hash, gossips partial sigs with subnet IDs.
2. **Inside each subnet**, a subnet aggregator (ECVRF-rotated) continuously collects partial sigs. It publishes **partial subnet aggregate** `subnet_aggregate[k] = (subnet_id, bitmap, bls_aggregate, signed_stake)` as soon as:
   - It reaches **target threshold** = 2/3 subnet stake (happy path), **OR**
   - It hits **publish deadline** $T_{\text{subnet}} = T_{\text{macropropose}} / 2 = 2$s — publish whatever stake was collected at that moment.

   Dual-condition rationale: if half the subnet stake is offline, the aggregator may never reach internal 2/3 ⇒ stall. Publish-on-deadline ensures subnets still contribute partial evidence ⇒ the macro proposer can combine across subnets.

3. **Macro proposer** collects subnet aggregates and applies the **quorum rule**:

   - **Quorum rule**: aggregate `MacroQC` from **any subset $S \subseteq \{0,\dots,K_e-1\}$** provided:
     - $\sum_{k \in S} \text{signed\_stake}_k \ge \lceil 2W/3 \rceil$, **and**
     - Each `subnet_aggregate[k]` in $S$ passes cryptographic verification (bitmap + BLS pairing).

   Normally $|S| = K_e$ with total stake near $W$. If some subnet aggregators crash or subnets split-brain, the proposer can still reach $2W/3$ using remaining subnets.

   `MacroQC` stores `included_subnets` bitmap (variable-length, $K_e$ bits) + `total_signed_stake` for verifiers to reproduce checks.

#### 7.4.4 Mode B — leaderless gossip aggregation (fallback; applies to Mode 0 and Mode A)

Activated when the macro proposer is DoSed or the primary mode times out after $T_{\text{macropropose}}$:

1. Validators broadcast partial signatures on gossip (with subnet IDs in Mode A).
2. Each node stores a local view of partial sigs seen. Once the quorum rule is met, a local node independently creates a `MacroQC candidate`.
3. **Canonical selection**: multiple nodes may create different MacroQC candidates (same height, same `MacroProposal_hash`, but different `included_subnets` or validator subsets). The spec defines **canonical ordering** over valid candidates (each must satisfy `total_signed_stake ≥ 2W/3`) within window $T_{\text{canonicalize}} = 2 \cdot T_{\text{macropropose}}$:

   $$\text{canonical}(C_1, C_2) = \begin{cases} C_1 & \text{if } \text{total\_signed\_stake}(C_1) > \text{total\_signed\_stake}(C_2) \\ C_2 & \text{if } \text{total\_signed\_stake}(C_2) > \text{total\_signed\_stake}(C_1) \\ \arg\min(\text{validator\_bitmap}) & \text{otherwise} \end{cases}$$

   Meaning: **highest signed stake wins**; tie-break by lexicographically smallest validator bitmap (Mode 0) or subnet bitmap (Mode A). Stake-priority avoids the perverse case where a bare 2W/3 candidate beats a fuller candidate solely due to a smaller bitmap — fuller evidence must win.

4. Every validator must re-broadcast the canonical MacroQC when it observes a higher-stake candidate (or lexicographically smaller bitmap at equal stake). Convergence occurs within $O(\log N)$ gossip rounds (Long et al., IEEE ICBC 2019).

Mode B preserves liveness even if the proposer is Byzantine, at ~2× primary-mode latency.

#### 7.4.5 Aggregate cost analysis

| Mode | Trigger | Per-validator cost | Aggregation cost | Latency overhead vs baseline |
| ---- | ------- | ------------------ | ---------------- | ---------------------------- |
| Mode 0 (N<500) | Default for small/medium sets | 1 sign + 1 broadcast | Macro proposer aggregates ~N sigs (~20ms with batch verify) | 0 (baseline) |
| Mode A (N>1000) | Default for large sets | 1 sign + 1 broadcast (+ subnet ID) | Subnet aggregator ~N/K sigs; macro proposer ~K sigs | +$T_{\text{subnet}} \approx 2$s (subnet hop) |
| Mode B | Proposer DoS / timeout | Same as primary | Each node aggregates locally; canonical convergence | +~$T_{\text{macropropose}}$ (extra gossip round) |

**Light-client verification**: one pairing check + bitmap parsing ⇒ ~3ms (same as rev.2; variable-length bitmap but still O(K), i.e., tiny).

**Why adaptive is right**: for target sets of 100–500 validators (Sui/Aptos-like), Mode 0 (flat) is **always** the default — small-set efficiency. Subnet mode activates only beyond ~1000 validators. This fixes rev.2’s hard-coded eight subnets borrowed from Ethereum-scale (~895k validators), which mismatched LUA-DAG’s profile.

**Crypto stack note**: BLS12-381 remains the default aggregation primitive. Li et al. (2023) show that for committees >40 validators, EdDSA can beat BLS computationally (at the cost of larger signatures). Benchmark on the P2 prototype before freezing — see §12.1.

### 7.5 2-chain finality rule

Following Casper FFG (Buterin & Griffith 2017), ported to the macro-checkpoint chain:

- MacroCheckpoint $C_h$ is **justified** if it has a MacroQC.
- $C_h$ is **finalized** if $C_h$ is justified **and** there exists $C_{h+1}$ justified with `parent_height_hash(C_{h+1}) = hash(C_h)`.

Thus hard finality needs **at least two consecutive macro checkpoints**; total latency = 2 × macro window = 16s worst case, ~10s typical.

### 7.6 Slashing conditions for the macro layer

Slashable evidence (cryptographically provable):


| Violation                  | Definition                                                                                            | Slash %                              |
| -------------------------- | ----------------------------------------------------------------------------------------------------- | ------------------------------------ |
| Macro double-vote          | Signing two `MacroProposal`s at the same height $h$ with different parents                             | 100%                                 |
| Surrounding vote           | Signing a vote for $C_h$ and later for $C_{h'}$ with $h' < h$ where $C_{h'}$ is not an ancestor of $C_h$ | 100%                                 |
| Double-propose             | Same validator publishes two different `MacroProposal`s at the same height                            | 100%                                 |
| Micro double-vote          | Signing two different MicroQCs in the same slot                                                       | 50%                                  |
| Data unavailability proven | Validator signed a certified vertex, then failed $f+1$ retrieval challenges                             | 5% per occurrence, soft-cap 50%/year |


100% slash means all stake is burned and the validator is ejected from the active set. Partial slashes for data unavailability cover transient bugs, not necessarily Byzantine behavior.

### 7.7 Inactivity leak

If the chain does not finalize within `INACTIVITY_LEAK_THRESHOLD = 4` macro windows (~32s), the protocol enters inactivity leak: offline or incorrect-voting validators lose stake gradually ($-0.5\%$ per consecutive macro window, computed on remaining stake) until online honest stake returns above 2/3.

This mirrors Ethereum’s Beacon Chain mechanism and ensures recovery after partitions even if >30% of validators stay offline permanently.

---

## 8. Light client and checkpoint sync

### 8.1 Sync committee

Each epoch (default 1024 macro heights spanning a few hours), the protocol selects `SYNC_COMMITTEE_SIZE = 512` slots (weighted random **with replacement** from the active validator set, seed = `R^{macro}_{epoch_start}`) as the sync committee. A high-stake validator may occupy multiple slots; each slot is an independent signing right. The sync committee signs every `MacroHeader` in that epoch.

Rationale for `with replacement`: keeps `SYNC_COMMITTEE_SIZE` fixed (=512) regardless of `|active_set|`, keeping light-client verification cost deterministic — same pattern as Ethereum 2.0 sync committees.

A light client only needs:

- One trusted weak-subjectivity checkpoint (trust-on-first-use).
- A stream of `MacroHeader` + sync committee aggregate signatures.

Per-header verification: one pairing + bitmap check ⇒ <5ms on mobile.

### 8.2 Sync committee transition

At each epoch end, `SyncCommitteeUpdate` is embedded in the `MacroCheckpoint` at the first height of the new epoch. Light clients verify the transition via signatures from the **old** committee over the **new** committee.

Note: the sync committee has **no** finality authority — it is only a light-client **gateway**. A corrupted sync committee cannot fork the chain; it can only feed invalid headers to light clients (detectable via full-node cross-checks).

### 8.3 Weak subjectivity policy (rev.3 updated)

`WEAK_SUBJECTIVITY_PERIOD = 1 week` (rev.3 lowered from two weeks). Newly syncing nodes must obtain a checkpoint no older than one week from a trusted source (foundation, friend, official explorer). Reasons for lowering:
- Bitcoin checkpointing default ON (§8.4) tightens long-range bounds (~60 minutes vs weeks).
- Weak subjectivity now bootstraps initial sync from a static checkpoint, not the primary defense.
- One week is wide enough for weekend-offline nodes to catch up via P2P while staying safe.

`WITHDRAWAL_DELAY = 6 BTC blocks (~60 min)` (rev.3, replaces “two weeks + one day”). Validator exit request ⇒ enter exit queue ⇒ stake remains locked until the `MacroCheckpoint` containing the exit is Bitcoin-anchored with six confirmations — the **Sovereign Epoch Finality requirement** (§1.4).

This follows Babylon (Tas 2022): with a Bitcoin anchor, the withdrawal window no longer relies primarily on weak-subjectivity protection — the finalized epoch hash is immutable on Bitcoin.

### 8.4 Bitcoin checkpointing (rev.3 default ON)

Tas et al. (2022, “Babylon”) prove impossibility: PoS security issues (non-slashable long-range, low liveness, bootstrap) are **inherent** without an external trusted source. They propose checkpointing PoS state into Bitcoin PoW. Pikachu (Azouvi 2022) applies the same idea with Bitcoin Taproot and constant transaction size.

rev.3 promotes Bitcoin checkpointing from **optional add-on → default ON, mandatory for v2.0 mainnet**, because `docs/improve.pdf` §6.4 showed keeping it optional creates a dangerous time-domain conflict (~5s “hard finality” vs two-week weak subjectivity) for rollup developers.

#### 8.4.1 Mechanism

Each **epoch** (~30 minutes, $W \cdot N_{\text{micro}}$ macro slots), a set of vigilante relayer nodes (subset of active validators) performs:

1. Aggregate `BitcoinCheckpoint = (epoch, macro_height, macro_hash, validator_set_root)` for the latest finalized `MacroCheckpoint`.
2. Aggregate BLS signatures from ≥ 2W/3 validators voting to sign the checkpoint.
3. Embed `(macro_hash || aggregate_sig_compressed)` in a Bitcoin Taproot transaction (script-path spend) — payload ≤ 64 bytes.
4. Broadcast the transaction to the Bitcoin mempool. Pay a reasonable fee (~50 sat/vbyte default; adaptive to congestion).
5. Monitor confirmations; at six confirmations, produce `BitcoinAnchorProof` (§4) and publish on LUA-DAG gossip ⇒ triggers `epoch_finalized` for all `MacroCheckpoint`s in the matching epoch.

#### 8.4.2 Vigilante incentives

- Vigilante relayers earn `BTC_RELAY_REWARD` (parameter, default 5% of macro proposer reward) per successful checkpoint.
- Multiple relayers compete; first confirmed wins the reward (reduces SPOF).
- Operating cost: ~$10K/year for Bitcoin fees + relayer ops (2026 estimate). Funded from protocol treasury (fee-burn allocation) or rollup surcharges.

#### 8.4.3 Failure modes

| Mode | Effect | Mitigation |
| ---- | ------ | ---------- |
| All vigilantes offline | `epoch_finalized` does not advance | Validator unbonding is blocked; the chain still produces Fast Exec Finality for rollup txs |
| Bitcoin reorg > 6 blocks | `BitcoinAnchorProof` invalid | Re-checkpoint the epoch after BTC stabilizes; `epoch_finalized` revert ⇒ accountable halt scenario |
| Bitcoin fee spike (e.g., congestion) | Checkpoint delayed | Adaptive fees + defer to next epoch; SLA: ≤ 2 missed epochs |

**Default ON** with explicit failure modes clarifies UX — rollup developers do not wonder whether a Bitcoin anchor exists.

### 8.5 Checkpoint sync flow

```
1. New node fetches WS checkpoint (from K independent sources, K ≥ 3)
2. Verify checkpoint signatures match validator set published at WS time
3. (rev.3) Cross-check the WS checkpoint against the latest BitcoinAnchorProof on the Bitcoin chain →
   if they disagree, the BTC anchor wins (immutable evidence)
4. Download MacroHeaders + sync committee sigs from checkpoint to head
5. Optionally: download state snapshot at latest finalized height
6. Begin live participation
```

The state snapshot in step 5 is **out of scope for rollup snapshots** — rollups handle their own state snapshots. LUA-DAG only provides DA roots + macro header chain + Bitcoin anchor headers.

### 8.6 Finality boundaries table (rev.3 addition)

rev.3 spells out explicit finality requirements per use case so rollup/bridge developers integrate correctly:

| Use case                                                  | Latency requirement | Mandatory finality tier           | Notes |
| --------------------------------------------------------- | -------------------- | --------------------------------- | ----- |
| Rollup UI preview                                         | < 2s                 | `soft_confirmed`                  | May revert on macro fork; hint “pending” in UX |
| Rollup fee charge (revertible)                            | < 2s                 | `soft_confirmed`                  | Same as above |
| Rollup tx commit (DEX trade, NFT mint, etc.)              | 5–10s                | `finalized` (Fast Exec)           | Accountable safety: revert only if ≥ W/3 stake is slashed |
| Cross-rollup messaging (same ecosystem)                   | 5–10s                | `finalized` (Fast Exec)           | Both chains share LUA-DAG trust assumptions |
| Bridge withdrawal **below `BRIDGE_VALUE_CAP`**            | 5–10s                | `finalized` (Fast Exec)           | Default cap = `1000 × MIN_STAKE`; bridges may tune lower |
| Bridge withdrawal **above `BRIDGE_VALUE_CAP`**          | ~60 min              | `epoch_finalized` (Sovereign)     | High value: must wait for BTC anchor |
| Validator unbond / withdrawal                             | ~60 min              | `epoch_finalized` (Sovereign)     | **Mandatory** — no exceptions |
| Validator set rotation (epoch boundary)                   | ~60 min              | `epoch_finalized` (Sovereign)     | Active-set changes must be Bitcoin-anchored |
| Slashing distribution / treasury withdrawal               | ~60 min              | `epoch_finalized` (Sovereign)     | Long-range protection |
| Light client sync (passive read)                          | ≥ `justified`        | Varies                            | Read-only; depends on risk tolerance |

`BRIDGE_VALUE_CAP` defaults to `1000 × MIN_STAKE` for mainnet starter — bridges may set **lower** values but **not** higher without opting out of Sovereign-grade safety. The cap is parameterized in genesis and adjustable via governance.

**Why this matters**: rev.2 and v1 implicitly mixed two notions of “finalized.” rev.3 enforces separation:
- “Fast Execution Finality” = accountable safety while attackers have not unbonded.
- “Sovereign Epoch Finality” = absolute safety anchored in Bitcoin PoW.

A validator with $W/3$ stake could bypass Fast Execution Finality by: (a) signing conflicting MacroQCs, (b) accepting slashing, (c) unbonding before slashing executes. If `WITHDRAWAL_DELAY < attack_window`, the attack succeeds. The Bitcoin anchor enforces `WITHDRAWAL_DELAY > attack_window` for every unbonding account.

---

## 9. Permissionless membership

v1 used the word “permissionless” without defining how to join or leave. v2 specifies it completely.

### 9.1 Activation queue

Validators deposit stake → enter the activation queue. Activation is limited to `MAX_ACTIVATION_PER_EPOCH = 4` validators per epoch to:

- Avoid sudden validator-set jumps that break stable stake-weight assumptions.
- Give the sync committee time to transition.

While queued, deposits earn no voting power and no rewards.

### 9.2 Withdrawal (rev.3 updated)

Validators request exit → enter the exit queue (`MAX_EXIT_PER_EPOCH = 4`). After exit is accepted, stake remains locked until `epoch_finalized` (§3.5) — `WITHDRAWAL_DELAY = 6 BTC blocks (~60 min)` (rev.3 lowered from two weeks thanks to default-on Bitcoin checkpointing, §8.4).

Detailed flow:
1. Validator submits `ExitRequest { validator_id, epoch_request, exit_sig }`.
2. `ExitRequest` is included in MacroCheckpoint $C_h$ (next available slot, subject to `MAX_EXIT_PER_EPOCH`).
3. $C_h$ must reach `finalized` (Fast Exec) before the exit queue advances.
4. $C_h$ must reach `epoch_finalized` (Sovereign — six BTC confirmations of `BitcoinAnchorProof`) before stake is released.
5. The validator may withdraw funds.

Between steps 3 and 4 (~60 min), the validator remains in the exit queue but **does not** vote, produce vertices, or participate in consensus. Stake remains slashable if equivocation evidence is submitted.

### 9.3 Churn limit

Total (activation + exit) rate is hard-capped so validator-set evolution stays slow enough for:

- Smooth sync-committee transitions.
- Stable VRF beacon predictability.
- Stable subnet rebalancing.

### 9.4 Randomness beacon refresh

Beacon $R$ is derived from the prior MacroQC. Because MacroQC aggregates 2/3 of stake, the adversary cannot bias the beacon without controlling >1/3 stake (at which point they have bigger problems).

**Grinding resistance**: a macro proposer could technically omit some partial signatures (slightly biasing the beacon), but two mechanisms bound this:

- **Inclusion-delay rewards**: validators whose votes are included late suffer exponentially lower rewards. Proposers that deliberately drop votes are reported by those validators and lose macro-proposer rewards.
- **Subnet aggregation guard** (rev.2 updated): subnet aggregators pre-publish `subnet_aggregate[k]` on gossip (see §7.4 Mode A step 2) **before** the macro proposer builds `MacroQC`. Every validator can observe published subnet aggregates. The macro proposer cannot “hide” a published aggregate — intentional exclusion causes validators to refuse `MacroQC` votes (they know a valid aggregate was omitted) and the proposer loses rewards. Proposers may exclude a subnet only if it truly failed to publish by $T_{\text{subnet}}$.

Together, grinding entropy per epoch is capped to a few bits — insufficient to bias VRF anchor selection meaningfully in steady state. This mirrors Ethereum’s RANDAO grinding pattern.

### 9.5 Validator set root publishing

Each `MacroCheckpoint` carries `validator_set_root` — the Merkle root of `(validator_id, pubkey, weight)` tuples. Light clients use this root to verify subnet and sync-committee membership.

### 9.6 Slashing to leave the validator set

If a validator is 100% slashed, it auto-exits. Stake is burned (it does not return to the attacker even via delegation).

### 9.7 Anti-Sybil mechanisms (rev.3 addition)

Direct response to `docs/improve.pdf` §6.5: the 5% cap is a vanity metric if an attacker can trivially split 30% stake into six anonymous 5% nodes. rev.3 adds three defense-in-depth layers:

#### 9.7.1 Network identity declaration

At activation, validators **must** submit `ValidatorIdentity { validator_id, asn, cloud_provider, region_code, attestation_sig }` (§4 struct):

- **ASN**: Autonomous System Number of the primary uplink. Verifiable via passive measurement (PoP-style) by monitoring nodes.
- **Cloud provider**: enum `{aws, gcp, azure, hetzner, ovh, self_hosted, ...}`. If cloud, optional L2 region (e.g., `aws:us-east-1`).
- **Region code**: ISO 3166-2 or cloud-specific zone.
- **Re-attestation**: required every `IDENTITY_REATTEST_PERIOD = 4 epochs` (~2 hours) to capture IP/cloud changes; failure to re-attest is treated as “all unknown” ⇒ maximum `concentration_score` ⇒ maximum reward decay (§10.6).

This is **not** KYC and does not link to legal entities. It only enables **technical clustering** to compute concentration scores.

#### 9.7.2 Concentration score

Each epoch the system computes `concentration_score(v_i)` for each validator $v_i$:

$$
\text{concentration\_score}(v_i) = \alpha \cdot s_{\text{ASN}}(v_i) + \beta \cdot s_{\text{cloud}}(v_i) + \gamma \cdot s_{\text{voting}}(v_i)
$$

where:
- $s_{\text{ASN}}(v_i) = (\sum_{v_j \in \text{same\_ASN}} w_j) / W$ — fraction of stake sharing the ASN.
- $s_{\text{cloud}}(v_i) = (\sum_{v_j \in \text{same\_cloud\_region}} w_j) / W$ — fraction of stake sharing cloud:region.
- $s_{\text{voting}}(v_i)$ = max correlation coefficient with another validator’s voting pattern over a rolling window of 100 macro slots.
- $\alpha, \beta, \gamma$: tuning parameters, default $(0.4, 0.4, 0.2)$.

Validators declaring `self_hosted` (not cloud) have $s_{\text{cloud}} = 0$ — incentive to self-host.

#### 9.7.3 DKG-based key origin fingerprinting

(Applies v2.1+; v2.0 only ships infrastructure to collect data, without enforcement slashing.)

Validators may (optional, opt-in for reward boosts) join a “Distributed Key Ceremony Registry” at activation:
- Generate signing keys via DKG (Pedersen DKG or tBLS) with a foundation-coordinated quorum.
- DKG yields verifiable evidence that keys derive from independent fresh randomness.
- Attackers splitting keys from the same seed ⇒ DKG transcripts expose the origin ⇒ slashable.

`DKG_SLASH_BASE = 20%`; each additional validator detected sharing key origin slashes exponentially: `slash = DKG_SLASH_BASE × 2^(n-1)` for `n` validators sharing an origin (n=2: 20%, n=3: 40%, n=4: 80%, ...).

v2.0 ships the DKG registry as **opt-in** (not mandatory) to gather data; v2.1 makes it mandatory for new activations.

#### 9.7.4 Limitations

Anti-Sybil mechanisms are **not** perfect:
- ASN/cloud declarations can be faked (trade-off: false declarations pay opportunity costs from incorrect concentration).
- VPN/Tor can obscure ASNs; unknown ASNs are treated as maximum concentration.
- Sophisticated attackers with multi-cloud, multi-region accounts can distribute stake ⇒ cost scales ~$n$× versus the bare 5% cap.

Defense-in-depth: the 5% cap plus (a) reward decay, (b) DKG fingerprinting (cryptographic detection in v2.1+), (c) governance review (off-chain social layer for egregious cases) materially raises Sybil costs. It does **not** prove impossibility, but it raises the bar meaningfully.

---

## 10. Incentives and accountability

### 10.1 Reward decomposition

Each epoch, the reward pool splits as:


| Reward type                       | %   | Condition                                                                                |
| --------------------------------- | --- | ---------------------------------------------------------------------------------------- |
| Base reward                       | 28% | Validator online; signatures appear in ≥80% of certified vertices in the epoch          |
| Vertex authoring                  | 15% | Validator’s vertices are certified on time                                               |
| Anchor proposing                  | 10% | Validator selected as anchor and anchor commits successfully                             |
| Micro committee                   | 5%  | Validator selected into the committee and votes MicroQC                                  |
| Macro voting                      | 28% | Validator votes MacroQC on ≥95% of macro heights                                         |
| Macro proposing                   | 5%  | Validator proposes macro checkpoints that become justified                               |
| Subnet/flat aggregation           | 4%  | Validator acts as aggregator (subnet in Mode A, or proposer in Mode 0) and submits on time |
| Bitcoin vigilante relay (rev.3)   | 5%  | Validator serves as vigilante relayer and Bitcoin checkpoint confirms (see §8.4.2)     |


These percentages are parameters adjustable via governance.

### 10.2 Fee market

Ingress validators (receiving blobs from rollups) collect fees from rollups. Fee split:

- 50% to the ingress validator (incentivizes blob processing).
- 30% to the macro proposer (incentivizes inclusion in the DA root).
- 20% burned (deflationary).

EIP-1559-style fee market: base fee tracks target DA usage (default 50% capacity), priority tips go to ingress validators.

### 10.3 Validator economics targets

Steady-state targets:

- Inflation: 4–6% APR on stake (gross).
- Validator operating cost: ~$2–5k/month (cloud) or ~$500/month (self-hosted).
- Minimum stake $50k–100k equivalent ⇒ break-even unless token price is very low.

Detailed tokenomics (distribution, vesting, foundation allocation) are **out of scope** for this technical document — covered in a separate tokenomics paper.

### 10.4 Inactivity leak (see §7.7)

Already specified.

### 10.5 Slashing recipients

Slashed stake is **100% burned** for equivocation/double-votes. For data unavailability and inactivity leaks, part of the slash pool may be allocated to whistleblowers (validators submitting evidence).

### 10.6 Diminishing returns formula (rev.3 addition)

Rewards for validator $v_i$ each epoch are scaled by an anti-Sybil factor:

$$
R_{\text{actual}}(v_i) = R_{\text{base}}(v_i) \times (1 - \text{REWARD\_DECAY\_RATE} \times \text{concentration\_score}(v_i))
$$

where `concentration_score` follows §9.7.2 and `REWARD_DECAY_RATE = 1.0` by default (full decay at maximum clustering). Floor: `R_actual ≥ 0`.

**Examples**:
- Solo validator, declared `self_hosted`, unique ASN, no correlation ⇒ `concentration_score ≈ 0` ⇒ full reward.
- Stake split into six 5% nodes on the same `AWS:us-east-1` ⇒ each node has $s_{\text{cloud}} \approx 0.30$ ⇒ `concentration_score` $\approx 0.4 \times s_{\text{ASN}} + 0.4 \times 0.30 + 0.2 \times s_{\text{voting}}$ ≈ 0.30 ⇒ 30% reward decay per node ⇒ attacker loses ~30% × 30% = 9% APR overall. This is **economic friction**, not a hard barrier; combine with DKG fingerprinting (§9.7.3).
- Declared ASN known to be a Tor exit ⇒ treated as maximum concentration (anti-anonymity feature).

**Bootstrap note**: at genesis with <50 validators, `concentration_score` is structurally high. The spec defines `BOOTSTRAP_GRACE_PERIOD = 30 epochs` (~15 days) where decay is scaled ×0.5 so early adopters are not punished while the set grows.

**Governance-tunable**: `REWARD_DECAY_RATE`, `α/β/γ` in `concentration_score`, and `BOOTSTRAP_GRACE_PERIOD` can be adjusted by governance after mainnet stabilizes.

---

## 11. Security analysis

### 11.1 Safety theorem (macro layer)

**Claim**: Suppose Byzantine stake $f < W/3$. Then no two conflicting `MacroCheckpoint`s are both finalized unless slashable evidence for ≥ $W/3$ stake is produced.

**Proof sketch**: Two finalized checkpoints $C, C'$ on conflicting chains each have MacroQC ≥ 2W/3. The two 2W/3 sets intersect in ≥ W/3 stake. Intersection validators signed votes for both $C$ and $C'$ ⇒ surrounding votes (slashable). $\square$

This is the standard Casper FFG safety argument; v2 reuses it.

### 11.2 Liveness theorem

**Claim**: After GST, if honest online stake > 2W/3, then for every $T > 0$ there exists $T' > T$ such that a new `MacroCheckpoint` finalizes before time $T'$.

**Proof sketch**:

- After GST, gossip among correct nodes delivers within $\Delta$.
- VRF anchor selection gives ≥2/3 probability the anchor is honest each wave.
- Honest anchor + 2f+1 honest vertices in the next round ⇒ shortcut commit.
- Macro proposer with honest >2/3 stake ⇒ collects 2/3 votes ⇒ MacroQC.
- Two consecutive honest macro proposers ⇒ finality. Eventually this occurs. $\square$

Note: liveness is **eventual** only — no deterministic latency upper bound. This follows from FLP and is acceptable for BFT-class protocols.

### 11.3 Committee safety probability

v1 lacked quantitative analysis. v2 supplies it.

Committee size $C$, Byzantine stake fraction $\beta$. Probability that the committee contains ≥ $C/3$ Byzantine stake (committee-level safety violation) is:

$$P(\text{capture}) = \sum_{k=\lceil C/3 \rceil}^{C} \binom{C}{k} \beta^k (1-\beta)^{C-k}$$

Reference table:


| C (committee size) | β = 0.20    | β = 0.30 | β = 0.33 |
| ------------------ | ----------- | -------- | -------- |
| 64                 | 1.5 × 10⁻³  | 0.18     | 0.42     |
| 128                | 2.7 × 10⁻⁵  | 0.06     | 0.40     |
| 256                | 1.0 × 10⁻⁸  | 0.005    | 0.37     |
| 512                | 1.5 × 10⁻¹⁵ | 4 × 10⁻⁵ | 0.34     |
| 1024               | < 10⁻³⁰     | 3 × 10⁻⁹ | 0.30     |


Key observation: near β ≈ 1/3 (global safety threshold), capture probability stays high for **every finite committee size**. Hence the **micro layer must not be marketed as hard finality** — soft confirmation is only safe when β is materially below 1/3.

v2 defaults to `C_micro = 256`. At β = 0.2, capture probability ≈10⁻⁸ — acceptable for UX-level confirmation. At β > 0.3, soft confirmation loses meaning and rollups should trust only hard finality.

### 11.3.1 Probabilistic robustness (rev.2 addition)

Worst-case 1/3 Byzantine analysis alone is **insufficient** for operations. Mighan et al. (2024) model Ethereum-like consensus with Markov chains and show consensus probability is **highly sensitive** to truthful voting rates. Even without Byzantine adversaries, “rational defection” (slow votes, mistaken votes from local state, abstention) at 5–10% can materially reduce throughput.

v2 must measure the following on adversarial testnets (P6):


| Metric                                                    | Pass threshold               |
| --------------------------------------------------------- | ---------------------------- |
| Truthful vote rate at macro layer                       | > 95% under normal conditions |
| Truthful vote rate in micro committee                     | > 92% under normal conditions |
| MacroQC formation p95 latency under 5% rational defection | < 7s                         |
| Time to recover after `INACTIVITY_LEAK_THRESHOLD`         | < 2 epochs                   |


If measured truthful vote rates fall below 90%, investigate root causes (network, client bugs, incentives) before mainnet.

### 11.4 Attack matrix and mitigations


| Attack                                          | Effect                                              | Mitigation (rev.3)                                                                                                                                       |
| ------------------------------------------------- | ---------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Long-range PoS fork                               | Tricks newly syncing nodes onto a fake chain        | One-week weak subjectivity + default-on Bitcoin checkpoint (§8.4) — attacker must re-mine BTC PoW history                                               |
| Data withholding                                  | Soft-confirmed blobs without retrievable data       | Certified vertices only accept blobs with verified chunks; retrieval challenges with slashing                                                           |
| Anchor DoS                                        | Loses shortcut commit; falls back to slow path      | VRF private sortition, 4-round slow path, backup propagation                                                                                             |
| Macro proposer DoS                                | Loses one macro window                              | Mode B leaderless gossip aggregation (§7.4.4); 4s timeout                                                                                                |
| Equivocation                                      | Conflicting checkpoints                             | 100% slashable evidence; 2-chain rule ensures accountable safety                                                                                          |
| Subnet capture (Mode A only, $k$ subnets >2/3 Byzantine) | Bad subnet aggregates corrupt MacroQC        | VRF-rebuilt subnet partitions each epoch (anti stacking); proposer verifies each subnet; quorum rule can skip bad subnets if remaining subnets sum ≥2W/3 (§7.4.3) |
| Mode B canonical race                             | Two honest validators pin different MacroQC candidates | Stake priority + lex tie-break (§7.4.4); convergence in $O(\log N)$ gossip rounds                                                                        |
| MEV ordering manipulation                           | Anchor reorders to extract MEV                    | Default OFF (rollup handles); optional Fino-style fairness (§1.2) when rollups opt in                                                                  |
| Storage spam                                      | Meaningless bytes through DA                       | Fee market + min fee floor + per-slot blob caps; v2.0’s 5 MB/s cap naturally bounds spam                                                               |
| Sync committee corruption                         | Light clients fed invalid headers                   | Sync committee has no finality power; full-node cross-checks catch issues; Bitcoin anchor (rev.3) is a third source of truth                             |
| VRF grinding                                      | Biased anchor selection                            | Beacon from MacroQC ⇒ grinding needs >1/3 stake; deterministic aggregation rule                                                                          |
| Eclipse attack on light client                    | Isolated light client                              | Multi-source headers recommended; Bitcoin anchor as external authority; deployment-level defenses                                                        |
| Adaptive corruption after VRF reveal            | Corrupt anchor only after reveal                   | Round duration < adaptive corruption window (~hours) ⇒ irrelevant                                                                                       |
| Sybil via stake split (rev.3)                     | One entity fakes six anonymous 5% validators       | (a) 5% cap, (b) ASN/cloud declarations + reward decay (§9.7.2, §10.6), (c) DKG fingerprint slashing v2.1+ (§9.7.3)                                       |
| Validator unbond + long-range (rev.3)             | Attacker unbonds ⇒ no slash ⇒ forge old chain      | `WITHDRAWAL_DELAY = 6 BTC blocks`; stake locked until `epoch_finalized` — never unbond without Bitcoin anchoring                                        |
| Bitcoin reorg attack                              | BTC reorg >6 blocks invalidates anchor             | Re-checkpoint after BTC stabilizes; accept rare `epoch_finalized` revert as a failure mode                                                               |
| Post-quantum attack (rev.3)                       | Future quantum breaks BLS + ECVRF                | OUT OF SCOPE v2.0 — acknowledge openly. Full PQ migration is a single coordinated **v3** event (BLS→STARK aggregation, ECVRF→lattice-VRF; see §13.4)   |


### 11.5 Cross-layer attack: micro flush after macro stall

**Scenario**: The macro layer stalls with 35% stake offline. Inactivity leak runs. Meanwhile the micro layer keeps committing quickly (committee-only). Rollups soft-confirm thousands of blobs. When macro resumes, the macro chain might finalize an **older** micro-chain, reverting all newer soft-confirmed blobs.

**Mitigation**: The micro layer must honor `lock_macro` — anchors in wave $w$ are built only on DAGs whose ancestor is `lock_macro = latest_justified_macro_checkpoint`. When macro stalls, `lock_macro` does not advance ⇒ anchors may still commit but must reference the same macro parent. When macro resumes, `MacroProposal` includes `micro_head` from that DAG — no revert.

**`lock_macro` race** (rev.2): under partial synchrony, two honest validators may disagree locally on `latest_justified_macro_checkpoint` — node A sees MacroQC for $C_h$, node B does not. If A pins anchors at $C_h$ while B still pins $C_{h-1}$, anchors can conflict in the same wave.

Resolution rules:

- Validators MUST **never downgrade** `lock_macro` (monotone: advance only, never roll back).
- When observing anchor `A` with `lock_macro(A) > my_lock_macro`, validators MUST **fetch** the MacroQC justifying `lock_macro(A)` and update locally before voting to certify `A`.
- Anchors with `lock_macro(A) < my_lock_macro` are rejected ⇒ no cert ⇒ wave skips via slow path (§6.4). One-wave latency penalty but no safety loss.
- `MicroCheckpoint` stores `parent_macro = lock_macro(A)` explicitly (§4) ⇒ easy consistency audits.

Only edge case: macro fork (two justified conflicting macro chains) ⇒ one side must be dismissed via slashing (§11.1). While resolving, all soft confirmations on both sides are unreliable; rollups must trust only `finalized` (§3.5).

### 11.6 Honest minority resilience

If honest stake = 51% and Byzantine = 49%:

- Liveness holds if Byzantines only withhold votes.
- Safety holds — Byzantines <2/3 cannot forge MacroQC.
- Soft confirmation is almost useless (committee capture >50%).

If Byzantines exceed 1/3:

- Safety breaks — conflicting MacroQCs may appear but **will** yield slashable evidence.
- This is an “accountable” failure mode — halt, slash, social recovery.
- Unbonded validators (past `WITHDRAWAL_DELAY`) evade slashing **but** (rev.3) the Bitcoin anchor keeps history immutable ⇒ forged subtrees cannot persist.

If Byzantines exceed 1/2:

- Censorship is possible — they can block honest blobs from the DAG.
- Liveness breaks for honest workloads.
- Safety still requires slashing to break (attackers still lose stake if not yet unbonded).
- (rev.3) Bitcoin checkpoints still protect history — attackers control only go-forward progress, not rewriting past beyond the latest BTC anchor.

### 11.7 Sybil resistance analysis (rev.3 addition)

Answering `docs/improve.pdf` §6.5: a bare 5% cap is **not** enough against Sybil in permissionless settings. rev.3’s defense-in-depth model:

#### 11.7.1 Attacker cost model

Assume attacker stake $w_A$ with $0.05 < w_A/W < 0.33$ (above the 5% cap per key but below global Byzantine threshold):

| Strategy | Cost | Effectiveness |
| -------- | ---- | ------------- |
| Single validator (raw) | 95% stake burned if attack succeeds | Voting power capped at 5% |
| Split into $n = w_A / 0.05$ anonymous nodes, same cloud:region (rev.2 attack) | Stake + cloud costs | Pre-rev.3: full $w_A$ voting power. rev.3: ~70% after reward decay (§10.6). |
| Split + multi-cloud distribution | Stake + ~$n$× cloud costs | ~85% voting power (good ASN diversity but correlated voting) |
| Split + multi-cloud + multi-region + DKG evasion (v2.1+) | Stake + ~$n$× cloud + sophisticated key gen | ~95% voting power; DKG fingerprinting may still detect via behavior |

Conclusion: rev.3 adds **economic friction** to every Sybil strategy — not a hard barrier, but raises attack cost ~1.5–10× depending on strategy.

#### 11.7.2 Detection-rate targets (new KPIs in §1.3)

Adversarial testnet P6 must measure:
- Anti-Sybil correlation alarm rate at baseline (no Sybil): <1% false positives.
- Detection rate when injecting a six-node split: ≥95% true positives.
- Detection latency: <5 epochs (~30 min).

If detection <90%, investigate alternatives — raise $\gamma$ (voting-correlation weight), enable mandatory DKG, or escalate socially.

#### 11.7.3 Limitations and remediation

The spec acknowledges:
- Deep-pocket attackers (multi-cloud, multi-region, sophisticated keys) may still evade detection.
- VPN/Tor bridges can hide ASNs.
- Mandatory DKG waits for v2.1 until crypto research matures.

Mitigations:
- Governance may pause/slash validators with extreme behavioral anomaly scores (last resort).
- The foundation may ban validators proven Sybil via off-chain investigation (Cosmos-style precedent).

---

## 12. Performance plan and expected numbers

### 12.1 Benchmark matrix


| Parameter            | Sweep values                                                                         |
| -------------------- | -------------------------------------------------------------------------------------- |
| Validator count      | 50, 100, 200, 350, 500                                                                 |
| Network topology     | Single DC, 5-region WAN, 10-region WAN                                                 |
| Round duration       | 100ms, 250ms, 500ms                                                                    |
| Wave length          | 4 (Bullshark default), 6, 8                                                            |
| Macro window W       | 4, 8, 16 micro-slots                                                                   |
| Micro committee size | 128, 256, 512                                                                          |
| Subnet count         | 4, 8, 16                                                                               |
| Blob size mix        | 4KB-only, 64KB-only, mixed (4KB-1MB)                                                   |
| Erasure rate         | 1/2, 1/4                                                                               |
| Byzantine fraction   | 0%, 10%, 20%, 30%                                                                      |
| Failure scenarios    | 0 fault; 10% offline; 20% offline; partition 30s; data withholding 5%; equivocation 5% |


### 12.2 Metrics to measure

- **Throughput**: bytes/s of blobs ingested, certified, soft-confirmed, finalized.
- **Latency**: p50/p95/p99 for ingest→cert, ingest→soft, ingest→finalized.
- **Bandwidth**: per-validator inbound/outbound MB/s, broken down by (vertex, vote, chunk, sig).
- **CPU**: per-validator, broken down by (sig verify, aggregate, hash, IO).
- **Storage**: GB/day for hot, warm, cold tiers; archive storage.
- **Recovery**: time-to-finality after partition heals; time to drain inactivity leak.
- **Fairness**: inclusion delay distribution per blob; per-namespace fairness.
- **Security signal rate**: false-positive rate for retrieval challenges; correlation of slashing events.

### 12.3 Expected bottleneck order (hypothesis)

Based on Narwhal/Bullshark/Mysticeti experience:

1. **Network bandwidth** for chunk gossip — likely bottleneck #1 above ~50 MB/s sustained.
2. **BLS verify pipeline** — bottleneck #2 above ~200 validators (each vertex needs 2f+1 sig verifies).
3. **Disk write IOPS** for the hot tier — bottleneck #3 on non-NVMe SSDs.
4. **Subnet aggregation latency** — bottleneck #4 above ~500 validators.

v2 is designed up front with:

- Parallel chunk gossip across peers (load-balanced).
- BLS verify batching (batch verify N signatures faster than N× singles).
- Append-only LSM vertex storage (RocksDB / Sled).
- Subnet aggregation pipelines (subnet aggregation in parallel with macro propose).

### 12.4 Comparison baselines

The v2 prototype benchmarks side-by-side against:

- **Celestia (Tendermint-based)** — DA throughput, finality latency.
- **Avail (BABE+GRANDPA)** — DA sampling cost.
- **Narwhal+HotStuff** — pure DAG mempool baseline.
- **Bullshark** — anchor commit baseline.

Same workload, topology, and hardware. Avoids v1’s apples-to-oranges comparisons.

---

## 13. Roadmap, risks, open questions

### 13.1 Phased delivery


| Phase                                | Milestone                                                          | Duration                 | Headcount              |
| ------------------------------------ | ------------------------------------------------------------------ | ------------------------ | ---------------------- |
| P0: Spec finalize                    | Complete whitepaper + TLA+ skeleton                                | 2 months                 | 2 protocol + 1 formal  |
| P1: Prototype L1 (Availability DAG)  | Vertex + cert + erasure + GC                                       | 3 months                 | 3 distsys              |
| P2: Prototype L2 (Bullshark)         | Anchor commit, fast/slow path, MicroQC, ECVRF                      | 3 months                 | 2 protocol + 1 perf    |
| P3: Prototype L3 (Macro)             | Adaptive aggregation (Mode 0/A/B), 2-chain finality, slashing      | 3 months                 | 2 protocol + 1 crypto  |
| P3.5: Bitcoin anchor (rev.3)         | Vigilante relayer, Taproot tx batching, BitcoinAnchorProof gossip    | 2 months                 | 1 protocol + 1 BTC dev |
| P4: Light client + state sync        | SDK + checkpoint sync, BTC anchor verification                     | 2 months                 | 2 client               |
| P5: Permissionless membership + anti-Sybil (rev.3) | Activation/exit queue, beacon, ASN/cloud declaration, concentration score, opt-in DKG registry | 3 months                 | 1 protocol + 1 testing + 1 anti-fraud |
| P6: Adversarial testnet              | Fault injection, partition recovery, anti-Sybil scenarios, public report | 3 months                 | full team              |
| P7: External audit + hardening       | 2 audits (consensus + crypto), fuzzing, chaos, BTC interop check   | 3 months                 | external + 1 internal  |
| **Total to v2.0 prototype–mainnet-ready** |                                                                  | **~24 months (parallel)** | **~12–14 core staff**    |
| P8: v2.1 — DAS implementation        | 2D Reed-Solomon + KZG OR RLNC-DAS; light client DAS verification    | 4 months                 | 2 crypto + 2 client    |
| P9: v3 — PQ migration                | STARK aggregation + lattice-VRF; coordinated migration             | 6+ months                | TBD (separate program) |


This is an honest estimate for a **v2.0 prototype** sufficient to start a public testnet (rev.3 increased from ~21 → ~24 months due to P3.5 Bitcoin anchor and expanded P5 anti-Sybil). **Production-grade** with ecosystem (rollup integrations, SDK, multi-client) needs another 12–24 months and a larger team (20–40 people). v2.1 and v3 are separate programs with their own scope.

### 13.2 Risk register


| Risk                                 | Severity                          | Mitigation                                                                                                  |
| ------------------------------------ | --------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| Cross-layer interaction bug          | High                              | TLA+ model checking from P0; integration tests with chaos suite                                             |
| Adaptive aggregation correctness     | High                              | Prioritize crypto audit; fallback to Mode B leaderless gossip                                               |
| Liveness under partial outage        | High                              | Inactivity leak; anchored slow path; Mode B fallback                                                        |
| Performance misses v2.0 target (5 MB/s) | Low (rev.3, lower bar)       | 5 MB/s throughput cap is conservative — if 200 validators cannot hit it, major debugging is required     |
| Bitcoin checkpoint operational issues (rev.3) | High                     | Multiple vigilante relayers (no SPOF); adaptive fees; explicit failure-mode handling (§8.4.3)              |
| Bitcoin reorg > 6 blocks (rev.3)     | Low (probability ~10⁻⁹/checkpoint)| Re-checkpoint after BTC stabilizes; documented failure mode                                                 |
| Storage growth out of control        | Low (rev.3, throughput cap)       | GC policy tested on adversarial testnet; dedicated archive nodes                                           |
| Sync committee corruption            | Medium                            | Client cross-validation; multi-source headers; Bitcoin anchor as third source-of-truth (rev.3)              |
| Token launch / regulatory            | Medium                            | Out of scope for this doc; separate legal review required                                                   |
| Adoption: which rollups use it?      | High                              | Bootstrap with 1–2 design partners before launch                                                            |
| Celestia ecosystem competition       | High                              | Clear differentiation: faster finality, Sovereign Epoch tier, accountable slashing, lower validator HW    |
| Anti-Sybil detection accuracy (rev.3) | Medium                           | P6 adversarial testnet with explicit Sybil scenarios; tune α/β/γ; foundation review fallback               |
| DKG mandatory for v2.1               | Medium                            | Crypto research must mature; v2.0 ships opt-in registry to gather data                                        |
| Quantum readiness (rev.3 honest)     | **Acknowledged v2.0 limitation** | v2.0 is **not** post-quantum (consistent stack: classical ECVRF + BLS); full migration in **v3** (§13.4) |


### 13.3 Open questions to resolve before writing the implementation plan

**Resolved in rev.3** (no longer open):
- ~~VRF cipher suite~~: **LOCKED ECVRF Edwards25519** (rev.3, §6.2). iVRF moved to v3 deferred list.
- ~~Aggregation primitive~~: BLS12-381 default for v2.0; EdDSA tradeoff still needs P2 benchmarks.
- ~~Bitcoin checkpoint mode~~: **LOCKED default ON, mandatory for v2.0 mainnet** (rev.3, §8.4).
- ~~Reputation range~~: **LOCKED narrow `[0.8, 1.2]`** (rev.3, §6.2).
- ~~Subnet count hard-code~~: **LOCKED adaptive** (rev.3, §7.4). Mode 0/A/B with thresholds 500/1000.

**Still open** (resolve before implementation):
- Reed-Solomon library: build in-house vs existing (`reed-solomon-erasure`, RaptorQ)? Default: `reed-solomon-erasure` for v2.0.
- Implementation language: Rust (default, Sui/Solana ecosystem), Go (Cosmos ecosystem), Zig (experimental). Default: Rust.
- State machine for validator role: actor model (Tokio) vs explicit FSM. Default: actor model.
- Light client SDK targets: TypeScript (web), Rust (native), Swift/Kotlin (mobile)? Default: TypeScript first.
- Tokenomics: defer to a separate doc. Min stake, inflation rate, fee burn rate need economics modeling.
- Governance: on-chain (delegated voting on macro layer) vs off-chain (foundation initially). Default: off-chain v1, on-chain v2.
- Bridge integration: build a reference Ethereum bridge first or defer? Default: defer.
- Multi-client strategy: bootstrap with one client; fund a second client after the second audit. Default: confirmed.
- **Optional MEV fairness mode (rev.2)**: enable Fino-style integration in v2 or defer? Default: spec ready, default OFF, rollup opt-in.
- **Formal verification framework (rev.2)**: TLA+ vs Coq + LiDO-DAG framework. Default: TLA+ in P0, evaluate Coq from P1 onward.
- **Mode B canonical MacroQC tie-break (rev.2, §7.4.4)**: lex-smallest bitmap (current) — still needs P6 verification that it does not create perverse incentives.
- **`lock_macro` advance protocol (rev.2, §11.5)**: dedicated gossip topic vs piggyback. Default: dedicated in P3 prototype.
- **Custody assignment churn (rev.2, §5.5.1)**: 1-epoch grace period; needs storage-spike risk check.
- **Bitcoin vigilante economics (rev.3, §8.4.2)**: $10K/year Bitcoin tx fees — funded from treasury vs rollup surcharge? Needs economic modeling.
- **Bitcoin Taproot vs alternative payload (rev.3, §8.4.1)**: Taproot script-path spend (current default) vs alternatives (OP_RETURN commitment, drivechain, BitVM). Default Taproot — evaluate fee trade-offs in P3.5.
- **Anti-Sybil concentration weights (rev.3, §9.7.2)**: $\alpha = 0.4, \beta = 0.4, \gamma = 0.2$ default — validate via P6 adversarial testnet across Sybil scenarios.
- **DKG ceremony protocol (rev.3, §9.7.3)**: Pedersen DKG vs threshold BLS DKG vs FROST. Decision required for v2.1; v2.0 ships opt-in scaffolding.
- **`BRIDGE_VALUE_CAP` default (rev.3, §8.6)**: `1000 × MIN_STAKE` default; needs feedback from design-partner bridges.

### 13.4 Deferred to v2.1+ (**not** in v2.0 prototype scope)

**v2.1 (hard requirement, scope-locked):**
- **DA sampling for light verification (mandatory v2.1)** — open design among 2D Reed-Solomon + KZG (Celestia-style), RLNC-based DAS (Grundei 2025), and polynomial-commitment-based (Hall-Andersen 2025). Decision deadline: end of P7. When DAS ships, throughput target unlocks from 5 → 30+ MB/s.
- **Mandatory DKG-based key-origin fingerprinting (rev.3)** — v2.0 ships opt-in scaffolding; v2.1 enforces for new validator activations.
- **Uncertified DAG mode** (Mysticeti-style) with Adelie mitigations — option to ~2× throughput after v2.0 stabilizes.

**v3 (major migration, separate program):**
- **Post-quantum signature migration (rev.3 reverted to v3)** — single coordinated event: BLS → STARK aggregation (Drake 2025) **and** ECVRF → lattice-VRF/iVRF. Rev.3 explicitly avoids a v2.1 patchwork — full migration timing aligned with Ethereum PQ roadmap and crypto research maturity.
- **GNN-based adaptive parameter tuning** (DAGWise++ 2025).

**Future / outside v2–v3 scope:**
- Restaking integration (EigenLayer AVS / Babylon BTC restaking).
- Shared sequencing layer (cross-rollup atomicity).
- ZK header proofs for bridge optimization.
- State rent / storage pricing.
- Cross-chain message passing (IBC-style).
- Validator reputation extensions (advanced beyond Shoal-style basic rep).

---

## 14. Head-to-head comparison: LUA-DAG v1 vs v2 vs similar protocols

### 14.1 v1 → v2


| Aspect                             | v1                                    | v2 rev.3 (current)                                                                       | Improvement vs v1                                            |
| ---------------------------------- | ------------------------------------- | ---------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| Scope                              | Generic L1 with execution placeholder | DA + finality only, no execution                                                         | Narrower scope ⇒ shippable                                   |
| Frontier rule                      | Vague “deterministic” wording         | Bullshark anchor commit, proven                                                          | Fixes the largest theoretical gap                            |
| Macro voting                       | Full validator set, naive aggregation | Adaptive Mode 0/A/B (rev.3) — flat <500, subnet >1000, fallback gossip                    | Scales validator count flexibly, no SPOF                   |
| Permissionless detail              | One line saying “permissionless”      | Full spec: activation/exit/churn + anti-Sybil declarations (rev.3)                       | Actually implementable                                       |
| Soft vs hard contract              | Risk notes but no API                 | Explicit API `accepted/soft/finalized/epoch_finalized` (rev.3)                           | Prevents rollups misusing tiers                              |
| MEV resistance                     | One line on “aged-inclusion fee”      | Optional Fino-style mode (rev.2)                                                         | Available for DeFi rollups without trading speed             |
| Long-range protection              | 2-week WS only                        | 1-week WS + Bitcoin checkpoint default ON (rev.3, ~60min withdrawal)                     | Sovereign-grade safety, shorter UX for validators            |
| Finality model                     | Vague single-tier “finalized”         | Two-tier: Fast Execution Finality (5–10s) + Sovereign Epoch Finality (~60min) (rev.3)   | Clean separation for rollup developers                       |
| VRF crypto                         | Generic VRF                           | ECVRF Edwards25519 (rev.3 — consistent with BLS, no PQ illusion)                         | Honest crypto stack, mature libraries                        |
| Anti-Sybil                         | 5% cap only                           | 5% cap + ASN/cloud declaration + concentration-based reward decay + DKG fingerprint (rev.3) | Defense-in-depth; higher economic cost for stake-split attacks |
| Committee safety                   | Qualitative only                      | Concrete probability tables + Markov robustness section (rev.2)                          | Defensible to reviewers                                      |
| Cross-layer recovery               | Unspecified                           | §11.5 specified                                                                          | Avoids micro-flush bugs                                     |
| Throughput target v2.0           | “30–100 MB/s” (rev.2 aspirational)    | **5 MB/s hard cap** with decentralization-preserving HW (rev.3); 30+ MB/s in v2.1 with DAS | Honest tradeoff: decentralization vs throughput              |
| PQ readiness positioning           | “PQ migration roadmap”                | v2.0 explicitly classical; v3 single coordinated migration (rev.3)                        | Avoids “PQ illusion” pitfall                                 |
| Market positioning                 | “Compete with every L1”               | “Compete in DA segment, distinguished by Sovereign tier”                                | Clear differentiation                                        |
| Effort estimate                    | 6–8 people × 12–18 months             | 12–14 people × ~24 months to testnet (rev.3 +3 months for BTC anchor & anti-Sybil)        | More honest estimate                                         |


### 14.2 LUA-DAG v2 vs similar protocols (rev.2 addition)


| Protocol                            | Overlap with LUA-DAG                                                         | Differentiation (rev.3)                                                                                                                                |
| ----------------------------------- | ------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Celestia (Tendermint + DA)**      | Modular DA + finality, comparable v2.0 throughput (~5 MB/s)                    | Celestia uses single-leader Tendermint; LUA-DAG uses a DAG (better load balancing) + 2-tier finality with Bitcoin anchor (rev.3) — no Sovereign tier on Celestia |
| **Avail (BABE + GRANDPA + KZG DA)** | Modular DA, polynomial commitments                                             | Avail has DA sampling from day one; LUA-DAG v2.0 defers DAS to v2.1 **but** has faster hard finality + Sovereign tier via Bitcoin                            |
| **Mysticeti / Sui**                 | DAG-based BFT, ebb-and-flow                                                    | Mysticeti is a full L1 with execution; LUA-DAG is DA-only + dual-tier accountable finality (Casper FFG + Bitcoin)                                         |
| **Acki Nacki**                      | 2-step consensus; separates execution-verification from propagation-attestation | Acki Nacki uses probabilistic safety + random committee per block; LUA-DAG uses full-validator macro votes + accountable safety + Bitcoin anchor        |
| **Babylon**                         | PoS with Bitcoin checkpoint (rev.3 heavily inspired)                            | LUA-DAG has native DAG availability + accountable finality; Babylon is a retrofit checkpoint protocol. Rev.3 natively integrates a Babylon-style anchor as Layer 4. |
| **EigenDA**                         | Modular DA for rollups, raw throughput target                                  | EigenDA borrows security from ETH restakers (DAC-like), no native consensus; LUA-DAG has native PoS + accountable slashing + Bitcoin Sovereign tier        |
| **Fino**                            | DAG + MEV resistance                                                           | Fino is an MEV-resistance overlay only; LUA-DAG (rev.2) integrates optional Fino-style fairness mode                                                         |


---

## 15. Appendix A — Parameter reference table


| Parameter                            | v2.0 default (rev.3)                | Tuning range |
| ------------------------------------ | ----------------------------------- | ------------ |
| `ROUND_DURATION`                     | 250 ms                              | 100–500 ms   |
| `WAVE_LENGTH`                        | 4 rounds                            | 2–8 rounds   |
| `MACRO_WINDOW_W`                     | 8 micro-slots                       | 4–16         |
| `MICRO_COMMITTEE_SIZE`               | 256                                 | 128–1024     |
| `SUBNET_FLAT_THRESHOLD` (rev.3)      | 500 validators                      | 200–1000     |
| `SUBNET_FULL_THRESHOLD` (rev.3)      | 1000 validators                     | 500–2000     |
| `SUBNET_TARGET_SIZE` (rev.3)         | 128 validators/subnet               | 64–256       |
| `SUBNET_MAX_COUNT` (rev.3)           | 32                                  | 16–64        |
| `MAX_VERTEX_PAYLOAD`                 | **256 KB** (rev.3 down from 1 MB)   | 64 KB – 1 MB |
| `MAX_BLOB_SIZE`                      | **1 MB** (rev.3 down from 8 MB)     | 256 KB – 8 MB |
| `THROUGHPUT_HARD_CAP_V20` (rev.3)    | 5 MB/s sustained                    | fixed v2.0   |
| `ERASURE_RATE`                       | 1/2                                 | 1/2 – 1/4    |
| `GC_HOT_HORIZON`                     | 200 rounds                          | 100–500      |
| `GC_WARM_HORIZON`                    | 10 000 rounds                       | 5 000–50 000 |
| `MAX_ACTIVATION_PER_EPOCH`           | 4                                   | 2–8          |
| `MAX_EXIT_PER_EPOCH`                 | 4                                   | 2–8          |
| `WEAK_SUBJECTIVITY_PERIOD`           | **1 week** (rev.3 down from 2 weeks)| 3–14 days    |
| `WITHDRAWAL_DELAY`                   | **6 BTC blocks (~60 min)** (rev.3)  | 6–18 BTC blocks |
| `INACTIVITY_LEAK_THRESHOLD`          | 4 macro windows                     | 2–16         |
| `INACTIVITY_LEAK_RATE`               | 0.5% / window                       | 0.1–1%       |
| `EQUIVOCATION_SLASH`                 | 100%                                | fixed        |
| `DOUBLE_VOTE_SLASH`                  | 100%                                | fixed        |
| `DATA_UNAVAILABILITY_SLASH`          | 5% per occurrence                   | 1–10%        |
| `K_CUSTODY`                          | 2f+1                                | f+1 – 2f+1   |
| `T_RETRIEVE`                         | 30 s                                | 10–120 s     |
| `MIN_CHALLENGE_INTERVAL`             | 60 s                                | 30–300 s     |
| `T_MACROPROPOSE`                     | 4 s                                 | 2–8 s        |
| `T_SUBNET`                           | 2 s (= T_MACROPROPOSE / 2)          | 1–4 s        |
| `T_CANONICALIZE`                     | 8 s (= 2 × T_MACROPROPOSE)          | 4–16 s       |
| `SYNC_COMMITTEE_SIZE`                | 512                                 | 256–1024     |
| `SYNC_COMMITTEE_PERIOD`              | 1024 macro height                   | 256–4096     |
| `MIN_STAKE`                          | parameterized; suggest $50–100k equiv | —        |
| `MAX_STAKE_FRACTION`                 | 5%                                  | 1–10%        |
| **Bitcoin anchor (rev.3, §8.4)**     |                                     |              |
| `BTC_CHECKPOINT_EPOCH_PERIOD`        | 1 LUA epoch (~30 min)               | 15–120 min   |
| `BTC_CONFIRMATIONS_FOR_FINAL`        | 6 blocks                            | 3–18         |
| `BTC_RELAY_REWARD`                   | 5% of macro proposer reward         | 1–10%        |
| `BTC_MIN_FEE_RATE`                   | 50 sat/vbyte (adaptive)             | dynamic      |
| **Anti-Sybil (rev.3, §9.7, §10.6)**  |                                     |              |
| `IDENTITY_REATTEST_PERIOD`           | 4 epochs (~2 hours)                 | 1–24 epochs  |
| `REWARD_DECAY_RATE`                  | 1.0 (full decay at max concentration) | 0.5–2.0  |
| `CONCENTRATION_ALPHA` (ASN weight)   | 0.4                                 | 0.2–0.6      |
| `CONCENTRATION_BETA` (cloud weight)  | 0.4                                 | 0.2–0.6      |
| `CONCENTRATION_GAMMA` (voting weight)| 0.2                                 | 0.1–0.4      |
| `BOOTSTRAP_GRACE_PERIOD`             | 30 epochs (~15 days)                | 0–60 epochs  |
| `DKG_SLASH_BASE` (v2.1+)             | 20%                                 | 10–50%       |
| **Bridge integration (rev.3, §8.6)** |                                     |              |
| `BRIDGE_VALUE_CAP`                   | 1000 × MIN_STAKE                    | bridge-customizable, lower allowed |


## 16. Appendix B — Glossary

- **Anchor**: vertex chosen by VRF as the commit point for a wave.
- **Anti-Sybil concentration score** (rev.3): a number in $[0, 1]$ reflecting how collocated a validator is with others by ASN, cloud:region, or voting pattern. Higher ⇒ more reward decay.
- **Bitcoin anchor / Sovereign Anchor (rev.3)**: hash of the latest finalized MacroCheckpoint committed in a Bitcoin Taproot tx; after 6 BTC confirmations it becomes an immutable proof for `epoch_finalized` state.
- **Blob**: byte payload from a rollup; the DA unit.
- **Bridge value cap (rev.3)**: threshold above which bridge withdrawals must wait for `epoch_finalized` instead of `finalized`.
- **Certified vertex**: vertex with 2f+1 signatures; usable as a parent.
- **Causal closure**: set of vertices with a path to a given vertex in the DAG.
- **DKG fingerprint (rev.3)**: cryptographic evidence (via a Distributed Key Generation registry) that many validator keys share the same origin seed → slashable.
- **Fast Execution Finality** (rev.3): finality state after the 2-chain MacroQC rule (~5–10s); accountable-safe within the slashable window.
- **Hard finality**: deprecated term — replaced by “Fast Execution Finality” or “Sovereign Epoch Finality” (rev.3).
- **MacroQC**: aggregate signature of 2/3 stake on a MacroCheckpoint.
- **MicroQC**: aggregate signature of 2/3 of the micro committee on a MicroCheckpoint.
- **Mode 0 / A / B (rev.3)**: three adaptive aggregation modes by Active Set size (flat / subnet-based / leaderless gossip fallback). See §7.4.
- **Soft confirmation**: state after MicroQC; may revert under rare conditions.
- **Sovereign Epoch Finality** (rev.3): finality state after Bitcoin checkpoint 6-confirmation; cannot be bypassed without re-mining BTC PoW.
- **Sync committee**: set of 512 signing slots (sampled with replacement from the active validator set) signing headers for the light client for one epoch. A validator may hold multiple slots.
- **Validator identity (rev.3)**: declared `(asn, cloud_provider, region_code)` tuple per validator; input to `concentration_score`.
- **Vigilante relayer (rev.3)**: validator role responsible for batching MacroCheckpoint hashes into a Bitcoin Taproot transaction.
- **Wave**: 4-round window for one anchor commit attempt.
- **Weak subjectivity**: must trust a recent checkpoint to sync safely. Rev.3 reduces the window from 2 weeks to 1 week thanks to the Bitcoin anchor.

## 17. Appendix C — Primary references

### 17.1 Foundational

1. Castro & Liskov, "Practical Byzantine Fault Tolerance", OSDI 1999.
2. Yin et al., "HotStuff: BFT Consensus in the Lens of Blockchain", PODC 2019.
3. Buterin & Griffith, "Casper the Friendly Finality Gadget", arXiv 1710.09437, 2017.
4. David et al., "Ouroboros Praos: An Adaptively-Secure Semi-Synchronous PoS Blockchain", EUROCRYPT 2018.
5. Chen et al., "Algorand: Scaling Byzantine Agreements for Cryptocurrencies", SOSP 2017.
6. Dwork, Lynch, Stockmeyer, "Consensus in the Presence of Partial Synchrony", JACM 1988.
7. Fischer, Lynch, Paterson, "Impossibility of Distributed Consensus with One Faulty Process", JACM 1985.
8. Neu, Tas, Tse, "Ebb-and-Flow Protocols: A Resolution of the Availability-Finality Dilemma", IEEE S&P 2021.

### 17.2 DAG-based BFT

1. Danezis et al., "Narwhal and Tusk: A DAG-based Mempool and Efficient BFT Consensus", EuroSys 2022.
2. Spiegelman et al., "Bullshark: DAG BFT Protocols Made Practical", CCS 2022.
3. Spiegelman et al., "Shoal: Improving DAG-BFT Latency And Robustness", arXiv 2023.
4. Babel et al., "Mysticeti: Low-Latency DAG Consensus with Fast Commit Path", arXiv 2023.
5. Chursin, "Adelie: Detection and prevention of Byzantine behaviour in DAG-based consensus", arXiv 2024.
6. Ladelsky et al., "On Quorum Sizes in DAG-Based BFT Protocols", IEEE ICBC 2025.
7. Qiu et al., "LiDO-DAG: A Framework for Verifying Safety and Liveness of DAG-Based Consensus Protocols", PACMPL 2025.

### 17.3 Data availability

1. Nazirkhanova et al., "Information Dispersal with Provable Retrievability for Rollups (Semi-AVID-PR)", AFT 2022.
2. Hall-Andersen et al., "Foundations of Data Availability Sampling", IACR Commun. Cryptol. 2025.
3. Grundei et al., "From Indexing to Coding: A New Paradigm for Data Availability Sampling", arXiv 2025.
4. Fisch et al., "Permissionless Verifiable Information Dispersal (Data Availability for Bitcoin Rollups)", IEEE S&P 2025.
5. Cohen et al., "Proof of Availability and Retrieval in a Modular Blockchain Architecture", IACR ePrint 2022/2023.

### 17.4 Finality

1. D'Amato et al., "3-Slot-Finality Protocol for Ethereum", arXiv 2024.
2. Saraswat et al., "SoK: Speedy Secure Finality", 2025.
3. Zanolini, "A Simple Single Slot Finality Protocol For Ethereum", IACR ePrint 2023.
4. Tapolcai et al., "Fully Decentralized Collection of Attestations for Single-Slot Finality in Ethereum", IEEE ICDCS 2025.
5. Goroshevsky et al., "Acki Nacki: A Probabilistic Proof-of-Stake Consensus Protocol with Fast Finality", 2024.

### 17.5 Cryptography

1. Esgin et al., "A New Look at Blockchain Leader Election: Simple, Efficient, Sustainable and Post-Quantum (iVRF)", AsiaCCS 2023.
2. Giunta et al., "Unbiasable Verifiable Random Functions", 2024.
3. Drake et al., "Hash-Based Multi-Signatures for Post-Quantum Ethereum", IACR ePrint 2025.
4. Boneh et al., "Compact Multi-Signatures for Smaller Blockchains", 2018.
5. Hofmeier et al., "One For All: Formally Verifying Protocols which use Aggregate Signatures", IEEE CSF 2025.
6. Li et al., "Performance of EdDSA and BLS Signatures in Committee-Based Consensus", 2023.
7. Long et al., "Scalable BFT Consensus Mechanism Through Aggregated Signature Gossip", IEEE ICBC 2019.

### 17.6 PoS security & long-range

1. Gazi et al., "Stake-Bleeding Attacks on Proof-of-Stake Blockchains", CVCBT 2018.
2. Tas et al., "Bitcoin-Enhanced Proof-of-Stake Security: Possibilities and Impossibilities (Babylon)", IEEE S&P 2023.
3. Azouvi et al., "Pikachu: Securing PoS Blockchains from Long-Range Attacks by Checkpointing into Bitcoin PoW", 2022.
4. Azouvi et al., "Winkle: Foiling Long-Range Attacks in Proof-of-Stake Systems", AFT 2020.
5. Mighan et al., "Performance of Ethereum 2.0-Like Consensus Under Single-Slot Finality", IEEE ICC 2024.
6. Babylon Labs, "Babylon Architecture (vigilante relayers, Taproot OP_RETURN payload)", 2024–2026 docs.

### 17.7 MEV resistance

1. Malkhi et al., "Maximal Extractable Value (MEV) Protection on a DAG (Fino)", 2022.
2. Kavousi et al., "BlindPerm: Efficient MEV Mitigation with an Encrypted Mempool and Permutation", IACR ePrint 2023.
3. Yang et al., "SoK: MEV Countermeasures: Theory and Practice", arXiv 2022.
4. Nasrulin et al., "LO: An Accountable Mempool for MEV Resistance", Middleware 2023.

### 17.8 Anti-Sybil & decentralization (rev.3 addition)

1. Douceur, "The Sybil Attack", IPTPS 2002.
2. Cohen et al., "Proof of Personhood / SybilQuorum", IEEE S&P 2017.
3. Stathakopoulou et al., "Proof of Latency in Decentralized Networks", 2024.
4. Pedersen, "A Threshold Cryptosystem without a Trusted Party", EUROCRYPT 1991 (foundation for DKG).
5. Komlo & Goldberg, "FROST: Flexible Round-Optimized Schnorr Threshold Signatures", SAC 2020.

### 17.9 External critique source (rev.3)

1. `docs/improve.pdf` — "In-depth assessment of LUA-DAG v2 architecture: analysis of the data availability (DA) model, software structure, and redesign proposals" (original Vietnamese title, 2026) — independent review identifying five architectural mismatches in rev.2 and proposing five redesign directions. Rev.3 implements all five recommendations.

### 17.10 Operational

1. Ethereum.org, "Consensus Mechanisms / Sync Committees / Weak Subjectivity", 2024–2026.

