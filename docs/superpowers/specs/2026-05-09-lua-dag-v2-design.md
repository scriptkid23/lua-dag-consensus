# LUA-DAG v2 — Modular DA + Accountable Finality on a DAG

> **Phiên bản**: v2 design draft
> **Ngày**: 2026-05-09
> **Trạng thái**: Spec đề xuất — chưa implement, chưa audit
> **Tiền nhiệm**: `docs/luadag.pdf` (LUA-DAG v1)
> **Mục đích**: Tái thiết kế LUA-DAG theo positioning "modular DA + finality layer" (Celestia-class), khắc phục các lỗ hổng lý thuyết và thiếu sót thiết kế đã được nhận diện trong đánh giá v1.

---

## 0. Tóm tắt điều hành

LUA-DAG v2 là một giao thức **đồng thuận và data availability** dành cho rollups và app-chains. Giao thức **không có execution layer**, **không cạnh tranh trực tiếp với L1 generic** như Sui/Aptos/Monad, mà cạnh tranh trong segment modular DA + shared finality, đối thủ chính là Celestia, Avail và EigenDA.

So với v1, v2 thực hiện sáu thay đổi cốt lõi:

1. **Loại bỏ execution và "block consensus" generic** — thu hẹp scope tới đúng hai sản phẩm: published-and-available data, và accountable hard-finality của một header chain.
2. **Thay quy tắc frontier "xác định" mơ hồ của v1 bằng Bullshark anchor commit rule** — đây là điểm sửa lý thuyết quan trọng nhất.
3. **Macro-finality dùng subnet BLS aggregation** — thay vì O(N) signature verify, root proposer chỉ aggregate 8 subnet signature; cho phép full-validator-set vote vẫn nhanh dưới 5s.
4. **Permissionless membership được spec hoàn chỉnh** — activation queue, withdrawal delay, churn limit, randomness beacon refresh; v1 chỉ nói "permissionless" mà không định nghĩa.
5. **Soft-confirm và hard-finality có hợp đồng API rõ ràng** — `accepted`, `soft_confirmed`, `finalized` là ba trạng thái user-visible khác nhau, nhằm ngăn rollup developer dùng nhầm soft như hard.
6. **Safety analysis xác suất cho committee được trình bày bằng số cụ thể** — không gloss-over.

Các trục KHÔNG đổi so với v1: PoS, partial synchrony, accountable safety dưới 1/3 Byzantine stake, VRF private sortition, accountable finality kiểu Casper FFG, light client với sync committee.

Performance target (kỳ vọng có cơ sở, chưa chứng minh thực nghiệm):

| Metric | Target | So sánh tham chiếu |
|---|---|---|
| DA throughput | 30–100 MB/s | Celestia ~6 MB/s, Avail ~2 MB/s (2024) |
| Soft-confirm latency p95 | 0.5–1.5s | Bullshark ~2s, Mysticeti ~600ms |
| Hard-finality latency p95 | 5–10s | Celestia ~12s, Ethereum ~15min |
| Validator HW yêu cầu | 16 vCPU / 64 GB / 2 TB NVMe / 500 Mbps | Tương đương Sui/Aptos validator |

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
- **NOT an MEV solution**: encrypted mempool, fair ordering, PBS — đều OUT OF SCOPE. Rollup tự xử lý.
- **NOT a light DA**: không có DA sampling cho light node ở v2 (defer v2.1). Light client phải tin sync committee về DA.
- **NOT a restaking AVS**: không borrow security từ Ethereum/BTC. Native PoS only.

### 1.3 Success metrics (KPI)

Một implementation v2 được coi là "successful" nếu trên adversarial testnet:

| KPI | Threshold pass |
|---|---|
| Hard-finality latency p95 | < 10s với 200 validator, WAN 5 region, 0 Byzantine |
| Hard-finality latency p95 | < 20s với 200 validator, 1/4 stake offline + 5% packet loss |
| DA throughput sustained | > 30 MB/s với 200 validator, blob size 64KB–1MB mix |
| Soft-confirm latency p95 | < 2s |
| State sync from weak-subjectivity checkpoint | < 30 phút trên home internet 100 Mbps |
| Light client header verify | < 5ms trên mobile (Snapdragon 8-class) |
| Storage growth rate | < 500 GB / validator / tháng tại 30 MB/s sustained |
| Slashable evidence detection | 100% cho equivocation; > 99% cho data unavailability trong test scenario |

KPI **không** phải cho v2: TPS application-level (vì không có execution), MEV resistance (vì out of scope), cross-rollup atomic latency (vì out of scope).

---

## 2. System model và threat model

### 2.1 Validator set và stake

Tập validator $\mathcal{V} = \{v_1, \dots, v_N\}$ với trọng số stake $w_i \ge w_{\min}$. Tổng stake hoạt động tại epoch $e$ là $W_e = \sum_{i \in \text{active}_e} w_i$.

Stake bị cap ở $w_{\max} = 0.05 \cdot W_e$ — không validator nào nắm quá 5% voting power. Vượt cap thì phần dư bị "burned to voting power 0" (vẫn cho stake nhưng không tăng vote weight); validator được khuyến khích split.

### 2.2 Mạng

Mô hình **partial synchrony** kinh điển (DLS 1988): tồn tại $\Delta$ chưa biết và $\text{GST}$ chưa biết, sao cho sau $\text{GST}$, mọi message giữa hai node đúng tới được trong $\Delta$. Trước GST kẻ tấn công kiểm soát hoàn toàn lịch trình.

Topology: gossip-based overlay với eager push cho metadata (vertex headers, votes) và pull-on-demand cho blob chunks. KHÔNG dùng deterministic relay topology (tránh single-point-of-failure như Solana turbine block leader).

### 2.3 Mô hình tin cậy

| Đối tượng | Giả định |
|---|---|
| Hash (SHA-256, BLAKE3) | Collision-resistant, second-preimage-resistant |
| Chữ ký (BLS12-381) | EUF-CMA secure |
| Aggregate signatures | Rogue-key attack đã được phòng bằng proof-of-possession |
| VRF (ECVRF Edwards25519) | Pseudo-random, unpredictable cho non-holder |
| Time | KHÔNG giả định clock đồng bộ; chỉ dùng local timeout tăng dần |

### 2.4 Threat model

| Thuộc tính | Mức |
|---|---|
| Byzantine stake tối đa cho safety | $f < W/3$ |
| Online honest stake tối thiểu cho liveness | $> 2W/3$ sau GST |
| Adaptive corruption | Cho phép — kẻ tấn công có thể chọn validator để corrupt sau khi nhìn thấy beacon, nhưng không nhanh hơn 1 epoch |
| Crash + Byzantine kết hợp | Tổng $\le f$ |
| Network partition trước GST | Cho phép arbitrary; safety vẫn giữ |
| Network partition sau GST | Không xảy ra theo định nghĩa |
| Long-range attack | Chống bằng weak subjectivity (mục 8) |
| Data withholding | Chống bằng certified vertex + retrieval challenge (mục 5) |

Adaptive corruption mạnh hơn so với BFT cổ điển và là lý do bắt buộc dùng VRF private sortition cho mọi vai trò leader/collector.

### 2.5 Cận lý thuyết tham chiếu

LUA-DAG v2 không vi phạm cận nào dưới đây:

- **FLP 1985**: không thể đạt termination xác định trong asynchronous với 1 fault. ⇒ V2 dùng partial synchrony, có termination *eventual* sau GST.
- **DLS 1988**: $f < N/3$ là tight bound cho partial synchrony BFT có signature. ⇒ V2 chọn baseline 1/3.
- **CAP**: dưới partition, v2 chọn **safety over liveness** (hard-finality stall thay vì fork).

---

## 3. Architecture overview

### 3.1 Ba lớp

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
        │  - VRF private sortition picks anchor each wave         │
        │  - anchor commit rule: 2f+1 next-round vertices link    │
        │  - committed sub-DAG → deterministic linearization      │
        │  Output: MicroQC at end of each wave                    │
        │                                                         │
        ├─────────────────────────────────────────────────────────┤
        │                                                         │
        │  Layer 3: MACRO-FINALITY (Casper FFG-class)             │
        │  - every W micro-slots, build MacroCheckpoint           │
        │  - all validators vote MACRO-VOTE                       │
        │  - subnet BLS aggregation (8 subnets)                   │
        │  - 2-chain finality rule                                │
        │  Output: MacroQC + finalized header                     │
        │                                                         │
        └────────────────────────────────────────────────────────┘
                                        │ finalized header + proof
                                        ▼
                         ┌─────────────────────────────────┐
                         │  Rollup / bridge / light client │
                         └─────────────────────────────────┘
```

### 3.2 Tại sao đúng 3 lớp

Mỗi lớp giải quyết đúng một bài toán mà **không lớp nào khác giải tốt**:

| Lớp | Bài toán | Tại sao không gộp được |
|---|---|---|
| Availability DAG | Reliable broadcast của large data | Gộp với ordering ⇒ leader trở thành bottleneck băng thông; bằng chứng Narwhal/Tusk |
| Micro-ordering | Linearization nhanh của causal DAG | Gộp với DA ⇒ data có thể "được nhắc tên" nhưng chưa available; gộp với macro ⇒ chậm |
| Macro-finality | Hard slashable settlement cho bridge/light client | Gộp với micro ⇒ committee nhỏ không đủ accountable; aggregate cost cao |

### 3.3 Boundary rõ ràng giữa lớp

Mỗi lớp expose **một interface duy nhất** cho lớp trên:

- L1 → L2: function `causal_set(round_cut)` trả về tập certified vertices ≤ round_cut.
- L2 → L3: function `micro_head()` trả về (slot, frontier_hash, ordered_blob_refs, micro_qc).
- L3 → consumer: function `latest_finalized()` trả về (height, header, macro_qc).

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
t+5.5s  MacroCheckpoint proposed; subnet aggregation
t+6.5s  MacroQC formed                                ← justified
...
t+12s   next MacroCheckpoint also justified           ← FINALIZED
```

---

## 4. Data structures

| Struct | Mục đích | Trường chính |
|---|---|---|
| `Blob` | Đơn vị payload từ rollup | `namespace_id`, `data` (bytes), `commitment` (Merkle root over chunks) |
| `Chunk` | Mảnh của blob sau erasure coding | `blob_id`, `index`, `data`, `proof` (Merkle proof against commitment) |
| `Vertex` | Đỉnh DAG | `round`, `author`, `parents` (≥ 2f+1 cert vertex hashes from r-1), `blob_refs` (list of commitments), `signature` |
| `CertifiedVertex` | Vertex + quorum sigs | `vertex`, `quorum_sigs` (2f+1 BLS aggregate) |
| `MicroCheckpoint` | Output của một wave | `slot`, `parent_macro`, `anchor_vertex`, `committed_sub_dag_root`, `ordered_blob_refs_root`, `micro_qc` |
| `MicroQC` | Aggregate vote on MicroCheckpoint | `slot`, `committee_bitmap`, `bls_aggregate` |
| `MacroCheckpoint` | Hard-finality unit | `height`, `parent_height_hash`, `micro_head_hash`, `da_root`, `validator_set_root`, `epoch`, `proposer_id` |
| `MacroQC` | Aggregate vote on MacroCheckpoint | `height`, `subnet_aggregates[8]`, `validator_bitmap`, `bls_aggregate` |
| `MacroHeader` | Light header for clients | `height`, `parent_hash`, `micro_head_hash`, `da_root`, `epoch`, `aggregate_sig`, `validator_bitmap_compressed` |
| `SyncCommitteeUpdate` | Cập nhật sync committee cho light | `epoch`, `next_committee_root`, `aggregate_sig` |
| `SlashEvidence` | Bằng chứng slashable | `kind`, `validator_id`, `evidence_a`, `evidence_b` (hai message conflict đã ký) |

Quan sát: **không có** struct nào tên "block" trong v2. Đây là cố ý — "block" là khái niệm quá tải khi gộp data + order + finality vào một struct.

---

## 5. Layer 1 — Availability DAG

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

- $k = \lceil |\text{blob}| / 32\,\text{KB} \rceil$ (chunk data size cố định).
- Commitment = Merkle root của $n$ chunk hashes.
- Khi gossip, validator phát chunks song song qua các peer. Theo Narwhal, mỗi validator gửi chỉ $1/N$ tổng bytes ⇒ load-balanced.

**Tại sao 1D thay vì 2D**: 2D Reed-Solomon (Celestia-style) cho phép DA sampling cho light node, nhưng tăng overhead 2-4x. V2 chọn 1D vì light client tin sync committee về DA (xem mục 8). 2D có thể được introduce ở v2.1 mà không thay đổi vertex schema.

### 5.4 Garbage collection

Vertex và chunks có lifecycle 3 trạng thái:

- **Hot**: round ≤ `current - GC_HOT_HORIZON` (default `GC_HOT_HORIZON = 200`). Phải giữ full bytes trên mọi validator.
- **Warm**: round nằm dưới latest finalized macro nhưng trên `GC_WARM_HORIZON` (default 10000 rounds). Validator phải giữ ít nhất commitment + 1 chunk; có thể tham gia retrieval challenge.
- **Cold**: round dưới `GC_WARM_HORIZON` so với finalized macro. Validator có thể GC hoàn toàn. Archive node (opt-in role) giữ vĩnh viễn.

### 5.5 Retrieval challenge

Bất kỳ rollup hoặc validator nào có thể issue retrieval challenge cho một `(blob_id, chunk_idx)`. Validator được hỏi phải trả lời trong $T_{\text{retrieve}} = 30$s, gửi chunk + Merkle proof. Nếu ≥ $f+1$ challenge **đồng thời** với cùng blob đều fail, blob bị mark `unavailable`; bằng chứng này dùng để slash ingress validator của blob đó (mục 11).

Quan trọng: challenge KHÔNG áp dụng cho cold blobs. Rollup phải tự đảm bảo họ pull blobs xuống warm/cold transition trước khi mất quyền challenge.

---

## 6. Layer 2 — Micro-ordering (Bullshark anchor commit)

Đây là **section quan trọng nhất** vì đây là chỗ v1 hand-waved. V2 dùng đúng commit rule của Bullshark thay vì khái niệm "frontier xác định" mơ hồ.

### 6.1 Wave structure

DAG được chia thành các wave dài 4 round mỗi wave (steady-state có thể commit trong 2 round qua shortcut path). Wave $w$ gồm round $4w, 4w+1, 4w+2, 4w+3$.

### 6.2 Anchor selection (VRF private sortition)

Tại đầu wave $w$, randomness beacon $R_w$ được derive:

$$R_w = H(R_{w-1} \,\|\, \text{MacroQC of latest finalized})$$

Mỗi validator $v_i$ tính:

$$y_i = \text{VRF}_i(R_w \,\|\, \text{"anchor"})$$

Anchor proposer của wave $w$ là validator có $y_i \cdot W / w_i$ nhỏ nhất trong wave (weighted). **Không ai biết anchor là ai cho đến khi anchor publish vertex của mình ở round $4w$ — đây là điểm key chống adaptive DoS.**

Khi anchor vertex được certified, hắn reveal VRF proof. Mọi validator verify proof ⇒ xác nhận anchor đúng.

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
- API expose state này dưới flag `soft_confirmed = true, finalized = false`.
- Soft confirmation **có thể bị revert** nếu macro layer chọn một micro chain khác (xem mục 7.5). Rollup KHÔNG nên dùng soft cho settlement-grade decision.

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

Một backup proposer được chọn ranked thứ 2; nếu primary không publish trong $T_{\text{macro\_propose}} = 4$s, backup tiếp quản.

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

### 7.4 Subnet BLS aggregation

Đây là kỹ thuật scale macro vote — KEY innovation so với v1.

Validator set được phân chia thành **8 subnet** (deterministic theo `validator_id mod 8`). Mỗi subnet có ~25–60 validator (cho N=200–500).

Quy trình vote:

1. Mọi validator nhận MacroProposal, verify, ký hash.
2. **Trong mỗi subnet**, một subnet aggregator (rotated by VRF) thu chữ ký. Khi đủ 2/3 subnet stake, aggregator gọi `bls_aggregate()` thành `subnet_aggregate[k]`.
3. **Macro proposer** thu 8 subnet aggregates. Tổng stake ≥ 2/3 W ⇒ proposer aggregate lần 2 thành `MacroQC`.

Cost analysis:

- Per-validator: 1 sign + 1 broadcast → O(1) crypto ops.
- Per-subnet aggregator: aggregate ~30 sigs → ~3ms với BLS12-381.
- Macro proposer: aggregate 8 sigs → < 1ms.
- Verification của MacroQC bởi light client: 1 pairing check + bitmap parsing → ~3ms.

So với naive O(N) aggregation tại proposer, scheme này **cắt latency từ ~100ms (cho N=500) xuống ~5ms**. Đây là điểm quyết định cho phép full-validator-set vote vẫn fit trong 5–10s hard-finality target.

### 7.5 2-chain finality rule

Theo Casper FFG (Buterin & Griffith 2017), được port qua macro-checkpoint chain:

- MacroCheckpoint $C_h$ là **justified** nếu có MacroQC.
- $C_h$ là **finalized** nếu $C_h$ justified VÀ có $C_{h+1}$ justified với `parent_height_hash(C_{h+1}) = hash(C_h)`.

Vì vậy hard-finality cần **ít nhất 2 macro checkpoint liên tiếp**, tổng latency = 2 × macro window = 16s worst case, ~10s typical.

### 7.6 Slashing conditions cho macro layer

Slashable evidence (cryptographically provable):

| Vi phạm | Definition | Slash % |
|---|---|---|
| Macro double-vote | Ký hai `MacroProposal` ở cùng height $h$ với parent khác nhau | 100% |
| Surrounding vote | Ký vote cho $C_h$ và sau đó ký vote cho $C_{h'}$ với $h' < h$ và $C_{h'}$ không là ancestor của $C_h$ | 100% |
| Double-propose | Cùng validator publish 2 MacroProposal khác nhau ở cùng height | 100% |
| Micro double-vote | Ký 2 MicroQC khác nhau ở cùng slot | 50% |
| Data unavailability proven | Validator ký certified vertex, sau đó fail $f+1$ retrieval challenges | 5% per occurrence, soft-cap 50%/year |

Slash 100% có nghĩa là toàn bộ stake bị burn + validator bị tejected. Slash partial cho data unavailability vì có thể là transient bug, không nhất thiết Byzantine.

### 7.7 Inactivity leak

Nếu chain không finalize trong `INACTIVITY_LEAK_THRESHOLD = 4` macro windows (~32s), giao thức vào chế độ inactivity leak: stake của validator offline hoặc vote sai sẽ bị giảm dần ($-0.5\% / \text{macro window}$ liên tục) cho tới khi tỷ lệ online honest stake quay về > 2/3.

Đây là cùng cơ chế Ethereum dùng từ Beacon Chain. Đảm bảo recovery sau partition ngay cả khi 30%+ validator vĩnh viễn offline.

---

## 8. Light client và checkpoint sync

### 8.1 Sync committee

Mỗi epoch (default 1024 macro height ~ vài giờ), giao thức chọn `SYNC_COMMITTEE_SIZE = 512` validator (weighted random) làm sync committee. Sync committee ký mọi MacroHeader trong epoch đó.

Light client chỉ cần:

- 1 trusted weak-subjectivity checkpoint (trust on first use).
- Stream MacroHeader + sync committee aggregate signature.

Verification per header: 1 pairing + bitmap check ⇒ < 5ms trên mobile.

### 8.2 Sync committee transition

Cuối mỗi epoch, SyncCommitteeUpdate được embed vào MacroCheckpoint của height đầu epoch mới. Light client xác minh transition bằng signature của committee CŨ trên committee MỚI.

Lưu ý: sync committee KHÔNG có quyền finality. Họ chỉ là "gateway" cho light client. Một sync committee bị corrupt vẫn không thể fork chain — chỉ có thể cung cấp invalid header cho light client (light client phát hiện được nếu cross-check với full node).

### 8.3 Weak subjectivity policy

`WEAK_SUBJECTIVITY_PERIOD = 2 weeks`. Node mới sync phải có một checkpoint không quá 2 tuần tuổi từ một nguồn tin cậy (foundation, friend, official explorer). Lý do: sau 2 tuần, validator có thể withdraw stake → không còn slashable → có thể tạo long-range fork hợp lệ trên giấy.

`WITHDRAWAL_DELAY = WEAK_SUBJECTIVITY_PERIOD + 1 day`. Sau khi validator request exit, stake bị lock thêm 2 tuần + buffer → đảm bảo slashing window đủ rộng.

### 8.4 Checkpoint sync flow

```
1. New node fetches WS checkpoint (from K independent sources, K ≥ 3)
2. Verify checkpoint signatures match validator set published at WS time
3. Download MacroHeaders + sync committee sigs from checkpoint to head
4. Optionally: download state snapshot at latest finalized height
5. Begin live participation
```

State snapshot (mục 4 trên) là **out of scope cho rollup snapshot** — rollup tự lo snapshot của state mình. LUA-DAG chỉ cung cấp DA root + macro header chain.

---

## 9. Permissionless membership

V1 dùng từ "permissionless" mà không định nghĩa cách join/leave. V2 spec hoàn chỉnh.

### 9.1 Activation queue

Validator deposit stake → enter activation queue. Activation rate giới hạn `MAX_ACTIVATION_PER_EPOCH = 4` validator/epoch để:

- Tránh sudden validator set jump → vỡ assumption stake-weight stable.
- Đảm bảo sync committee có thời gian transition.

Trong queue, deposit không tạo voting power, không nhận reward.

### 9.2 Withdrawal

Validator request exit → enter exit queue (`MAX_EXIT_PER_EPOCH = 4`). Sau khi exit accept, stake vẫn lock thêm `WITHDRAWAL_DELAY` để có thể slash retroactively.

### 9.3 Churn limit

Tổng (activation + exit) rate được cap cứng để giữ validator set evolution slow enough cho:

- Sync committee transition smooth.
- VRF beacon predictability không bị biến động lớn.
- Subnet rebalancing ổn định.

### 9.4 Randomness beacon refresh

Beacon $R$ được tạo từ MacroQC trước. Vì MacroQC là output của 2/3 stake aggregate, kẻ tấn công không thể bias beacon trừ khi kiểm soát > 1/3 stake (tại đó hắn có vấn đề lớn hơn).

**Grinding resistance**: macro proposer về kỹ thuật vẫn có thể chọn không include một số partial signature (ví dụ để bias beacon hơi nhỏ), nhưng bị hạn chế bằng hai cơ chế:

- **Inclusion-delay reward**: validator có vote include muộn hơn được giảm reward exponential. Proposer cố tình bỏ vote sẽ bị các validator đó tố cáo và proposer mất reward macro propose.
- **Subnet aggregation guard**: subnet aggregator đã pre-aggregate và publish bằng chứng "tôi có 2/3 stake của subnet" trước khi gửi cho proposer. Proposer không thể "ẩn" cả subnet aggregate; chỉ có thể chọn không include subnet, nhưng sẽ làm fail MacroQC nếu thiếu > 1/3 stake.

Kết hợp lại, biên grinding bị cap ở vài bit entropy mỗi epoch — không đủ để bias VRF anchor selection theo cách meaningful trong steady state. Đây là cùng pattern Ethereum dùng cho RANDAO grinding.

### 9.5 Validator set root publishing

Mỗi MacroCheckpoint chứa `validator_set_root` — Merkle root của (validator_id, pubkey, weight) tuple. Light client dùng root này để verify subnet membership và sync committee membership.

### 9.6 Slashing để thoát khỏi validator set

Nếu validator bị slash 100%, hắn auto-exit. Stake bị burn (không quay lại tay attacker dù qua delegation).

---

## 10. Incentive và accountability

### 10.1 Reward decomposition

Mỗi epoch, reward pool được phân chia:

| Loại reward | % | Điều kiện |
|---|---|---|
| Base reward | 30% | Validator online, chữ ký xuất hiện trong ≥ 80% certified vertices của epoch |
| Vertex authoring | 15% | Vertex của validator được certify đúng hạn |
| Anchor proposing | 10% | Validator được chọn anchor và anchor commit thành công |
| Micro committee | 5% | Validator được chọn vào committee và vote MicroQC |
| Macro voting | 30% | Validator vote MacroQC trong ≥ 95% macro height |
| Macro proposing | 5% | Validator propose macro và checkpoint được justified |
| Subnet aggregation | 5% | Validator làm subnet aggregator và submit subnet aggregate đúng hạn |

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

| C (committee size) | β = 0.20 | β = 0.30 | β = 0.33 |
|---|---|---|---|
| 64 | 1.5 × 10⁻³ | 0.18 | 0.42 |
| 128 | 2.7 × 10⁻⁵ | 0.06 | 0.40 |
| 256 | 1.0 × 10⁻⁸ | 0.005 | 0.37 |
| 512 | 1.5 × 10⁻¹⁵ | 4 × 10⁻⁵ | 0.34 |
| 1024 | < 10⁻³⁰ | 3 × 10⁻⁹ | 0.30 |

Quan sát quan trọng: ở β gần 1/3 (ngưỡng global safety), committee capture probability vẫn cao **với mọi committee size hữu hạn**. Đây là lý do **micro layer KHÔNG được quảng bá là hard finality** — soft confirmation chỉ an toàn khi β đáng kể nhỏ hơn 1/3.

V2 chọn `C_micro = 256` cho default. Với β = 0.2, capture probability ~10⁻⁸ — chấp nhận được cho UX-level confirmation. Với β > 0.3, soft confirmation mất ý nghĩa và rollup nên switch sang chỉ trust hard-finality.

### 11.4 Bảng tấn công và đối sách

| Tấn công | Hậu quả | Đối sách v2 |
|---|---|---|
| Long-range PoS fork | Lừa node mới sync vào chain giả | Weak subjectivity 2 tuần + multi-source pinning |
| Data withholding | Soft-confirm blob nhưng data không lấy được | Certified vertex chỉ accept blob có chunks verified; retrieval challenge với slashing |
| Anchor DoS | Mất commit shortcut, fall back slow path | VRF private sortition, slow path 4 round, backup propagation |
| Macro proposer DoS | Mất 1 macro window | Backup proposer ranked thứ 2, timeout 4s |
| Equivocation | Tạo conflict checkpoint | Slashable 100%, 2-chain rule đảm bảo accountable safety |
| Subnet capture (k subnets bị > 2/3 Byzantine) | Subnet aggregate sai → corrupted MacroQC | Macro proposer verify 8 subnet aggregates riêng; nếu 1 subnet sai, vẫn có thể pass nếu 7 subnet đúng tổng > 2/3 stake (resilient) |
| MEV ordering manipulation | Anchor reorder để extract MEV | OUT OF SCOPE — rollup tự xử lý |
| Storage spam | Đẩy bytes vô nghĩa qua DA | Fee market + min fee floor + max blob size cap per slot |
| Sync committee corruption | Light client bị fed invalid header | Sync committee không có quyền finality; full node cross-check phát hiện |
| VRF grinding | Bias anchor selection | Beacon từ MacroQC nên grinding cần > 1/3 stake; deterministic aggregation rule |
| Eclipse attack on light client | Light client bị isolated | Multi-source header sources khuyến nghị; defense thuộc deployment |
| Adaptive corruption sau VRF reveal | Chỉ corrupt anchor sau khi anchor đã expose | Round duration < adaptive corruption time (~giờ) ⇒ irrelevant |
| Post-quantum attack | BLS broken by future quantum | Roadmap migration sang lattice-based aggregate sigs ở v3 |

### 11.5 Cross-layer attack: micro flush after macro stall

**Scenario**: Macro layer stall vì 35% stake offline. Inactivity leak chạy. Trong lúc đó micro layer vẫn tiếp tục commit (nhanh, chỉ cần committee). Rollup soft-confirm hàng nghìn blob. Khi macro resume, có thể macro chain chọn finalize một micro chain CŨ HƠN, revert toàn bộ soft-confirmed blob mới.

**Đối sách**: Micro layer phải tôn trọng `lock_macro` — anchor ở wave $w$ chỉ được build trên DAG có ancestor là `lock_macro = latest_justified_macro_checkpoint`. Khi macro stall, lock_macro không advance ⇒ anchor có thể tiếp tục commit nhưng phải chỉ đến cùng macro parent. Khi macro resume, MacroProposal sẽ include `micro_head` từ DAG này — không có revert.

Edge case duy nhất: nếu macro fork (hai macro chain conflict, cả hai justified) → conflict, một bên phải được dismiss bằng slashing (mục 11.1). Trong lúc resolve, tất cả soft-confirm trên cả hai bên đều không reliable.

### 11.6 Honest minority resilience

Nếu honest stake = 51%, Byzantine = 49%:

- Liveness vẫn giữ nếu Byzantine không actively prevent (chỉ withhold vote).
- Safety vẫn giữ — Byzantine < 2/3 không thể tạo MacroQC.
- Soft confirmation gần như mất hoàn toàn (committee capture > 50%).

Nếu Byzantine vượt 1/3:

- Safety break — có thể có hai MacroQC conflict nhưng SẼ generate slashable evidence.
- Đây là failure mode "accountable" — chain halt, slash, social recovery.

Nếu Byzantine vượt 1/2:

- Censorship attack possible: Byzantine có thể block honest blob khỏi DAG.
- Liveness broken cho honest workload.
- Safety vẫn cần slashing để break (kẻ tấn công vẫn lose stake).

---

## 12. Performance plan và expected numbers

### 12.1 Benchmark matrix

| Tham số | Giá trị quét |
|---|---|
| Validator count | 50, 100, 200, 350, 500 |
| Network topology | Single DC, 5-region WAN, 10-region WAN |
| Round duration | 100ms, 250ms, 500ms |
| Wave length | 4 (Bullshark default), 6, 8 |
| Macro window W | 4, 8, 16 micro-slots |
| Micro committee size | 128, 256, 512 |
| Subnet count | 4, 8, 16 |
| Blob size mix | 4KB-only, 64KB-only, mixed (4KB-1MB) |
| Erasure rate | 1/2, 1/4 |
| Byzantine fraction | 0%, 10%, 20%, 30% |
| Failure scenarios | 0 fault; 10% offline; 20% offline; partition 30s; data withholding 5%; equivocation 5% |

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

| Phase | Milestone | Duration | Headcount |
|---|---|---|---|
| P0: Spec finalize | Whitepaper hoàn chỉnh + TLA+ skeleton | 2 tháng | 2 protocol + 1 formal |
| P1: Prototype L1 (Availability DAG) | Vertex + cert + erasure + GC | 3 tháng | 3 distsys |
| P2: Prototype L2 (Bullshark) | Anchor commit, fast/slow path, MicroQC | 3 tháng | 2 protocol + 1 perf |
| P3: Prototype L3 (Macro) | Subnet aggregation, 2-chain finality, slashing | 3 tháng | 2 protocol + 1 crypto |
| P4: Light client + state sync | SDK + checkpoint sync | 2 tháng | 2 client |
| P5: Permissionless membership | Activation/exit queue, beacon | 2 tháng | 1 protocol + 1 testing |
| P6: Adversarial testnet | Fault injection, partition recovery, public report | 3 tháng | toàn đội |
| P7: External audit + hardening | 2 audits, fuzzing, chaos | 3 tháng | external + 1 internal |
| **Tổng tới prototype-mainnet-ready** | | **~21 tháng (parallel)** | **~10–12 nòng cốt** |

Đây là honest estimate cho **prototype** đủ để bắt đầu testnet công khai. **Production-grade** với ecosystem (rollup integrations, SDK, multi-client) cần thêm 12–24 tháng nữa và đội lớn hơn (20–40 người).

### 13.2 Risk register

| Risk | Severity | Mitigation |
|---|---|---|
| Cross-layer interaction bug | High | TLA+ model check ngay từ P0; integration test với chaos suite |
| Subnet aggregation correctness | High | Crypto audit ưu tiên; fallback to flat aggregation |
| Liveness under partial outage | High | Inactivity leak; anchored slow path; backup proposer |
| Performance không đạt target | Medium | Benchmark sớm ở P2; có plan B (smaller committee, larger wave) |
| Storage growth out of control | Medium | GC policy được test ở adversarial testnet; archive node tách riêng |
| Sync committee corruption | Medium | Cross-validation trong client; multi-source headers |
| Token launch / regulatory | Medium | Out of scope cho doc này; cần legal review riêng |
| Adoption: rollup nào dùng? | High | Bootstrap với 1–2 design partner trước launch |
| Cạnh tranh Celestia ecosystem effect | High | Differentiation phải rõ: faster finality, accountable slashing, lower validator HW |
| Quantum readiness | Low (long-term) | Migration plan trong v3 |

### 13.3 Open questions cần resolve trước khi viết implementation plan

- [ ] Chọn cụ thể cipher suite cho VRF: ECVRF Edwards25519 (Algorand) vs RSA-FDH-VRF? Default khuyến nghị: ECVRF.
- [ ] Reed-Solomon library: tự build hay dùng existing (`reed-solomon-erasure`, RaptorQ)? Default: `reed-solomon-erasure` cho v1, RaptorQ nếu cần fountain code.
- [ ] Implementation language: Rust (default, Sui/Solana ecosystem), Go (Cosmos ecosystem), Zig (experimental). Default: Rust.
- [ ] State machine cho validator role: actor model (Tokio) vs explicit FSM. Default: actor model.
- [ ] Light client SDK targets: TypeScript (web), Rust (native), Swift/Kotlin (mobile)? Default: TypeScript first.
- [ ] Tokenomics: defer cho doc riêng. Min stake, inflation rate, fee burn rate cần economics modeling.
- [ ] Governance: on-chain (delegated voting on macro layer) hay off-chain (foundation initial). Default: off-chain v1, on-chain v2.
- [ ] Bridge integration: làm reference bridge cho Ethereum trước hay defer? Default: defer.
- [ ] Multi-client strategy: bootstrap với 1 client, đến sau audit thứ 2 mới fund client thứ 2. Default: confirmed.

### 13.4 Deferred to v2.1+ (KHÔNG trong scope v2)

- DA sampling (2D Reed-Solomon + KZG) cho light DA verification.
- Restaking integration (EigenLayer / Babylon).
- Shared sequencing layer (cross-rollup atomic).
- ZK header proof cho bridge optimization.
- Encrypted mempool / threshold encryption / fair ordering.
- State rent / storage pricing.
- Cross-chain message passing (IBC-style).
- Validator reputation / weighted slashing.
- Post-quantum signature migration.

---

## 14. So sánh head-to-head: LUA-DAG v1 vs v2

| Khía cạnh | v1 | v2 | Cải thiện |
|---|---|---|---|
| Scope | Generic L1 với execution placeholder | DA + finality only, no execution | Thu nhỏ scope ⇒ ship-able |
| Frontier rule | "Xác định" mơ hồ | Bullshark anchor commit, đã proven | Fix lỗ hổng lý thuyết quan trọng nhất |
| Macro voting | Full validator set, naive aggregation | Full validator set + subnet BLS | Scale tới 500 validator vẫn < 5s |
| Permissionless detail | Một dòng nói "permissionless" | Full spec activation/exit/churn | Implementable thực sự |
| Soft vs hard contract | Note rủi ro nhưng không API | API explicit `accepted/soft/finalized` | Ngăn rollup dùng nhầm |
| MEV resistance | Một dòng "aged-inclusion fee" | Out of scope rõ ràng, rollup tự xử lý | Honest về scope |
| Committee safety | Định tính | Bảng xác suất số cụ thể | Có thể defend trước reviewer |
| Cross-layer recovery | Không spec | Mục 11.5 spec rõ | Tránh micro-flush bug |
| Cạnh tranh thị trường | "Compete với mọi L1" | "Compete trong DA segment" | Có shot ở thị trường không bị chiếm |
| Effort estimate | 6–8 người × 12–18 tháng | 10–12 người × ~21 tháng tới testnet | Honest hơn |

---

## 15. Phụ lục A — Bảng parameter tham chiếu

| Tham số | Default | Range cho tuning |
|---|---|---|
| `ROUND_DURATION` | 250 ms | 100–500 ms |
| `WAVE_LENGTH` | 4 rounds | 2–8 rounds |
| `MACRO_WINDOW_W` | 8 micro-slots | 4–16 |
| `MICRO_COMMITTEE_SIZE` | 256 | 128–1024 |
| `SUBNET_COUNT` | 8 | 4–16 |
| `MAX_VERTEX_PAYLOAD` | 1 MB | 256 KB – 4 MB |
| `MAX_BLOB_SIZE` | 8 MB | 1 MB – 32 MB |
| `ERASURE_RATE` | 1/2 | 1/2 – 1/4 |
| `GC_HOT_HORIZON` | 200 rounds | 100–500 |
| `GC_WARM_HORIZON` | 10 000 rounds | 5 000–50 000 |
| `MAX_ACTIVATION_PER_EPOCH` | 4 | 2–8 |
| `MAX_EXIT_PER_EPOCH` | 4 | 2–8 |
| `WEAK_SUBJECTIVITY_PERIOD` | 2 weeks | 1–4 weeks |
| `WITHDRAWAL_DELAY` | 2 weeks + 1 day | matches WS |
| `INACTIVITY_LEAK_THRESHOLD` | 4 macro windows | 2–16 |
| `INACTIVITY_LEAK_RATE` | 0.5% / window | 0.1–1% |
| `EQUIVOCATION_SLASH` | 100% | fixed |
| `DOUBLE_VOTE_SLASH` | 100% | fixed |
| `DATA_UNAVAILABILITY_SLASH` | 5% per occurrence | 1–10% |
| `SYNC_COMMITTEE_SIZE` | 512 | 256–1024 |
| `SYNC_COMMITTEE_PERIOD` | 1024 macro height | 256–4096 |
| `MIN_STAKE` | tham số hóa, suggest $50–100k equiv | — |
| `MAX_STAKE_FRACTION` | 5% | 1–10% |

## 16. Phụ lục B — Glossary

- **Anchor**: vertex được chọn bởi VRF làm điểm commit của một wave.
- **Blob**: payload bytes từ rollup, đơn vị DA.
- **Certified vertex**: vertex có 2f+1 chữ ký, dùng được làm parent.
- **Causal closure**: tập vertex có path tới một vertex cho trước trong DAG.
- **Hard finality**: trạng thái không thể revert trừ khi có slashable evidence cho ≥ W/3 stake.
- **MacroQC**: aggregate signature 2/3 stake trên một MacroCheckpoint.
- **MicroQC**: aggregate signature 2/3 micro committee trên một MicroCheckpoint.
- **Soft confirmation**: trạng thái sau MicroQC, có thể revert dưới điều kiện hiếm.
- **Sync committee**: tập 512 validator ký headers cho light client trong 1 epoch.
- **Wave**: 4-round window cho một anchor commit attempt.
- **Weak subjectivity**: phải tin một checkpoint gần đây để sync trustworthy.

## 17. Phụ lục C — Tham chiếu chính

1. Castro & Liskov, "Practical Byzantine Fault Tolerance", OSDI 1999.
2. Yin et al., "HotStuff: BFT Consensus in the Lens of Blockchain", PODC 2019.
3. Spiegelman et al., "Bullshark: DAG BFT Protocols Made Practical", CCS 2022.
4. Danezis et al., "Narwhal and Tusk: A DAG-based Mempool and Efficient BFT Consensus", EuroSys 2022.
5. Buterin & Griffith, "Casper the Friendly Finality Gadget", arXiv 1710.09437, 2017.
6. David et al., "Ouroboros Praos: An Adaptively-Secure Semi-Synchronous PoS Blockchain", EUROCRYPT 2018.
7. Chen et al., "Algorand: Scaling Byzantine Agreements for Cryptocurrencies", SOSP 2017.
8. Gilad et al., "Algorand", SOSP 2017.
9. Mysten Labs, "Mysticeti: Reaching the Limits of Latency with Uncertified DAGs", 2024.
10. Aptos Labs, "Shoal: Reducing Tail Latency in DAG-based Consensus", 2023.
11. Celestia documentation, "Data Availability Sampling", 2024.
12. Dwork, Lynch, Stockmeyer, "Consensus in the Presence of Partial Synchrony", JACM 1988.
13. Fischer, Lynch, Paterson, "Impossibility of Distributed Consensus with One Faulty Process", JACM 1985.
14. Ethereum.org, "Consensus Mechanisms / Sync Committees / Weak Subjectivity", 2024–2026.
