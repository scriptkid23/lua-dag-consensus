# LUA-DAG v2 — Modular DA + Accountable Finality on a DAG

> **Phiên bản**: v2 design draft (rev. 2 sau literature review)
> **Ngày**: 2026-05-09
> **Trạng thái**: Spec đề xuất — chưa implement, chưa audit
> **Tiền nhiệm**: `docs/luadag.pdf` (LUA-DAG v1)
> **Mục đích**: Tái thiết kế LUA-DAG theo positioning "modular DA + finality layer" (Celestia-class), khắc phục các lỗ hổng lý thuyết và thiếu sót thiết kế đã được nhận diện trong đánh giá v1.
> **Rev. 2 changelog**: cập nhật theo literature review (Consensus MCP, 70 papers): chuyển default VRF sang iVRF (post-quantum, unbiasable), bổ sung tùy chọn uncertified DAG kiểu Mysticeti, two-mode subnet aggregation (leader + leaderless fallback), tùy chọn Bitcoin checkpoint kiểu Babylon, vocabulary alignment với "ebb-and-flow / 3-slot finality", differentiation vs Acki Nacki, advance PQ migration từ v3 lên v2.1.

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


| Metric                    | Target                                 | So sánh tham chiếu                     |
| ------------------------- | -------------------------------------- | -------------------------------------- |
| DA throughput             | 30–100 MB/s                            | Celestia ~6 MB/s, Avail ~2 MB/s (2024) |
| Soft-confirm latency p95  | 0.5–1.5s                               | Bullshark ~2s, Mysticeti ~600ms        |
| Hard-finality latency p95 | 5–10s                                  | Celestia ~12s, Ethereum ~15min         |
| Validator HW yêu cầu      | 16 vCPU / 64 GB / 2 TB NVMe / 500 Mbps | Tương đương Sui/Aptos validator        |


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
- **NOT a light DA**: không có DA sampling cho light node ở v2 (defer v2.1). Light client phải tin sync committee về DA.
- **NOT a restaking AVS**: safety và liveness primary đều dựa hoàn toàn trên native PoS stake — không borrow trust live cho consensus từ Ethereum/BTC. Rev.2 cho phép **optional Bitcoin checkpoint** (Babylon-style) chỉ với mục đích **giảm long-range attack window** (rút `WITHDRAWAL_DELAY`); Bitcoin KHÔNG vote, KHÔNG ký block, KHÔNG tham gia path safety/liveness của các MacroQC bình thường.

### 1.3 Success metrics (KPI)

Một implementation v2 được coi là "successful" nếu trên adversarial testnet:


| KPI                                          | Threshold pass                                                           |
| -------------------------------------------- | ------------------------------------------------------------------------ |
| Hard-finality latency p95                    | < 10s với 200 validator, WAN 5 region, 0 Byzantine                       |
| Hard-finality latency p95                    | < 20s với 200 validator, 1/4 stake offline + 5% packet loss              |
| DA throughput sustained                      | > 30 MB/s với 200 validator, blob size 64KB–1MB mix                      |
| Soft-confirm latency p95                     | < 2s                                                                     |
| State sync from weak-subjectivity checkpoint | < 30 phút trên home internet 100 Mbps                                    |
| Light client header verify                   | < 5ms trên mobile (Snapdragon 8-class)                                   |
| Storage growth rate                          | < 500 GB / validator / tháng tại 30 MB/s sustained                       |
| Slashable evidence detection                 | 100% cho equivocation; > 99% cho data unavailability trong test scenario |


KPI **không** phải cho v2: TPS application-level (vì không có execution), MEV resistance (mặc định OFF; optional fairness mode được spec nhưng không là KPI cho v2 — xem §1.2), cross-rollup atomic latency (out of scope).

---

## 2. System model và threat model

### 2.1 Validator set và stake

Tập validator $\mathcal{V} = v_1, \dots, v_N$ với trọng số stake $w_i \ge w_{\min}$. Tổng stake hoạt động tại epoch $e$ là $W_e = \sum_{i \in \text{active}_e} w_i$.

Stake bị cap ở $w_{\max} = 0.05 \cdot W_e$ — không validator nào nắm quá 5% voting power. Vượt cap thì phần dư bị "burned to voting power 0" (vẫn cho stake nhưng không tăng vote weight); validator được khuyến khích split.

### 2.2 Mạng

Mô hình **partial synchrony** kinh điển (DLS 1988): tồn tại $\Delta$ chưa biết và $\text{GST}$ chưa biết, sao cho sau $\text{GST}$, mọi message giữa hai node đúng tới được trong $\Delta$. Trước GST kẻ tấn công kiểm soát hoàn toàn lịch trình.

Topology: gossip-based overlay với eager push cho metadata (vertex headers, votes) và pull-on-demand cho blob chunks. KHÔNG dùng deterministic relay topology (tránh single-point-of-failure như Solana turbine block leader).

### 2.3 Mô hình tin cậy


| Đối tượng                | Giả định                                                      |
| ------------------------ | ------------------------------------------------------------- |
| Hash (SHA-256, BLAKE3)   | Collision-resistant, second-preimage-resistant                |
| Chữ ký (BLS12-381)       | EUF-CMA secure                                                |
| Aggregate signatures     | Rogue-key attack đã được phòng bằng proof-of-possession       |
| VRF (iVRF default, ECVRF fallback) | Pseudo-random, unpredictable cho non-holder; iVRF cung cấp unbiasability + post-quantum (Esgin 2023) |
| Time                     | KHÔNG giả định clock đồng bộ; chỉ dùng local timeout tăng dần |


### 2.4 Threat model


| Thuộc tính                                 | Mức                                                                                                             |
| ------------------------------------------ | --------------------------------------------------------------------------------------------------------------- |
| Byzantine stake tối đa cho safety          | $f < W/3$                                                                                                       |
| Online honest stake tối thiểu cho liveness | $> 2W/3$ sau GST                                                                                                |
| Adaptive corruption                        | Cho phép — kẻ tấn công có thể chọn validator để corrupt sau khi nhìn thấy beacon, nhưng không nhanh hơn 1 epoch |
| Crash + Byzantine kết hợp                  | Tổng $\le f$                                                                                                    |
| Network partition trước GST                | Cho phép arbitrary; safety vẫn giữ                                                                              |
| Network partition sau GST                  | Không xảy ra theo định nghĩa                                                                                    |
| Long-range attack                          | Chống bằng weak subjectivity (mục 8)                                                                            |
| Data withholding                           | Chống bằng certified vertex + retrieval challenge (mục 5)                                                       |


Adaptive corruption mạnh hơn so với BFT cổ điển và là lý do bắt buộc dùng VRF private sortition cho mọi vai trò leader/collector.

### 2.5 Cận lý thuyết tham chiếu

LUA-DAG v2 không vi phạm cận nào dưới đây:

- **FLP 1985**: không thể đạt termination xác định trong asynchronous với 1 fault. ⇒ V2 dùng partial synchrony, có termination *eventual* sau GST.
- **DLS 1988**: $f < N/3$ là tight bound cho partial synchrony BFT có signature. ⇒ V2 chọn baseline 1/3.
- **CAP**: dưới partition, v2 chọn **safety over liveness** (hard-finality stall thay vì fork).

---

## 3. Architecture overview

### 3.0 Vị trí trong taxonomy

LUA-DAG v2 thuộc lớp **ebb-and-flow protocols** (Neu et al. SP 2021): kết hợp một synchronous dynamically-available protocol (DAG availability + micro-ordering — luôn live ngay cả khi participation dao động) với một partially-synchronous finality gadget (macro layer — đảm bảo accountable hard finality). Đây là cùng pattern với Ethereum 3-slot finality (3SF, D'Amato 2024) và RLMD-GHOST + finality gadget. Đặt LUA-DAG vào taxonomy này giúp reviewer dễ verify safety/liveness bằng cách reuse các bổ đề ebb-and-flow đã có.

Đáng lưu ý, kiến trúc 2-step + separation of execution-verification và block-propagation-attestation cũng có điểm tương đồng với **Acki Nacki** (Goroshevsky 2024). Khác biệt chính của LUA-DAG: (a) DAG availability layer riêng để load-balance bandwidth; (b) accountable safety theo Casper FFG 2-chain rule chứ không phải probabilistic; (c) macro-finality bằng full validator set chứ không phải random committee per block.

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


| Lớp              | Bài toán                                          | Tại sao không gộp được                                                              |
| ---------------- | ------------------------------------------------- | ----------------------------------------------------------------------------------- |
| Availability DAG | Reliable broadcast của large data                 | Gộp với ordering ⇒ leader trở thành bottleneck băng thông; bằng chứng Narwhal/Tusk  |
| Micro-ordering   | Linearization nhanh của causal DAG                | Gộp với DA ⇒ data có thể "được nhắc tên" nhưng chưa available; gộp với macro ⇒ chậm |
| Macro-finality   | Hard slashable settlement cho bridge/light client | Gộp với micro ⇒ committee nhỏ không đủ accountable; aggregate cost cao              |


### 3.3 Boundary rõ ràng giữa lớp

Mỗi lớp expose **một interface duy nhất** cho lớp trên:

- L1 → L2: function `causal_set(round_cut)` trả về tập `CertifiedVertex` có round ≤ round_cut.
- L2 → L3: function `micro_head()` trả về `MicroCheckpoint { slot, parent_macro, anchor_vertex, committed_sub_dag_root, ordered_blob_refs_root, micro_qc }` (định nghĩa §4).
- L3 → consumer: function `latest_finalized()` trả về `(MacroHeader, MacroQC)` cho checkpoint cao nhất đạt trạng thái `finalized` (§3.5).
- L3 → rollup API: function `blob_status(blob_id)` trả về một trong các trạng thái `{submitted, accepted, ordered, soft_confirmed, justified, finalized}` theo state machine §3.5.

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

### 3.5 Blob lifecycle state machine (rev.2 addition)

API exposed cho rollup developer là một state machine **đơn điệu, có một bước revert duy nhất** (`accepted → soft_confirmed`). Mọi rollup integration phải treat các trạng thái như sau:

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
                                       ← API: finalized=true
```

**Triggers (chính xác):**

| Transition                       | Trigger                                                                                                  |
| -------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `submitted → accepted`           | Ingress validator phát hành signed receipt `IngressReceipt { blob_id, commitment, slot, ingress_sig }`; client verify chữ ký = bằng chứng rằng ít nhất 1 validator đã commit chịu trách nhiệm về blob (slashable nếu sau đó fail availability) |
| `accepted → ordered`             | Có ≥ 1 `CertifiedVertex` chứa `blob_ref` (2f+1 BLS sigs)                                                |
| `ordered → soft_confirmed`       | `MicroQC` cho slot $s$ formed; blob nằm trong `ordered_blob_refs_root` của MicroCheckpoint $s$           |
| `soft_confirmed → justified`     | MacroCheckpoint $C_h$ chứa hash của MicroCheckpoint chứa blob được justified (có MacroQC)                |
| `justified → finalized`          | $C_{h+1}$ justified với `parent_height_hash(C_{h+1}) = hash(C_h)` (2-chain rule, §7.5)                   |

**Revert semantics:**

Tất cả các transition đều **monotonic ở local view của một node honest** trong steady-state. Các trường hợp "revert" chỉ xảy ra ở các failure mode đã được spec ràng buộc:

- `accepted` và `ordered`: **không bao giờ revert** (anchor commit Bullshark là monotonic theorem, §6.8).
- `soft_confirmed`: monotonic **khi `lock_macro` invariant (§11.5) được honor**. Vi phạm `lock_macro` được spec coi là protocol bug (test-time/audit-time), không phải runtime case.
- `justified → soft_confirmed`: chỉ xảy ra nếu MacroQC bị orphan trong macro fork; macro fork đòi hỏi ≥ W/3 stake equivocation và **luôn** sinh slashable evidence (§11.1). Rollup phải đợi resolve.
- `finalized`: irreversible trừ khi tồn tại slashable evidence cho ≥ W/3 stake (accountable safety, §11.1). Đây là failure mode "accountable halt" — chain dừng, slash, social recovery — KHÔNG phải silent revert.

**Khuyến nghị cho rollup**:

| Use case                           | Trạng thái tối thiểu khuyến nghị |
| ---------------------------------- | --------------------------------- |
| UI preview cho user                | `soft_confirmed`                  |
| Fee charge cho L2 tx (revertible)  | `soft_confirmed`                  |
| Bridge withdrawal release          | `finalized` (BẮT BUỘC)            |
| Cross-rollup messaging settlement  | `finalized` (BẮT BUỘC)            |
| Update L1-anchored state root      | `finalized`                       |
| Light client sync read             | `justified` đủ cho passive view; `finalized` cho settlement read |

API contract `latest_finalized()` (§3.3) **không bao giờ** trả về header chưa đạt `finalized`. Nếu rollup query `soft_confirmed`, response phải kèm `revert_risk: true` flag.

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
| `SyncCommitteeUpdate` | Cập nhật sync committee cho light | `epoch`, `next_committee_root`, `aggregate_sig`                                                                   |
| `SlashEvidence`       | Bằng chứng slashable              | `kind`, `validator_id`, `evidence_a`, `evidence_b` (hai message conflict đã ký)                                   |


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

**Tại sao 1D thay vì 2D**: 2D Reed-Solomon (Celestia-style) cho phép DA sampling cho light node, nhưng tăng overhead 2-4x. V2 chọn 1D vì light client tin sync committee về DA (xem mục 8). 2D có thể được introduce ở v2.1 mà không thay đổi vertex schema.

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

$$y_i = \text{iVRF}_i(R_w \,\|\, \text{"anchor"})$$

Anchor proposer của wave $w$ là validator có $y_i \cdot W / (w_i \cdot \text{rep}_i)$ nhỏ nhất trong wave, trong đó $\text{rep}_i \in [0.5, 1.5]$ là Shoal-style leader reputation score (rolling average của liveness gần đây — anchor lỗi nhiều bị giảm rep, anchor lành mạnh được boost). **Không ai biết anchor là ai cho đến khi anchor publish vertex của mình ở round $4w$ — đây là điểm key chống adaptive DoS.**

**Trade-off với unbiasability** (rev.2 note): hệ số `rep_i` là **non-cryptographic input** dựa trên local liveness measurement → mở một surface attack nhỏ:

- Reputation bị derive từ DAG observation; nếu adversary có thể ảnh hưởng "vertex của tôi có được include vào parents không", có thể bias `rep_i` của validator khác.
- Mức bias bị bound bởi hệ số $\text{rep}_i \in [0.5, 1.5]$ (max 3× lợi thế). Trong steady-state với honest > 2/3, drift của reputation rất chậm.
- Reputation **không** áp dụng cho macro proposer selection (§7.2) — chỉ áp dụng cho anchor selection ở micro layer, nơi safety không trực tiếp phụ thuộc vào fairness của leader rotation.
- Nếu reputation bias trở thành issue thực tế khi testnet, fallback là set `rep_i = 1` cho mọi validator (pure stake-weighted iVRF) — chỉ mất ~40% latency improvement của Shoal trong common case.

Khi anchor vertex được certified, hắn reveal iVRF proof. Mọi validator verify proof ⇒ xác nhận anchor đúng.

**Crypto choice (rev.2)**: spec mặc định dùng **iVRF (indexed VRF, Esgin et al. 2023)** thay vì ECVRF Edwards25519. Ba lý do:

1. **Bias resistance**: ECVRF dùng standard VRF definition (Micali-Rabin-Vadhan) không đủ mạnh cho secret leader election — Giunta et al. (2024) chỉ ra adversary có thể craft VRF key pair với output distribution skewed, unfairly tăng winning chance. iVRF có "unbiasability" property native.
2. **Performance**: iVRF eval/verify ~0.02ms, nhanh hơn ECVRF.
3. **Post-quantum readiness**: iVRF hash-based, security dựa trên hash function (post-quantum), không dựa trên elliptic curve discrete log như ECVRF.

ECVRF vẫn được giữ làm fallback nếu iVRF library chưa mature production-ready khi launch.

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

### 7.4 Two-mode subnet aggregation

Đây là kỹ thuật scale macro vote — KEY innovation so với v1. Rev.2 nâng cấp thành **two-mode aggregation** để giảm single-point-of-failure ở macro proposer.

**Subnet partition**: Validator set được phân chia thành **8 subnet** bằng hàm sybil-resistant per epoch:

$$\text{subnet}(v_i, e) = H(\text{pubkey}_i \,\|\, R^{\text{macro}}_{\text{epoch\_start}(e)}) \bmod 8$$

Lý do dùng VRF beacon thay vì `validator_id mod 8`: ngăn adversary chọn validator_id tại thời điểm deposit để stack vào cùng subnet (sybil-style subnet capture). Phân partition rebuild lại mỗi epoch ⇒ adversary không thể commit stake vào 1 subnet cụ thể trước khi biết beacon.

Với `MAX_STAKE_FRACTION = 5%`, stake tối đa của một subnet bị bound bởi Hoeffding-style ~`W/8 + O(√(W/8) · MAX_STAKE_FRACTION)` ⇒ không lệch hơn ±15% so với cân bằng lý tưởng (chứng minh chi tiết defer paper). Mỗi subnet có ~25–60 validator (cho N=200–500).

**Mode A — leader-based (default, fast path)**:

1. Mọi validator nhận MacroProposal, verify, ký hash, gossip partial sig kèm subnet ID.
2. **Trong mỗi subnet**, subnet aggregator (rotated by iVRF) liên tục thu partial sigs. Aggregator publish **partial subnet aggregate** `subnet_aggregate[k] = (subnet_id, bitmap, bls_aggregate, signed_stake)` ngay khi:
   - Hoặc đạt **target threshold** = 2/3 subnet stake (happy case), HOẶC
   - Hoặc đạt **publish deadline** $T_{\text{subnet}} = T_{\text{macropropose}} / 2 = 2$s — publish whatever stake đã collect được tại thời điểm đó.

   Lý do hai-điều-kiện: nếu một subnet bị 1/2 stake offline, aggregator KHÔNG BAO GIỜ đạt 2/3 internal → sẽ stuck. Publish-on-deadline đảm bảo subnet vẫn contribute partial evidence ⇒ macro proposer có thể combine với các subnet khác.

3. **Macro proposer** thu subnet aggregates và áp **quorum rule**:

   - **Quorum rule**: macro proposer aggregate `MacroQC` từ **bất kỳ subset $S \subseteq \{0,\dots,7\}$** miễn:
     - $\sum_{k \in S} \text{signed\_stake}_k \ge \lceil 2W/3 \rceil$, VÀ
     - Mỗi `subnet_aggregate[k]` trong $S$ pass cryptographic verification (bitmap + BLS pairing).

   Trong điều kiện normal $S = \{0,\dots,7\}$ và tổng stake gần $W$. Khi 1–2 subnet aggregator crash hoặc subnet bị split-brain, proposer vẫn đạt $2W/3$ với 6–7 subnet còn lại.

   `MacroQC` lưu `included_subnets` bitmap + `total_signed_stake` để verifier reproduce check (xem §4 struct).

**Mode B — leaderless gossip aggregation (fallback)**:

Activated khi macro proposer DoS hoặc Mode A timeout sau $T_{\text{macropropose}}$:

1. Validator broadcast partial signature qua gossip layer kèm subnet ID.
2. Mỗi node lưu local view các partial sigs đã thấy. Khi đạt quorum rule (§7.4 Mode A step 3), local node tự tạo `MacroQC candidate`.
3. **Canonical selection** (rev.2): vì nhiều node có thể tạo MacroQC candidate khác nhau (cùng height, cùng `MacroProposal_hash`, nhưng `included_subnets` khác nhau), spec định nghĩa **canonical ordering** trên tập candidate hợp lệ (mỗi candidate phải pass `total_signed_stake ≥ 2W/3`) trong window $T_{\text{canonicalize}} = 2 \cdot T_{\text{macropropose}}$:

   $$\text{canonical}(C_1, C_2) = \begin{cases} C_1 & \text{if } \text{total\_signed\_stake}(C_1) > \text{total\_signed\_stake}(C_2) \\ C_2 & \text{if } \text{total\_signed\_stake}(C_2) > \text{total\_signed\_stake}(C_1) \\ \arg\min(\text{included\_subnets bitmap}) & \text{otherwise} \end{cases}$$

   Tức là: **stake cao nhất thắng**; tie-break bằng lex-smallest `included_subnets` bitmap. Lý do thêm stake-priority: tránh perverse incentive nơi candidate `0b00000111` (3 subnet, vừa đủ 2W/3) thắng candidate `0b11111111` (full 8 subnet) chỉ vì bitmap nhỏ hơn — full candidate có evidence mạnh hơn nên phải win.

   Lý do dùng deterministic tie-break thay vì "first seen": tránh race condition nơi hai honest validator pin hai candidate khác nhau thành canonical, gây fork ở light client view.

4. Mọi validator phải re-broadcast canonical MacroQC khi quan sát thấy candidate có stake cao hơn (hoặc bitmap lex-smaller ở cùng stake). Convergence đạt được trong $O(\log N)$ gossip rounds (Long et al. 2019).

Mode B đảm bảo liveness ngay cả khi proposer Byzantine, đổi lại latency cao hơn ~2x Mode A.

**Cost analysis (Mode A, default)**:

- Per-validator: 1 sign + 1 broadcast → O(1) crypto ops.
- Per-subnet aggregator: aggregate ~30 sigs → ~3ms với BLS12-381.
- Macro proposer: aggregate 8 sigs → < 1ms.
- Verification của MacroQC bởi light client: 1 pairing check + bitmap parsing → ~3ms.

So với naive O(N) aggregation tại proposer, scheme này **cắt latency từ ~100ms (cho N=500) xuống ~5ms**. Đây là điểm quyết định cho phép full-validator-set vote vẫn fit trong 5–10s hard-finality target.

**Crypto stack note (rev.2)**: spec hiện default BLS12-381 cho aggregation, nhưng Li et al. (2023) benchmark cho thấy với committee >40 validator, **EdDSA có thể ưu việt hơn BLS** ở computation cost (đổi lại signature size lớn hơn). Vì target là 200-500 validator, quyết định BLS vs EdDSA cần được **benchmark thực tế trong P2 prototype** trước khi chốt — xem benchmark matrix §12.1.

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

### 8.3 Weak subjectivity policy (default)

`WEAK_SUBJECTIVITY_PERIOD = 2 weeks`. Node mới sync phải có một checkpoint không quá 2 tuần tuổi từ một nguồn tin cậy (foundation, friend, official explorer). Lý do: sau 2 tuần, validator có thể withdraw stake → không còn slashable → có thể tạo long-range fork hợp lệ trên giấy. Đây là sound bound — Gazi et al. (2018) đã chứng minh PoS không có checkpointing là vulnerable to stake-bleeding attacks.

`WITHDRAWAL_DELAY = WEAK_SUBJECTIVITY_PERIOD + 1 day`. Sau khi validator request exit, stake bị lock thêm 2 tuần + buffer → đảm bảo slashing window đủ rộng.

### 8.4 Optional Bitcoin checkpointing (rev.2 addition)

Tas et al. (2022, "Babylon") chứng minh impossibility result: các vấn đề security của PoS (non-slashable long-range, low liveness, bootstrap) **inherent** nếu không có external trusted source. Họ propose checkpoint PoS state vào Bitcoin PoW — kết quả: **WITHDRAWAL_DELAY giảm từ tuần xuống <5 giờ**, cost ~$10K/year. Pikachu (Azouvi 2022) cùng ý tưởng dùng Bitcoin Taproot, transaction size constant.

LUA-DAG v2 cung cấp **optional Bitcoin checkpoint mode** dưới dạng add-on:

- Mỗi epoch (~hours), aggregate hash của latest finalized MacroCheckpoint được commit vào Bitcoin qua Taproot transaction.
- Rollup hoặc bridge có thể đăng ký consume Bitcoin-anchored checkpoint thay vì native checkpoint.
- Khi có Bitcoin anchor, **WITHDRAWAL_DELAY giảm xuống 5 giờ** thay vì 2 tuần — UX validator improvement đáng kể.

Operating cost: ~$10K/year cho Bitcoin transaction fees (giá 2026 estimate). Funded từ protocol treasury hoặc opt-in fee từ rollup.

Quyết định opt-in vs always-on để mainnet community quyết, không chốt cứng ở spec.

### 8.5 Checkpoint sync flow

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
- **Subnet aggregation guard** (rev.2 updated): subnet aggregator pre-publish `subnet_aggregate[k]` lên gossip (xem §7.4 Mode A bước 2) **trước khi** macro proposer build MacroQC. Mọi validator có thể quan sát các subnet_aggregate đã public. Macro proposer không thể "ẩn" một subnet_aggregate đã public — nếu cố ý exclude, các validator sẽ refuse-to-vote MacroQC (vì biết có aggregate hợp lệ chưa được include) và proposer mất reward. Proposer chỉ có thể exclude subnet nếu subnet thực sự không publish kịp $T_{\text{subnet}}$.

Kết hợp lại, biên grinding bị cap ở vài bit entropy mỗi epoch — không đủ để bias VRF anchor selection theo cách meaningful trong steady state. Đây là cùng pattern Ethereum dùng cho RANDAO grinding.

### 9.5 Validator set root publishing

Mỗi MacroCheckpoint chứa `validator_set_root` — Merkle root của (validator_id, pubkey, weight) tuple. Light client dùng root này để verify subnet membership và sync committee membership.

### 9.6 Slashing để thoát khỏi validator set

Nếu validator bị slash 100%, hắn auto-exit. Stake bị burn (không quay lại tay attacker dù qua delegation).

---

## 10. Incentive và accountability

### 10.1 Reward decomposition

Mỗi epoch, reward pool được phân chia:


| Loại reward        | %   | Điều kiện                                                                   |
| ------------------ | --- | --------------------------------------------------------------------------- |
| Base reward        | 30% | Validator online, chữ ký xuất hiện trong ≥ 80% certified vertices của epoch |
| Vertex authoring   | 15% | Vertex của validator được certify đúng hạn                                  |
| Anchor proposing   | 10% | Validator được chọn anchor và anchor commit thành công                      |
| Micro committee    | 5%  | Validator được chọn vào committee và vote MicroQC                           |
| Macro voting       | 30% | Validator vote MacroQC trong ≥ 95% macro height                             |
| Macro proposing    | 5%  | Validator propose macro và checkpoint được justified                        |
| Subnet aggregation | 5%  | Validator làm subnet aggregator và submit subnet aggregate đúng hạn         |


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


| Tấn công                                      | Hậu quả                                     | Đối sách v2                                                                                                                       |
| --------------------------------------------- | ------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| Long-range PoS fork                           | Lừa node mới sync vào chain giả             | Weak subjectivity 2 tuần + multi-source pinning                                                                                   |
| Data withholding                              | Soft-confirm blob nhưng data không lấy được | Certified vertex chỉ accept blob có chunks verified; retrieval challenge với slashing                                             |
| Anchor DoS                                    | Mất commit shortcut, fall back slow path    | VRF private sortition, slow path 4 round, backup propagation                                                                      |
| Macro proposer DoS                            | Mất 1 macro window                          | Backup proposer ranked thứ 2, timeout 4s                                                                                          |
| Equivocation                                  | Tạo conflict checkpoint                     | Slashable 100%, 2-chain rule đảm bảo accountable safety                                                                           |
| Subnet capture (k subnets bị > 2/3 Byzantine) | Subnet aggregate sai → corrupted MacroQC    | Subnet partition VRF-rebuild mỗi epoch (chống sybil-stacking); proposer verify từng subnet riêng; quorum rule cho phép skip subnet xấu nếu các subnet còn lại tổng ≥ 2W/3 (xem §7.4)  |
| MEV ordering manipulation                     | Anchor reorder để extract MEV               | Default OFF (rollup tự xử lý); optional Fino-style fairness mode (§1.2) khi rollup opt-in                                         |
| Storage spam                                  | Đẩy bytes vô nghĩa qua DA                   | Fee market + min fee floor + max blob size cap per slot                                                                           |
| Sync committee corruption                     | Light client bị fed invalid header          | Sync committee không có quyền finality; full node cross-check phát hiện                                                           |
| VRF grinding                                  | Bias anchor selection                       | Beacon từ MacroQC nên grinding cần > 1/3 stake; deterministic aggregation rule                                                    |
| Eclipse attack on light client                | Light client bị isolated                    | Multi-source header sources khuyến nghị; defense thuộc deployment                                                                 |
| Adaptive corruption sau VRF reveal            | Chỉ corrupt anchor sau khi anchor đã expose | Round duration < adaptive corruption time (~giờ) ⇒ irrelevant                                                                     |
| Post-quantum attack                           | BLS broken by future quantum                | Migration sang hash-based / lattice-based aggregate sigs ở v2.1 (Drake 2025)                                                      |


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

Nếu Byzantine vượt 1/2:

- Censorship attack possible: Byzantine có thể block honest blob khỏi DAG.
- Liveness broken cho honest workload.
- Safety vẫn cần slashing để break (kẻ tấn công vẫn lose stake).

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


| Phase                                | Milestone                                          | Duration                 | Headcount              |
| ------------------------------------ | -------------------------------------------------- | ------------------------ | ---------------------- |
| P0: Spec finalize                    | Whitepaper hoàn chỉnh + TLA+ skeleton              | 2 tháng                  | 2 protocol + 1 formal  |
| P1: Prototype L1 (Availability DAG)  | Vertex + cert + erasure + GC                       | 3 tháng                  | 3 distsys              |
| P2: Prototype L2 (Bullshark)         | Anchor commit, fast/slow path, MicroQC             | 3 tháng                  | 2 protocol + 1 perf    |
| P3: Prototype L3 (Macro)             | Subnet aggregation, 2-chain finality, slashing     | 3 tháng                  | 2 protocol + 1 crypto  |
| P4: Light client + state sync        | SDK + checkpoint sync                              | 2 tháng                  | 2 client               |
| P5: Permissionless membership        | Activation/exit queue, beacon                      | 2 tháng                  | 1 protocol + 1 testing |
| P6: Adversarial testnet              | Fault injection, partition recovery, public report | 3 tháng                  | toàn đội               |
| P7: External audit + hardening       | 2 audits, fuzzing, chaos                           | 3 tháng                  | external + 1 internal  |
| **Tổng tới prototype-mainnet-ready** |                                                    | **~21 tháng (parallel)** | **~10–12 nòng cốt**    |


Đây là honest estimate cho **prototype** đủ để bắt đầu testnet công khai. **Production-grade** với ecosystem (rollup integrations, SDK, multi-client) cần thêm 12–24 tháng nữa và đội lớn hơn (20–40 người).

### 13.2 Risk register


| Risk                                 | Severity        | Mitigation                                                                         |
| ------------------------------------ | --------------- | ---------------------------------------------------------------------------------- |
| Cross-layer interaction bug          | High            | TLA+ model check ngay từ P0; integration test với chaos suite                      |
| Subnet aggregation correctness       | High            | Crypto audit ưu tiên; fallback to flat aggregation                                 |
| Liveness under partial outage        | High            | Inactivity leak; anchored slow path; backup proposer                               |
| Performance không đạt target         | Medium          | Benchmark sớm ở P2; có plan B (smaller committee, larger wave)                     |
| Storage growth out of control        | Medium          | GC policy được test ở adversarial testnet; archive node tách riêng                 |
| Sync committee corruption            | Medium          | Cross-validation trong client; multi-source headers                                |
| Token launch / regulatory            | Medium          | Out of scope cho doc này; cần legal review riêng                                   |
| Adoption: rollup nào dùng?           | High            | Bootstrap với 1–2 design partner trước launch                                      |
| Cạnh tranh Celestia ecosystem effect | High            | Differentiation phải rõ: faster finality, accountable slashing, lower validator HW |
| Quantum readiness                    | Medium          | Migration sang hash-based aggregate sigs ở v2.1 (đã advance từ v3); xem §13.4      |


### 13.3 Open questions cần resolve trước khi viết implementation plan

- **VRF cipher suite (rev.2 updated)**: iVRF (Esgin 2023, post-quantum, unbiasable) vs ECVRF Edwards25519 (mature library) vs DVRF (distributed). Default rev.2: **iVRF** với fallback ECVRF; cần benchmark eval/verify trên target HW.
- **Aggregation primitive (rev.2 updated)**: BLS12-381 (compact, pairing-based) vs EdDSA (faster cho >40 validator per Li 2023) vs hash-based XMSS variant (post-quantum, Drake 2025). Default cho v2 prototype: **BLS12-381**, nhưng quyết định cuối phải dựa vào benchmark §12.1.
- Reed-Solomon library: tự build hay dùng existing (`reed-solomon-erasure`, RaptorQ)? Default: `reed-solomon-erasure` cho v2.
- Implementation language: Rust (default, Sui/Solana ecosystem), Go (Cosmos ecosystem), Zig (experimental). Default: Rust.
- State machine cho validator role: actor model (Tokio) vs explicit FSM. Default: actor model.
- Light client SDK targets: TypeScript (web), Rust (native), Swift/Kotlin (mobile)? Default: TypeScript first.
- Tokenomics: defer cho doc riêng. Min stake, inflation rate, fee burn rate cần economics modeling.
- Governance: on-chain (delegated voting on macro layer) hay off-chain (foundation initial). Default: off-chain v1, on-chain v2.
- Bridge integration: làm reference bridge cho Ethereum trước hay defer? Default: defer.
- Multi-client strategy: bootstrap với 1 client, đến sau audit thứ 2 mới fund client thứ 2. Default: confirmed.
- **Bitcoin checkpoint (rev.2 new)**: opt-in mode at genesis vs always-on vs deferred to v2.1? Default: opt-in tại deployment, infrastructure ready từ v2.
- **Optional MEV fairness mode (rev.2 new)**: enable Fino-style integration trong v2 hay defer? Default: spec ready, default OFF, rollup opt-in.
- **Formal verification framework (rev.2 new)**: TLA+ (mature, model checking) vs Coq + LiDO-DAG framework (Qiu 2025, mechanized proof, đã verify Narwhal/Bullshark). Default: bắt đầu với TLA+ ở P0, evaluate Coq cho P1 onwards.
- **Reputation vs unbiasability tradeoff (rev.2 new, §6.2)**: giữ Shoal-style `rep_i` ở range `[0.5, 1.5]` hay narrower `[0.8, 1.2]` để giảm bias surface? Hay tách thành 2 mode (rep ON cho throughput, rep OFF cho high-stakes anchor)? Cần adversarial testnet (P6) để measure rep gaming attempts.
- **Mode B canonical MacroQC tie-break (rev.2 new, §7.4)**: lex-smallest `included_subnets` bitmap (current default) vs "highest signed_stake total" vs "first seen by VRF-elected witness committee"? Default lex-smallest vì simplest và deterministic; cần verify không tạo perverse incentive (e.g. validator chậm pin subnet 0 để force candidate có bitmap nhỏ thắng).
- **`lock_macro` advance protocol (rev.2 new, §11.5)**: cần spec rõ message type `MacroJustifyAnnouncement` để validator nhanh chóng sync `latest_justified_macro_checkpoint` qua dedicated gossip topic, hay tận dụng existing macro vote gossip? Default: dedicated topic trong P3 prototype, evaluate piggyback option ở P6.
- **Custody assignment churn (rev.2 new, §5.5.1)**: khi validator exit/activation, custody set của một blob có thể thay đổi giữa lúc blob ở warm tier — cần grace period transition để custody mới có cơ hội pull chunks từ custody cũ. Default: 1 epoch grace; cần kiểm tra storage spike risk.

### 13.4 Deferred to v2.1+ (KHÔNG trong scope v2 prototype, nhưng spec để open)

- **DA sampling cho light verification** — design open giữa 2D Reed-Solomon + KZG (Celestia-style), RLNC-based DAS (Grundei 2025) và polynomial-commitment-based (Hall-Andersen 2025). Quyết định trước v2.1 implementation.
- **Uncertified DAG mode** (Mysticeti-style) với Adelie mitigations — option để boost throughput 2x sau khi v2 stable.
- **Post-quantum signature migration** (rev.2 advanced từ v3 lên v2.1) — Drake 2025 hash-based multi-sigs ready, Ethereum đang chuẩn bị migration; không nên defer xa hơn.
- Restaking integration (EigenLayer AVS / Babylon BTC restaking).
- Shared sequencing layer (cross-rollup atomic).
- ZK header proof cho bridge optimization.
- State rent / storage pricing.
- Cross-chain message passing (IBC-style).
- Validator reputation extensions (advanced beyond Shoal-style basic rep).
- GNN-based adaptive parameter tuning (DAGWise++ 2025) — v3 candidate.

---

## 14. So sánh head-to-head: LUA-DAG v1 vs v2 vs các giao thức tương tự

### 14.1 v1 → v2


| Khía cạnh             | v1                                    | v2                                                                | Cải thiện                                      |
| --------------------- | ------------------------------------- | ----------------------------------------------------------------- | ---------------------------------------------- |
| Scope                 | Generic L1 với execution placeholder  | DA + finality only, no execution                                  | Thu nhỏ scope ⇒ ship-able                      |
| Frontier rule         | "Xác định" mơ hồ                      | Bullshark anchor commit, đã proven                                | Fix lỗ hổng lý thuyết quan trọng nhất          |
| Macro voting          | Full validator set, naive aggregation | Two-mode (leader + leaderless gossip fallback) subnet aggregation | Scale tới 500 validator vẫn < 5s, no SPOF      |
| Permissionless detail | Một dòng nói "permissionless"         | Full spec activation/exit/churn                                   | Implementable thực sự                          |
| Soft vs hard contract | Note rủi ro nhưng không API           | API explicit `accepted/soft/finalized`                            | Ngăn rollup dùng nhầm                          |
| MEV resistance        | Một dòng "aged-inclusion fee"         | Optional Fino-style mode (rev.2)                                  | Available cho DeFi rollup mà không trade speed |
| Long-range protection | 2-week WS only                        | 2-week WS + optional Bitcoin checkpoint (rev.2, <5h withdrawal)   | Đáng kể cho bridge UX                          |
| VRF crypto            | Generic VRF                           | iVRF default (post-quantum, unbiasable) (rev.2)                   | Đóng VRF biasing attack                        |
| Committee safety      | Định tính                             | Bảng xác suất số cụ thể + Markov robustness section (rev.2)       | Có thể defend trước reviewer                   |
| Cross-layer recovery  | Không spec                            | Mục 11.5 spec rõ                                                  | Tránh micro-flush bug                          |
| Cạnh tranh thị trường | "Compete với mọi L1"                  | "Compete trong DA segment"                                        | Có shot ở thị trường không bị chiếm            |
| Effort estimate       | 6–8 người × 12–18 tháng               | 10–12 người × ~21 tháng tới testnet                               | Honest hơn                                     |


### 14.2 LUA-DAG v2 vs các giao thức tương đồng (rev.2 addition)


| Giao thức                           | Điểm chung với LUA-DAG                                                         | Differentiation                                                                                                                |
| ----------------------------------- | ------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------ |
| **Celestia (Tendermint + DA)**      | Modular DA + finality                                                          | Celestia dùng single-leader Tendermint; LUA-DAG dùng DAG ⇒ throughput cao hơn ~5-15x kỳ vọng                                   |
| **Avail (BABE + GRANDPA + KZG DA)** | Modular DA, polynomial commitments                                             | Avail có DA sampling từ đầu; LUA-DAG defer DAS sang v2.1 nhưng có hard finality nhanh hơn                                      |
| **Mysticeti / Sui**                 | DAG-based BFT, ebb-and-flow                                                    | Mysticeti là full L1 với execution; LUA-DAG là DA-only + macro accountable finality (Casper FFG-style)                         |
| **Acki Nacki**                      | 2-step consensus, separation execution-verification và propagation-attestation | Acki Nacki dùng probabilistic safety + random committee per block; LUA-DAG dùng full validator macro vote + accountable safety |
| **Babylon**                         | PoS với Bitcoin checkpoint                                                     | LUA-DAG có DAG availability + accountable finality riêng; Babylon là retrofit checkpoint cho PoS chain hiện hữu                |
| **EigenDA**                         | Modular DA cho rollups                                                         | EigenDA borrow security từ ETH restakers, no native consensus; LUA-DAG có native PoS + accountable slashing                    |
| **Fino**                            | DAG + MEV resistance                                                           | Fino chỉ là MEV resistance overlay; LUA-DAG (rev.2) tích hợp Fino-style fairness mode optional                                 |


---

## 15. Phụ lục A — Bảng parameter tham chiếu


| Tham số                     | Default                             | Range cho tuning |
| --------------------------- | ----------------------------------- | ---------------- |
| `ROUND_DURATION`            | 250 ms                              | 100–500 ms       |
| `WAVE_LENGTH`               | 4 rounds                            | 2–8 rounds       |
| `MACRO_WINDOW_W`            | 8 micro-slots                       | 4–16             |
| `MICRO_COMMITTEE_SIZE`      | 256                                 | 128–1024         |
| `SUBNET_COUNT`              | 8                                   | 4–16             |
| `MAX_VERTEX_PAYLOAD`        | 1 MB                                | 256 KB – 4 MB    |
| `MAX_BLOB_SIZE`             | 8 MB                                | 1 MB – 32 MB     |
| `ERASURE_RATE`              | 1/2                                 | 1/2 – 1/4        |
| `GC_HOT_HORIZON`            | 200 rounds                          | 100–500          |
| `GC_WARM_HORIZON`           | 10 000 rounds                       | 5 000–50 000     |
| `MAX_ACTIVATION_PER_EPOCH`  | 4                                   | 2–8              |
| `MAX_EXIT_PER_EPOCH`        | 4                                   | 2–8              |
| `WEAK_SUBJECTIVITY_PERIOD`  | 2 weeks                             | 1–4 weeks        |
| `WITHDRAWAL_DELAY`          | 2 weeks + 1 day                     | matches WS       |
| `INACTIVITY_LEAK_THRESHOLD` | 4 macro windows                     | 2–16             |
| `INACTIVITY_LEAK_RATE`      | 0.5% / window                       | 0.1–1%           |
| `EQUIVOCATION_SLASH`        | 100%                                | fixed            |
| `DOUBLE_VOTE_SLASH`         | 100%                                | fixed            |
| `DATA_UNAVAILABILITY_SLASH` | 5% per occurrence                   | 1–10%            |
| `K_CUSTODY`                 | 2f+1                                | f+1 – 2f+1       |
| `T_RETRIEVE`                | 30 s                                | 10–120 s         |
| `MIN_CHALLENGE_INTERVAL`    | 60 s                                | 30–300 s         |
| `T_MACROPROPOSE`            | 4 s                                 | 2–8 s            |
| `T_SUBNET`                  | 2 s (= T_MACROPROPOSE / 2)          | 1–4 s            |
| `T_CANONICALIZE`            | 8 s (= 2 × T_MACROPROPOSE)          | 4–16 s           |
| `SYNC_COMMITTEE_SIZE`       | 512                                 | 256–1024         |
| `SYNC_COMMITTEE_PERIOD`     | 1024 macro height                   | 256–4096         |
| `MIN_STAKE`                 | tham số hóa, suggest $50–100k equiv | —                |
| `MAX_STAKE_FRACTION`        | 5%                                  | 1–10%            |


## 16. Phụ lục B — Glossary

- **Anchor**: vertex được chọn bởi VRF làm điểm commit của một wave.
- **Blob**: payload bytes từ rollup, đơn vị DA.
- **Certified vertex**: vertex có 2f+1 chữ ký, dùng được làm parent.
- **Causal closure**: tập vertex có path tới một vertex cho trước trong DAG.
- **Hard finality**: trạng thái không thể revert trừ khi có slashable evidence cho ≥ W/3 stake.
- **MacroQC**: aggregate signature 2/3 stake trên một MacroCheckpoint.
- **MicroQC**: aggregate signature 2/3 micro committee trên một MicroCheckpoint.
- **Soft confirmation**: trạng thái sau MicroQC, có thể revert dưới điều kiện hiếm.
- **Sync committee**: tập 512 ký-slot (sampled with replacement từ active validator set) ký headers cho light client trong 1 epoch. Một validator có thể nắm nhiều slot.
- **Wave**: 4-round window cho một anchor commit attempt.
- **Weak subjectivity**: phải tin một checkpoint gần đây để sync trustworthy.

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

### 17.7 MEV resistance

1. Malkhi et al., "Maximal Extractable Value (MEV) Protection on a DAG (Fino)", 2022.
2. Kavousi et al., "BlindPerm: Efficient MEV Mitigation with an Encrypted Mempool and Permutation", IACR ePrint 2023.
3. Yang et al., "SoK: MEV Countermeasures: Theory and Practice", arXiv 2022.
4. Nasrulin et al., "LO: An Accountable Mempool for MEV Resistance", Middleware 2023.

### 17.8 Operational

1. Ethereum.org, "Consensus Mechanisms / Sync Committees / Weak Subjectivity", 2024–2026.

