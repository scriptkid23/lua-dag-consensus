# LUA-DAG v2 — Modular DA + Accountable Finality on a DAG

> **Phiên bản**: v2 design draft (rev. 3 sau external critical review)
> **Ngày**: 2026-05-10
> **Trạng thái**: Spec đề xuất — chưa implement, chưa audit
> **Tiền nhiệm**: `docs/luadag.pdf` (LUA-DAG v1), `docs/improve.pdf` (đánh giá độc lập rev.2)
> **Mục đích**: Tái thiết kế LUA-DAG theo positioning "modular DA + finality layer" (Celestia-class), khắc phục các lỗ hổng lý thuyết và thiếu sót thiết kế đã được nhận diện trong đánh giá v1, **và xử lý các architectural mismatch của rev.2 đã được external critique chỉ ra**.
> **Rev. 2 changelog**: cập nhật theo literature review (Consensus MCP, 70 papers): chuyển default VRF sang iVRF (post-quantum, unbiasable), bổ sung tùy chọn uncertified DAG kiểu Mysticeti, two-mode subnet aggregation (leader + leaderless fallback), tùy chọn Bitcoin checkpoint kiểu Babylon, vocabulary alignment với "ebb-and-flow / 3-slot finality", differentiation vs Acki Nacki, advance PQ migration từ v3 lên v2.1.
> **Rev. 3 changelog (PDF-driven, 2026-05-10)** — xử lý 5 architectural mismatch đã được independent review chỉ ra:
> 1. **Đồng nhất cấp độ mật mã (resolve PQ paradox)**: downgrade default VRF từ iVRF (PQ-claim) sang **ECVRF (Edwards25519)** ở v2.0; loại bỏ "post-quantum" labeling khỏi v2.0; PQ migration toàn stack (BLS → STARK aggregation, ECVRF → iVRF/lattice-VRF) lùi sang **v3** (consistent timeline với Ethereum PQ roadmap, Drake 2025).
> 2. **Throughput target hợp lý + DAS làm hard requirement**: hạ DA throughput target v2.0 xuống **5 MB/s sustained** (ngang Celestia hiện hành) để cho phép home-internet full node; mục tiêu 30–100 MB/s **bắt buộc đi kèm DAS** (Reed-Solomon 2D + KZG, hoặc RLNC-DAS) ở v2.1 — KHÔNG ship 100 MB/s mà không có DAS.
> 3. **Adaptive subnetting**: bỏ hard-code 8 subnets; introducing 3-mode aggregation: **flat gossip** cho N<500 (Mode 0), **subnet-based** cho N>1000 (Mode A — adaptive count = ⌈N/128⌉), **interpolated** cho 500≤N≤1000.
> 4. **Phân định finality boundaries**: Bitcoin checkpointing (Babylon-style) chuyển từ **optional → default ON**; định nghĩa explicit hai tier: **Fast Execution Finality** (5–10s, MacroQC, dùng cho rollup transactions) vs **Sovereign Epoch Finality** (~60min, 6 BTC blocks, BẮT BUỘC trước khi validator unbond/withdraw hoặc bridge release lượng lớn).
> 5. **Multi-dim anti-Sybil**: 5% stake cap được giữ NHƯNG bổ sung lớp behavioral + crypto: (a) IP/ASN/cloud declaration với reward decay theo concentration score, (b) DKG-based key origin fingerprinting, (c) exponential slashing nếu chứng minh được shared key origin giữa các "validator" được tách ra.

---

## 0. Tóm tắt điều hành

LUA-DAG v2 là một giao thức **đồng thuận và data availability** dành cho rollups và app-chains. Giao thức **không có execution layer**, **không cạnh tranh trực tiếp với L1 generic** như Sui/Aptos/Monad, mà cạnh tranh trong segment modular DA + shared finality, đối thủ chính là Celestia, Avail và EigenDA.

So với v1, v2 thực hiện tám thay đổi cốt lõi:

1. **Loại bỏ execution và "block consensus" generic** — thu hẹp scope tới đúng hai sản phẩm: published-and-available data, và accountable hard-finality của một header chain.
2. **Thay quy tắc frontier "xác định" mơ hồ của v1 bằng Bullshark anchor commit rule** — đây là điểm sửa lý thuyết quan trọng nhất.
3. **Macro-finality dùng adaptive subnet BLS aggregation (rev.3)** — flat gossip cho validator set nhỏ (<500), subnet-based cho lớn (>1000); thay vì hard-code 8 subnets như rev.2.
4. **Permissionless membership được spec hoàn chỉnh** — activation queue, withdrawal delay, churn limit, randomness beacon refresh; v1 chỉ nói "permissionless" mà không định nghĩa.
5. **Soft-confirm và hard-finality có hợp đồng API rõ ràng** — `accepted`, `soft_confirmed`, `finalized`, `epoch_finalized` (rev.3) là bốn trạng thái user-visible khác nhau, nhằm ngăn rollup developer dùng nhầm tier yếu hơn như tier mạnh hơn.
6. **Safety analysis xác suất cho committee được trình bày bằng số cụ thể** — không gloss-over.
7. **Bitcoin checkpoint default ON (rev.3)** — định nghĩa hai tier finality: Fast Execution Finality (5–10s, transaction-grade) vs Sovereign Epoch Finality (~60min, 6 BTC blocks, validator-rotation-grade).
8. **Multi-dim anti-Sybil (rev.3)** — bổ sung behavioral + crypto layer trên 5% stake cap để chống stake-split sybil.

Các trục KHÔNG đổi so với v1: PoS, partial synchrony, accountable safety dưới 1/3 Byzantine stake, VRF private sortition, accountable finality kiểu Casper FFG, light client với sync committee.

Performance target (kỳ vọng có cơ sở, chưa chứng minh thực nghiệm):


| Metric                          | Target v2.0                              | Target v2.1+ (sau khi DAS sẵn sàng)    | So sánh tham chiếu                     |
| ------------------------------- | ---------------------------------------- | -------------------------------------- | -------------------------------------- |
| DA throughput sustained         | **5 MB/s** (rev.3, hard-cap)             | 30–100 MB/s với DAS                    | Celestia ~6 MB/s, Avail ~2 MB/s (2024) |
| Soft-confirm latency p95        | 0.5–1.5s                                 | 0.5–1.5s                               | Bullshark ~2s, Mysticeti ~600ms        |
| Fast Execution Finality p95     | 5–10s (MacroQC, rollup tx-grade)         | 5–10s                                  | Celestia ~12s, Ethereum ~15min         |
| Sovereign Epoch Finality p95    | **~60 min** (6 BTC blocks, rev.3)        | ~60 min                                | Babylon ~60 min                        |
| Validator HW yêu cầu (v2.0)     | **8 vCPU / 32 GB / 1 TB NVMe / 100 Mbps** (rev.3 hạ xuống nhờ throughput cap) | 16 vCPU / 64 GB / 2 TB NVMe / 500 Mbps (v2.1 với DAS) | v2.0 home-internet friendly; v2.1 datacenter |


**Lưu ý rev.3**: rev.2 đặt target 30–100 MB/s ngay v2.0 nhưng KHÔNG có DAS — independent review (`docs/improve.pdf`) đã chỉ ra đây là "decentralization suicide" vì 100 MB/s ≈ 8.6 TB/ngày, chỉ datacenter-grade hardware mới chạy được full node. Rev.3 tách target thành hai phase: v2.0 ship ở mức decentralization-preserving (5 MB/s), v2.1 ship throughput cao + DAS đồng thời.


---

## 1. Định vị, scope, và success metrics

### 1.1 Định vị

LUA-DAG v2 = **"published, available, and finalized blob data"** as a service. Khách hàng là rollups, app-chains, và bridge protocols — KHÔNG phải end user của dApp.

Mỗi rollup gửi vào LUA-DAG:

- Blobs (tx batches của rollup, đã serialize).
- Optionally: state-root commitments / fraud proof references.

LUA-DAG cung cấp lại:

- **Bằng chứng availability** (DAG vertex certificates).
- **Bằng chứng ordering** (anchor commit) — đảm bảo blob được sắp thứ tự deterministically *trong* mỗi namespace của rollup.
- **Bằng chứng hard-finality** (macro checkpoint header với BLS aggregate signature).
- **Slashable evidence** khi có vi phạm an toàn.

Rollup tự lo execution, tự lo cross-rollup atomicity (nếu cần), tự lo encrypted mempool. LUA-DAG **không sequence theo nghĩa shared sequencer** — đây là quyết định scope quan trọng để giữ tốc độ và độ phức tạp dưới tầm kiểm soát.

### 1.2 What we are NOT

Phần này tồn tại vì v1 cố gắng làm quá nhiều thứ trong một spec.

- **NOT a generic L1**: không EVM, không Move VM, không SVM, không có execution.
- **NOT a shared sequencer**: rollup giữ ordering layer của mình; LUA-DAG chỉ commit thứ tự của blobs trong cùng namespace.
- **OPTIONAL MEV resistance** (revised rev.2): mặc định MEV là rollup concern, nhưng spec cung cấp **optional fairness mode** dựa trên Fino-style integration (Malkhi et al. 2022) — zero message overhead, không cần threshold encryption. Rollup opt-in tại deployment.
- **NOT a light DA at v2.0** (rev.3 clarified): không có DA sampling cho light node ở v2.0 — đây là lý do throughput v2.0 bị **hard-cap ở 5 MB/s** (full node phải tải toàn bộ data). DAS BẮT BUỘC ở v2.1 và là điều kiện tiên quyết để mở throughput >5 MB/s. Light client v2.0 phải tin sync committee về DA (giống Celestia full-replication light mode).
- **NOT post-quantum at v2.0** (rev.3 honest labeling): rev.2 quảng bá iVRF "post-quantum" nhưng cùng lúc dùng BLS12-381 (KHÔNG post-quantum) cho aggregation — đây là "ảo giác bảo mật". Rev.3 thừa nhận **toàn bộ crypto stack v2.0 là classical** (ECVRF + BLS12-381). PQ migration là một **single coordinated event ở v3** (BLS → STARK aggregation, ECVRF → lattice/iVRF), không patchwork.
- **NOT a restaking AVS**: safety và liveness primary đều dựa hoàn toàn trên native PoS stake — không borrow trust live cho consensus từ Ethereum/BTC. Bitcoin **không vote, không ký block, không tham gia path safety/liveness của MacroQC**.
- **Bitcoin checkpoint = default ON ở v2.0** (rev.3 promoted from optional): Babylon-style checkpoint của latest finalized MacroCheckpoint vào Bitcoin Taproot mỗi epoch là **bắt buộc** ở v2.0 mainnet, không phải optional. Lý do: chỉ với checkpoint này, validator unbonding/rotation mới có window an toàn ngắn (60 min ~ 6 BTC blocks) thay vì chu kỳ weak-subjectivity 2 tuần. Xem §1.4 và §8.6 cho hai-tier finality model.

### 1.3 Success metrics (KPI)

Một implementation v2 được coi là "successful" nếu trên adversarial testnet:


| KPI                                                   | Threshold pass v2.0 (rev.3)                                                       |
| ----------------------------------------------------- | --------------------------------------------------------------------------------- |
| Fast Execution Finality latency p95                   | < 10s với 200 validator, WAN 5 region, 0 Byzantine                                |
| Fast Execution Finality latency p95 (degraded)        | < 20s với 200 validator, 1/4 stake offline + 5% packet loss                       |
| Sovereign Epoch Finality latency p95 (rev.3)          | < 90 phút (6 BTC confirmations + propagation)                                     |
| DA throughput sustained                               | **> 5 MB/s với 200 validator, blob size 64KB–1MB mix** (rev.3 hạ từ 30 MB/s)      |
| Soft-confirm latency p95                              | < 2s                                                                              |
| State sync from weak-subjectivity checkpoint          | < 30 phút trên **home internet 100 Mbps** (rev.3 lower bound nhờ throughput cap)  |
| Light client header verify                            | < 5ms trên mobile (Snapdragon 8-class)                                            |
| Storage growth rate                                   | < 100 GB / validator / tháng tại 5 MB/s sustained (rev.3 hạ từ 500 GB)            |
| Slashable evidence detection                          | 100% cho equivocation; > 99% cho data unavailability trong test scenario          |
| Anti-Sybil correlation alarm rate (rev.3)             | < 1% false positive trong baseline; ≥ 95% true positive khi inject 6-node split   |


KPI **không** phải cho v2.0:
- TPS application-level (vì không có execution).
- MEV resistance (mặc định OFF; optional fairness mode được spec nhưng không là KPI cho v2 — xem §1.2).
- Cross-rollup atomic latency (out of scope).
- Throughput >5 MB/s — rev.3 promotes this to v2.1+ KPI sau khi DAS sẵn sàng.

### 1.4 Finality tiers (rev.3 addition)

Rev.3 introduce explicit **two-tier finality model**, được expose qua API:


| Tier                                | Latency target | Underlying mechanism                                        | Use case BẮT BUỘC                                                                          |
| ----------------------------------- | -------------- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| Soft confirmation                   | 0.5–1.5s       | MicroQC (Bullshark anchor commit + micro committee)         | UI preview, revertible fee charge                                                          |
| **Fast Execution Finality** (rev.3) | 5–10s          | MacroQC × 2 (Casper FFG 2-chain rule), accountable slashable | Rollup transactions, cross-rollup messaging với cap, bridge withdrawal **dưới value cap**  |
| **Sovereign Epoch Finality** (rev.3)| ~60 min        | Bitcoin checkpoint (6 BTC confirmations) trên header        | Validator unbonding/rotation **(bắt buộc)**, bridge withdrawal **trên value cap**, sovereign settlement |


Lý do tách hai tier (PDF critique §6.4):
- 5s "hard-finality" là an toàn cho rollup transactions thông thường (accountable safety đảm bảo: nếu revert, ≥ W/3 stake bị slash).
- NHƯNG khi attacker đã unbonded stake (vượt qua `WITHDRAWAL_DELAY`), slashing không còn execute được → cần một anchor external (Bitcoin) để khóa lịch sử.
- Vì vậy: validator **không thể unbond** chỉ dựa vào Fast Execution Finality; phải đợi đến khi MacroCheckpoint hash của họ được commit và 6-block-confirmed trong Bitcoin.

Spec định nghĩa "value cap" cho bridge ở §8.6 — bridge protocol chọn cap riêng dựa trên risk appetite, default `1000 × validator_min_stake` cho mainnet starter.

---

## 2. System model và threat model

### 2.1 Validator set và stake

Tập validator $\mathcal{V} = v_1, \dots, v_N$ với trọng số stake $w_i \ge w_{\min}$. Tổng stake hoạt động tại epoch $e$ là $W_e = \sum_{i \in \text{active}_e} w_i$.

Stake bị cap ở $w_{\max} = 0.05 \cdot W_e$ — không validator nào nắm quá 5% voting power. Vượt cap thì phần dư bị "burned to voting power 0" (vẫn cho stake nhưng không tăng vote weight); validator được khuyến khích split.

#### 2.1.1 Anti-Sybil obligations (rev.3 addition)

Independent review (`docs/improve.pdf` §6.5) chỉ ra rằng 5% cap chỉ là **vanity metric** trong môi trường permissionless: một thực thể với 30% stake có thể trivially split thành 6 ẩn danh node × 5% trên cùng cloud instance. Để 5% cap có ý nghĩa kỹ thuật, mỗi validator phải:

1. **Declare network identity** at registration:
   - ASN (Autonomous System Number) của primary uplink.
   - Cloud provider (nếu có): AWS/GCP/Azure/Hetzner/self-hosted.
   - Region code (ISO 3166-2 hoặc cloud-specific zone).
   - Khai báo này được commit on-chain trong activation transaction; thay đổi yêu cầu re-attest.

2. **Submit DKG-fingerprint commitment** (rev.3, §9.7):
   - Validator key được derive qua một deterministic-but-blinded process từ một stake-address-bound seed.
   - Foundation/governance có thể (off-chain) verify rằng nhiều validator được derive từ cùng seed → slashable evidence.

3. **Accept correlation-based reward decay**:
   - Reward × `(1 - decay_rate × concentration_score)` với `concentration_score` tính từ ASN/cloud/voting-pattern overlap với các validator khác.
   - Chi tiết §10.6.

Validator KHÔNG declare ASN/cloud/region được treat như "all unknown" → max concentration_score → max decay (gần như zero reward). Đây là incentive đủ mạnh để buộc khai báo trung thực mà không cần KYC (kẻ gian có thể fake declaration nhưng phải pay opportunity cost của false declaration).

### 2.2 Mạng

Mô hình **partial synchrony** kinh điển (DLS 1988): tồn tại $\Delta$ chưa biết và $\text{GST}$ chưa biết, sao cho sau $\text{GST}$, mọi message giữa hai node đúng tới được trong $\Delta$. Trước GST kẻ tấn công kiểm soát hoàn toàn lịch trình.

Topology: gossip-based overlay với eager push cho metadata (vertex headers, votes) và pull-on-demand cho blob chunks. KHÔNG dùng deterministic relay topology (tránh single-point-of-failure như Solana turbine block leader).

### 2.3 Mô hình tin cậy


| Đối tượng                          | Giả định                                                                                                                |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Hash (SHA-256, BLAKE3)             | Collision-resistant, second-preimage-resistant                                                                          |
| Chữ ký (BLS12-381)                 | EUF-CMA secure **dưới giả định cổ điển** (rev.3): pre-quantum DLP-hard. KHÔNG safe trước máy tính lượng tử có đủ Qubit. |
| Aggregate signatures (BLS)         | Rogue-key attack đã được phòng bằng proof-of-possession                                                                 |
| VRF (ECVRF Edwards25519 default — rev.3) | Pseudo-random, unpredictable cho non-holder; bias-resistant ở mức acceptable cho consensus (Algorand, Polkadot precedent). KHÔNG post-quantum. iVRF deferred làm option v3. |
| Bitcoin PoW (rev.3)                | Hashrate ≫ attacker budget; standard Nakamoto consensus với 6-block confirmation rule                                   |
| Time                               | KHÔNG giả định clock đồng bộ; chỉ dùng local timeout tăng dần                                                           |


**Crypto stack consistency note (rev.3)**: rev.2 đã claim "post-quantum readiness" qua iVRF nhưng cùng lúc dùng BLS12-381 cho aggregation — tạo thành "ảo giác bảo mật" (xem `docs/improve.pdf` §6.1). Rev.3 khẳng định rõ ràng: **toàn bộ v2.0 là classical-secure**. PQ migration là một single coordinated event ở v3 (BLS → STARK aggregation theo Drake 2025; ECVRF → lattice-VRF/iVRF). Không patchwork.


### 2.4 Threat model


| Thuộc tính                                 | Mức                                                                                                             |
| ------------------------------------------ | --------------------------------------------------------------------------------------------------------------- |
| Byzantine stake tối đa cho safety          | $f < W/3$                                                                                                       |
| Online honest stake tối thiểu cho liveness | $> 2W/3$ sau GST                                                                                                |
| Adaptive corruption                        | Cho phép — kẻ tấn công có thể chọn validator để corrupt sau khi nhìn thấy beacon, nhưng không nhanh hơn 1 epoch |
| Crash + Byzantine kết hợp                  | Tổng $\le f$                                                                                                    |
| Network partition trước GST                | Cho phép arbitrary; safety vẫn giữ                                                                              |
| Network partition sau GST                  | Không xảy ra theo định nghĩa                                                                                    |
| Long-range attack                          | Chống bằng (a) weak subjectivity 1-tuần (rev.3 hạ từ 2-tuần nhờ BTC anchor), (b) **Bitcoin checkpoint default ON** (§8.4); attacker phải re-mine BTC PoW history để bypass |
| Data withholding                           | Chống bằng certified vertex + retrieval challenge (mục 5)                                                       |
| Sybil via stake split                      | Chống bằng (rev.3): (a) 5% cap, (b) ASN/cloud declaration + reward decay, (c) DKG-fingerprint slashing (§9.7)   |
| Quantum compromise của BLS / ECVRF (rev.3) | **OUT OF SCOPE v2.0**: ack openly là classical crypto. PQ migration tới v3 là roadmap commitment.               |


Adaptive corruption mạnh hơn so với BFT cổ điển và là lý do bắt buộc dùng VRF private sortition cho mọi vai trò leader/collector.

### 2.5 Cận lý thuyết tham chiếu

LUA-DAG v2 không vi phạm cận nào dưới đây:

- **FLP 1985**: không thể đạt termination xác định trong asynchronous với 1 fault. ⇒ V2 dùng partial synchrony, có termination *eventual* sau GST.
- **DLS 1988**: $f < N/3$ là tight bound cho partial synchrony BFT có signature. ⇒ V2 chọn baseline 1/3.
- **CAP**: dưới partition, v2 chọn **safety over liveness** (hard-finality stall thay vì fork).

---

## 3. Architecture overview

### 3.0 Vị trí trong taxonomy

LUA-DAG v2 thuộc lớp **ebb-and-flow protocols** (Neu et al. SP 2021): kết hợp một synchronous dynamically-available protocol (DAG availability + micro-ordering — luôn live ngay cả khi participation dao động) với một partially-synchronous finality gadget (macro layer — đảm bảo accountable hard finality). Đây là cùng pattern với Ethereum 3-slot finality (3SF, D'Amato 2024) và RLMD-GHOST + finality gadget.

**Rev.3 addition**: Layer 4 (Sovereign Anchor) borrowed từ Babylon/Pikachu (Tas 2022, Azouvi 2022) — đặt LUA-DAG ở giao điểm giữa **ebb-and-flow** và **PoS-checkpointed-into-PoW**. Đây là pattern hỗn hợp đạt được "best of both worlds": throughput + accountable finality của native PoS, kèm với immutability của Bitcoin PoW cho long-range protection. Đặt LUA-DAG vào taxonomy này giúp reviewer dễ verify safety/liveness bằng cách reuse các bổ đề ebb-and-flow + Babylon's checkpointing security results đã có.

Đáng lưu ý, kiến trúc 2-step + separation of execution-verification và block-propagation-attestation cũng có điểm tương đồng với **Acki Nacki** (Goroshevsky 2024). Khác biệt chính của LUA-DAG: (a) DAG availability layer riêng để load-balance bandwidth; (b) accountable safety theo Casper FFG 2-chain rule chứ không phải probabilistic; (c) macro-finality bằng full validator set chứ không phải random committee per block; (d) Sovereign Anchor tier (rev.3) cung cấp Bitcoin-grade immutability cho high-value settlement.

### 3.1 Bốn lớp (rev.3, was three before Layer 4 promotion)

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
        │  - commit vào Bitcoin Taproot tx mỗi epoch              │
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

### 3.2 Tại sao 4 lớp (rev.3)

Rev.3 add Sovereign Anchor làm Layer 4 (thay vì optional add-on như rev.2). Mỗi lớp giải quyết đúng một bài toán mà **không lớp nào khác giải tốt**:


| Lớp                     | Bài toán                                          | Tại sao không gộp được                                                              |
| ----------------------- | ------------------------------------------------- | ----------------------------------------------------------------------------------- |
| Availability DAG        | Reliable broadcast của large data                 | Gộp với ordering ⇒ leader trở thành bottleneck băng thông; bằng chứng Narwhal/Tusk  |
| Micro-ordering          | Linearization nhanh của causal DAG                | Gộp với DA ⇒ data có thể "được nhắc tên" nhưng chưa available; gộp với macro ⇒ chậm |
| Macro-finality          | Fast Execution Finality cho rollup transactions   | Gộp với micro ⇒ committee nhỏ không đủ accountable; aggregate cost cao              |
| Sovereign Anchor (rev.3)| Long-range / unbond-window protection             | PoS-only finality không đủ khi attacker đã unbonded; cần external trust source (BTC PoW) |


### 3.3 Boundary rõ ràng giữa lớp

Mỗi lớp expose **một interface duy nhất** cho lớp trên:

- L1 → L2: function `causal_set(round_cut)` trả về tập `CertifiedVertex` có round ≤ round_cut.
- L2 → L3: function `micro_head()` trả về `MicroCheckpoint { slot, parent_macro, anchor_vertex, committed_sub_dag_root, ordered_blob_refs_root, micro_qc }` (định nghĩa §4).
- L3 → consumer: function `latest_finalized()` trả về `(MacroHeader, MacroQC)` cho checkpoint cao nhất đạt trạng thái `finalized` (Fast Execution Finality, §3.5).
- L3 + BTC anchor → consumer (rev.3): function `latest_epoch_finalized()` trả về `(MacroHeader, MacroQC, BitcoinAnchorProof)` cho checkpoint cao nhất đã đạt 6-block confirmation trong Bitcoin (Sovereign Epoch Finality).
- L3 → rollup API: function `blob_status(blob_id)` trả về một trong các trạng thái `{submitted, accepted, ordered, soft_confirmed, justified, finalized, epoch_finalized}` theo state machine §3.5 (rev.3 thêm `epoch_finalized`).

Inter-layer chỉ giao tiếp qua các interface này. Không lớp nào reach-through xuống state nội bộ của lớp khác. Điều này quan trọng cho test (mỗi lớp test độc lập với mock của lớp dưới) và cho proof (mỗi lớp có safety proof riêng, compose lại bằng interface contracts).

### 3.4 Data flow điển hình

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
t+~30min  MacroCheckpoint hash committed vào BTC Taproot tx (mỗi epoch ~30min)
t+~60min  6 BTC blocks confirm                        ← SOVEREIGN EPOCH FINALIZED
                                                      (validator unbond/withdrawal allowed)
```

### 3.5 Blob lifecycle state machine (rev.3 expanded)

API exposed cho rollup developer là một state machine **đơn điệu, có một bước revert duy nhất** (`accepted → soft_confirmed`). Rev.3 thêm trạng thái cuối `epoch_finalized` để align với two-tier finality model (§1.4). Mọi rollup integration phải treat các trạng thái như sau:

```
        submitted
            │
            │  ingress validator nhận blob,
            │  broadcast chunks + Merkle commitment
            ▼
        accepted                      ← API: accepted=true
            │
            │  blob_ref được include trong ≥ 1 certified vertex
            │  (vertex tham chiếu blob_ref + Merkle proof của ≥ 1 chunk)
            ▼
        ordered                       ← API: ordered=true
            │
            │  chunk thuộc Closure(A_w) của một anchor commit
            │  (xuất hiện trong `ordered_blob_refs_root` của một MicroCheckpoint)
            ▼
        soft_confirmed  ← MAY REVERT  ← API: soft_confirmed=true, finalized=false
            │
            │  MicroCheckpoint được chứa trong window
            │  của MacroCheckpoint C_h justified
            ▼
        justified       ← MAY REVERT  ← API: justified=true, finalized=false
            │
            │  C_{h+1} cũng justified với parent_hash = hash(C_h)
            ▼
        finalized       ← IRREVERSIBLE except slashable evidence
                                       ← API: finalized=true (Fast Execution Finality)
            │
            │  (rev.3) MacroCheckpoint hash committed vào Bitcoin Taproot tx +
            │  đợi 6 BTC confirmations
            ▼
        epoch_finalized ← TRULY IRREVERSIBLE (rev.3)
                                       ← API: epoch_finalized=true (Sovereign Epoch Finality)
```

**Triggers (chính xác):**

| Transition                                  | Trigger                                                                                                                        |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `submitted → accepted`                      | Ingress validator phát hành signed receipt `IngressReceipt { blob_id, commitment, slot, ingress_sig }`; client verify chữ ký = bằng chứng rằng ít nhất 1 validator đã commit chịu trách nhiệm về blob (slashable nếu sau đó fail availability) |
| `accepted → ordered`                        | Có ≥ 1 `CertifiedVertex` chứa `blob_ref` (2f+1 BLS sigs)                                                                       |
| `ordered → soft_confirmed`                  | `MicroQC` cho slot $s$ formed; blob nằm trong `ordered_blob_refs_root` của MicroCheckpoint $s$                                 |
| `soft_confirmed → justified`                | MacroCheckpoint $C_h$ chứa hash của MicroCheckpoint chứa blob được justified (có MacroQC)                                      |
| `justified → finalized`                     | $C_{h+1}$ justified với `parent_height_hash(C_{h+1}) = hash(C_h)` (2-chain rule, §7.5) — **Fast Execution Finality**           |
| `finalized → epoch_finalized` (rev.3)       | MacroCheckpoint hash của $C_h$ được embed trong một Bitcoin Taproot OP_RETURN tx (qua vigilante relayer); 6 BTC blocks confirm — **Sovereign Epoch Finality** |

**Revert semantics:**

Tất cả các transition đều **monotonic ở local view của một node honest** trong steady-state. Các trường hợp "revert" chỉ xảy ra ở các failure mode đã được spec ràng buộc:

- `accepted` và `ordered`: **không bao giờ revert** (anchor commit Bullshark là monotonic theorem, §6.8).
- `soft_confirmed`: monotonic **khi `lock_macro` invariant (§11.5) được honor**. Vi phạm `lock_macro` được spec coi là protocol bug (test-time/audit-time), không phải runtime case.
- `justified → soft_confirmed`: chỉ xảy ra nếu MacroQC bị orphan trong macro fork; macro fork đòi hỏi ≥ W/3 stake equivocation và **luôn** sinh slashable evidence (§11.1). Rollup phải đợi resolve.
- `finalized`: irreversible **trong window slashable** (~`WITHDRAWAL_DELAY` = 6 BTC blocks ~ 60min, rev.3). Vượt qua window này, attacker có thể đã unbonded → cần `epoch_finalized` để đảm bảo.
- `epoch_finalized` (rev.3): irreversible một cách tuyệt đối — attacker phải re-mine ≥ 6 Bitcoin blocks để bypass, kinh tế bất khả thi với hashrate hiện tại.

**Khuyến nghị cho rollup (rev.3 updated)**:

| Use case                                               | Trạng thái tối thiểu khuyến nghị |
| ------------------------------------------------------ | -------------------------------- |
| UI preview cho user                                    | `soft_confirmed`                 |
| Fee charge cho L2 tx (revertible)                      | `soft_confirmed`                 |
| Cross-rollup messaging (revertible, nội bộ ecosystem)  | `finalized`                      |
| Bridge withdrawal **dưới value cap**                   | `finalized` (Fast Exec)          |
| Bridge withdrawal **trên value cap** (rev.3)           | `epoch_finalized` (Sovereign)    |
| Validator unbond / set rotation (rev.3, BẮT BUỘC)      | `epoch_finalized` (Sovereign)    |
| Update L1-anchored state root (DA layer settlement)    | `finalized` (đủ); `epoch_finalized` cho high-value chain |
| Light client sync read                                 | `justified` đủ cho passive view; `finalized` cho settlement read |

API contracts:
- `latest_finalized()` trả về header `finalized` (Fast Exec) cao nhất.
- `latest_epoch_finalized()` (rev.3) trả về header `epoch_finalized` (Sovereign) cao nhất; có thể lag ~30–90 phút sau `latest_finalized`.
- Nếu rollup query `soft_confirmed` hoặc `finalized`, response **bắt buộc** kèm `revert_risk` flag (`revert_risk_local: true` cho soft, `revert_risk_long_range: true` cho finalized chưa epoch_finalized).

---

## 4. Data structures


| Struct                | Mục đích                          | Trường chính                                                                                                      |
| --------------------- | --------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `Blob`                | Đơn vị payload từ rollup          | `namespace_id`, `data` (bytes), `commitment` (Merkle root over chunks)                                            |
| `Chunk`               | Mảnh của blob sau erasure coding  | `blob_id`, `index`, `data`, `proof` (Merkle proof against commitment)                                             |
| `IngressReceipt`      | Bằng chứng `accepted` (§3.5)      | `blob_id`, `commitment`, `slot`, `ingress_validator_id`, `ingress_sig` (slashable nếu fail availability sau đó)   |
| `Vertex`              | Đỉnh DAG                          | `round`, `author`, `parents` (≥ 2f+1 cert vertex hashes from r-1), `blob_refs` (list of commitments), `signature` |
| `CertifiedVertex`     | Vertex + quorum sigs              | `vertex`, `quorum_sigs` (2f+1 BLS aggregate)                                                                      |
| `MicroCheckpoint`     | Output của một wave               | `slot`, `parent_macro`, `anchor_vertex`, `committed_sub_dag_root`, `ordered_blob_refs_root`, `micro_qc`           |
| `MicroQC`             | Aggregate vote on MicroCheckpoint | `slot`, `committee_bitmap`, `bls_aggregate`                                                                       |
| `MacroCheckpoint`     | Hard-finality unit                | `height`, `parent_height_hash`, `micro_head_hash`, `da_root`, `validator_set_root`, `epoch`, `proposer_id`        |
| `MacroQC`             | Aggregate vote on MacroCheckpoint | `height`, `included_subnets` (8-bit bitmap), `subnet_aggregates` (sparse list, indexed by bitmap), `validator_bitmap`, `bls_aggregate`, `total_signed_stake` |
| `MacroHeader`         | Light header for clients          | `height`, `parent_hash`, `micro_head_hash`, `da_root`, `epoch`, `aggregate_sig`, `validator_bitmap_compressed`    |
| `SyncCommitteeUpdate`  | Cập nhật sync committee cho light | `epoch`, `next_committee_root`, `aggregate_sig`                                                                                                    |
| `BitcoinAnchorProof` (rev.3) | Bằng chứng `epoch_finalized`      | `macro_height`, `macro_hash`, `btc_txid`, `btc_block_height`, `btc_block_hash`, `merkle_proof_to_btc_block`, `confirmations` (≥6 ⇒ epoch_finalized) |
| `ValidatorIdentity` (rev.3) | Khai báo network identity         | `validator_id`, `asn`, `cloud_provider`, `region_code`, `attestation_sig`, `epoch_attested`                                                        |
| `DKGCommitment` (rev.3)| Cam kết key derivation cho fingerprint  | `validator_id`, `commitment_root`, `dkg_session_id`, `proof_of_possession`                                                                         |
| `SlashEvidence`        | Bằng chứng slashable              | `kind`, `validator_id`, `evidence_a`, `evidence_b` (hai message conflict đã ký)                                                                    |


Quan sát: **không có** struct nào tên "block" trong v2. Đây là cố ý — "block" là khái niệm quá tải khi gộp data + order + finality vào một struct.

---

## 5. Layer 1 — Availability DAG

### 5.0 Decision: certified DAG (default v2) vs uncertified DAG (option v2.1)

LUA-DAG v2 default chọn **certified DAG** kiểu Narwhal/Bullshark — vertex chỉ được dùng làm parent khi đã có 2f+1 chữ ký. Đánh đổi: thêm 1 round latency, lấy availability guarantee mạnh và spec đơn giản hơn.

Literature 2023-2024 cho thấy **uncertified DAG** kiểu Mysticeti (Babel 2023) có thể đạt WAN latency 0.5s ở 50k+ TPS — nhanh hơn certified DAG ~2x. Tuy nhiên uncertified DAG mở ra attack vectors mới mà Adelie (Chursin 2024) phải address. Quyết định defer uncertified mode sang v2.1 là cố ý — giảm rủi ro spec/audit cho v2 release đầu tiên.

Một bổ sung **không tốn nhiều effort** đã được tích hợp: **leader reputation + pipelining** kiểu Shoal (Spiegelman 2023) — eliminate timeouts trong common case (property "Prevalent Responsiveness"), giảm latency 40%+ khi không có failure. Reputation score được tính từ historical liveness và included trong anchor selection (xem §6.2).

### 5.1 Vertex creation

Mỗi validator $v_i$ ở mỗi round $r$:

1. Đợi đến khi có ≥ 2f+1 certified vertices ở round $r-1$ (gọi là `parents_ready`).
2. Đóng gói local blob queue thành blob_refs (giới hạn: tổng size ≤ `MAX_VERTEX_PAYLOAD = 1 MB`).
3. Tạo `Vertex { round: r, author: i, parents: parents_ready, blob_refs, signature: sign(...) }`.
4. Gossip vertex tới mọi validator khác.

Round 0 có vertex genesis cho mỗi validator (no parents).

### 5.2 Vertex certification

Khi validator $v_j$ nhận vertex $V$ từ $v_i$:

1. Verify signature.
2. Verify `parents` đều là certified vertex từ round $r-1$ (đã thấy local).
3. Verify mọi `blob_ref` có corresponding chunks **available**: ở fast path chỉ verify Merkle proof của ít nhất một chunk đã nhận được; full availability cho toàn bộ chunks được defer sang retrieval challenge mechanism ở §5.5 (lazy verification + slashing nếu fail).
4. Nếu pass, $v_j$ ký vertex hash và gossip vote.

Khi $v_i$ thu được ≥ 2f+1 vote, hắn aggregate thành `CertifiedVertex` và gossip lại.

**Quan trọng**: vertex chưa certified KHÔNG được dùng làm parent. Điều này khác với một số DAG variant cho phép uncertified parents — quyết định này đánh đổi 1 round latency lấy availability guarantee mạnh hơn.

### 5.3 Erasure coding cho blobs

Mỗi blob được chia thành $k$ data chunks và mở rộng thành $n = 2k$ chunks bằng Reed-Solomon (rate 1/2). Bất kỳ $k$ chunks nào cũng đủ recover blob.

- $k = \lceil |\text{blob}| / 32\text{KB} \rceil$ (chunk data size cố định).
- Commitment = Merkle root của $n$ chunk hashes.
- Khi gossip, validator phát chunks song song qua các peer. Theo Narwhal, mỗi validator gửi chỉ $1/N$ tổng bytes ⇒ load-balanced.

**Throughput cap v2.0 (rev.3)**: tổng bytes mới được commit qua DA layer được hard-cap ở `THROUGHPUT_HARD_CAP_V20 = 5 MB/s` ở runtime — enforce bằng (a) `MAX_VERTEX_PAYLOAD = 256 KB`, (b) tham số fee market base fee adjust để target 50% capacity (~2.5 MB/s normal, có thể spike đến 5 MB/s). Validator reject vertex có total blob_refs size vượt vertex payload cap. Lý do hạ từ rev.2's 30–100 MB/s: xem `docs/improve.pdf` §6.2 — 100 MB/s mà KHÔNG có DAS = 8.6 TB/ngày, chỉ datacenter chạy được full node.

**Tại sao 1D thay vì 2D ở v2.0**: 2D Reed-Solomon (Celestia-style) cho phép DA sampling cho light node, nhưng tăng overhead 2-4x. V2.0 chọn 1D vì:
1. Light client tin sync committee về DA (xem mục 8).
2. Throughput cap 5 MB/s đủ thấp để full node home-internet replicate toàn bộ data — không cần sampling.

**v2.1 transition (rev.3 commitment)**: 2D Reed-Solomon + KZG polynomial commitments (Hall-Andersen 2025 hoặc RLNC-DAS Grundei 2025) **bắt buộc** ở v2.1. Vertex schema sẽ thêm `kzg_commitment` field; trong window transition, vertex cũ (chỉ Merkle commitment) vẫn được honor cho backwards compat.

### 5.4 Garbage collection

Vertex và chunks có lifecycle 3 trạng thái:

- **Hot**: round ≤ `current - GC_HOT_HORIZON` (default `GC_HOT_HORIZON = 200`). Phải giữ full bytes trên mọi validator.
- **Warm**: round nằm dưới latest finalized macro nhưng trên `GC_WARM_HORIZON` (default 10000 rounds). Validator phải giữ ít nhất commitment + 1 chunk; có thể tham gia retrieval challenge.
- **Cold**: round dưới `GC_WARM_HORIZON` so với finalized macro. Validator có thể GC hoàn toàn. Archive node (opt-in role) giữ vĩnh viễn.

### 5.5 Custody assignment và retrieval challenge

#### 5.5.1 Custody assignment (rev.2 addition)

Mỗi chunk của một blob được assign deterministically tới một tập **custody validator**:

$$\text{custody}(blob\_id, chunk\_idx) = \{ v_i : H(\text{pubkey}_i \,\|\, blob\_id \,\|\, chunk\_idx) \bmod N < K_{\text{custody}} \}$$

với `K_custody = 2f+1` (default) ⇒ mỗi chunk có ≥ 2f+1 custody validator. Vì việc assignment dùng VRF beacon $blob\_id$ (chứa randomness từ ingress validator + slot hash), adversary với <1/3 stake không thể guarantee chiếm cả tập custody của bất kỳ chunk nào.

**Obligation custody validator**:

- Phải lưu chunk full bytes trong **hot tier** (round ≤ `current - GC_HOT_HORIZON`).
- Phải answer retrieval challenge cho chunk mình custody trong $T_{\text{retrieve}} = 30$s.
- Khi blob chuyển warm tier, vẫn phải giữ ít nhất 1 chunk + Merkle proof.

Validator KHÔNG thuộc custody set có thể GC chunk sớm hơn (sau `GC_HOT_HORIZON`); họ chỉ giữ commitment + bitmap "chunk nào tôi đã thấy" cho mục đích vote certification.

#### 5.5.2 Retrieval challenge

Bất kỳ rollup hoặc full node nào có thể issue retrieval challenge `Challenge { blob_id, chunk_idx, challenger_id, deadline }` tới **target set** = `custody(blob_id, chunk_idx)`. Mỗi target validator phải trả lời trong $T_{\text{retrieve}} = 30$s với:

- `Response { chunk_data, merkle_proof, signature_over_challenge }`, HOẶC
- `Defense { reason: ColdGCed | NotInCustody, proof }` nếu hợp lệ.

**Slashable evidence form**:

$$\text{UnavailabilityEvidence} = \{\text{Challenge}, \text{TargetSet}, \text{NoResponseProof}_{\ge 2f+1 \text{ witnesses}}\}$$

Trong đó `NoResponseProof` là **gossip-level evidence**, KHÔNG phải data-level: aggregated signed attestations từ ≥ 2f+1 validator (witness có thể là bất kỳ validator nào trong network, không cần thuộc custody set) confirming proposition

$$\text{Witness}_v = \text{"tôi đã observe Challenge}_c\text{ gossip tại t}_0\text{; tôi đã wait } T_{\text{retrieve}}\text{; tôi KHÔNG quan sát thấy bất kỳ valid Response/Defense message nào từ } v_j \text{ trên gossip topic}\text{"}$$

Witness chỉ cần verify (a) signature trên `Response` nếu thấy, (b) wall-clock đo deadline. Witness KHÔNG cần verify chunk data correctness — chỉ cần khẳng định **silence trên gossip layer**. Vì honest validator không lie về điều này (lying = signed false statement = slashable nếu adversary sản xuất counter-evidence dạng "tôi đã thấy response"), gossip-level attestation đủ làm bằng chứng.

Khi evidence valid:

- Slash mỗi non-responding custody validator theo `DATA_UNAVAILABILITY_SLASH = 5%`.
- Nếu **toàn bộ** custody set fail (≥ 2f+1 validator không response), blob bị mark `unavailable`; ngoài việc slash custody set, ingress validator (validator đầu tiên broadcast commitment) cũng bị slash 5% — nhằm chống ingress validator commit blob mà không thực sự push chunks lên gossip layer.

#### 5.5.3 Constraints

- Challenge KHÔNG áp dụng cho cold blobs (sau `GC_WARM_HORIZON`). Rollup muốn pin blob lâu hơn phải dùng archive node opt-in.
- Cùng `(blob_id, chunk_idx, challenger)` không thể issue challenge mới quá `MIN_CHALLENGE_INTERVAL = 60s` (chống spam).
- Challenge phải pay nhỏ một fee floor → refund nếu target fail (asymmetric incentive khuyến khích challenge thật, chặn DoS).

---

## 6. Layer 2 — Micro-ordering (Bullshark anchor commit)

Đây là **section quan trọng nhất** vì đây là chỗ v1 hand-waved. V2 dùng đúng commit rule của Bullshark thay vì khái niệm "frontier xác định" mơ hồ.

### 6.1 Wave structure

DAG được chia thành các wave dài 4 round mỗi wave (steady-state có thể commit trong 2 round qua shortcut path). Wave $w$ gồm round $4w, 4w+1, 4w+2, 4w+3$.

### 6.2 Anchor selection (VRF private sortition)

Tại đầu wave $w$, randomness beacon $R_w$ được derive:

$$R_w = H(R_{w-1} \,\|\, \text{MacroQC of latest finalized})$$

Mỗi validator $v_i$ tính:

$$y_i = \text{ECVRF}_i(R_w \,\|\, \text{"anchor"})$$

Anchor proposer của wave $w$ là validator có $y_i \cdot W / (w_i \cdot \text{rep}_i)$ nhỏ nhất trong wave, trong đó $\text{rep}_i \in [0.5, 1.5]$ là Shoal-style leader reputation score (rolling average của liveness gần đây — anchor lỗi nhiều bị giảm rep, anchor lành mạnh được boost). **Không ai biết anchor là ai cho đến khi anchor publish vertex của mình ở round $4w$ — đây là điểm key chống adaptive DoS.**

**Trade-off với unbiasability** (rev.3 narrowed): hệ số `rep_i` là **non-cryptographic input** dựa trên local liveness measurement → mở một surface attack nhỏ:

- Reputation bị derive từ DAG observation; nếu adversary có thể ảnh hưởng "vertex của tôi có được include vào parents không", có thể bias `rep_i` của validator khác.
- Rev.3 narrow range: `rep_i ∈ [0.8, 1.2]` (max 1.5× lợi thế thay vì 3×) để giảm bias surface, đánh đổi ~20% Shoal latency improvement (vẫn lớn hơn pure stake-weighted bằng phần cứng).
- Reputation **không** áp dụng cho macro proposer selection (§7.2) — chỉ áp dụng cho anchor selection ở micro layer, nơi safety không trực tiếp phụ thuộc vào fairness của leader rotation.
- Nếu reputation bias vẫn là issue thực tế khi testnet, fallback là set `rep_i = 1` cho mọi validator (pure stake-weighted ECVRF).

Khi anchor vertex được certified, hắn reveal ECVRF proof. Mọi validator verify proof ⇒ xác nhận anchor đúng.

**Crypto choice (rev.3 — replaces rev.2)**: spec mặc định dùng **ECVRF Edwards25519 (RFC 9381)**. Lý do:

1. **Consistency với BLS aggregation**: cả anchor selection (ECVRF) và macro vote aggregation (BLS12-381) đều là **classical elliptic-curve-based**. Rev.2 dùng iVRF (PQ-claim) cùng với BLS (classical) tạo "ảo giác bảo mật" — nếu quantum attacker break BLS thì ECVRF/iVRF không quan trọng, attacker đã giả mạo MacroQC. Xem `docs/improve.pdf` §6.1.
2. **Maturity**: ECVRF có production reference implementations (libsodium, Algorand, Polkadot) — phù hợp cho v2 prototype timeline.
3. **Bias resistance acceptable**: ECVRF không có "unbiasability" property formal như iVRF, nhưng trong setting với private sortition + stake-weighted threshold, bias surface bị bound bởi grinding-resistance của randomness beacon (§9.4) — không phải VRF property cá nhân.

**Post-quantum migration timeline (rev.3)**: full crypto stack migration là một single coordinated event ở **v3** (không patchwork ở v2.1):
- ECVRF → lattice-based VRF hoặc iVRF (Esgin 2023) khi production-ready.
- BLS12-381 → STARK aggregation (Drake 2025) hoặc hash-based multi-sig.
- Migration plan riêng sẽ được spec ở v2.1 → v3 transition document.

iVRF reference giữ trong §17 cho future use ở v3.

### 6.3 Commit rule (steady-state, 2-round shortcut)

Anchor vertex $A_w$ ở round $4w$ được **committed** nếu:

- Tồn tại ≥ $2f+1$ certified vertices ở round $4w+1$ trong đó mỗi vertex link trực tiếp tới $A_w$ qua trường `parents`.

Đây là shortcut path — happy case commit trong 2 round (~500ms với round 250ms).

### 6.4 Commit rule (slow path, 4-round)

Nếu shortcut path không hình thành (anchor vertex không được certified hoặc không đủ link), wave sang slow path:

- Round $4w+2$: validator broadcast `wave_vote(w, support_anchor_or_skip)`.
- Round $4w+3$: nếu ≥ $2f+1$ vote support anchor, commit. Nếu ≥ $2f+1$ vote skip, wave bị skip (no MicroCheckpoint cho wave này, sang wave $w+1$).

Slow path tốn 4 round (~1s), nhưng đảm bảo liveness ngay cả khi anchor crash hoặc mạng burst.

### 6.5 Linearization

Khi anchor $A_w$ committed:

1. Tính causal closure $\text{Closure}(A_w)$ = tập certified vertices có path tới $A_w$ trong DAG, **trừ đi** closure của anchor đã commit trước ($A_{w-1}$ nếu có).
2. Sắp xếp $\text{Closure}(A_w)$ theo **deterministic topological order**: BFS từ $A_w$, tie-break bằng lexicographic order của vertex hash. Mọi node đúng cùng tính ra cùng thứ tự.
3. Trong order đó, gom blob_refs theo namespace. Trong cùng namespace, giữ thứ tự xuất hiện.
4. Tạo `MicroCheckpoint` với `committed_sub_dag_root = MerkleRoot(ordered vertices)` và `ordered_blob_refs_root = MerkleRoot(per-namespace ordered refs)`.

Đây là **deterministic linearization** thực sự — khác với v1 nói "frontier xác định" mà không định nghĩa hàm cụ thể.

### 6.6 MicroQC formation

Anchor proposer broadcast `MicroCheckpoint`. Một micro committee gồm $C_{\text{micro}}$ validator (default 256, weighted-sampled bằng VRF) ký MicroCheckpoint hash. Khi anchor thu đủ $\lceil 2/3 \cdot C_{\text{micro}}\rceil$ chữ ký, hắn aggregate thành `MicroQC`.

### 6.7 Soft-confirmation contract

Khi MicroQC tồn tại cho slot $s$:

- Mọi blob trong `ordered_blob_refs_root` của slot $s$ được coi là **soft_confirmed**.
- API expose state này dưới flag `soft_confirmed = true, finalized = false, revert_risk = true`.
- Soft confirmation **có thể bị revert** trong các điều kiện hiếm (xem §3.5 lifecycle state machine và §11.5 cross-layer attack). Rollup KHÔNG nên dùng soft cho settlement-grade decision; xem bảng "use case → trạng thái tối thiểu" ở §3.5.

### 6.8 Tại sao Bullshark anchor rule fix vấn đề của v1

V1 nói: "frontier_root xác định vì mọi node đúng tính cùng hàm trên cùng DAG snapshot". Vấn đề: **không tồn tại "snapshot" toàn cục** trong asynchronous network.

Bullshark giải quyết bằng cách định nghĩa commit rule theo **causal evidence trong DAG**, không phải theo timing. Khi 2f+1 vertex ở round $r+1$ link tới anchor ở round $r$, bằng chứng này lan truyền: bất kỳ node đúng nào nhìn thấy 2f+1 vertex đó **bắt buộc** đã thấy anchor và đã thấy certificate cha. Vì vậy mọi node đúng eventually agree on commit.

Proof formally đã được paper Bullshark (Spiegelman et al. 2022) chứng minh; v2 reuse trực tiếp.

---

## 7. Layer 3 — Macro-finality

### 7.1 Cadence

Mỗi $W = 8$ micro-slots (default), giao thức tạo một `MacroCheckpoint`. Với round 250ms và wave 4 round, mỗi micro-slot ~1s ⇒ macro window ~8s.

### 7.2 Proposer selection

Macro proposer của height $h$ được chọn bằng VRF private sortition tương tự anchor (mục 6.2), nhưng dùng beacon $R^{\text{macro}}_h$:

$$R^{\text{macro}}_h = H(R^{\text{macro}}_{h-1} \,\|\, \text{MacroQC}_{h-1})$$

Một backup proposer được chọn ranked thứ 2; nếu primary không publish trong $T_{\text{macropropose}} = 4$s, backup tiếp quản.

### 7.3 MacroCheckpoint construction

Proposer ở height $h$ tạo:

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

Proposer broadcast `MacroProposal`.

### 7.4 Adaptive aggregation: Mode 0 / A / B (rev.3 rewrite)

Đây là kỹ thuật scale macro vote — KEY innovation so với v1. Rev.3 thay thế "static 8 subnets" của rev.2 bằng **adaptive aggregation** dựa trên kích cỡ active validator set, sau khi `docs/improve.pdf` §6.3 chỉ ra rằng hard-code 8 subnets cho mạng 100–500 validator (target của LUA-DAG, tương tự Sui/Aptos) tạo network hops thừa và tăng latency.

#### 7.4.1 Adaptive subnet count

Tại đầu mỗi epoch $e$, số subnet được tính từ Active Set size $N_e$:

$$
K_e = \begin{cases}
0 & \text{if } N_e < N_{\text{flat}} = 500 & \text{(Mode 0: flat gossip)} \\
\left\lceil N_e / 128 \right\rceil & \text{if } N_{\text{flat}} \le N_e \le N_{\text{full}} = 1000 & \text{(interpolated, 4–8 subnets)} \\
\left\lceil N_e / 128 \right\rceil & \text{if } N_e > N_{\text{full}} & \text{(Mode A: subnets)}
\end{cases}
$$

cap ở $K_{\max} = 32$ để tránh fragmentation thái quá. Tham số `SUBNET_TARGET_SIZE = 128` chọn sao cho mỗi subnet vẫn đủ stake để 2/3-internal threshold hoạt động (dù không phải prerequisite — xem Mode A bước 2 cho deadline-based publish).

**Subnet partition** (khi $K_e > 0$): rebuild mỗi epoch bằng VRF beacon:

$$\text{subnet}(v_i, e) = H(\text{pubkey}_i \,\|\, R^{\text{macro}}_{\text{epoch\_start}(e)}) \bmod K_e$$

Lý do dùng VRF beacon thay vì `validator_id mod K_e`: ngăn adversary chọn validator_id tại thời điểm deposit để stack vào cùng subnet (sybil-style subnet capture). Phân partition rebuild lại mỗi epoch ⇒ adversary không thể commit stake vào 1 subnet cụ thể trước khi biết beacon.

Với `MAX_STAKE_FRACTION = 5%`, stake tối đa của một subnet bị bound bởi Hoeffding-style ~`W/K_e + O(√(W/K_e) · MAX_STAKE_FRACTION)`.

#### 7.4.2 Mode 0 — flat gossip (N < 500)

Activated khi $K_e = 0$. Không có subnet structure; aggregation chạy trực tiếp ở macro proposer:

1. Mọi validator nhận `MacroProposal`, verify, ký hash, gossip partial sig.
2. Macro proposer thu partial sigs trực tiếp; aggregate khi đạt `total_signed_stake ≥ 2W/3` hoặc deadline $T_{\text{macropropose}} = 4$s.
3. Nếu proposer fail / DoS → fall back Mode B (gossip aggregation, §7.4.4).

**Cost (Mode 0)**:
- Per-validator: 1 sign + 1 broadcast.
- Macro proposer: aggregate ~$N$ sigs trực tiếp; với N=200 và BLS12-381 batch verify → ~20ms; chấp nhận được.
- Latency saving so với rev.2 8-subnet structure ở N=200: bỏ subnet aggregation hop (~$T_{\text{subnet}} = 2$s) → tổng macro round-trip ~3s thay vì ~5s.

#### 7.4.3 Mode A — subnet-based (N ≥ 500, default cho large set)

Activated khi $K_e \ge 4$:

1. Mọi validator nhận MacroProposal, verify, ký hash, gossip partial sig kèm subnet ID.
2. **Trong mỗi subnet**, subnet aggregator (rotated by ECVRF) liên tục thu partial sigs. Aggregator publish **partial subnet aggregate** `subnet_aggregate[k] = (subnet_id, bitmap, bls_aggregate, signed_stake)` ngay khi:
   - Hoặc đạt **target threshold** = 2/3 subnet stake (happy case), HOẶC
   - Hoặc đạt **publish deadline** $T_{\text{subnet}} = T_{\text{macropropose}} / 2 = 2$s — publish whatever stake đã collect được tại thời điểm đó.

   Lý do hai-điều-kiện: nếu một subnet bị 1/2 stake offline, aggregator KHÔNG BAO GIỜ đạt 2/3 internal → sẽ stuck. Publish-on-deadline đảm bảo subnet vẫn contribute partial evidence ⇒ macro proposer có thể combine với các subnet khác.

3. **Macro proposer** thu subnet aggregates và áp **quorum rule**:

   - **Quorum rule**: macro proposer aggregate `MacroQC` từ **bất kỳ subset $S \subseteq \{0,\dots,K_e-1\}$** miễn:
     - $\sum_{k \in S} \text{signed\_stake}_k \ge \lceil 2W/3 \rceil$, VÀ
     - Mỗi `subnet_aggregate[k]` trong $S$ pass cryptographic verification (bitmap + BLS pairing).

   Trong điều kiện normal $|S| = K_e$ và tổng stake gần $W$. Khi một số subnet aggregator crash hoặc subnet bị split-brain, proposer vẫn đạt $2W/3$ với các subnet còn lại.

   `MacroQC` lưu `included_subnets` bitmap (variable-length, $K_e$ bits) + `total_signed_stake` để verifier reproduce check.

#### 7.4.4 Mode B — leaderless gossip aggregation (fallback, applicable cả Mode 0 và Mode A)

Activated khi macro proposer DoS hoặc primary mode timeout sau $T_{\text{macropropose}}$:

1. Validator broadcast partial signature qua gossip layer (kèm subnet ID nếu Mode A).
2. Mỗi node lưu local view các partial sigs đã thấy. Khi đạt quorum rule, local node tự tạo `MacroQC candidate`.
3. **Canonical selection**: nhiều node có thể tạo MacroQC candidate khác nhau (cùng height, cùng `MacroProposal_hash`, nhưng `included_subnets` hoặc validator subset khác nhau), spec định nghĩa **canonical ordering** trên tập candidate hợp lệ (mỗi candidate phải pass `total_signed_stake ≥ 2W/3`) trong window $T_{\text{canonicalize}} = 2 \cdot T_{\text{macropropose}}$:

   $$\text{canonical}(C_1, C_2) = \begin{cases} C_1 & \text{if } \text{total\_signed\_stake}(C_1) > \text{total\_signed\_stake}(C_2) \\ C_2 & \text{if } \text{total\_signed\_stake}(C_2) > \text{total\_signed\_stake}(C_1) \\ \arg\min(\text{validator\_bitmap}) & \text{otherwise} \end{cases}$$

   Tức là: **stake cao nhất thắng**; tie-break bằng lex-smallest validator bitmap (Mode 0) hoặc subnet bitmap (Mode A). Lý do thêm stake-priority: tránh perverse incentive nơi candidate vừa đủ 2W/3 thắng candidate full coverage chỉ vì bitmap nhỏ hơn — full candidate có evidence mạnh hơn nên phải win.

4. Mọi validator phải re-broadcast canonical MacroQC khi quan sát thấy candidate có stake cao hơn (hoặc bitmap lex-smaller ở cùng stake). Convergence đạt được trong $O(\log N)$ gossip rounds (Long et al. 2019).

Mode B đảm bảo liveness ngay cả khi proposer Byzantine, đổi lại latency cao hơn ~2x primary mode.

#### 7.4.5 Cost analysis tổng hợp

| Mode | Trigger | Per-validator cost | Aggregation cost | Latency overhead so với baseline |
| ---- | ------- | ------------------ | ---------------- | -------------------------------- |
| Mode 0 (N<500) | Default cho small/medium set | 1 sign + 1 broadcast | Macro proposer aggregate ~N sigs (~20ms với batch verify) | 0 (baseline) |
| Mode A (N>1000) | Default cho large set | 1 sign + 1 broadcast (kèm subnet ID) | Subnet aggregator ~N/K sigs; macro proposer ~K sigs | +$T_{\text{subnet}} \approx 2$s (subnet hop) |
| Mode B | Proposer DoS / timeout | Same as primary | Mỗi node tự aggregate; canonical convergence | +~$T_{\text{macropropose}}$ (extra gossip round) |

**Light client verification**: 1 pairing check + bitmap parsing → ~3ms (giống rev.2; bitmap variable-length nhưng vẫn O(K) tức là rất nhỏ).

**Tại sao adaptive là đúng**: với target validator set 100–500 (như Sui/Aptos), Mode 0 (flat) **luôn** là default — tận dụng small-set efficiency. Subnet-based chỉ kick in khi network grow lên >1000 validators. Đây là cải tiến kỹ thuật so với rev.2 hard-code 8 subnets, mà tham khảo từ Ethereum scale (~895k validators) không phù hợp với LUA-DAG profile.

**Crypto stack note**: BLS12-381 default cho aggregation. Li et al. (2023) benchmark chỉ ra với committee >40 validator, EdDSA có thể ưu việt hơn BLS ở computation cost (đổi lại signature size lớn hơn). Quyết định BLS vs EdDSA cần được benchmark thực tế trong P2 prototype trước khi chốt — xem benchmark matrix §12.1.

### 7.5 2-chain finality rule

Theo Casper FFG (Buterin & Griffith 2017), được port qua macro-checkpoint chain:

- MacroCheckpoint $C_h$ là **justified** nếu có MacroQC.
- $C_h$ là **finalized** nếu $C_h$ justified VÀ có $C_{h+1}$ justified với `parent_height_hash(C_{h+1}) = hash(C_h)`.

Vì vậy hard-finality cần **ít nhất 2 macro checkpoint liên tiếp**, tổng latency = 2 × macro window = 16s worst case, ~10s typical.

### 7.6 Slashing conditions cho macro layer

Slashable evidence (cryptographically provable):


| Vi phạm                    | Definition                                                                                            | Slash %                              |
| -------------------------- | ----------------------------------------------------------------------------------------------------- | ------------------------------------ |
| Macro double-vote          | Ký hai `MacroProposal` ở cùng height $h$ với parent khác nhau                                         | 100%                                 |
| Surrounding vote           | Ký vote cho $C_h$ và sau đó ký vote cho $C_{h'}$ với $h' < h$ và $C_{h'}$ không là ancestor của $C_h$ | 100%                                 |
| Double-propose             | Cùng validator publish 2 MacroProposal khác nhau ở cùng height                                        | 100%                                 |
| Micro double-vote          | Ký 2 MicroQC khác nhau ở cùng slot                                                                    | 50%                                  |
| Data unavailability proven | Validator ký certified vertex, sau đó fail $f+1$ retrieval challenges                                 | 5% per occurrence, soft-cap 50%/year |


Slash 100% có nghĩa là toàn bộ stake bị burn + validator bị ejected khỏi active set. Slash partial cho data unavailability vì có thể là transient bug, không nhất thiết Byzantine.

### 7.7 Inactivity leak

Nếu chain không finalize trong `INACTIVITY_LEAK_THRESHOLD = 4` macro windows (~32s), giao thức vào chế độ inactivity leak: stake của validator offline hoặc vote sai sẽ bị giảm dần ($-0.5\%$ / macro window liên tục, tính trên stake còn lại) cho tới khi tỷ lệ online honest stake quay về > 2/3.

Đây là cùng cơ chế Ethereum dùng từ Beacon Chain. Đảm bảo recovery sau partition ngay cả khi 30%+ validator vĩnh viễn offline.

---

## 8. Light client và checkpoint sync

### 8.1 Sync committee

Mỗi epoch (default 1024 macro height ~ vài giờ), giao thức chọn `SYNC_COMMITTEE_SIZE = 512` slots (weighted random **with replacement** từ active validator set, theo seed = `R^{macro}_{epoch_start}`) làm sync committee. Một validator có thể chiếm nhiều slot nếu stake lớn; mỗi slot tương ứng với một quyền ký độc lập. Sync committee ký mọi MacroHeader trong epoch đó.

Lý do `with replacement`: cho phép `SYNC_COMMITTEE_SIZE` cố định (= 512) bất kể `|active_set|` lớn hay nhỏ, giữ verification cost của light client deterministic. Đồng thời đây là cùng pattern Ethereum 2.0 sync committee.

Light client chỉ cần:

- 1 trusted weak-subjectivity checkpoint (trust on first use).
- Stream MacroHeader + sync committee aggregate signature.

Verification per header: 1 pairing + bitmap check ⇒ < 5ms trên mobile.

### 8.2 Sync committee transition

Cuối mỗi epoch, SyncCommitteeUpdate được embed vào MacroCheckpoint của height đầu epoch mới. Light client xác minh transition bằng signature của committee CŨ trên committee MỚI.

Lưu ý: sync committee KHÔNG có quyền finality. Họ chỉ là "gateway" cho light client. Một sync committee bị corrupt vẫn không thể fork chain — chỉ có thể cung cấp invalid header cho light client (light client phát hiện được nếu cross-check với full node).

### 8.3 Weak subjectivity policy (rev.3 updated)

`WEAK_SUBJECTIVITY_PERIOD = 1 week` (rev.3 hạ từ 2 tuần). Node mới sync phải có một checkpoint không quá 1 tuần tuổi từ một nguồn tin cậy (foundation, friend, official explorer). Lý do hạ:
- Bitcoin checkpoint default ON (§8.4) cung cấp tighter long-range bound (~60min thay vì tuần).
- WS chỉ còn vai trò bootstrap đầu (sync từ static checkpoint), không phải primary defense.
- 1 tuần đủ rộng để node bị offline cuối tuần catch-up qua peer-to-peer mà vẫn an toàn.

`WITHDRAWAL_DELAY = 6 BTC blocks (~60 min)` (rev.3, replaces "2 weeks + 1 day"). Validator request exit → enter exit queue → stake lock cho đến khi MacroCheckpoint chứa exit transaction được Bitcoin-anchored với 6 confirmations. Đây là **Sovereign Epoch Finality requirement** (§1.4).

Đây là hệ quả trực tiếp của Babylon design (Tas 2022): với Bitcoin anchor, withdrawal window không còn cần weak-subjectivity protection — hash của epoch finalized đã immutable trong Bitcoin.

### 8.4 Bitcoin checkpointing (rev.3 default ON)

Tas et al. (2022, "Babylon") chứng minh impossibility result: các vấn đề security của PoS (non-slashable long-range, low liveness, bootstrap) **inherent** nếu không có external trusted source. Họ propose checkpoint PoS state vào Bitcoin PoW. Pikachu (Azouvi 2022) cùng ý tưởng dùng Bitcoin Taproot, transaction size constant.

Rev.3 promote Bitcoin checkpointing từ **optional add-on → default ON, mandatory cho v2.0 mainnet**, sau khi `docs/improve.pdf` §6.4 chỉ ra rằng giữ nó optional tạo time-domain conflict (5s "hard finality" vs 2 tuần WS) gây nhầm lẫn nguy hiểm cho rollup developer.

#### 8.4.1 Mechanism

Mỗi **epoch** (~30 phút, $W \cdot N_{\text{micro}}$ macro slot), một set vigilante relayer nodes (subset của active validator) thực hiện:

1. Tổng hợp `BitcoinCheckpoint = (epoch, macro_height, macro_hash, validator_set_root)` cho latest finalized MacroCheckpoint.
2. Aggregate BLS signature từ ≥ 2W/3 validator vote signing checkpoint.
3. Embed `(macro_hash || aggregate_sig_compressed)` vào một Bitcoin Taproot transaction (script-path spend) — payload ≤ 64 bytes.
4. Broadcast transaction tới Bitcoin mempool. Pay reasonable fee (~50 sat/vbyte default; adaptive to congestion).
5. Monitor confirmations; tại 6 confirmations, sản xuất `BitcoinAnchorProof` (§4) và publish vào LUA-DAG gossip → trigger `epoch_finalized` state cho tất cả MacroCheckpoint trong epoch tương ứng.

#### 8.4.2 Vigilante incentive

- Vigilante relayer earn `BTC_RELAY_REWARD` (parameter, default 5% of macro proposer reward) cho mỗi successful checkpoint.
- Multiple relayer compete; first to confirm gets reward (giảm SPOF).
- Operating cost: ~$10K/year cho Bitcoin transaction fees + relayer node ops (giá 2026 estimate). Funded từ protocol treasury (transaction fee burn allocation) hoặc rollup fee surcharge.

#### 8.4.3 Failure modes

| Mode | Hậu quả | Đối sách |
| ---- | ------- | -------- |
| All vigilantes offline | epoch_finalized không advance | Validator unbond bị block; chain tiếp tục produce Fast Exec Finality cho rollup tx |
| Bitcoin reorg > 6 blocks | BitcoinAnchorProof bị invalid | Re-checkpoint epoch sau khi BTC chain stabilize; `epoch_finalized` revert ⇒ accountable halt scenario |
| Bitcoin fee spike (e.g., congestion) | Checkpoint delayed | Adaptive fee + fallback to next epoch; SLA: ≤ 2 missed epochs |

Quyết định **default ON** với explicit failure modes thay vì optional là cần thiết cho UX rõ ràng — rollup developer không phải worry về "có Bitcoin anchor hay không".

### 8.5 Checkpoint sync flow

```
1. New node fetches WS checkpoint (from K independent sources, K ≥ 3)
2. Verify checkpoint signatures match validator set published at WS time
3. (rev.3) Cross-check WS checkpoint với latest BitcoinAnchorProof từ Bitcoin chain →
   nếu disagreement, BTC anchor wins (immutable evidence)
4. Download MacroHeaders + sync committee sigs from checkpoint to head
5. Optionally: download state snapshot at latest finalized height
6. Begin live participation
```

State snapshot (mục 5 trên) là **out of scope cho rollup snapshot** — rollup tự lo snapshot của state mình. LUA-DAG chỉ cung cấp DA root + macro header chain + Bitcoin anchor headers.

### 8.6 Finality Boundaries Table (rev.3 addition)

Rev.3 spec out explicit finality requirement cho từng use case, để rollup/bridge developer integrate đúng:

| Use case                                                  | Latency yêu cầu | Tier finality bắt buộc           | Notes |
| --------------------------------------------------------- | --------------- | -------------------------------- | ----- |
| Rollup UI preview                                         | < 2s            | `soft_confirmed`                 | Có thể revert nếu macro fork; UX hint cho user "pending" |
| Rollup fee charge (revertible)                            | < 2s            | `soft_confirmed`                 | Same as above |
| Rollup tx commit (DEX trade, NFT mint, etc.)              | 5–10s           | `finalized` (Fast Exec)          | Accountable safety: revert chỉ khi ≥ W/3 stake slashed |
| Cross-rollup messaging (cùng ecosystem)                   | 5–10s           | `finalized` (Fast Exec)          | Both chains share trust assumption với LUA-DAG |
| Bridge withdrawal **dưới `BRIDGE_VALUE_CAP`**             | 5–10s           | `finalized` (Fast Exec)          | Default cap = `1000 × MIN_STAKE`; bridge customize |
| Bridge withdrawal **trên `BRIDGE_VALUE_CAP`**             | ~60 min         | `epoch_finalized` (Sovereign)    | High-value: phải đợi BTC anchor |
| Validator unbond / withdrawal                             | ~60 min         | `epoch_finalized` (Sovereign)    | **BẮT BUỘC** — không có exception |
| Validator set rotation (epoch boundary)                   | ~60 min         | `epoch_finalized` (Sovereign)    | Active set thay đổi phải Bitcoin-anchored |
| Slashing distribution / treasury withdrawal               | ~60 min         | `epoch_finalized` (Sovereign)    | Long-range protection |
| Light client sync (passive read)                          | ≥ `justified`   | Vary                             | Read-only; choice dựa trên risk tolerance |

`BRIDGE_VALUE_CAP` default = `1000 × MIN_STAKE` cho mainnet starter — bridge protocol có thể set lower nhưng **không thể** set higher mà không opt-out khỏi Sovereign-grade safety. Cap được parameterize trong genesis và adjustable via governance.

**Why this matters**: rev.2 và v1 implicitly mix two finality concepts trong term "finalized". Rev.3 enforces clean separation:
- "Fast Execution Finality" = accountable safety dưới giả định attacker chưa unbonded.
- "Sovereign Epoch Finality" = absolute safety (relies on Bitcoin PoW).

Một validator có $W/3$ stake có thể bypass Fast Execution Finality bằng cách: (a) sign conflicting MacroQC, (b) chấp nhận slashing, (c) unbond trước khi slashing executes. Nếu `WITHDRAWAL_DELAY < attack_window`, attack thành công. Bitcoin anchor đảm bảo `WITHDRAWAL_DELAY > attack_window` cho mọi tài khoản unbond.

---

## 9. Permissionless membership

V1 dùng từ "permissionless" mà không định nghĩa cách join/leave. V2 spec hoàn chỉnh.

### 9.1 Activation queue

Validator deposit stake → enter activation queue. Activation rate giới hạn `MAX_ACTIVATION_PER_EPOCH = 4` validator/epoch để:

- Tránh sudden validator set jump → vỡ assumption stake-weight stable.
- Đảm bảo sync committee có thời gian transition.

Trong queue, deposit không tạo voting power, không nhận reward.

### 9.2 Withdrawal (rev.3 updated)

Validator request exit → enter exit queue (`MAX_EXIT_PER_EPOCH = 4`). Sau khi exit accept, stake vẫn lock cho đến khi `epoch_finalized` (§3.5) đạt được — `WITHDRAWAL_DELAY = 6 BTC blocks (~60 min)` (rev.3 hạ từ 2 tuần nhờ Bitcoin checkpoint default ON, §8.4).

Quy trình chi tiết:
1. Validator submit `ExitRequest { validator_id, epoch_request, exit_sig }`.
2. ExitRequest được include trong MacroCheckpoint $C_h$ (next available slot, subject to `MAX_EXIT_PER_EPOCH`).
3. $C_h$ phải đạt `finalized` (Fast Exec) trước khi exit_queue advance.
4. $C_h$ phải đạt `epoch_finalized` (Sovereign — 6 BTC blocks confirm BitcoinAnchorProof) trước khi stake được released.
5. Validator có thể withdraw funds.

Trong window từ bước 3 đến bước 4 (~60 min), validator vẫn ở exit_queue nhưng KHÔNG còn vote / produce vertex / participate consensus. Stake vẫn slashable nếu equivocation evidence được submit.

### 9.3 Churn limit

Tổng (activation + exit) rate được cap cứng để giữ validator set evolution slow enough cho:

- Sync committee transition smooth.
- VRF beacon predictability không bị biến động lớn.
- Subnet rebalancing ổn định.

### 9.4 Randomness beacon refresh

Beacon $R$ được tạo từ MacroQC trước. Vì MacroQC là output của 2/3 stake aggregate, kẻ tấn công không thể bias beacon trừ khi kiểm soát > 1/3 stake (tại đó hắn có vấn đề lớn hơn).

**Grinding resistance**: macro proposer về kỹ thuật vẫn có thể chọn không include một số partial signature (ví dụ để bias beacon hơi nhỏ), nhưng bị hạn chế bằng hai cơ chế:

- **Inclusion-delay reward**: validator có vote include muộn hơn được giảm reward exponential. Proposer cố tình bỏ vote sẽ bị các validator đó tố cáo và proposer mất reward macro propose.
- **Subnet aggregation guard** (rev.2 updated): subnet aggregator pre-publish `subnet_aggregate[k]` lên gossip (xem §7.4 Mode A bước 2) **trước khi** macro proposer build MacroQC. Mọi validator có thể quan sát các subnet_aggregate đã public. Macro proposer không thể "ẩn" một subnet_aggregate đã public — nếu cố ý exclude, các validator sẽ refuse-to-vote MacroQC (vì biết có aggregate hợp lệ chưa được include) và proposer mất reward. Proposer chỉ có thể exclude subnet nếu subnet thực sự không publish kịp $T_{\text{subnet}}$.

Kết hợp lại, biên grinding bị cap ở vài bit entropy mỗi epoch — không đủ để bias VRF anchor selection theo cách meaningful trong steady state. Đây là cùng pattern Ethereum dùng cho RANDAO grinding.

### 9.5 Validator set root publishing

Mỗi MacroCheckpoint chứa `validator_set_root` — Merkle root của (validator_id, pubkey, weight) tuple. Light client dùng root này để verify subnet membership và sync committee membership.

### 9.6 Slashing để thoát khỏi validator set

Nếu validator bị slash 100%, hắn auto-exit. Stake bị burn (không quay lại tay attacker dù qua delegation).

### 9.7 Anti-Sybil mechanisms (rev.3 addition)

Đây là response trực tiếp cho `docs/improve.pdf` §6.5: 5% cap chỉ là vanity metric nếu attacker có thể trivially split 30% stake thành 6 ẩn danh node × 5%. Rev.3 add ba lớp defense-in-depth:

#### 9.7.1 Network identity declaration

Tại activation, validator MUST submit `ValidatorIdentity { validator_id, asn, cloud_provider, region_code, attestation_sig }` (§4 struct):

- **ASN**: Autonomous System Number của primary uplink. Verifiable qua passive measurement (PoP-style) bởi monitoring nodes.
- **Cloud provider**: enum `{aws, gcp, azure, hetzner, ovh, self_hosted, ...}`. Nếu cloud, optional region cấp 2 (e.g., `aws:us-east-1`).
- **Region code**: ISO 3166-2 hoặc cloud-specific zone.
- **Re-attestation**: yêu cầu mỗi `IDENTITY_REATTEST_PERIOD = 4 epochs` (~2 hours) để catch IP/cloud changes; KHÔNG re-attest = treated như "all unknown" → max concentration_score → max reward decay (§10.6).

Khai báo này KHÔNG phải KYC; không link tới legal entity. Mục đích là chỉ enable **technical clustering** để compute concentration score.

#### 9.7.2 Concentration score

Tại mỗi epoch, hệ thống compute `concentration_score(v_i)` cho mỗi validator $v_i$:

$$
\text{concentration\_score}(v_i) = \alpha \cdot s_{\text{ASN}}(v_i) + \beta \cdot s_{\text{cloud}}(v_i) + \gamma \cdot s_{\text{voting}}(v_i)
$$

trong đó:
- $s_{\text{ASN}}(v_i) = (\sum_{v_j \in \text{same\_ASN}} w_j) / W$ — tỷ lệ stake cùng ASN.
- $s_{\text{cloud}}(v_i) = (\sum_{v_j \in \text{same\_cloud\_region}} w_j) / W$ — tỷ lệ stake cùng cloud:region.
- $s_{\text{voting}}(v_i)$ = max correlation coefficient với voting pattern của validator khác trong rolling window 100 macro slot.
- $\alpha, \beta, \gamma$: tuning parameters, default $(0.4, 0.4, 0.2)$.

Validator declared "self_hosted" (không thuộc cloud) có $s_{\text{cloud}} = 0$ — incentive cho self-host.

#### 9.7.3 DKG-based key origin fingerprinting

(Áp dụng v2.1+; v2.0 chỉ ship infrastructure để collect data, không enforce slashing.)

Validator có thể (optional, opt-in cho reward boost) tham gia một "Distributed Key Ceremony Registry" tại activation:
- Validator generate signing key qua DKG protocol (Pedersen DKG hoặc tBLS) với một foundation-coordinated quorum.
- DKG produces verifiable evidence rằng key được derive từ một independent fresh randomness.
- Kẻ tấn công cố tách key từ cùng seed → DKG transcript expose origin → slashable.

`DKG_SLASH_BASE = 20%`; mỗi validator detected sharing key origin với thêm validator khác → slash exponentially: `slash = DKG_SLASH_BASE × 2^(n-1)` cho `n` validators sharing origin (n=2: 20%, n=3: 40%, n=4: 80%, ...).

V2.0 ship DKG registry as **opt-in mode** (không mandatory) để gather data và refine; v2.1 sẽ make mandatory cho new validator activations.

#### 9.7.4 Limitations

Anti-Sybil mechanisms KHÔNG perfect:
- ASN/cloud declaration có thể fake (trade off: false declaration → opportunity cost của being concentrated incorrectly).
- VPN/Tor có thể obscure ASN. Hệ thống treat unknown ASN như max concentration.
- Sophisticated attacker với multiple cloud accounts ở multiple regions có thể distribute → tăng cost gấp $n$ lần (so với native 5% cap alone).

Defense-in-depth model: 5% cap cộng với (a) reward decay (economic disincentive), (b) DKG fingerprinting (cryptographic detection trong v2.1+), (c) governance review (off-chain social layer cho egregious cases) tăng total cost của Sybil đáng kể, **không** chứng minh bất khả thi nhưng nâng bar lên một cách có ý nghĩa.

---

## 10. Incentive và accountability

### 10.1 Reward decomposition

Mỗi epoch, reward pool được phân chia:


| Loại reward                       | %   | Điều kiện                                                                                |
| --------------------------------- | --- | ---------------------------------------------------------------------------------------- |
| Base reward                       | 28% | Validator online, chữ ký xuất hiện trong ≥ 80% certified vertices của epoch              |
| Vertex authoring                  | 15% | Vertex của validator được certify đúng hạn                                               |
| Anchor proposing                  | 10% | Validator được chọn anchor và anchor commit thành công                                   |
| Micro committee                   | 5%  | Validator được chọn vào committee và vote MicroQC                                        |
| Macro voting                      | 28% | Validator vote MacroQC trong ≥ 95% macro height                                          |
| Macro proposing                   | 5%  | Validator propose macro và checkpoint được justified                                     |
| Subnet/flat aggregation           | 4%  | Validator làm aggregator (subnet ở Mode A, hoặc proposer ở Mode 0) và submit đúng hạn   |
| Bitcoin vigilante relay (rev.3)   | 5%  | Validator làm vigilante relayer và Bitcoin checkpoint confirmed (xem §8.4.2)             |


Tỷ lệ này được tham số hóa và có thể adjust qua governance.

### 10.2 Fee market

Ingress validator (nhận blob từ rollup) thu fee từ rollup. Fee phân chia:

- 50% cho ingress validator (incentive process blob).
- 30% cho macro proposer (incentive include vào DA root).
- 20% burn (deflation cho token).

Fee market dạng EIP-1559: base fee adjust theo target DA usage (default 50% capacity), priority tip cho ingress validator.

### 10.3 Validator economics target

Target steady-state:

- Inflation: 4–6% APR cho stake (gross).
- Validator running cost: ~$2–5k/month (cloud) hoặc ~$500/month (self-hosted).
- Min stake $50k–100k equivalent → break-even ở stake price không quá thấp.

Tokenomics chi tiết (initial distribution, vesting, foundation allocation) **out of scope** cho doc kỹ thuật này, sẽ là tokenomics paper riêng.

### 10.4 Inactivity leak (xem mục 7.7)

Đã spec.

### 10.5 Slashing recipient

Slashed stake **burn 100%** cho equivocation/double-vote. Cho data unavailability và inactivity leak, slash pool có thể allocate một phần cho whistleblower (validator submit evidence).

### 10.6 Diminishing returns formula (rev.3 addition)

Reward earned bởi validator $v_i$ tại mỗi epoch được scale bởi anti-Sybil factor:

$$
R_{\text{actual}}(v_i) = R_{\text{base}}(v_i) \times (1 - \text{REWARD\_DECAY\_RATE} \times \text{concentration\_score}(v_i))
$$

trong đó `concentration_score` được tính theo §9.7.2 và `REWARD_DECAY_RATE = 1.0` (default — full decay nếu cùng cluster). Floor: `R_actual ≥ 0`.

**Examples**:
- Validator solo với declared self_hosted, ASN unique, không correlate với ai → concentration_score ≈ 0 → full reward.
- Validator chia stake thành 6 node × 5% trên cùng AWS:us-east-1 → mỗi node có $s_{\text{cloud}} \approx 0.30$ → concentration_score $\approx 0.4 \times s_{\text{ASN}} + 0.4 \times 0.30 + 0.2 \times s_{\text{voting}}$ ≈ 0.30 → 30% reward decay → mỗi node mất 30% reward → tổng attacker mất ~30% × 30% = 9% APR. Đây là **economic friction** nhưng KHÔNG phải hard barrier; cần kết hợp với DKG fingerprinting (§9.7.3).
- Validator với declared ASN known to be Tor exit node → treated như max concentration (anti-anonymity-attack feature).

**Bootstrap consideration**: tại genesis với <50 validators, concentration_score sẽ structurally cao (small population). Spec định ngĩa `BOOTSTRAP_GRACE_PERIOD = 30 epochs` (~15 days) trong đó decay được scale × 0.5 để cho phép validator set grow without punishing early adopters.

**Governance-adjustable**: `REWARD_DECAY_RATE`, `α/β/γ` trong concentration_score, và `BOOTSTRAP_GRACE_PERIOD` đều có thể adjust qua governance vote sau mainnet stable.

---

## 11. Security analysis

### 11.1 Safety theorem (macro layer)

**Mệnh đề**: Giả sử Byzantine stake $f < W/3$. Thì không tồn tại hai MacroCheckpoint conflict cùng được finalized, trừ khi bằng chứng slashable cho ít nhất $W/3$ stake được tạo ra.

**Proof sketch**: Hai finalized checkpoint $C, C'$ conflict (không cùng chain) ⇒ mỗi cái có MacroQC ≥ 2W/3. Hai tập 2W/3 phải intersect ở ≥ W/3. Validator trong intersection đã ký vote cho cả $C$ và $C'$ ⇒ surrounding vote (slashable). $\square$

Đây là port trực tiếp của Casper FFG safety. Proof formal đã có trong paper Casper FFG; v2 reuse.

### 11.2 Liveness theorem

**Mệnh đề**: Sau GST, nếu honest online stake > 2W/3, thì với mọi $T > 0$, tồn tại $T' > T$ sao cho có MacroCheckpoint mới được finalized trước thời điểm $T'$.

**Proof sketch**:

- Sau GST, mọi gossip giữa node đúng deliver trong $\Delta$.
- VRF anchor selection có ≥ 2/3 xác suất anchor là honest mỗi wave.
- Honest anchor + 2f+1 honest vertices ở round tiếp ⇒ commit shortcut path.
- Macro proposer với honest > 2/3 ⇒ gather 2/3 stake votes ⇒ MacroQC.
- Hai macro liên tiếp với honest proposer ⇒ finality. Eventually đạt được. $\square$

Lưu ý: liveness chỉ guaranteed eventual — không có upper bound deterministic trên latency. Đây là hệ quả của FLP và là chấp nhận được cho mọi BFT-class protocol.

### 11.3 Committee safety probability

V1 không phân tích định lượng. V2 cung cấp:

Committee size $C$, Byzantine stake fraction $\beta$. Xác suất committee có ≥ $C/3$ Byzantine (committee-level safety violation) là:

$$P(\text{capture}) = \sum_{k=\lceil C/3 \rceil}^{C} \binom{C}{k} \beta^k (1-\beta)^{C-k}$$

Bảng tham chiếu:


| C (committee size) | β = 0.20    | β = 0.30 | β = 0.33 |
| ------------------ | ----------- | -------- | -------- |
| 64                 | 1.5 × 10⁻³  | 0.18     | 0.42     |
| 128                | 2.7 × 10⁻⁵  | 0.06     | 0.40     |
| 256                | 1.0 × 10⁻⁸  | 0.005    | 0.37     |
| 512                | 1.5 × 10⁻¹⁵ | 4 × 10⁻⁵ | 0.34     |
| 1024               | < 10⁻³⁰     | 3 × 10⁻⁹ | 0.30     |


Quan sát quan trọng: ở β gần 1/3 (ngưỡng global safety), committee capture probability vẫn cao **với mọi committee size hữu hạn**. Đây là lý do **micro layer KHÔNG được quảng bá là hard finality** — soft confirmation chỉ an toàn khi β đáng kể nhỏ hơn 1/3.

V2 chọn `C_micro = 256` cho default. Với β = 0.2, capture probability ~10⁻⁸ — chấp nhận được cho UX-level confirmation. Với β > 0.3, soft confirmation mất ý nghĩa và rollup nên switch sang chỉ trust hard-finality.

### 11.3.1 Probabilistic robustness (rev.2 addition)

Phân tích worst-case 1/3 Byzantine ở trên là **không đủ** cho operational planning. Mighan et al. (2024) dùng Markov chain model cho ETH 2.0-like consensus và kết luận: **xác suất consensus highly sensitive với truthful voting probability**. Cụ thể, ngay cả khi không có Byzantine adversary, xác suất "rational defection" (validator vote chậm, vote sai vì local state issue, hoặc abstain) ở mức 5-10% có thể giảm consensus throughput đáng kể.

LUA-DAG v2 cần đo các tỷ lệ sau ở adversarial testnet (P6):


| Đại lượng                                                 | Threshold pass               |
| --------------------------------------------------------- | ---------------------------- |
| Truthful vote rate ở macro layer                          | > 95% trong điều kiện normal |
| Truthful vote rate ở micro committee                      | > 92% trong điều kiện normal |
| MacroQC formation latency p95 dưới 5% rational defection  | < 7s                         |
| Time to recover after `INACTIVITY_LEAK_THRESHOLD` reached | < 2 epochs                   |


Nếu testnet measure thấy truthful vote rate < 90%, cần điều tra root cause (network issues, client bug, incentive misalignment) trước khi mainnet.

### 11.4 Bảng tấn công và đối sách


| Tấn công                                          | Hậu quả                                              | Đối sách (rev.3)                                                                                                                                          |
| ------------------------------------------------- | ---------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Long-range PoS fork                               | Lừa node mới sync vào chain giả                      | Weak subjectivity 1 tuần + Bitcoin checkpoint default ON (§8.4) — attacker phải re-mine BTC PoW history                                                  |
| Data withholding                                  | Soft-confirm blob nhưng data không lấy được          | Certified vertex chỉ accept blob có chunks verified; retrieval challenge với slashing                                                                     |
| Anchor DoS                                        | Mất commit shortcut, fall back slow path             | VRF private sortition, slow path 4 round, backup propagation                                                                                              |
| Macro proposer DoS                                | Mất 1 macro window                                   | Mode B leaderless gossip aggregation (§7.4.4); timeout 4s                                                                                                 |
| Equivocation                                      | Tạo conflict checkpoint                              | Slashable 100%, 2-chain rule đảm bảo accountable safety                                                                                                   |
| Subnet capture (Mode A only, $k$ subnets bị > 2/3 Byzantine) | Subnet aggregate sai → corrupted MacroQC | Subnet partition VRF-rebuild mỗi epoch (chống sybil-stacking); proposer verify từng subnet riêng; quorum rule cho phép skip subnet xấu nếu các subnet còn lại tổng ≥ 2W/3 (§7.4.3)  |
| Mode B canonical race                             | Hai honest validator pin different MacroQC candidate | Stake-priority + lex tie-break (§7.4.4); convergence trong $O(\log N)$ gossip rounds                                                                      |
| MEV ordering manipulation                         | Anchor reorder để extract MEV                        | Default OFF (rollup tự xử lý); optional Fino-style fairness mode (§1.2) khi rollup opt-in                                                                 |
| Storage spam                                      | Đẩy bytes vô nghĩa qua DA                            | Fee market + min fee floor + max blob size cap per slot; v2.0 throughput cap 5 MB/s tự nhiên hard-bounds spam                                             |
| Sync committee corruption                         | Light client bị fed invalid header                   | Sync committee không có quyền finality; full node cross-check phát hiện; Bitcoin anchor (rev.3) là third source-of-truth                                  |
| VRF grinding                                      | Bias anchor selection                                | Beacon từ MacroQC nên grinding cần > 1/3 stake; deterministic aggregation rule                                                                            |
| Eclipse attack on light client                    | Light client bị isolated                             | Multi-source header sources khuyến nghị; Bitcoin anchor là external authoritative source; defense thuộc deployment                                        |
| Adaptive corruption sau VRF reveal                | Chỉ corrupt anchor sau khi anchor đã expose          | Round duration < adaptive corruption time (~giờ) ⇒ irrelevant                                                                                             |
| Sybil via stake split (rev.3)                     | Một entity có 30% stake giả mạo 6 ẩn danh validator  | (a) 5% cap, (b) ASN/cloud declaration + reward decay (§9.7.2, §10.6), (c) DKG-fingerprint slashing v2.1+ (§9.7.3)                                         |
| Validator unbond + long-range (rev.3)             | Attacker unbond → no slashable → forge old chain     | `WITHDRAWAL_DELAY = 6 BTC blocks`; validator stake locked đến `epoch_finalized` — không bao giờ unbonded mà chưa Bitcoin-anchored                         |
| Bitcoin reorg attack                              | BTC reorg > 6 blocks → anchor invalid                | Re-checkpoint epoch sau khi BTC stabilize; chấp nhận `epoch_finalized` revert là failure mode (very rare in practice)                                     |
| Post-quantum attack (rev.3)                       | BLS + ECVRF broken by future quantum                 | OUT OF SCOPE v2.0 — ack openly. Full PQ migration là single coordinated event ở **v3** (BLS → STARK aggregation, ECVRF → lattice-VRF; xem §13.4)         |


### 11.5 Cross-layer attack: micro flush after macro stall

**Scenario**: Macro layer stall vì 35% stake offline. Inactivity leak chạy. Trong lúc đó micro layer vẫn tiếp tục commit (nhanh, chỉ cần committee). Rollup soft-confirm hàng nghìn blob. Khi macro resume, có thể macro chain chọn finalize một micro chain CŨ HƠN, revert toàn bộ soft-confirmed blob mới.

**Đối sách**: Micro layer phải tôn trọng `lock_macro` — anchor ở wave $w$ chỉ được build trên DAG có ancestor là `lock_macro = latest_justified_macro_checkpoint`. Khi macro stall, lock_macro không advance ⇒ anchor có thể tiếp tục commit nhưng phải chỉ đến cùng macro parent. Khi macro resume, MacroProposal sẽ include `micro_head` từ DAG này — không có revert.

**`lock_macro` race condition** (rev.2 addition): trong partial-sync, hai honest validator có thể có local view khác nhau về `latest_justified_macro_checkpoint` — node A thấy MacroQC cho $C_h$, node B chưa. Nếu node A tạo anchor pinned tại $C_h$ nhưng node B vẫn pin $C_{h-1}$, hai anchor có thể conflict trong cùng wave.

Quy tắc giải quyết:

- Validator MUST **never downgrade** `lock_macro` (monotonic: chỉ advance, không bao giờ rollback).
- Khi quan sát anchor `A` với `lock_macro(A) > my_lock_macro`, validator MUST **fetch** MacroQC justify cho `lock_macro(A)` và update local trước khi vote certification cho `A`.
- Anchor với `lock_macro(A) < my_lock_macro` bị reject ⇒ không cert ⇒ wave skip qua slow path (§6.4). Latency penalty 1 wave nhưng không ảnh hưởng safety.
- `MicroCheckpoint` lưu `parent_macro = lock_macro(A)` explicit (đã có trong §4 struct) ⇒ verifier dễ dàng audit consistency.

Edge case duy nhất: nếu macro fork (hai macro chain conflict, cả hai justified) → conflict, một bên phải được dismiss bằng slashing (mục 11.1). Trong lúc resolve, tất cả soft-confirm trên cả hai bên đều không reliable; rollup phải chuyển sang chỉ trust `finalized` (§3.5).

### 11.6 Honest minority resilience

Nếu honest stake = 51%, Byzantine = 49%:

- Liveness vẫn giữ nếu Byzantine không actively prevent (chỉ withhold vote).
- Safety vẫn giữ — Byzantine < 2/3 không thể tạo MacroQC.
- Soft confirmation gần như mất hoàn toàn (committee capture > 50%).

Nếu Byzantine vượt 1/3:

- Safety break — có thể có hai MacroQC conflict nhưng SẼ generate slashable evidence.
- Đây là failure mode "accountable" — chain halt, slash, social recovery.
- Validator chưa unbonded sẽ bị slash; validator đã unbonded (vượt `WITHDRAWAL_DELAY`) bypass được slash NHƯNG (rev.3) Bitcoin anchor đảm bảo lịch sử immutable → giả mạo sub-tree không persist.

Nếu Byzantine vượt 1/2:

- Censorship attack possible: Byzantine có thể block honest blob khỏi DAG.
- Liveness broken cho honest workload.
- Safety vẫn cần slashing để break (kẻ tấn công vẫn lose stake nếu chưa unbonded).
- (rev.3) Bitcoin checkpoint vẫn bảo vệ historical chain — attacker chỉ control go-forward, không thể rewrite past beyond latest BTC anchor.

### 11.7 Sybil resistance analysis (rev.3 addition)

Trả lời cho `docs/improve.pdf` §6.5: 5% cap đơn lẻ KHÔNG đủ chống Sybil trong permissionless setting. Defense-in-depth model rev.3:

#### 11.7.1 Cost model cho attacker

Giả định attacker có stake $w_A$ với $0.05 < w_A/W < 0.33$ (nghĩa là vượt 5% cap nhưng dưới Byzantine threshold):

| Strategy | Cost | Effectiveness |
| -------- | ---- | ------------- |
| Một validator (raw) | Stake bị burn 95% nếu attack thành công | Voting power capped tại 5% |
| Split thành $n = w_A / 0.05$ ẩn danh nodes, cùng cloud:region (rev.2 attack) | Stake + cloud cost | Pre-rev.3: full $w_A$ voting power. Rev.3: ~70% sau reward decay (§10.6). |
| Split + multi-cloud distribution | Stake + ~$n$x cloud costs | ~85% voting power (asn diversity tốt nhưng vẫn correlate ở voting pattern) |
| Split + multi-cloud + multi-region + DKG-evade (v2.1+) | Stake + ~$n$x cloud + sophisticated key generation | ~95% voting power; DKG fingerprinting có thể vẫn detect via behavioral analysis |

Conclusion: rev.3 tạo **economic friction** ở mọi Sybil strategy, không phải hard barrier nhưng raise attack cost lên 1.5–10× tùy strategy.

#### 11.7.2 Detection rate target (KPI mới ở §1.3)

Adversarial testnet ở P6 phải đo:
- Anti-Sybil correlation alarm rate ở baseline (no Sybil): < 1% false positive.
- Detection rate khi inject 6-node split scenario: ≥ 95% true positive.
- Detection latency: < 5 epochs (~30 min).

Nếu detection rate < 90%, cần investigate alternatives — tăng $\gamma$ (voting correlation weight), enable mandatory DKG, hoặc social-layer escalation.

#### 11.7.3 Limitations và assumed remediation

Spec acknowledges:
- Attacker với deep pockets (multi-cloud, multi-region, sophisticated key generation) vẫn có thể bypass detection.
- VPN/Tor/Bridge ẩn danh có thể obscure ASN.
- DKG mandatory sẽ defer tới v2.1 vì cần crypto research mature trước.

Mitigation cho gaps:
- Governance can pause/slash validators có behavioral anomaly score quá cao (last-resort).
- Foundation reserves quyền ban validator đã được proven Sybil qua off-chain investigation (similar tới Cosmos governance precedent).

---

## 12. Performance plan và expected numbers

### 12.1 Benchmark matrix


| Tham số              | Giá trị quét                                                                           |
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
- **Latency**: p50/p95/p99 cho ingest→cert, ingest→soft, ingest→finalized.
- **Bandwidth**: per-validator inbound/outbound MB/s, breakdown theo (vertex, vote, chunk, sig).
- **CPU**: per-validator, breakdown theo (sig verify, aggregate, hash, IO).
- **Storage**: GB/day cho hot, warm, cold tier; archive storage.
- **Recovery**: time-to-finality after partition heal; time to drain inactivity leak.
- **Fairness**: inclusion delay distribution per blob; per-namespace fairness.
- **Security signal rate**: false positive rate cho retrieval challenge; correlation of slashing events.

### 12.3 Expected bottleneck order (hypothesis)

Dựa trên kinh nghiệm Narwhal/Bullshark/Mysticeti:

1. **Network bandwidth** cho chunk gossip — sẽ là bottleneck #1 ở > 50 MB/s sustained.
2. **BLS verify pipeline** — bottleneck #2 ở > 200 validator (mỗi vertex cần 2f+1 sigs verify).
3. **Disk write IOPS** cho hot tier — bottleneck #3 ở SSD non-NVMe.
4. **Subnet aggregation latency** — bottleneck #4 ở > 500 validator.

V2 thiết kế từ đầu có:

- Chunk gossip parallel qua peers (load-balanced).
- BLS verify batching (verify N signatures cùng lúc nhanh hơn N×1).
- Vertex storage append-only LSM (RocksDB / Sled).
- Subnet aggregation pipeline (subnet agg song song với macro propose).

### 12.4 Comparison baselines

LUA-DAG v2 prototype sẽ benchmark side-by-side với:

- **Celestia (Tendermint-based)** — DA throughput, finality latency.
- **Avail (BABE+GRANDPA)** — DA sampling cost.
- **Narwhal+HotStuff** — pure DAG mempool baseline.
- **Bullshark** — anchor commit baseline.

Cùng workload, cùng topology, cùng hardware. Tránh apple-to-orange comparison của v1.

---

## 13. Roadmap, risks, open questions

### 13.1 Phased delivery


| Phase                                | Milestone                                                          | Duration                 | Headcount              |
| ------------------------------------ | ------------------------------------------------------------------ | ------------------------ | ---------------------- |
| P0: Spec finalize                    | Whitepaper hoàn chỉnh + TLA+ skeleton                              | 2 tháng                  | 2 protocol + 1 formal  |
| P1: Prototype L1 (Availability DAG)  | Vertex + cert + erasure + GC                                       | 3 tháng                  | 3 distsys              |
| P2: Prototype L2 (Bullshark)         | Anchor commit, fast/slow path, MicroQC, ECVRF                      | 3 tháng                  | 2 protocol + 1 perf    |
| P3: Prototype L3 (Macro)             | Adaptive aggregation (Mode 0/A/B), 2-chain finality, slashing      | 3 tháng                  | 2 protocol + 1 crypto  |
| P3.5: Bitcoin anchor (rev.3)         | Vigilante relayer, Taproot tx batching, BitcoinAnchorProof gossip  | 2 tháng                  | 1 protocol + 1 BTC dev |
| P4: Light client + state sync        | SDK + checkpoint sync, BTC anchor verification                     | 2 tháng                  | 2 client               |
| P5: Permissionless membership + anti-Sybil (rev.3) | Activation/exit queue, beacon, ASN/cloud declaration, concentration score, opt-in DKG registry | 3 tháng                  | 1 protocol + 1 testing + 1 anti-fraud |
| P6: Adversarial testnet              | Fault injection, partition recovery, anti-Sybil scenarios, public report | 3 tháng                  | toàn đội               |
| P7: External audit + hardening       | 2 audits (consensus + crypto), fuzzing, chaos, BTC interop check   | 3 tháng                  | external + 1 internal  |
| **Tổng tới v2.0 prototype-mainnet-ready** |                                                                  | **~24 tháng (parallel)** | **~12–14 nòng cốt**    |
| P8: v2.1 — DAS implementation        | 2D Reed-Solomon + KZG OR RLNC-DAS; light client DAS verification   | 4 tháng                  | 2 crypto + 2 client    |
| P9: v3 — PQ migration                | STARK aggregation + lattice-VRF; coordinated migration             | 6+ tháng                 | TBD (separate program) |


Đây là honest estimate cho **v2.0 prototype** đủ để bắt đầu testnet công khai (rev.3 increased từ ~21 → ~24 tháng do thêm P3.5 Bitcoin anchor và P5 expanded với anti-Sybil). **Production-grade** với ecosystem (rollup integrations, SDK, multi-client) cần thêm 12–24 tháng nữa và đội lớn hơn (20–40 người). v2.1 và v3 là separate programs với scope riêng.

### 13.2 Risk register


| Risk                                 | Severity                          | Mitigation                                                                                                  |
| ------------------------------------ | --------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| Cross-layer interaction bug          | High                              | TLA+ model check ngay từ P0; integration test với chaos suite                                               |
| Adaptive aggregation correctness     | High                              | Crypto audit ưu tiên; fallback to Mode B leaderless gossip                                                  |
| Liveness under partial outage        | High                              | Inactivity leak; anchored slow path; Mode B fallback                                                        |
| Performance không đạt v2.0 target (5 MB/s) | Low (rev.3, lower bar)      | Throughput cap 5 MB/s là conservative — nếu 200 validator không đạt, có vấn đề lớn cần debug                |
| Bitcoin checkpoint operational issues (rev.3) | High                     | Multiple vigilante relayers (no SPOF); fee adaptive; explicit failure mode handling (§8.4.3)                |
| Bitcoin reorg > 6 blocks (rev.3)     | Low (probability ~10⁻⁹/checkpoint)| Re-checkpoint sau khi BTC stabilize; documented failure mode                                                |
| Storage growth out of control        | Low (rev.3, throughput cap)       | GC policy được test ở adversarial testnet; archive node tách riêng                                          |
| Sync committee corruption            | Medium                            | Cross-validation trong client; multi-source headers; Bitcoin anchor là third source-of-truth (rev.3)        |
| Token launch / regulatory            | Medium                            | Out of scope cho doc này; cần legal review riêng                                                            |
| Adoption: rollup nào dùng?           | High                              | Bootstrap với 1–2 design partner trước launch                                                               |
| Cạnh tranh Celestia ecosystem effect | High                              | Differentiation phải rõ: faster finality, Sovereign Epoch tier, accountable slashing, lower validator HW    |
| Anti-Sybil detection accuracy (rev.3) | Medium                           | P6 adversarial testnet với explicit Sybil scenarios; tune α/β/γ parameters; foundation review fallback      |
| DKG mandatory cho v2.1               | Medium                            | Crypto research must mature; v2.0 ship opt-in registry để gather data                                       |
| Quantum readiness (rev.3 honest)     | **Acknowledged limitation v2.0** | v2.0 KHÔNG post-quantum (consistent crypto stack: ECVRF + BLS classical); full migration ở **v3** (§13.4)   |


### 13.3 Open questions cần resolve trước khi viết implementation plan

**Resolved in rev.3** (no longer open):
- ~~VRF cipher suite~~: **CHỐT ECVRF Edwards25519** (rev.3, §6.2). iVRF moved to v3 deferred list.
- ~~Aggregation primitive~~: BLS12-381 default cho v2.0; EdDSA tradeoff vẫn cần benchmark P2.
- ~~Bitcoin checkpoint mode~~: **CHỐT default ON, mandatory cho v2.0 mainnet** (rev.3, §8.4).
- ~~Reputation range~~: **CHỐT narrow `[0.8, 1.2]`** (rev.3, §6.2).
- ~~Subnet count hard-code~~: **CHỐT adaptive** (rev.3, §7.4). Mode 0/A/B với threshold 500/1000.

**Still open** (cần resolve trước implementation):
- Reed-Solomon library: tự build hay dùng existing (`reed-solomon-erasure`, RaptorQ)? Default: `reed-solomon-erasure` cho v2.0.
- Implementation language: Rust (default, Sui/Solana ecosystem), Go (Cosmos ecosystem), Zig (experimental). Default: Rust.
- State machine cho validator role: actor model (Tokio) vs explicit FSM. Default: actor model.
- Light client SDK targets: TypeScript (web), Rust (native), Swift/Kotlin (mobile)? Default: TypeScript first.
- Tokenomics: defer cho doc riêng. Min stake, inflation rate, fee burn rate cần economics modeling.
- Governance: on-chain (delegated voting on macro layer) hay off-chain (foundation initial). Default: off-chain v1, on-chain v2.
- Bridge integration: làm reference bridge cho Ethereum trước hay defer? Default: defer.
- Multi-client strategy: bootstrap với 1 client, đến sau audit thứ 2 mới fund client thứ 2. Default: confirmed.
- **Optional MEV fairness mode (rev.2)**: enable Fino-style integration trong v2 hay defer? Default: spec ready, default OFF, rollup opt-in.
- **Formal verification framework (rev.2)**: TLA+ vs Coq + LiDO-DAG framework. Default: TLA+ ở P0, evaluate Coq cho P1 onwards.
- **Mode B canonical MacroQC tie-break (rev.2, §7.4.4)**: lex-smallest bitmap (current) — vẫn cần verify ở P6 không tạo perverse incentive.
- **`lock_macro` advance protocol (rev.2, §11.5)**: dedicated gossip topic vs piggyback. Default: dedicated trong P3 prototype.
- **Custody assignment churn (rev.2, §5.5.1)**: 1 epoch grace period; cần kiểm tra storage spike risk.
- **Bitcoin vigilante economics (rev.3, §8.4.2)**: $10K/year Bitcoin tx fees — funded từ treasury hay rollup surcharge? Cần economic modeling.
- **Bitcoin Taproot vs alternative payload (rev.3, §8.4.1)**: Taproot script-path spend (current default) vs alternative methods (commitment via OP_RETURN, drivechain, BitVM). Default Taproot — cần evaluate fee cost trade-off ở P3.5.
- **Anti-Sybil concentration weights (rev.3, §9.7.2)**: $\alpha = 0.4, \beta = 0.4, \gamma = 0.2$ default — cần validate qua P6 adversarial testnet với various Sybil scenarios.
- **DKG ceremony protocol (rev.3, §9.7.3)**: Pedersen DKG vs threshold BLS DKG vs FROST. Decision required for v2.1; v2.0 ship opt-in scaffolding.
- **`BRIDGE_VALUE_CAP` default (rev.3, §8.6)**: `1000 × MIN_STAKE` default; cần feedback từ design partner bridges.

### 13.4 Deferred to v2.1+ (KHÔNG trong scope v2.0 prototype)

**v2.1 (Hard requirement, scope-locked):**
- **DA sampling cho light verification (BẮT BUỘC v2.1)** — design open giữa 2D Reed-Solomon + KZG (Celestia-style), RLNC-based DAS (Grundei 2025) và polynomial-commitment-based (Hall-Andersen 2025). Decision deadline: end of P7. Khi DAS ship, throughput target unlock từ 5 → 30+ MB/s.
- **Mandatory DKG-based key origin fingerprinting (rev.3)** — v2.0 ship opt-in scaffolding; v2.1 enforce cho new validator activations.
- **Uncertified DAG mode** (Mysticeti-style) với Adelie mitigations — option để boost throughput 2x sau khi v2.0 stable.

**v3 (Major migration, separate program):**
- **Post-quantum signature migration (rev.3 reverted to v3)** — single coordinated event: BLS → STARK aggregation (Drake 2025) AND ECVRF → lattice-VRF/iVRF. Rev.3 explicitly chooses NOT patchwork at v2.1 — full migration timing aligned with Ethereum PQ roadmap and crypto research maturity.
- **GNN-based adaptive parameter tuning** (DAGWise++ 2025).

**Future / out of v2-v3 scope:**
- Restaking integration (EigenLayer AVS / Babylon BTC restaking).
- Shared sequencing layer (cross-rollup atomic).
- ZK header proof cho bridge optimization.
- State rent / storage pricing.
- Cross-chain message passing (IBC-style).
- Validator reputation extensions (advanced beyond Shoal-style basic rep).

---

## 14. So sánh head-to-head: LUA-DAG v1 vs v2 vs các giao thức tương tự

### 14.1 v1 → v2


| Khía cạnh                          | v1                                    | v2 rev.3 (current)                                                                       | Cải thiện vs v1                                            |
| ---------------------------------- | ------------------------------------- | ---------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| Scope                              | Generic L1 với execution placeholder  | DA + finality only, no execution                                                         | Thu nhỏ scope ⇒ ship-able                                  |
| Frontier rule                      | "Xác định" mơ hồ                      | Bullshark anchor commit, đã proven                                                       | Fix lỗ hổng lý thuyết quan trọng nhất                      |
| Macro voting                       | Full validator set, naive aggregation | Adaptive Mode 0/A/B (rev.3) — flat <500, subnet >1000, fallback gossip                   | Scale validator count flexibility, no SPOF                 |
| Permissionless detail              | Một dòng nói "permissionless"         | Full spec activation/exit/churn + anti-Sybil declarations (rev.3)                        | Implementable thực sự                                      |
| Soft vs hard contract              | Note rủi ro nhưng không API           | API explicit `accepted/soft/finalized/epoch_finalized` (rev.3)                           | Ngăn rollup dùng nhầm tier                                 |
| MEV resistance                     | Một dòng "aged-inclusion fee"         | Optional Fino-style mode (rev.2)                                                         | Available cho DeFi rollup mà không trade speed             |
| Long-range protection              | 2-week WS only                        | 1-week WS + Bitcoin checkpoint default ON (rev.3, ~60min withdrawal)                     | Sovereign-grade safety, ngắn UX cho validator              |
| Finality model                     | "Finalized" mơ hồ (1 tier)            | Two-tier: Fast Execution Finality (5–10s) + Sovereign Epoch Finality (~60min) (rev.3)    | Clean separation cho rollup developer integration          |
| VRF crypto                         | Generic VRF                           | ECVRF Edwards25519 (rev.3 — consistent với BLS, no PQ illusion)                          | Honest crypto stack, mature library                        |
| Anti-Sybil                         | 5% cap only                           | 5% cap + ASN/cloud declaration + concentration-based reward decay + DKG fingerprint (rev.3) | Defense-in-depth, kinh tế khó hơn cho stake-split attacker |
| Committee safety                   | Định tính                             | Bảng xác suất số cụ thể + Markov robustness section (rev.2)                              | Có thể defend trước reviewer                               |
| Cross-layer recovery               | Không spec                            | Mục 11.5 spec rõ                                                                         | Tránh micro-flush bug                                      |
| Throughput target v2.0             | "30–100 MB/s" (rev.2 aspirational)    | **5 MB/s hard-cap** với decentralization-preserving HW (rev.3); 30+ MB/s tới v2.1 với DAS | Honest về tradeoff decentralization vs throughput          |
| PQ readiness positioning           | "PQ migration roadmap"                | v2.0 explicitly classical; v3 single coordinated migration (rev.3)                        | No "PQ illusion" pitfall                                   |
| Cạnh tranh thị trường              | "Compete với mọi L1"                  | "Compete trong DA segment, distinguished by Sovereign tier"                              | Clear differentiation                                      |
| Effort estimate                    | 6–8 người × 12–18 tháng               | 12–14 người × ~24 tháng tới testnet (rev.3 +3 tháng cho BTC anchor & anti-Sybil)         | Honest hơn                                                 |


### 14.2 LUA-DAG v2 vs các giao thức tương đồng (rev.2 addition)


| Giao thức                           | Điểm chung với LUA-DAG                                                         | Differentiation (rev.3)                                                                                                                                |
| ----------------------------------- | ------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Celestia (Tendermint + DA)**      | Modular DA + finality, comparable throughput v2.0 (~5 MB/s)                    | Celestia dùng single-leader Tendermint; LUA-DAG dùng DAG (better load balancing) + 2-tier finality với Bitcoin anchor (rev.3) — no Sovereign tier ở Celestia |
| **Avail (BABE + GRANDPA + KZG DA)** | Modular DA, polynomial commitments                                             | Avail có DA sampling từ đầu; LUA-DAG v2.0 defer DAS sang v2.1 NHƯNG có hard finality nhanh hơn + Sovereign tier qua Bitcoin                            |
| **Mysticeti / Sui**                 | DAG-based BFT, ebb-and-flow                                                    | Mysticeti là full L1 với execution; LUA-DAG là DA-only + dual-tier accountable finality (Casper FFG + Bitcoin)                                         |
| **Acki Nacki**                      | 2-step consensus, separation execution-verification và propagation-attestation | Acki Nacki dùng probabilistic safety + random committee per block; LUA-DAG dùng full validator macro vote + accountable safety + Bitcoin anchor        |
| **Babylon**                         | PoS với Bitcoin checkpoint (rev.3 inspired heavily)                            | LUA-DAG có DAG availability + accountable finality riêng (native); Babylon là retrofit checkpoint protocol. Rev.3 LUA-DAG natively integrates Babylon-style anchor as Layer 4. |
| **EigenDA**                         | Modular DA cho rollups, raw throughput target                                  | EigenDA borrow security từ ETH restakers (DAC-like), no native consensus; LUA-DAG có native PoS + accountable slashing + Bitcoin Sovereign tier        |
| **Fino**                            | DAG + MEV resistance                                                           | Fino chỉ là MEV resistance overlay; LUA-DAG (rev.2) tích hợp Fino-style fairness mode optional                                                         |


---

## 15. Phụ lục A — Bảng parameter tham chiếu


| Tham số                              | Default v2.0 (rev.3)                | Range cho tuning |
| ------------------------------------ | ----------------------------------- | ---------------- |
| `ROUND_DURATION`                     | 250 ms                              | 100–500 ms       |
| `WAVE_LENGTH`                        | 4 rounds                            | 2–8 rounds       |
| `MACRO_WINDOW_W`                     | 8 micro-slots                       | 4–16             |
| `MICRO_COMMITTEE_SIZE`               | 256                                 | 128–1024         |
| `SUBNET_FLAT_THRESHOLD` (rev.3)      | 500 validators                      | 200–1000         |
| `SUBNET_FULL_THRESHOLD` (rev.3)      | 1000 validators                     | 500–2000         |
| `SUBNET_TARGET_SIZE` (rev.3)         | 128 validators/subnet               | 64–256           |
| `SUBNET_MAX_COUNT` (rev.3)           | 32                                  | 16–64            |
| `MAX_VERTEX_PAYLOAD`                 | **256 KB** (rev.3 hạ từ 1 MB)       | 64 KB – 1 MB     |
| `MAX_BLOB_SIZE`                      | **1 MB** (rev.3 hạ từ 8 MB)         | 256 KB – 8 MB    |
| `THROUGHPUT_HARD_CAP_V20` (rev.3)    | 5 MB/s sustained                    | fixed v2.0       |
| `ERASURE_RATE`                       | 1/2                                 | 1/2 – 1/4        |
| `GC_HOT_HORIZON`                     | 200 rounds                          | 100–500          |
| `GC_WARM_HORIZON`                    | 10 000 rounds                       | 5 000–50 000     |
| `MAX_ACTIVATION_PER_EPOCH`           | 4                                   | 2–8              |
| `MAX_EXIT_PER_EPOCH`                 | 4                                   | 2–8              |
| `WEAK_SUBJECTIVITY_PERIOD`           | **1 week** (rev.3 hạ từ 2 weeks)    | 3–14 days        |
| `WITHDRAWAL_DELAY`                   | **6 BTC blocks (~60 min)** (rev.3)  | 6–18 BTC blocks  |
| `INACTIVITY_LEAK_THRESHOLD`          | 4 macro windows                     | 2–16             |
| `INACTIVITY_LEAK_RATE`               | 0.5% / window                       | 0.1–1%           |
| `EQUIVOCATION_SLASH`                 | 100%                                | fixed            |
| `DOUBLE_VOTE_SLASH`                  | 100%                                | fixed            |
| `DATA_UNAVAILABILITY_SLASH`          | 5% per occurrence                   | 1–10%            |
| `K_CUSTODY`                          | 2f+1                                | f+1 – 2f+1       |
| `T_RETRIEVE`                         | 30 s                                | 10–120 s         |
| `MIN_CHALLENGE_INTERVAL`             | 60 s                                | 30–300 s         |
| `T_MACROPROPOSE`                     | 4 s                                 | 2–8 s            |
| `T_SUBNET`                           | 2 s (= T_MACROPROPOSE / 2)          | 1–4 s            |
| `T_CANONICALIZE`                     | 8 s (= 2 × T_MACROPROPOSE)          | 4–16 s           |
| `SYNC_COMMITTEE_SIZE`                | 512                                 | 256–1024         |
| `SYNC_COMMITTEE_PERIOD`              | 1024 macro height                   | 256–4096         |
| `MIN_STAKE`                          | tham số hóa, suggest $50–100k equiv | —                |
| `MAX_STAKE_FRACTION`                 | 5%                                  | 1–10%            |
| **Bitcoin anchor (rev.3, §8.4)**     |                                     |                  |
| `BTC_CHECKPOINT_EPOCH_PERIOD`        | 1 LUA epoch (~30 min)               | 15–120 min       |
| `BTC_CONFIRMATIONS_FOR_FINAL`        | 6 blocks                            | 3–18             |
| `BTC_RELAY_REWARD`                   | 5% of macro proposer reward         | 1–10%            |
| `BTC_MIN_FEE_RATE`                   | 50 sat/vbyte (adaptive)             | dynamic          |
| **Anti-Sybil (rev.3, §9.7, §10.6)**  |                                     |                  |
| `IDENTITY_REATTEST_PERIOD`           | 4 epochs (~2 hours)                 | 1–24 epochs      |
| `REWARD_DECAY_RATE`                  | 1.0 (full decay at max concentration) | 0.5–2.0        |
| `CONCENTRATION_ALPHA` (ASN weight)   | 0.4                                 | 0.2–0.6          |
| `CONCENTRATION_BETA` (cloud weight)  | 0.4                                 | 0.2–0.6          |
| `CONCENTRATION_GAMMA` (voting weight)| 0.2                                 | 0.1–0.4          |
| `BOOTSTRAP_GRACE_PERIOD`             | 30 epochs (~15 days)                | 0–60 epochs      |
| `DKG_SLASH_BASE` (v2.1+)             | 20%                                 | 10–50%           |
| **Bridge integration (rev.3, §8.6)** |                                     |                  |
| `BRIDGE_VALUE_CAP`                   | 1000 × MIN_STAKE                    | bridge-customizable, lower allowed |


## 16. Phụ lục B — Glossary

- **Anchor**: vertex được chọn bởi VRF làm điểm commit của một wave.
- **Anti-Sybil concentration score** (rev.3): số trong $[0, 1]$ phản ánh mức độ một validator collocated với các validator khác về ASN, cloud:region, hoặc voting pattern. Cao = nhiều decay reward.
- **Bitcoin anchor / Sovereign Anchor (rev.3)**: hash của latest finalized MacroCheckpoint được commit vào Bitcoin Taproot tx; sau 6 BTC confirmations trở thành proof bất biến cho `epoch_finalized` state.
- **Blob**: payload bytes từ rollup, đơn vị DA.
- **Bridge value cap (rev.3)**: ngưỡng giá trị bridge withdrawal trên đó phải đợi `epoch_finalized` thay vì `finalized`.
- **Certified vertex**: vertex có 2f+1 chữ ký, dùng được làm parent.
- **Causal closure**: tập vertex có path tới một vertex cho trước trong DAG.
- **DKG fingerprint (rev.3)**: cryptographic evidence (qua Distributed Key Generation registry) cho thấy nhiều validator key có cùng origin seed → slashable.
- **Fast Execution Finality** (rev.3): finality state đạt sau 2-chain MacroQC rule (~5–10s); accountable safe trong window slashable.
- **Hard finality**: deprecated term — replaced bởi "Fast Execution Finality" hoặc "Sovereign Epoch Finality" (rev.3).
- **MacroQC**: aggregate signature 2/3 stake trên một MacroCheckpoint.
- **MicroQC**: aggregate signature 2/3 micro committee trên một MicroCheckpoint.
- **Mode 0 / A / B (rev.3)**: ba aggregation modes adaptive theo Active Set size (flat / subnet-based / leaderless gossip fallback). Xem §7.4.
- **Soft confirmation**: trạng thái sau MicroQC, có thể revert dưới điều kiện hiếm.
- **Sovereign Epoch Finality** (rev.3): finality state sau Bitcoin checkpoint 6-confirmation; bất khả bypass mà không re-mine BTC PoW.
- **Sync committee**: tập 512 ký-slot (sampled with replacement từ active validator set) ký headers cho light client trong 1 epoch. Một validator có thể nắm nhiều slot.
- **Validator identity (rev.3)**: declared `(asn, cloud_provider, region_code)` tuple per validator; input cho concentration_score.
- **Vigilante relayer (rev.3)**: validator role responsible cho batching MacroCheckpoint hash vào Bitcoin Taproot transaction.
- **Wave**: 4-round window cho một anchor commit attempt.
- **Weak subjectivity**: phải tin một checkpoint gần đây để sync trustworthy. Rev.3 reduces window từ 2 tuần xuống 1 tuần nhờ Bitcoin anchor.

## 17. Phụ lục C — Tham chiếu chính

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
4. Pedersen, "A Threshold Cryptosystem without a Trusted Party", EUROCRYPT 1991 (foundation cho DKG).
5. Komlo & Goldberg, "FROST: Flexible Round-Optimized Schnorr Threshold Signatures", SAC 2020.

### 17.9 External critique source (rev.3)

1. `docs/improve.pdf` — "Báo Cáo Đánh Giá Chuyên Sâu Về Kiến Trúc LUA-DAG v2: Phân Tích Mô Hình Cơ Sở Dữ Liệu Khả Dụng (DA), Cấu Trúc Phần Mềm Và Các Đề Xuất Tái Thiết Kế", 2026 — independent review identify 5 architectural mismatches của rev.2 và đề xuất 5 hướng tái thiết kế. Rev.3 implements all 5 recommendations.

### 17.10 Operational

1. Ethereum.org, "Consensus Mechanisms / Sync Committees / Weak Subjectivity", 2024–2026.

