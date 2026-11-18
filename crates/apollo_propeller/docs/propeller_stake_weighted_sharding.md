# Propeller Stake-Weighted Sharding: Option Analysis

> For detailed proofs, binding coalition algebra, and implementation considerations, see
> the [full analysis](propeller_stake_weighted_sharding_analysis.md).

## Motivation

Tendermint consensus assumes that fewer than 1/3 of **stake** is malicious. The current
Propeller implementation assumes that fewer than 1/3 of **nodes** are malicious. This is an
additional assumption: an attacker with low stake but many node identities can disrupt
Propeller's message delivery without being able to disrupt Tendermint consensus.

To close this gap, Propeller's sharding and reconstruction logic should be stake-aware.
This document presents five options and analyzes their security models, bandwidth
requirements, and trade-offs.

## Setup and Notation

| Symbol | Meaning |
|--------|---------|
| N | Total nodes in the channel (including publisher) |
| M | Message data throughput (MiB/s) |
| s\_p | Publisher's stake fraction |
| s\_i | Validator *i*'s stake fraction |
| s\_max | Largest (non-publisher) validator's stake fraction |

**Protocol structure:**

Options 1, 2, & 3 (publisher has **no shard**, publisher excluded in 2 & 3):

1. The publisher creates shards and sends one to each of the N−1 receivers.
2. Each receiver gossips their assigned shard(s) to all other N−2 peers.
3. The publisher does **not** hold a shard.
4. In Options 2 & 3, the publisher is **excluded** from the stake distribution used for
   thresholds and/or shard generation.

Options 4 & 5 (publisher has **own shard(s)**, broadcasts them directly):

1. The publisher creates shards for **all N nodes** (including itself).
2. The publisher distributes each receiver's shard to that receiver, and **broadcasts
   their own shard(s)** to all N−1 receivers.
3. Each receiver gossips their assigned shard(s) to all other N−2 peers.
4. The publisher's stake is counted **only when the publisher's shard(s) are received**
   — no "free" stake. The publisher actively participates as a broadcaster.
5. Option 4: fixed allocation (1 shard per node, all equal size). Option 5: proportional
   allocation (shard size ∝ stake).

**Thresholds:**

| Counting method | Build (reconstruct message) | Receive (deliver to application) |
|----|----|----|
| Node counting (Option 1) | `num_data_shards` shards | `2 × num_data_shards` shards |
| Stake counting, publisher excluded, fixed (Option 2) | ≥ 1/3 non-publisher stake | ≥ 2S/3 − sₚ receiver stake |
| Stake counting, publisher excluded, proportional (Option 3) | ≥ 1/3 non-publisher stake | ≥ 2S/3 − sₚ receiver stake |
| Stake counting, publisher in pool, fixed (Option 4) | ≥ 1/3 total stake | ≥ 2/3 total stake |
| Stake counting, publisher in pool, proportional (Option 5) | ≥ 1/3 total stake | ≥ 2/3 total stake |

---

## Option 1: Node Counting (Current Implementation)

### Description

Each of the N−1 receivers gets exactly one shard. The message is split into
`floor((N−1)/3)` data shards plus coding shards for redundancy. Reconstruction
requires `floor((N−1)/3)` shards — counted by number of shards, not by stake.

### Parameters

| Parameter | Formula |
|-----------|---------|
| Total shards | N − 1 |
| Data shards (d) | floor((N−1) / 3) |
| Coding shards | N − 1 − d |
| Shard size | M / d |

### Bandwidth

| Node | Upload |
|------|--------|
| Publisher | (N−1) × M / d |
| Any receiver | (N−2) × M / d |

All receivers upload the same amount regardless of stake.

### Security Model

Assumes fewer than **1/3 of nodes** are malicious. This is an additional assumption
beyond Tendermint's model and is **not Sybil-resistant** at the broadcast layer.

---

## Option 2: Stake Counting, Publisher Excluded, Fixed Allocation

### Description

Each of the N−1 receivers still gets exactly one shard (same allocation as Options 1
& 4), but the build and receive thresholds are based on a **stake distribution that
excludes the publisher**. The publisher's stake is completely ignored for threshold
calculations. Thresholds are measured against the non-publisher stake pool:
S' = S − s\_p.

### Key Insight

By excluding the publisher from the stake distribution, we avoid the "free" stake
mechanism that causes expansion blowup in Option 4. The build threshold is 1/3 of the
non-publisher stake S', and the receive threshold is 2S/3 − sₚ (which equals
2S'/3 − sₚ/3). This threshold is set to exactly match what honest receivers can
guarantee under the standard Tendermint assumption, requiring no strengthened
assumption.

Because each validator holds exactly one shard, `num_data_shards` is determined by the
**smallest coalition** (by number of validators) whose combined non-publisher stake
reaches 1/3 of S'.

Formally, let S' = S − s\_p. Sort validators by stake descending:
s₁ ≥ s₂ ≥ … ≥ s\_{N−1}. Let **K'** be the smallest *k* such that:

```
s₁ + s₂ + … + sₖ  ≥  S' / 3
```

Then `num_data_shards = K'`.

Because the publisher's stake does not appear in the threshold calculation, the binding
coalition among receivers is typically larger than K from Option 4 (which is determined
over all N nodes including the publisher). With concentrated stake, K' is substantially
larger than K — **but only when the large staker is the publisher**. When a small
staker publishes, the large staker becomes a receiver and may dominate the binding
coalition, making K' very small and expansion very high. See the
[full analysis](propeller_stake_weighted_sharding_analysis.md) for publisher stake
sensitivity details.

### Parameters

| Parameter | Formula |
|-----------|---------|
| Total shards | N − 1 |
| Data shards | K' |
| Coding shards | N − 1 − K' |
| Shard size | M / K' |

### Bandwidth

| Node | Upload |
|------|--------|
| Publisher | (N−1) × M / K' |
| Any receiver | (N−2) × M / K' |

All receivers upload the same amount regardless of stake.

### Security Model

Assumes fewer than **1/3 of total stake** is malicious. The publisher is excluded
from the stake distribution for thresholds and shard allocation, but the security
model uses the standard Tendermint assumption (stake(B) < S/3). The receive
threshold is set to 2S/3 − sₚ, which is exactly what honest receivers can guarantee.
Sybil-resistant. Aligns with Tendermint.

### Example (publisher = 32%, second-largest = 5%, rest ≈ 1%)

S' = 1 − 0.32 = 0.68. Build threshold = S'/3 ≈ 0.227. Receive threshold =
2/3 − 0.32 ≈ 0.347.

K' = min k such that top-k validators' combined stake ≥ 0.227:
- k=1: 0.05 < 0.227
- k=2: 0.06 < 0.227
- ...
- k=19: 0.05 + 18×0.01 = 0.23 ≥ 0.227 → **K' = 19**

| | Value |
|---|---|
| Data shards | **19** |
| Shard size | 5/19 ≈ **0.26 MiB** |
| Expansion | 64/19 ≈ **3.37×** |

| Node | Upload (MiB/s) |
|------|-----------------|
| Publisher | 64 × 5/19 ≈ **16.8** |
| Any receiver | 63 × 5/19 ≈ **16.6** |

---

## Option 3: Stake Counting, Publisher Excluded, Proportional Allocation

### Description

Like Option 2, the publisher is **excluded** from the stake distribution for threshold
calculations. But additionally, shards are allocated **proportionally to the
non-publisher stake** among the N−1 receivers. This means higher-stake receivers get
more shards and lower-stake receivers get fewer.

### Key Insight

By using proportional shard allocation among receivers (based on non-publisher stake),
the binding coalition becomes the minimum-**stake** qualifying subset among receivers:

```
s_coal'  =  min { Σ s_i  over subsets S ⊆ receivers :  Σ s_i ≥ S'/3 }
```

where S' = S − s\_p is the total non-publisher stake.

Because shards are proportional to stake, a receiver with fraction f of the
non-publisher stake holds fraction f of the shards. The binding coalition's total
shards are proportional to s\_coal', so:

```
num_data_shards  ≈  (s_coal' / S') × T
shard_size       ≈  M × S' / (s_coal' × T)
expansion        ≈  S' / s_coal'
```

Since s\_coal' ≈ S'/3 (the minimum qualifying coalition barely exceeds 1/3 of
non-publisher stake), the expansion is approximately **3×**.

Total shard count T is tunable (T ≥ N−1). Each receiver *i* gets
`round(s_i × T / S')` shards (at least 1).

### Parameters

| Parameter | Formula |
|-----------|---------|
| Total shards | T (tunable, T ≥ N−1) |
| Data shards | ≈ s\_coal' × T / S' |
| Coding shards | T − num\_data\_shards |
| Shard size | ≈ M × S' / (s\_coal' × T) |

### Bandwidth

| Node | Upload (approximate) |
|------|--------|
| Publisher | T × shard\_size = M × S' / s\_coal' |
| Receiver with stake s\_i | (s\_i / S') × (N−2) × M × S' / s\_coal' |

Receiver upload is **proportional to stake**. Higher-stake receivers upload more,
lower-stake receivers upload less.

### Security Model

Assumes fewer than **1/3 of total stake** is malicious. The publisher is excluded
from the stake distribution for thresholds and shard generation, but the security
model uses the standard Tendermint assumption (stake(B) < S/3). The receive
threshold is 2S/3 − sₚ. Sybil-resistant. Aligns with Tendermint.

### Example (publisher = 32%, second-largest = 5%, rest ≈ 1%)

S' = 0.68. s\_coal' / S' ≈ 0.338 (minimum qualifying coalition fraction within
receivers; s\_coal' ≈ 0.23, i.e. 23% of total stake, which is 0.23/0.68 ≈ 33.8% of
non-publisher stake S').

| | Value |
|---|---|
| Data shards | ≈ (s\_coal'/S') × T ≈ 0.338 × T |
| Shard size | ≈ M / ((s\_coal'/S') × T) |
| Expansion | S'/s\_coal' ≈ 0.68/0.23 ≈ **2.96×** |

| Node | Upload (MiB/s) |
|------|-----------------|
| Publisher | M / (s\_coal'/S') ≈ 5/0.338 ≈ **14.8** |
| 5% staker | (0.05/0.68) × 63 × 14.8 ≈ **68.5** |
| 1% staker | (0.01/0.68) × 63 × 14.8 ≈ **13.7** |

**Key properties:**

- **Expansion ≈ 3×** — comparable to Option 1.
- **Publisher upload is moderate** — comparable to Options 1 and 2.
- **Receiver upload is proportional to stake** — higher-stake receivers upload more,
  lower-stake receivers upload less. A 5% staker uploads ~68.5 MiB/s while a 1%
  staker uploads only ~13.7 MiB/s.
- The publisher is excluded from the stake pool, avoiding the "free" stake problem.
- **Publisher-independent expansion** — s\_coal' depends only on the receiver set,
  not on which node publishes. However, the receiver set changes when a different
  node becomes the publisher, so there is an indirect dependence.

---

## Option 4: Stake Counting, Publisher in Pool, Fixed Allocation

### Description

Every node — **including the publisher** — gets exactly one shard. There are N total
shards (not N−1). The publisher creates all N shards, distributes each receiver's shard
to that receiver, and **broadcasts their own shard to all N−1 receivers**. Each receiver
gossips their shard to the other N−2 peers (excluding publisher and themselves).

The publisher's stake is **not "free"**: it is backed by the publisher's actual shard
data. The publisher's stake is credited only when the publisher's shard is received.

### Key Insight

Because each node holds exactly one shard, `num_data_shards` is determined by the
**smallest coalition** (by number of nodes) over **all N nodes** whose combined stake
reaches 1/3.

Formally, sort all N nodes by stake descending: s₁ ≥ s₂ ≥ … ≥ sₙ.
Let **K** be the smallest *k* such that:

```
s₁ + s₂ + … + sₖ  ≥  S/3
```

Then `num_data_shards = K`.

**K is publisher-independent** — it depends only on the stake distribution, not on
which node publishes. However, with concentrated stake (e.g. s\_max = 32% and
s₂ = 5%), K = 2 (32% + 5% = 37% ≥ 33.3%), giving high expansion.

### Parameters

| Parameter | Formula |
|-----------|---------|
| Total shards | N |
| Data shards | K |
| Coding shards | N − K |
| Shard size | M / K |

### Bandwidth

| Node | Upload |
|------|--------|
| Publisher | 2(N−1) × M / K |
| Any receiver | (N−2) × M / K |

The publisher's upload is doubled: (N−1) for distribution + (N−1) for broadcasting
their own shard. All receivers upload the same amount regardless of stake.

### Security Model

Assumes fewer than **1/3 of stake** is malicious. Aligns with Tendermint. Sybil-resistant.

---

## Option 5: Stake Counting, Publisher in Pool, Proportional Allocation

### Description

Shards are allocated **proportionally to stake for all N nodes** — including the
publisher. The publisher creates all shards, distributes each receiver's shards to
that receiver, and **broadcasts their own shards to all N−1 receivers**. Each receiver
gossips their own shards to the other N−2 peers.

The publisher's stake is **not "free"**: it is backed by the publisher's actual shard
data, which the publisher broadcasts directly. The publisher's stake is credited only
when the publisher's shards are received.

### Key Insight

Because the publisher is included in the shard pool and their stake is backed by
actual shard data, the binding coalition is determined over **all N nodes**:

```
s_coal  =  min { Σ s_i  over subsets S ⊆ all N nodes :  Σ s_i ≥ 1/3 }
```

**s\_coal is publisher-independent.** The publisher is just another node in the pool,
so the binding coalition does not change based on who publishes.

Because no stake is "free", qualifying coalitions must accumulate ≥ 1/3 total stake
from actual shard data. This means **s\_coal ≈ 1/3**, which gives expansion ≈ 3× —
comparable to Option 1.

Total shard count T is tunable (T ≥ N). Node *i* (including publisher) receives
`round(s_i × T)` shards (at least 1). Then, approximately:

```
num_data_shards  ≈  s_coal × T
shard_size       ≈  M / (s_coal × T)
```

### Bandwidth

The publisher distributes non-publisher shards and broadcasts their own shards:

```
publisher upload  =  (S − sₚ) × M / s_coal  +  sₚ × (N−1) × M / s_coal
                  =  (S + sₚ(N−2)) × M / s_coal
```

Each receiver broadcasts only their own shards to N−2 peers:

| Node | Upload (approximate) |
|------|--------|
| Publisher | (S + sₚ(N−2)) × M / s\_coal |
| Receiver with stake s\_i | s\_i × (N−2) × M / s\_coal |

**Key properties:**

- **Publisher upload is high when sₚ is large** — the publisher must broadcast sₚ fraction
  of all shards to N−1 peers. For sₚ = 32%, publisher upload ≈ 311 MiB/s.
- **Publisher upload is publisher-dependent** — it grows with sₚ because the publisher
  broadcasts more shards.
- **Receiver upload is strictly proportional to stake** — no delegation surcharge.
  A 5% staker uploads ~46 MiB/s; a 1% staker uploads ~9.3 MiB/s.
- **Expansion ≈ 3×** — comparable to Option 1.

The per-unit-stake cost for receivers is approximately:

```
bandwidth per unit stake  ≈  (N−2) × M / s_coal
```

T (total shard count) is tunable and does not affect correctness. Higher T gives finer
proportional granularity; minimum T = N.

### Security Model

Assumes fewer than **1/3 of stake** is malicious. Aligns with Tendermint. Sybil-resistant.

---

## Concrete Example

**Parameters:** N = 65 nodes, M = 5 MiB/s, s\_max = 32%.

- **Publisher** = 32% staker (s\_p = 0.32)
- **Second-largest staker** = 5% (s₂ = 0.05)
- **Remaining 63 validators** ≈ 1% each
- **Option 2**: Non-publisher stake S' = 0.68. Threshold = S'/3 ≈ 0.227.
  K' = 19 (5% + 18×1% = 23% ≥ 22.7%).
- **Option 3**: s\_coal' ≈ 0.23 over receivers. Expansion ≈ S'/s\_coal' ≈ 2.96×.
- **Option 4**: Binding coalition over all N nodes (publisher included with own shard).
  K = 2 (32% + 5% = 37% ≥ 33.3%).
- **Option 5**: Same binding coalition as Option 4 but proportional allocation.
  s\_coal ≈ 0.34 (e.g. {32% publisher, two 1% validators} = 34% ≥ 33.3%,
  or {34 × 1% validators} = 34%).

### Option 1

Unaffected by publisher stake (node counting).

| | Value |
|---|---|
| Data shards | floor(64/3) = **21** |
| Shard size | 5/21 ≈ **0.24 MiB** |
| Expansion | 64/21 ≈ **3.05×** |

| Node | Upload (MiB/s) |
|------|-----------------|
| Publisher | 64 × 5/21 ≈ **15.2** |
| Any receiver | 63 × 5/21 ≈ **15.0** |

### Option 2 (K' = 19)

K' = 19 when the 32% staker publishes (need 22.7% of non-publisher stake from
receivers; 5% + 18×1% = 23%).

| | Value |
|---|---|
| Data shards | **19** |
| Shard size | 5/19 ≈ **0.26 MiB** |
| Expansion | 64/19 ≈ **3.37×** |

| Node | Upload (MiB/s) |
|------|-----------------|
| Publisher | 64 × 5/19 ≈ **16.8** |
| Any receiver | 63 × 5/19 ≈ **16.6** |

### Option 3 (s\_coal' ≈ 0.23)

s\_coal' ≈ 0.23 regardless of publisher identity within receiver set. Expansion ≈
S'/s\_coal' ≈ 0.68/0.23 ≈ 2.96×.

| | Value |
|---|---|
| Data shards | ≈ 0.23/0.68 × T ≈ 0.34 × T |
| Shard size | ≈ 5 × 0.68 / (0.23 × T) |
| Expansion | 0.68/0.23 ≈ **2.96×** |

| Node | Upload (MiB/s) |
|------|-----------------|
| Publisher | 5 × 0.68/0.23 ≈ **14.8** |
| 5% staker | (0.05/0.68) × 63 × 14.8 ≈ **68.5** |
| 1% staker | (0.01/0.68) × 63 × 14.8 ≈ **13.7** |

### Option 4 (K = 2)

K = 2 regardless of who publishes (32% + 5% = 37% ≥ 33.3%). Publisher-independent.

| | Value |
|---|---|
| Data shards | **2** |
| Shard size | 5/2 = **2.5 MiB** |
| Expansion | 65/2 = **32.5×** |

| Node | Upload (MiB/s) |
|------|-----------------|
| Publisher | 2 × 64 × 5/2 = **320** |
| Any receiver | 63 × 5/2 = **157.5** |

### Option 5 (s\_coal ≈ 0.34)

s\_coal ≈ 0.34 regardless of who publishes (same binding coalition as Option 4).
Expansion ≈ S/s\_coal ≈ 1/0.34 ≈ 2.94×.

| | Value |
|---|---|
| Data shards | ≈ 0.34 × T |
| Shard size | ≈ 5 / (0.34 × T) |
| Expansion | 1/0.34 ≈ **2.94×** |

| Node | Upload (MiB/s) |
|------|-----------------|
| Publisher (32% staker) | (1 + 0.32 × 63) × 5/0.34 ≈ **311** |
| 5% staker | 0.05 × 63 × 5/0.34 ≈ **46.3** |
| 1% staker | 0.01 × 63 × 5/0.34 ≈ **9.3** |

---

## Side-by-Side Comparison

Worst case: publisher = 32% staker, second-largest = 5%, rest ≈ 1%.

| | Option 1 | Option 2 | Option 3 | Option 4 | Option 5 |
|---|---|---|---|---|---|
| **Security model** | < 1/3 nodes | < 1/3 stake | < 1/3 stake | < 1/3 stake | < 1/3 stake |
| **Aligns with Tendermint** | No | Yes | Yes | Yes | Yes |
| **Sybil-resistant** | No | Yes | Yes | Yes | Yes |
| **Publisher stake "free"** | N/A | **No** (excluded) | **No** (excluded) | **No** (own shard) | **No** (own shards) |
| **Expansion factor** | 3.05× | **~3.37×** | **~2.96×** | 32.5× | **~2.94×** |
| **Publisher upload** | ~15.2 | **~16.8** | **~14.8** | 320 | **~311** |
| **Max receiver upload** | ~15.0 | **~16.6** | **~68.5** (5%) | 157.5 | **~46.3** (5% staker) |
| **Min receiver upload** | ~15.0 | **~16.6** | **~13.7** (1%) | 157.5 | **~9.3** (1% staker) |
| **Bandwidth distribution** | Uniform | Uniform | ∝ stake | Uniform | ∝ stake |
| **Publisher-independent expansion** | Yes | **No** (see §Discussion) | **Yes** (indirect) | **Yes** | **Yes** |
| **Implementation complexity** | Existing | Low | Moderate | Low | Moderate |

All bandwidth values in MiB/s.

> **Key observations:**
> - Options 2–5 all align with Tendermint's security model (< 1/3 stake assumption).
> - Option 3 achieves expansion comparable to Option 1 (~3×) by excluding the
>   publisher from the stake distribution and using proportional allocation. The
>   expansion is nearly publisher-independent.
> - **Option 2 has publisher-dependent expansion.** While ~3.37× when the largest
>   staker publishes, it degrades to up to 64× when a small staker publishes (because
>   a high-stake receiver alone exceeds S'/3, making K' = 1). Option 2 is the simplest
>   stake-aware upgrade but impractical with concentrated stake distributions.
> - Option 3 adds proportional shard allocation, making bandwidth proportional to
>   stake. This is fair but means high-stake receivers upload significantly more
>   (~68.5 MiB/s for a 5% staker vs ~16.6 MiB/s uniform in Option 2).
> - Options 4 and 5 include the publisher in the shard pool with their own shard(s).
>   The publisher broadcasts their own shards directly, eliminating "free" stake.
>   Option 5 achieves ~3× expansion but the publisher's upload is high (~311 MiB/s
>   when s\_p = 32%) because the publisher broadcasts a large proportion of shards.
>   Option 4 has high expansion (32.5×) because the fixed allocation still makes the
>   binding coalition small (K = 2).

---

## Pros and Cons

### Option 1: Node Counting (Current)

| Pros | Cons |
|------|------|
| Already implemented and tested | Security mismatch with Tendermint |
| Uniform, predictable bandwidth (~15 MiB/s) | Vulnerable to Sybil attacks at broadcast layer |
| Low publisher bandwidth (~15.2 MiB/s) | Small stakers pay disproportionately per unit stake |

### Option 2: Stake Counting, Publisher Excluded, Fixed Allocation

| Pros | Cons |
|------|------|
| Simplest stake-aware upgrade | Uniform bandwidth (not proportional to stake) |
| ~3.37× expansion when large staker publishes | **Expansion is publisher-dependent**: can reach 64× when a small staker publishes |
| Same shard allocation as Option 1 (easy to implement) | Receive threshold depends on publisher stake |
| Aligns with Tendermint (< 1/3 stake) | |
| Sybil-resistant | |
| No "free" stake mechanism | |
| Low publisher bandwidth (~16.8 MiB/s) | |

### Option 3: Stake Counting, Publisher Excluded, Proportional Allocation

| Pros | Cons |
|------|------|
| ~2.96× expansion — comparable to Option 1 | More complex implementation than Option 2 |
| Bandwidth proportional to stake (fair) | Need to choose T (granularity trade-off) |
| Low publisher bandwidth (~14.8 MiB/s) | 5% staker uploads ~68.5 MiB/s (higher than Option 2's uniform ~16.6) |
| Low-stake receivers upload less (~13.7 MiB/s for 1%) | Receive threshold depends on publisher stake |
| Aligns with Tendermint (< 1/3 stake) | |
| No "free" stake mechanism | |
| Sybil-resistant | |

### Option 4: Stake Counting, Publisher in Pool, Fixed Allocation

| Pros | Cons |
|------|------|
| Security aligns with Tendermint | High expansion (32.5×) with concentrated stake |
| Simple shard assignment (1 per node) | High publisher upload (320 MiB/s) due to distribution + gossip |
| Publisher-independent expansion | Receiver upload still high (157.5 MiB/s) |
| No "free" stake — publisher has own shard | Binding coalition (K) is small with concentrated stake |
| Sybil-resistant | |

### Option 5: Stake Counting, Publisher in Pool, Proportional Allocation

| Pros | Cons |
|------|------|
| Security aligns with Tendermint | Very high publisher upload (~311 MiB/s for 32% publisher) |
| ~3× expansion — comparable to Option 1 | Need to choose T (granularity trade-off) |
| Receiver upload strictly proportional to stake (fair) | Publisher upload grows with sₚ and N |
| No "free" stake — publisher has own shards | |
| Publisher-independent expansion | |
| Low receiver bandwidth (~9.3 MiB/s for 1% staker) | |
| Sybil-resistant | |
| Converges to Option 1 when stake is uniform | |

---

## Discussion

### Why Option 4 Has High Expansion

In Option 4, each node (including the publisher) gets exactly one shard. The binding
coalition is determined over all N nodes — the smallest group of nodes whose combined
stake reaches S/3. With concentrated stake (32% + 5% = 37% ≥ 33.3%), K = 2, giving
expansion 65/2 = 32.5×. While this is better than the original "free stake" design
(which had K = 1, expansion = 64×), it remains impractical for deployment. The
fundamental issue is that fixed allocation (1 shard per node) with stake-based thresholds
allows small high-stake coalitions to qualify.

### Why Option 3 Achieves ~3× Expansion

Option 3 excludes the publisher from the stake distribution used for thresholds.
The non-publisher stake S' = S − s\_p is used as the total stake for threshold
calculations. The build threshold becomes S'/3 (purely from receiver stakes), and the
binding coalition is determined over the N−1 receivers only. Since shards are
**proportional to stake**, the binding coalition's stake (s\_coal') is the relevant
quantity, and s\_coal' ≈ S'/3 regardless of which node publishes. This gives expansion
≈ S'/s\_coal' ≈ 3×.

### Why Option 2 Does NOT Always Achieve ~3× Expansion

Option 2 uses the same publisher-excluded threshold but with **fixed allocation** (one
shard per receiver). The binding coalition is measured by **member count** (K'), not
stake. When the largest staker publishes, they are excluded and K' reflects the many
small receivers (K' = 19, expansion ≈ 3.37×). But when a small staker publishes, the
largest staker becomes a receiver and can single-handedly exceed S'/3, collapsing K' to
1 and expansion to 64×. The ~3× expansion holds only when the largest staker
publishes.

### Why Option 5 Achieves ~3× Expansion

By backing the publisher's stake with actual shard data and proportional allocation,
Option 5 eliminates the "free" stake mechanism. Every qualifying coalition must
accumulate ≥ 1/3 total stake from actual shards. With proportional allocation, the
binding coalition's shards are proportional to its stake, so s\_coal ≈ S/3 and
expansion ≈ 3×. However, unlike Options 2 and 3, the publisher must broadcast their
own (proportionally large) shards to all N−1 peers, resulting in high publisher upload
when sₚ is large.

### Publisher-Independence

Options 3–5 have publisher-independent (or nearly independent) expansion factors.
Option 3 achieves this because the proportional allocation makes the binding coalition's
total **stake** (s\_coal') the relevant quantity, and s\_coal' ≈ S'/3 regardless of
publisher identity — expansion stays ≈ 3×. Options 4 and 5 achieve it because the
binding coalition is determined over all N nodes (publisher included as a regular
participant).

**Option 2 is NOT publisher-independent.** Because each receiver holds exactly one shard,
the binding coalition size K' (a **count** of validators) determines expansion. K'
depends heavily on which node publishes: when a large staker publishes, they are excluded
from the receiver pool and K' is large (low expansion). When a small staker publishes,
the large staker becomes a receiver and can form a tiny qualifying coalition, making K'
very small (high expansion). For example, with [32%, 5%, 1%×63]: when the 32% staker
publishes, K' = 19 and expansion = 3.37×; but when the 5% staker publishes, the 32%
staker alone exceeds S'/3, giving K' = 1 and expansion = 64×.

### Uniform Stake Convergence

When all validators have equal stake, all five options converge to approximately the
same bandwidth. The differences only manifest with non-uniform stake distributions.

### Future Optimizations

Two orthogonal optimizations could further reduce bandwidth on top of any option:

1. **Re-gossip** — accept shards from any peer (not just the designated broadcaster).
   This could reduce expansion further but requires changes to `validate_origin` and
   introduces duplicate shard traffic.
2. **Tree-based gossip** — replace full-mesh with a tree topology, reducing the N−2
   sends-per-shard multiplier.

See the [full analysis](propeller_stake_weighted_sharding_analysis.md) for detailed
discussion of these optimizations, the gossip property proofs, binding coalition
algebra, and implementation considerations.
