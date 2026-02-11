# Propeller Stake-Weighted Sharding: Full Analysis

> For a concise summary of the five options and their trade-offs, see the
> [option analysis](propeller_stake_weighted_sharding.md).

This document captures the complete analysis of stake-weighted sharding options for the
Propeller broadcast protocol. It is intended to be self-contained: any reader (human or
AI) should be able to understand the full problem, the options considered, the errors
discovered during analysis, and the open questions, without needing prior context.

---

## Table of Contents

1. [What is Propeller](#1-what-is-propeller)
2. [The Security Mismatch Problem](#2-the-security-mismatch-problem)
3. [Setup and Notation](#3-setup-and-notation)
4. [Five Options for Stake-Weighted Sharding](#4-five-options-for-stake-weighted-sharding)
5. [Binding Coalition Analysis](#5-binding-coalition-analysis)
6. [Bandwidth Analysis](#6-bandwidth-analysis)
7. [The s\_coal Formula Error](#7-the-s_coal-formula-error)
8. [Publisher Stake Sensitivity](#8-publisher-stake-sensitivity)
9. [The Greedy Shard Allocation Algorithm](#9-the-greedy-shard-allocation-algorithm)
10. [Modified Stake Allocation](#10-modified-stake-allocation)
11. [Fundamental Limits](#11-fundamental-limits)
12. [The Gossip Property](#12-the-gossip-property)
13. [Comparison Table](#13-comparison-table)
14. [Open Questions](#14-open-questions)

---

## 1. What is Propeller

Propeller is a distributed broadcast protocol implemented as a libp2p network behaviour
in the `apollo_propeller` crate. It is inspired by Solana's Turbine protocol. Its
purpose is to efficiently disseminate messages (e.g., block proposals) across a
peer-to-peer network of validators.

### How It Works

1. **Publisher shards the message**: A message of size M is split into `num_data_shards`
   data shards. Reed-Solomon erasure coding generates `num_coding_shards` additional
   coding shards. All shards are the same size: `M / num_data_shards`.

2. **Merkle tree**: A Merkle tree is built over all shards. The root is signed by the
   publisher. Each shard is paired with its Merkle proof.

3. **Distribution**: The publisher sends one shard (a `PropellerUnit`) to each of the
   N−1 receivers. Each unit contains: the shard data, its index, the Merkle root, a
   Merkle proof, and the publisher's signature.

4. **Gossip**: When a receiver gets their assigned shard, they gossip it to all other
   N−2 peers (everyone except the publisher and themselves). This is **full-mesh
   gossip** — every shard is sent to every other peer.

5. **Reconstruction**: Once a receiver has collected enough shards (`num_data_shards`),
   they can reconstruct the original message using Reed-Solomon decoding. The Merkle
   root is verified against the reconstructed data.

6. **Delivery**: The reconstructed message is delivered to the application layer after
   meeting the "receive" threshold (a higher bar than reconstruction, to ensure gossip
   properties are satisfied).

### Key Implementation Details

- **Protocol**: Uses libp2p with protobuf-encoded messages over long-lived substreams.
- **Validation**: Each received shard is validated for: correct sender (origin
  validation), valid Merkle proof, and valid publisher signature.
- **No re-gossip**: The `validate_origin` function in `tree.rs` only accepts shard i
  from two sources: the publisher (only if the local peer is the designated broadcaster
  for shard i) or the designated broadcaster for shard i. Shards received from any
  other peer are rejected. Peers only broadcast their **own assigned shard**, never
  other peers' shards. This means each shard propagates at most one hop from its
  designated broadcaster — there is no transitive forwarding. (See Section 12 for why
  this matters.)
- **Reconstruction cascade**: When a peer accumulates enough shards to reconstruct the
  full message but has not yet received their own assigned shard from the publisher
  (e.g., because the publisher is Byzantine), the peer extracts their own shard from
  the reconstructed message and broadcasts it. This is the only mechanism by which
  shards propagate beyond the initial publisher → designated broadcaster → all peers
  path.
- **Parallelism**: Validation and reconstruction are offloaded to a Rayon thread pool.
- **Per-message processing**: Each unique message (identified by channel + publisher +
  Merkle root) gets its own `MessageProcessor` task.
- **Channels**: Peers are organized into channels with pre-registered members and
  weights. The publisher must be a channel member.
- **Stake field exists but is unused**: The current implementation accepts stake weights
  during channel registration (`Vec<(PeerId, Stake)>`) but does not use them for
  shard distribution or threshold calculations.

### Current Shard Parameters

In the current implementation (see `tree.rs`):

```
num_data_shards  = max(1, floor((N − 1) / 3))
num_coding_shards = (N − 1) − num_data_shards
total_shards     = N − 1
```

Each of the N−1 receivers gets exactly one shard. Reconstruction requires
`num_data_shards` shards (any combination of data and coding shards).

---

## 2. The Security Mismatch Problem

### Tendermint's Assumption

Tendermint consensus assumes that fewer than **1/3 of total stake** is controlled by
malicious actors. This is a stake-weighted assumption:

- A validator with 10% stake has 10% of the voting power.
- Creating additional validator identities (Sybil attack) doesn't help unless the
  attacker acquires more stake.
- The protocol is safe as long as honest validators control > 2/3 of stake.

### Propeller's Current Assumption

Propeller currently assumes that fewer than **1/3 of nodes** are malicious. This is a
node-counting assumption:

- Every node counts equally regardless of stake.
- An attacker can create many node identities (Sybil attack) to exceed the 1/3
  threshold without acquiring additional stake.

### The Mismatch

When Propeller is used as the broadcast layer for a Tendermint-based network, there is
a security gap:

- **Tendermint is safe**: An attacker with 0% stake cannot disrupt consensus.
- **Propeller is vulnerable**: The same attacker can create 34+ Propeller peer
  identities and prevent message delivery (by exceeding 1/3 of nodes).

The goal is to make Propeller's security model align with Tendermint's by using
**stake-weighted** thresholds instead of node-counting thresholds.

---

## 3. Setup and Notation

| Symbol | Meaning |
|--------|---------|
| N | Total nodes in the channel (including the publisher) |
| M | Message data throughput (MiB/s) — the rate of useful data being broadcast |
| s\_p | Publisher's stake fraction |
| s\_i | Validator *i*'s stake fraction (every validator has s\_i > 0) |
| s\_max | Largest validator's stake fraction |
| S | Total stake (= Σ sᵢ over all N nodes) |
| S' | Non-publisher stake (= S − s\_p) |
| T | Total number of shards (tunable; T ≥ N−1 for Options 1–3, T = N for Option 4, T ≥ N for Option 5) |
| K | Minimum number of validators in the binding coalition (Option 4 — over all N nodes) |
| K' | Minimum number of validators in the binding coalition (Option 2, publisher excluded) |
| s\_coal | Combined stake of the binding coalition (Option 5 — over all N nodes, publisher-independent) |
| s\_coal' | Combined stake of the binding coalition (Option 3 — over receivers only, publisher excluded) |

### Protocol Structure

**Options 1, 2, & 3 (publisher has no shard):**

1. The publisher creates shards and sends **at least one** to each of the N−1 receivers.
2. Each receiver gossips their assigned shard(s) to all other **N−2 peers** (full-mesh).
3. The publisher does **not** hold a shard.
4. In Options 2 & 3, the publisher is **excluded** from the stake distribution used for
   thresholds and/or shard generation.
5. All validators have non-zero stake.

**Option 4 (publisher has own shard, fixed allocation):**

1. The publisher creates **N shards** (one per node, including itself) and broadcasts its
   **own shard** directly to all N−1 receivers.
2. The publisher sends each receiver's shard only to that receiver. The publisher sends
   its own shard to every receiver.
3. Each receiver (and the publisher, for its own shard) gossips their assigned shard to
   all other **N−2 peers** (full-mesh).
4. K = min coalition over **all N nodes** s.t. stake ≥ S/3. No "free" stake. Publisher-independent.
5. All nodes have non-zero stake and exactly one shard each.

**Option 5 (publisher has own shards, proportional allocation):**

1. The publisher creates shards for **all N nodes** (including itself), proportional to
   stake.
2. The publisher **broadcasts its own shards directly** to all N−1 receivers. No delegation.
3. The publisher sends each receiver's shard only to that receiver.
4. Each receiver gossips their assigned shard(s) to all other **N−2 peers**. No cascade
   enhancement.
5. s\_coal = min subset sum ≥ S/3 over all N nodes. Publisher-independent.

### Thresholds

| Counting method | Build (can reconstruct) | Receive (deliver to app) |
|---|---|---|
| Node counting (Option 1) | `num_data_shards` shards | `2 × num_data_shards` shards |
| Stake counting, publisher excluded, fixed (Option 2) | ≥ S'/3 receiver stake | ≥ 2S/3 − sₚ receiver stake |
| Stake counting, publisher excluded, proportional (Option 3) | ≥ S'/3 receiver stake | ≥ 2S/3 − sₚ receiver stake |
| Stake counting, publisher in pool, fixed (Option 4) | ≥ S/3 total stake (K over all N) | ≥ 2S/3 total stake |
| Stake counting, publisher in pool (Option 5) | ≥ S/3 total stake | ≥ 2S/3 total stake |

In Options 4 & 5, "≥ S/3 total stake" means stake from actual shard data — the publisher
has its own shard(s) and contributes to the pool like any other node. No "free" stake.
In Options 2 & 3, the publisher's stake is not counted — the build threshold is S'/3
(receiver stake only) and the receive threshold is 2S/3 − sₚ. This receive threshold
is set to exactly match what honest receivers can guarantee under the standard Tendermint
assumption.

The **build** threshold is a hard requirement: the erasure coding parameters must
guarantee reconstruction is possible once enough stake has been accumulated. The
**receive** threshold is a policy that delays delivery until sufficient stake has
confirmed the message, ensuring gossip properties hold.

---

## 4. Five Options for Stake-Weighted Sharding

### Option 1: Node Counting (Current Implementation)

Each receiver gets exactly one shard. Thresholds are based on shard count, not stake.

| Parameter | Formula |
|-----------|---------|
| Total shards | N − 1 |
| Data shards (d) | floor((N−1) / 3) |
| Coding shards | N − 1 − d |
| Shard size | M / d |

**Bandwidth:**

| Node | Upload |
|------|--------|
| Publisher | (N−1) × M / d |
| Any receiver | (N−2) × M / d |

All receivers upload the same amount regardless of stake.

**Security model:** < 1/3 of **nodes** are malicious. Not Sybil-resistant. Does not
align with Tendermint.

### Option 2: Stake Counting, Publisher Excluded, Fixed Allocation

Each receiver still gets exactly one shard (same as Options 1 & 4), but build/receive
thresholds are based on **non-publisher stake** S' = S − s\_p. The publisher's stake
is completely excluded from threshold calculations.

**Key consequence:** `num_data_shards` must be small enough that any qualifying
non-publisher stake coalition can reconstruct. Since each validator holds exactly one
shard, `num_data_shards` equals the minimum **number of validators** in any qualifying
coalition (measured against S'/3).

Define K': sort validators by stake descending (s₁ ≥ s₂ ≥ … ≥ s\_{N−1}). Let K' be
the smallest k such that:

```
s₁ + s₂ + … + sₖ  ≥  S' / 3
```

Then `num_data_shards = K'`.

| Parameter | Formula |
|-----------|---------|
| Total shards | N − 1 |
| Data shards | K' |
| Coding shards | N − 1 − K' |
| Shard size | M / K' |

**Bandwidth:**

| Node | Upload |
|------|--------|
| Publisher | (N−1) × M / K' |
| Any receiver | (N−2) × M / K' |

**Security model:** < 1/3 of **total stake** is malicious. The publisher is excluded
from the stake distribution for thresholds, but the security model uses the standard
Tendermint assumption. The receive threshold is 2S/3 − sₚ. Sybil-resistant. Aligns
with Tendermint.

### Option 3: Stake Counting, Publisher Excluded, Proportional Allocation

Like Option 2, the publisher is **excluded** from the stake distribution for threshold
calculations. Additionally, shards are allocated **proportionally to non-publisher
stake** among the N−1 receivers.

The binding coalition is the minimum-**stake** qualifying subset among receivers:

```
s_coal'  =  min { Σ s_i  over subsets S ⊆ receivers :  Σ s_i ≥ S'/3 }
```

Total shard count T is tunable (T ≥ N−1). Node *i* receives
`round(s_i × T / S')` shards (at least 1). Then, approximately:

```
num_data_shards  ≈  (s_coal' / S') × T
shard_size       ≈  M × S' / (s_coal' × T)
expansion        ≈  S' / s_coal'
```

**Bandwidth:**

| Node | Upload (approximate) |
|------|--------|
| Publisher | M × S' / s\_coal' |
| Receiver with stake s\_i | (s\_i / S') × (N−2) × M × S' / s\_coal' |

**Security model:** < 1/3 of **total stake** is malicious. The publisher is excluded
from the stake distribution for thresholds and shard generation, but the security
model uses the standard Tendermint assumption. The receive threshold is 2S/3 − sₚ.
Sybil-resistant. Aligns with Tendermint.

### Option 4: Stake Counting, Fixed Allocation

The publisher has its **own shard** and broadcasts it directly to all N−1 receivers.
Each of the N nodes holds exactly one shard. Build/receive thresholds are based on
**stake** over all N nodes. No "free" stake — publisher-independent.

**Key consequence:** `num_data_shards` must be small enough that any qualifying
stake coalition can reconstruct. Since each node holds exactly one shard,
`num_data_shards` equals the minimum **number of nodes** in any qualifying
coalition.

Define K: sort **all N nodes** (including publisher) by stake descending. Let K be the
smallest k such that:

```
s₁ + s₂ + … + sₖ  ≥  S/3
```

Then `num_data_shards = K`.

| Parameter | Formula |
|-----------|---------|
| Total shards | N |
| Data shards | K |
| Coding shards | N − K |
| Shard size | M / K |

**Bandwidth:**

| Node | Upload |
|------|--------|
| Publisher | 2(N−1) × M / K |
| Receiver with stake s\_i | (N−2) × M / K |

The publisher sends its own shard to all N−1 receivers and each receiver's shard to
that receiver only. Receiver upload is uniform (each broadcasts one shard to N−2 peers).

**Security model:** < 1/3 of **stake** is malicious. Sybil-resistant. Aligns with
Tendermint.

### Option 5: Stake Counting, Proportional Allocation

Shards are allocated **proportionally to stake for all N nodes** — including the
publisher. The publisher has its **own shards** and broadcasts them directly to all
N−1 receivers. No delegation, no cascade enhancement.

**Key insight:** The binding coalition is determined over **all N nodes**:

```
s_coal  =  min { Σ s_i  over subsets S ⊆ all N nodes :  Σ s_i ≥ S/3 }
```

**s\_coal is publisher-independent.** No "free" stake. Qualifying coalitions must
accumulate ≥ 1/3 total stake from actual shard data → **s\_coal ≈ S/3**, expansion ≈ 3×.

Total shard count T is tunable (T ≥ N). Node *i* (including publisher) receives
`round(s_i × T / S)` shards (at least 1). Then, approximately:

```
num_data_shards  ≈  s_coal × T / S
shard_size       ≈  M × S / (s_coal × T)
```

**Bandwidth:**

The publisher broadcasts its own shards to all N−1 receivers and sends each receiver's
shards to that receiver. Publisher upload:

```
publisher upload  =  (S + s_p(N−2)) × M / s_coal
```

Each receiver broadcasts only their own shards to N−2 peers:

| Node | Upload (approximate) |
|------|--------|
| Publisher | (S + s\_p(N−2)) × M / s\_coal |
| Receiver with stake s\_i | s\_i × (N−2) × M / s\_coal |

**Key properties:**

- **Expansion ≈ 3×** — s\_coal ≈ S/3, same as before.
- **Publisher upload is publisher-dependent** — the s\_p(N−2) term increases with publisher
  stake.
- **Receiver upload is proportional to stake** — s\_i × (N−2) × M / s\_coal.
- **No delegation or cascade** — simpler design.

**Merkle tree optimization:** Although Reed-Solomon internally operates on T equal-sized
pieces, all pieces belonging to the same peer are concatenated into a single payload.
The Merkle tree is built over N leaves (one per node), and each peer receives a single
Merkle proof. Option 5 does **not** increase Merkle tree overhead compared to Options 1
through 4.

**Choice of T:** T does not affect correctness or the ideal bandwidth formulas, but it
controls the quantization error between ideal and actual shard allocation. Higher T =
finer proportional split. Minimum: T ≥ N (every node, including publisher, must have
at least one shard).

**Security model:** < 1/3 of **stake** is malicious. Sybil-resistant. Aligns with
Tendermint.

### Summary of the Five Options

| Option | Shard Allocation | Publisher Has Shard | Stake Counting |
|--------|-----------------|---------------------|----------------|
| 1 | Fixed (1 per receiver) | No | None (count nodes) |
| 2 | Fixed (1 per receiver) | No | Yes (publisher excluded) |
| 3 | Proportional to non-pub stake | No | Yes (publisher excluded) |
| 4 | Fixed (1 per node) | **Yes — broadcasts directly** | Yes (publisher in pool) |
| 5 | Proportional to stake (incl. publisher) | **Yes — broadcasts directly** | Yes (publisher in pool) |

Options 2 and 3 exclude the publisher from the stake distribution. Option 4 and 5
include the publisher in the shard pool: the publisher has its own shard(s) and
broadcasts them directly to all N−1 receivers. No "free" stake, no delegation.
Publisher-independent expansion.

---

## 5. Binding Coalition Analysis

### Definition

The **binding coalition** is the set of validators that constrains `num_data_shards`.
It is the qualifying coalition (combined stake reaches the build threshold) with the
**fewest shards**. `num_data_shards` must be ≤ the number of shards in this coalition,
otherwise they would meet the stake threshold but be unable to reconstruct.

### For Option 2 (Publisher Excluded, Fixed Allocation)

Each validator has exactly one shard. The binding coalition is the qualifying coalition
with the **fewest members** (since each member contributes exactly 1 shard).

Threshold is S'/3 (non-publisher stake only). Using the sorted-descending definition:
K' = smallest k such that `s₁ + … + sₖ ≥ S'/3`. This is correct because the greedy
top-down approach minimizes the number of members.

**Example** (publisher = 32%, second-largest = 5%, rest ~1%):
- S' = 68%. S'/3 ≈ 22.7%.
- K' = 19 (5% + 18×1% = 23% ≥ 22.7%).
- Expansion: 64/19 ≈ 3.37×.

Compare with Option 4: K = 2 (32% + 5% = 37% ≥ 33.3%), expansion 65/2 = 32.5×.

### For Option 3 (Publisher Excluded, Proportional Allocation)

Shards are proportional to non-publisher stake among receivers. The binding coalition
is the minimum-**stake** qualifying subset among receivers:

```
s_coal' = min { Σ s_i  over subsets S ⊆ receivers :  Σ s_i ≥ S'/3 }
```

**Example** (publisher = 32%, second-largest = 5%, rest ~1%):
- S' = 68%. S'/3 ≈ 22.7%.
- s\_coal' ≈ 23% (e.g. {5% + 18 × 1%} = 23%).
- Expansion: S'/s\_coal' = 68%/23% ≈ 2.96×.

### For Option 4 (Fixed Allocation)

Each of the N nodes has exactly one shard. The binding coalition is the qualifying
coalition over **all N nodes** with the **fewest members** (since each member contributes
exactly 1 shard).

Using the sorted-descending definition: K = smallest k such that
`s₁ + … + sₖ ≥ S/3` over all N nodes. No "free" stake — publisher is in the pool like
any other node. Publisher-independent.

### For Option 5 (Proportional Allocation)

In Option 5, the publisher is included in the shard pool and broadcasts its own shards
directly. No delegation. The binding coalition is the minimum-**stake** qualifying
subset over **all N nodes**:

```
s_coal = min { Σ s_i  over subsets S ⊆ all N nodes :  Σ s_i ≥ S/3 }
```

**s\_coal is publisher-independent** — the publisher is just another node in the pool.

**Example** (publisher = 32%, second-largest = 5%, rest ~1%):

- s\_coal ≈ 0.34 (need ≥ 33.33% from actual shards; e.g., 32% + two 1% validators = 34%,
  or {34 × 1% validators} = 34%). Expansion = 1/0.34 ≈ **2.94×**.

### Why s\_coal ≈ S/3 in Option 5

In Options 4 and 5 (new design), there is no "free" stake. Every qualifying coalition
must itself have ≥ S/3 total stake from actual shard data. The minimum such coalition
will have ≈ S/3 + ε (where ε is the discrete rounding needed to exceed S/3). For
practical validator sets, s\_coal ≈ S/3, giving expansion ≈ 3×.

### Why s\_coal' ≈ S'/3 in Options 2 and 3

Similarly, in Options 2 and 3, the threshold is S'/3 (non-publisher stake). The binding
coalition must accumulate ≥ S'/3 from receiver stakes alone. The minimum qualifying
coalition will have ≈ S'/3 + ε. For Option 2, K' is the count of members in this
coalition; for Option 3, s\_coal' is the total stake. Both give expansion ≈ 3×.

### Key Takeaway (Updated)

| Option | Binding coalition type | Pool | Formula | Publisher-dependent? |
|--------|----------------------|------|---------|---------------------|
| 2 | Min members | N−1 receivers | K' = min k s.t. Σ top-k ≥ S'/3 | **Yes** (strongly with concentrated stake) |
| 3 | Min total stake | N−1 receivers | s\_coal' = min Σ s.t. Σ ≥ S'/3 | Mildly (via S') |
| 4 | Min members | All N nodes | K = min k s.t. Σ top-k ≥ S/3 | **No** |
| 5 | Min total stake | All N nodes | s\_coal = min Σ s.t. Σ ≥ S/3 | **No** |

Option 2's publisher dependence is strong with concentrated stake: K' counts
**members**, so a single high-stake receiver can form a 1-member coalition (K' = 1),
causing expansion up to 64×. Option 3's dependence is mild because s\_coal' measures
**stake**, which stays near S'/3 regardless of which node publishes.

Options 4 and 5 are publisher-independent: the binding coalition is over all N nodes,
and the publisher is in the pool with its own shard(s) like any other node.

---

## 6. Bandwidth Analysis

### General Formulas

For any option, the bandwidth depends on:

- **Shard size** = M / num\_data\_shards
- **Expansion factor** = total\_shards / num\_data\_shards

**Options 1 & 2 (publisher has no shard, fixed allocation):**
- **Publisher upload** = (N−1) × shard\_size
- **Receiver upload** = (N−2) × shard\_size

**Option 3 (publisher excluded, proportional allocation):**
- **Publisher upload** = T × shard\_size = M × S' / s\_coal'
- **Receiver upload** = (s\_i / S') × (N−2) × M × S' / s\_coal'

**Option 4 (publisher has own shard, fixed allocation):**
- **Publisher upload** = 2(N−1) × M / K (own shard to all N−1 + each receiver's shard to that receiver)
- **Receiver upload** = (N−2) × M / K

**Option 5 (publisher broadcasts own shards directly):**
- **Publisher upload** = (S + s\_p(N−2)) × M / s\_coal
- **Receiver upload** = s\_i × (N−2) × M / s\_coal

### The 315 MiB/s Fundamental Cost

Any validator whose accumulated stake (their shards' owners' stake + publisher stake)
reaches 1/3 can reconstruct the **entire message**. If they then gossip all their
shards to N−2 peers, their upload is:

```
upload = M × (N − 2)
```

For N = 65, M = 5 MiB/s: **5 × 63 = 315 MiB/s**.

This cost is independent of the option chosen. It falls on the **binding coalition
validator** — the smallest-stake receiver that, combined with enough other receivers,
can reconstruct. This is unavoidable with full-mesh gossip.

### Concrete Example

**Parameters:** N = 65, M = 5 MiB/s, publisher = 32% staker, second-largest = 5%,
remaining 63 validators ≈ 1% each.

#### Option 1 (Node Counting)

Unaffected by stake distribution.

- d = floor(64/3) = 21
- Expansion: 64/21 ≈ **3.05×**

| Node | Upload (MiB/s) |
|------|----------------|
| Publisher | 64 × 5/21 ≈ **15.2** |
| Any receiver | 63 × 5/21 ≈ **15.0** |

#### Option 2 (Publisher Excluded, Fixed)

S' = 68%. Threshold = S'/3 ≈ 22.7%. K' = 19.

- Expansion: 64/19 ≈ **3.37×**

| Node | Upload (MiB/s) |
|------|----------------|
| Publisher | 64 × 5/19 ≈ **16.8** |
| Any receiver | 63 × 5/19 ≈ **16.6** |

#### Option 3 (Publisher Excluded, Proportional)

S' = 68%. s\_coal' ≈ 23%. Expansion ≈ 68%/23% ≈ **2.96×**.

| Node | Upload (MiB/s) |
|------|----------------|
| Publisher | 5 × 0.68/0.23 ≈ **14.8** |
| 5% staker | (0.05/0.68) × 63 × 14.8 ≈ **68.5** |
| 1% staker | (0.01/0.68) × 63 × 14.8 ≈ **13.7** |

#### Option 4 (Stake Counting, Fixed)

K = 2 (32% + 5% = 37% ≥ 33.3%). N = 65 total shards.

- Expansion: 65/2 = **32.5×**

| Node | Upload (MiB/s) |
|------|----------------|
| Publisher | 2 × 64 × 5/2 = **320** |
| Any receiver | 63 × 5/2 = **157.5** |

#### Option 5 (Stake Counting, Proportional)

s\_coal ≈ 0.34 (the minimum qualifying coalition over all 65 nodes is any subset with
≥ 33.33% stake; e.g. {32% publisher, two 1% validators} = 34%, or {34 × 1% validators}
= 34%). s\_coal is publisher-independent.

- Expansion: 1/0.34 ≈ **2.94×**

| Node | Upload (MiB/s) |
|------|----------------|
| Publisher | (1 + 0.32×63) × 5/0.34 ≈ **311** |
| 5% staker | 0.05 × 63 × 5/0.34 ≈ **46.3** |
| 1% staker | 0.01 × 63 × 5/0.34 ≈ **9.3** |

**Key observations:**

1. **Option 3 achieves ~3× expansion** — comparable to Option 1 — by excluding the
   publisher and using proportional allocation. **Option 2 achieves ~3× only when the
   largest staker publishes**; with other publishers, expansion can reach 64× (see §8).
2. **Option 4's expansion is 32.5×** — improved from the old design (64×) because K is
   over all N nodes with no "free" stake. Publisher upload = 320 MiB/s; receivers
   upload 157.5 MiB/s (half of the old 315).
3. **Option 5's expansion is ~3×** — publisher-independent. Publisher upload is
   publisher-dependent (~311 MiB/s for 32% staker); receiver upload is proportional
   to stake.
4. **Options 4 and 5 no longer have "free" stake** — the publisher contributes its own
   shard(s) to the pool. Both are publisher-independent for expansion.

---

## 7. The s\_coal Formula Error

During analysis, we discovered that the document's original formula for s\_coal was
**incorrect** for a **deprecated** proportional allocation design (Option 4 with
proportional shards and "free" publisher stake — no longer used). The error and its
impact on that old design:

### The Error

The original document defined s\_coal using the same greedy top-K approach as Option 4:

```
s_coal = s₁ + s₂ + … + s_K    (INCORRECT for proportional allocation)
```

This finds the qualifying coalition with the **fewest members**, which is correct for
Option 4 (each member = 1 shard) but wrong for proportional allocation (shards ∝ stake, so we need
the qualifying coalition with the **least total stake**).

### The Correct Definition

```
s_coal = min { Σ s_i  over subsets S :  Σ s_i + s_p ≥ S/3 }
```

This minimum is achieved by choosing the **smallest validators** that barely cross the
threshold, not the largest.

### Impact on Numbers

With publisher = 32%, second-largest = 5%, rest ≈ 1%:

| | Incorrect s\_coal | Correct s\_coal |
|---|---|---|
| **Value** | 0.05 (5% staker alone) | 0.02 (two 1% validators) |
| **Expansion** | 20× | 50× |
| **Publisher upload** | 100 MiB/s | 250 MiB/s |
| **1% staker upload** | 63 MiB/s | 157.5 MiB/s |

The correct expansion is **2.5× worse** than initially calculated. **Note:** The current
Options 4 and 5 use the redesigned protocol (publisher has own shard(s), no "free"
stake) and do not suffer from this formula error. This section is preserved for
historical context.

---

## 8. Publisher Stake Sensitivity

### Option 4: Publisher-Independent

In the redesigned Option 4, the publisher has its own shard. The binding coalition K
is determined over **all N nodes** with no "free" stake. K does not depend on which
node publishes. Expansion is publisher-independent.

### Option 3: Publisher Excluded, Proportional — Mild Sensitivity

In Option 3, the publisher's stake is excluded from threshold calculations. The
binding coalition is determined by non-publisher stake S' = S − s\_p. Because shards
are allocated proportionally to stake, the binding coalition's total **stake**
(s\_coal') is the relevant quantity. Since s\_coal' ≈ S'/3 regardless of publisher
identity, the expansion stays ≈ 3× across all publishers:

| Publisher stake | S' | s\_coal' | Expansion (Option 3) |
|---|---|---|---|
| ~1.5% (average) | ~98.5% | ~32.8% | ~3.00× |
| 32% (maximum) | 68% | ~22.7% | ~2.96× |

Option 3's expansion is effectively publisher-independent.

### Option 2: Publisher Excluded, Fixed — **Strong Sensitivity**

Option 2 uses fixed allocation (one shard per receiver), so `num_data_shards` equals
K' — the minimum **number** of receivers whose combined stake reaches S'/3. This
makes K' highly sensitive to the receiver set composition:

| Publisher stake | S' | S'/3 threshold | K' (Option 2) | Expansion (Option 2) |
|---|---|---|---|---|
| 32% (maximum) | 68% | ~22.7% | 19 | ~3.37× |
| 5% | 95% | ~31.7% | **1** | **64×** |
| ~1% | ~99% | ~33% | **2** | **32×** |

When the largest staker (32%) publishes, they are excluded from the receiver pool, and
K' is large because many small validators are needed to reach the threshold. But when a
small staker publishes, the 32% staker becomes a receiver and alone exceeds S'/3,
making K' = 1 and expansion = 64×.

**This is a fundamental limitation of Option 2's fixed allocation.** The binding
coalition counts **members**, not **stake**. A single high-stake receiver can form a
1-member qualifying coalition, collapsing K' to 1. This problem does not affect
Option 3 (proportional allocation) because the binding coalition is measured by
**stake**, which stays near S'/3.

> **Note:** The severity of this effect depends on the stake distribution. With many
> near-equal validators (e.g., all ~1.5%), K' varies only mildly (~19 to ~21).
> The extreme variation above occurs with concentrated stake (e.g., one 32% validator
> among many ~1% validators).

### Option 5: Publisher-Independent Expansion, Publisher-Dependent Upload

In Option 5, the binding coalition s\_coal is determined over all N nodes and does
**not depend on which node is the publisher**. Expansion is publisher-independent.
However, **publisher upload** depends on s\_p: (S + s\_p(N−2)) × M / s\_coal. Larger
publisher stake → higher publisher upload.

| Publisher stake | s\_coal (Option 5) | Expansion (Option 5) | Publisher upload (example) |
|---|---|---|---|
| Any value | ≈ 0.34 | ≈ 2.94× | ~311 MiB/s (32% staker) |

**The expansion factor is the same regardless of publisher identity.** Receiver
bandwidth is proportional to own stake and publisher-independent.

### Comparison: Sensitivity Summary

| | Option 2 | Option 3 | Option 4 | Option 5 |
|---|---|---|---|---|
| **Expansion depends on publisher?** | **Yes** (strongly with concentrated stake) | Mildly (via S') | **No** | **No** |
| **Receiver BW depends on publisher?** | **Yes** (strongly) | Mildly | **No** | **No** |
| **Publisher upload depends on publisher?** | **Yes** (strongly) | Mildly | **No** | **Yes** |
| **Expansion factor (32% pub)** | ~3.37× | ~2.96× | ~32.5× | ~2.94× |
| **Expansion factor (1% pub)** | **~32×** | ~3.00× | ~32.5× | ~2.94× |
| **Smallest staker (1%)** | ~16.6 MiB/s | ~13.7 MiB/s | 157.5 MiB/s | ~9.3 MiB/s |

---

## 9. The Greedy Shard Allocation Algorithm

### Description

The original proposal for Options 3 and 5 was an incremental algorithm:

1. Start with N−1 shards (one per receiver, identical to Option 1).
2. Compute the ratio `stake_i / num_shards_i` for each receiver.
3. Give the receiver with the highest ratio one additional shard.
4. Repeat from step 2 until the desired total T is reached.

This algorithm greedily converges toward proportional allocation (shards ∝ stake)
as T grows.

### The Convergence Problem (Deprecated Design)

**The starting point (T = N−1) had BETTER expansion than the proportional limit
(T → ∞)** under the *deprecated* design where publisher stake was "free" (Option 4
old design). With "free" stake, tiny coalitions of receivers could qualify.

### Implication

The greedy algorithm as described (using real stake for the ratio) was
**counterproductive** for the expansion factor under that deprecated design. Each
additional shard initially went to large stakers, making the overall expansion worse.

**Note:** For Options 2, 3, 4, and 5 (current designs), there is no "free" stake. The
binding coalition threshold is ~1/3 of the relevant stake pool. The expansion is
already ~3× (Options 3, 5) or ~32.5× (Option 4, fixed K=2) at the design point and
stays stable. The greedy algorithm for Option 5 converges to proportional allocation
without the convergence pathology.

---

## 10. Modified Stake Allocation

### The Insight

Since thresholds are based on **real stake** (not shard count), the shard allocation
does **not** need to be proportional to real stake. We can use a **modified stake**
distribution (ms\_i) for shard allocation while still counting real stake (s\_i) for
reconstruction thresholds.

This decouples two concerns:
- **Security**: determined by real stake thresholds (fixed by the protocol).
- **Bandwidth distribution**: determined by shard allocation (tunable).

### The Optimization Problem (Deprecated Design)

The LP formulation applied to the *deprecated* design (proportional allocation with
"free" publisher stake). Choose ms\_i to maximize num\_data\_shards, subject to
qualifying coalitions under `Σ s_i + s_p ≥ S/3` over receivers.

**For Options 4 and 5 (current design), the "free" stake problem is gone.** The
publisher has its own shard(s); qualifying coalitions are over all N nodes with
≥ S/3 stake. Option 4 uses fixed allocation (K over all N); Option 5 uses proportional
allocation with s\_coal ≈ S/3. The modified stake optimization was aimed at the
deprecated design and is less relevant now.

### Why the Gap to Option 1 Differed (Historical)

Under the deprecated design, the publisher's high stake created many tiny qualifying
coalitions (any pair with combined stake ≥ 1.33% when s\_p = 32%). That drove expansion
to 50× or 64×.

**Current designs address this:**
1. **Option 3 (publisher excluded, proportional)** — threshold S'/3, expansion ~3×
   (nearly publisher-independent).
2. **Option 2 (publisher excluded, fixed)** — expansion ~3× only when the largest
   staker publishes; up to 64× otherwise (publisher-dependent).
3. **Option 4 (publisher in pool, fixed)** — K over all N nodes, no "free" stake.
   K = 2 for the example, expansion 32.5× (better than old 64×).
4. **Option 5 (publisher in pool, proportional)** — s\_coal ≈ S/3, expansion ~3×.
5. **Add re-gossip** or **tree-based gossip** — alternative architectural changes.

---

## 11. Fundamental Limits

### The Expansion Factor Lower Bound (Option 4)

For stake-weighted sharding where the publisher has its own shard (Option 4) and uses
full-mesh gossip:

- The binding coalition is over **all N nodes** with ≥ S/3 stake. No "free" stake.
- K = minimum number of nodes in any qualifying coalition. Each node has 1 shard.
- Expansion = N / K.

With publisher = 32%, second = 5%, rest ~1%:

- K = 2 (32% + 5% = 37% ≥ 33.3%).
- Expansion = 65/2 = **32.5×**.

**Key conclusion for Option 4:** The expansion factor is bounded by K, the minimum
coalition size over all N nodes. Options 4 and 5 no longer have "free" stake — the
publisher contributes its own shard(s) to the pool. Publisher-independent.

### The Expansion Factor Lower Bound (Option 3)

When the publisher is excluded from the stake distribution (Option 3), the "free"
stake mechanism is eliminated. Qualifying coalitions must accumulate genuine ≥ S'/3
from receiver stakes. Because shards are proportional to stake, the binding coalition
is measured by **stake** (s\_coal'), not member count:

- s\_coal' ≈ S'/3, expansion ≈ S'/s\_coal' ≈ 3×.
- The bound is **weakly publisher-dependent** (through S') but stays near ~3×.

With publisher = 32% and many ~1% validators:
- S' = 68%, S'/3 ≈ 22.7%.
- Option 3: s\_coal' ≈ 23%, expansion ≈ 2.96×.

### The Expansion Factor Lower Bound (Option 2)

Option 2 uses the same publisher-excluded threshold (S'/3) but with fixed allocation
(one shard per receiver). The binding coalition is measured by **member count** (K'),
not stake. This makes expansion strongly publisher-dependent:

- When the largest staker publishes: K' is large (many small receivers needed),
  expansion ≈ 3×.
- When a small staker publishes: a single high-stake receiver may exceed S'/3 alone,
  giving K' = 1 and expansion = N−1.

With publisher = 32%: K' = 19, expansion ≈ 3.37×.
With publisher = 5%: K' = 1, expansion = 64×.
With publisher = 1%: K' = 2, expansion = 32×.

### The Expansion Factor Lower Bound (Option 5)

When the publisher has its own shards (Option 5), there is no "free" stake.
Qualifying coalitions must accumulate genuine ≥ S/3 total stake from actual shards.

With **any** stake distribution:

- The minimum qualifying coalition has ≈ S/3 total stake (plus discrete rounding).
- s\_coal ≈ S/3, giving expansion ≈ 3×.
- This bound is **publisher-independent** and **nearly identical to Option 1**.

With publisher = 32% and many ~1% validators:
- s\_coal = 0.34, expansion ≈ 2.94×.

**Key conclusion for Options 2, 3, 4, and 5:** Options 4 and 5 no longer use "free"
stake. The publisher has its own shard(s). Expansion is publisher-independent. Option 4
has higher expansion (32.5×) due to fixed allocation; Option 5 achieves ~3× with
proportional allocation.

### The 315 MiB/s Constant (Revisited)

Any validator that can reconstruct the full message and participates in full-mesh gossip
will upload `M × (N−2)`. For N = 65, M = 5: this is 315 MiB/s. This is true regardless
of the option chosen and cannot be reduced without changing the gossip topology.

In Option 4 (32.5× expansion), receiver upload is 157.5 MiB/s — half of 315.

In Options 3 and 5, the ~3× expansion keeps shard sizes small; the bandwidth is
distributed according to the shard allocation strategy. Option 2 also achieves ~3×
expansion when the largest staker publishes, but expansion is publisher-dependent and
can reach 64× with other publishers (see §8).

---

## 12. The Gossip Property

Tendermint consensus relies on its network layer satisfying the **gossip property**: if
an honest peer delivers a message, all honest peers eventually deliver it. This section
summarizes how Propeller satisfies this property under the correct assumptions for
the redesigned Options 4 and 5.

For formal proofs of all options, see
[propeller_stake_weighted_sharding_proofs.md](propeller_stake_weighted_sharding_proofs.md).

### Propeller's Gossip Architecture

Propeller does **not** use re-gossip. Each shard has a single designated broadcaster,
and the `validate_origin` function rejects shards from any other sender. The only
propagation paths are:

1. Publisher → designated broadcaster (direct delivery of assigned shard).
2. Designated broadcaster → all N−2 other peers (one-hop gossip).
3. **Reconstruction cascade**: a peer who reconstructs the full message can extract and
   broadcast their own assigned shard, even if the publisher never sent it to them.

Path 3 is the cascade mechanism that makes the gossip property work.

### Shared Assumptions and Definitions

**Definition (Gossip Property).** A broadcast protocol satisfies the *gossip
property* if: whenever an honest node delivers a message m, every honest node
eventually delivers m.

**Assumption (Tendermint Stake Model).** There are N nodes with positive integer
stakes s₁, …, sₙ. Let S = Σ sᵢ denote the total stake. The set of Byzantine
(malicious) nodes B satisfies:

```
stake(B)  =  Σᵢ∈B sᵢ  <  S / 3
```

Equivalently, honest nodes control more than 2S/3 of the total stake:
stake(H) > 2S/3, where H = {1, …, N} \ B is the set of honest nodes.

**Notation.** sₚ denotes the publisher's stake. For a set of nodes X,
stake(X) = Σᵢ∈X sᵢ.

### Proof Sketches

Detailed proofs for all five options are in the
[proofs document](propeller_stake_weighted_sharding_proofs.md). The key ideas:

**Option 1 (Node Counting):** Standard doubling argument. Deliver at 2d ≥ build + f,
so at least d honest broadcasters exist. Cascade works.

**Options 2 and 3 (Publisher Excluded):** The receiver set excludes the publisher.
Build threshold is S'/3 receiver stake; receive threshold is 2S/3 − sₚ. Under the
standard Tendermint assumption (stake(B) < S/3), the doubling argument for the
Byzantine-publisher case yields honest sender stake ≥ S/3 > S'/3 — sufficient for
build. Cascade ensures all honest receivers deliver. In the honest-publisher case,
honest receivers accumulate > 2S/3 − sₚ = receive threshold. For Option 3, the
argument is analogous with proportional allocation.

**Option 4 (Stake Counting, Fixed):** Publisher has its own shard and broadcasts it to
all N−1 receivers. Build/receive thresholds are over all N nodes with ≥ S/3 and ≥ 2S/3.
Standard doubling argument: deliver at ≥ 2S/3 total stake, so honest stake ≥ S/3. The
publisher's shard is either received from the publisher (honest) or can be reconstructed
via cascade. No "free" stake — the publisher contributes actual shard data.

**Option 5 (Stake Counting, Proportional):** Publisher has its own shards and broadcasts
them directly to all N−1 receivers. No delegation. Build/receive thresholds over all N
nodes. Same argument as Option 4 — publisher's shards are part of the pool. Two cases
(publisher honest/Byzantine) with broadcast stake accounting.

### Why Node Counting Breaks with Stake Assumptions

The node-counting proof uses f ≤ d in Step 1 to bound the number of malicious senders.
Under the stake assumption (< 1/3 of stake is malicious), the **number** of malicious
nodes is unbounded — an attacker can create many low-stake identities.

Example: 30 malicious nodes each with 0.5% stake (total 15% < 33.3%). Among A's 2d = 42
shard-senders, up to 30 could be malicious. Honest senders: 42 − 30 = 12. But d = 21.
12 < 21, so honest peers cannot build. The cascade fails.

This is the fundamental reason Propeller must move to stake-weighted thresholds to align
with Tendermint's security model.

### Implication for Expansion

The gossip property proof establishes that stake-weighted security aligns with
Tendermint's model. In Options 4 and 5, the publisher has its own shard(s) and
broadcasts them directly. There is no "free" stake — the publisher contributes
actual shard data. The build threshold is met when qualifying coalitions (≥ S/3
stake over all N nodes) have received enough shards.

The design space:

1. **Option 3 (publisher excluded, proportional)** — exclude the publisher from the
   stake distribution. Thresholds against S'. Expansion ~3× (publisher-independent).
2. **Option 2 (publisher excluded, fixed)** — same as Option 3 but with fixed
   allocation. Expansion ~3× only when the largest staker publishes; **up to 64×
   when a small staker publishes** (publisher-dependent).
3. **Option 4 (publisher in pool, fixed)** — K over all N nodes. Expansion ~32.5×
   for the example. Publisher broadcasts its own shard to all receivers.
4. **Option 5 (publisher in pool, proportional)** — s\_coal over all N nodes.
   Expansion ~3×. Publisher broadcasts its own shards directly. No delegation.
5. **Add re-gossip** or **tree-based gossip** — alternative architectural changes.

---

## 13. Comparison Table

Worst case: publisher = 32% staker, second-largest = 5%, rest ≈ 1%.

### Full Comparison (Five Options)

| | Option 1 | Option 2 | Option 3 | Option 4 | Option 5 |
|---|---|---|---|---|---|
| **Security model** | < 1/3 nodes | < 1/3 stake | < 1/3 stake | < 1/3 stake | < 1/3 stake |
| **Aligns with Tendermint** | No | Yes | Yes | Yes | Yes |
| **Sybil-resistant** | No | Yes | Yes | Yes | Yes |
| **Publisher in shard pool** | N/A | **No** (excluded) | **No** (excluded) | **Yes** (own shard) | **Yes** (own shards) |
| **Expansion factor** | 3.05× | **~3.37×** | **~2.96×** | 32.5× | **~2.94×** |
| **Publisher upload** | ~15.2 | **~16.8** | **~14.8** | 320 | **~311** |
| **Max receiver upload** | ~15.0 | **~16.6** | **~68.5** (5%) | 157.5 | **~46.3** (5% staker) |
| **Min receiver upload** | ~15.0 | **~16.6** | **~13.7** (1%) | 157.5 | **~9.3** (1% staker) |
| **Bandwidth distribution** | Uniform | Uniform | ∝ stake | Uniform | ∝ stake |
| **Publisher-indep. expansion** | Yes | **No** (see §8) | **Yes** (indirect) | **Yes** | **Yes** |
| **Publisher-indep. receiver BW** | Yes | **No** (see §8) | **Yes** (indirect) | **Yes** | **Yes** |
| **Implementation complexity** | Existing | Low | Moderate | Moderate | Higher |

All bandwidth values in MiB/s.

### Key Observations

1. **Option 3 is a practical solution with full Tendermint alignment.** It achieves
   ~3× expansion by excluding the publisher from the stake distribution. The receive
   threshold is 2S/3 − sₚ, which works under the standard Tendermint assumption.
   Option 3 adds proportional bandwidth at the cost of moderate implementation work.
   **Option 2 is simpler but has a critical flaw: expansion is strongly
   publisher-dependent** — when a small staker publishes, a large-stake receiver
   can form a 1-member qualifying coalition (K' = 1), causing expansion up to 64×.
   This makes Option 2 impractical for concentrated stake distributions.

2. **Options 4 and 5 put the publisher in the shard pool.** The publisher has its own
   shard(s) and broadcasts them directly to all N−1 receivers. No "free" stake, no
   delegation. Both have publisher-independent expansion. Option 4: 32.5× expansion,
   uniform receiver upload. Option 5: ~3× expansion, proportional receiver upload.

3. **Option 5 has publisher-independent expansion and receiver bandwidth.** Publisher
   upload is publisher-dependent (~311 MiB/s for 32% staker). The receive threshold
   (2S/3) does not depend on the publisher's stake.

4. **Option 3 gives proportional bandwidth** — ~13.7 MiB/s for a 1% staker vs ~68.5
   MiB/s for a 5% staker, because proportional allocation assigns shards ∝ stake.

---

## 14. Open Questions

### Publisher Stake for Build Threshold (Resolved)

**Options 2 and 3:** The publisher is excluded from the stake distribution. Thresholds
are against S' = S − sₚ. The gossip property proof uses a standard doubling argument.

**Options 4 and 5:** The publisher has its own shard(s) and broadcasts them directly.
There is no "free" stake — the publisher contributes actual shard data to the pool.
The build threshold is met when qualifying coalitions (≥ S/3 over all N nodes) have
received enough shards.

### Should We Use Option 3?

Option 3 offers a simpler path to stake-weighted security than Option 5:

**Arguments in favor:**
- ~3× expansion, comparable to Option 1, and nearly publisher-independent.
- No need for publisher to hold/broadcast shards.
- Proportional bandwidth distribution (fair to all validators).
- Fully aligns with Tendermint (< 1/3 stake assumption) — no strengthened assumption
  needed. The receive threshold 2S/3 − sₚ is exactly what honest receivers can
  guarantee.

**Arguments against / open concerns:**
- The receive threshold is publisher-dependent (2S/3 − sₚ). Higher-stake publishers
  lower the delivery bar, reducing redundancy margin. For a 32% publisher, the
  build-to-receive ratio is 3:2 rather than the symmetric 2:1.
- The receiver set changes when a different node publishes, creating a mild indirect
  publisher dependence.
- More complex implementation than Option 2 (proportional shard allocation, need to
  choose T).

### Should We Use Option 2?

Option 2 is the simplest stake-aware upgrade (same allocation as Option 1, different
thresholds), but it has a critical limitation:

**Arguments in favor:**
- Simplest implementation — same shard allocation as Option 1, just different thresholds.
- Aligns with Tendermint (< 1/3 stake assumption).

**Arguments against / critical concerns:**
- **Expansion is strongly publisher-dependent** with concentrated stake. When a small
  staker publishes, a single high-stake receiver can form a 1-member qualifying coalition
  (K' = 1), causing expansion up to 64× and bandwidth up to 320 MiB/s. This makes
  Option 2 impractical unless the stake distribution is relatively uniform.
- Uniform bandwidth (not proportional to stake) — small stakers pay the same as large
  stakers.

### Should We Use Option 5?

Option 5 appears to be a strong candidate for full Tendermint alignment:

**Arguments in favor:**
- ~3× expansion, comparable to Option 1.
- Full stake-weighted security, aligned with Tendermint.
- Publisher-independent expansion and receiver bandwidth.
- Proportional bandwidth distribution (fair to all validators).
- No "free" stake mechanism; publisher broadcasts own shards directly.

**Arguments against / open concerns:**
- Publisher upload is publisher-dependent (~311 MiB/s for 32% staker) — higher than
  Options 2 and 3.
- Implementation requires publisher to broadcast its own shards to all N−1 receivers.
- Higher implementation complexity than Options 2 or 3.

### Can Tree-Based Gossip Replace Full-Mesh Gossip?

Full-mesh gossip requires each validator to send their shard(s) to all N−2 peers. A
tree-based topology (like Solana's Turbine) could reduce this to O(log N) sends per
shard, drastically reducing the N−2 multiplier. The trade-off: tree topology introduces
new failure modes if interior nodes are malicious.

This optimization is **orthogonal** to the choice of Options 1–5 and could be applied
on top of any option.

### What is the Optimal Modified Stake Distribution? (Partially Resolved)

The LP formulation in Section 10 gives a theoretical optimum for Option 4.

**For Options 2, 3, and 5, this question is less critical.** The expansion factor is
already ≈ 3× without any allocation optimization.

### Should Propeller Add Re-gossip?

Adding re-gossip (accepting and forwarding shards from any peer, not just the designated
broadcaster) would allow excluding publisher stake from the build threshold in Option 4,
reducing worst-case expansion. However, Options 2, 3, and 5 may make re-gossip less
necessary by achieving ~3× expansion through other means.

### Implementation Considerations for Option 2

Implementing Option 2 requires minimal changes to the current Propeller codebase:

1. **Threshold calculation (`tree.rs`):** Instead of `num_data_shards = floor((N-1)/3)`,
   compute K' based on non-publisher stake. Sort validators by stake descending and
   find the smallest k such that their combined stake ≥ S'/3.
2. **Build/receive thresholds:** Track accumulated receiver stake (excluding publisher).
   Build at ≥ S'/3, receive at ≥ 2S/3 − sₚ.
3. **No changes to shard allocation** — still 1 shard per receiver.
4. **No changes to origin validation** — same `validate_origin` logic.

### Implementation Considerations for Option 3

Option 3 requires moderate changes:

1. **Shard allocation:** Allocate T shards proportionally to non-publisher stake among
   N−1 receivers. Each receiver gets `round(s_i × T / S')` shards (at least 1).
2. **Threshold calculation:** Same as Option 2 but with s\_coal' instead of K'.
3. **Reed-Solomon parameters:** Total shard count T, data shards ≈ s\_coal' × T / S'.
4. **Origin validation:** May need updates if receivers hold multiple shards.

### Implementation Considerations for Option 5

Implementing Option 5 requires the following changes to the current Propeller codebase:

1. **Shard assignment (`tree.rs`):** The publisher must be included in the shard pool.
   Create T shards over all N nodes (including publisher), allocated proportionally to
   stake. Each node (including publisher) gets `round(s_i × T / S)` shards (at least 1).

2. **Distribution:** The publisher sends each receiver's shards to that receiver. The
   publisher **broadcasts its own shards** to all N−1 receivers (no delegation).

3. **Origin validation (`tree.rs`):** The `validate_origin` function accepts shards
   from the publisher (for any recipient) or the designated broadcaster. No cascade
   enhancement.

4. **Erasure coding (`sharding.rs`, `reed_solomon.rs`):** Total shard count changes
   from N−1 to T based on all N nodes.

5. **Threshold checking:** Build and receive thresholds credit only received shards.
   No "free" stake — publisher's stake is backed by its own shards.

6. **Engine/channel registration (`engine.rs`):** The channel peer list already includes
   the publisher. The main change is in how the schedule manager uses stake for shard
   assignment.
