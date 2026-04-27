# Propeller Stake-Weighted Sharding: Gossip Property Proofs

This document contains formal proofs for all five options. For each option we prove
two properties:

1. **Gossip property (Byzantine publisher)** — if a Byzantine publisher publishes a
   message m and at least one honest node delivers m, then all honest nodes eventually
   deliver m.
2. **Censorship resistance (honest publisher)** — f malicious peers cannot prevent
   acceptance of a message published by an honest publisher. That is: when the
   publisher is honest, all honest nodes eventually deliver m.

**Why two properties, not three.** When the publisher is honest, the gossip property's
"honest-publisher case" asks: "if honest node A delivered, do all honest nodes
deliver?" But since the publisher is honest and distributes all shards, A's delivery
is guaranteed — there is no scenario where an honest publisher publishes yet no honest
node delivers (that would be a censorship resistance failure). Therefore the
honest-publisher gossip property is equivalent to censorship resistance, and we
present them as a single proof.

The gossip property adds something beyond censorship resistance only in the
**Byzantine publisher** case: a Byzantine publisher selectively distributes shards
such that at least one honest node delivers; the question is whether this forces all
honest nodes to deliver.

All proofs use the **standard Tendermint stake assumption** (stake(B) < S/3).
No option requires a stronger assumption.

For the option descriptions and bandwidth analysis, see
[propeller_stake_weighted_sharding.md](propeller_stake_weighted_sharding.md).
For detailed implementation considerations, see
[propeller_stake_weighted_sharding_analysis.md](propeller_stake_weighted_sharding_analysis.md).

---

## Table of Contents

1. [Shared Assumptions and Definitions](#shared-assumptions-and-definitions)
2. [Propeller's Gossip Architecture](#propellers-gossip-architecture)
3. [Option 1 (Node Counting)](#option-1-node-counting)
4. [Option 2 (Publisher Excluded, Fixed Allocation)](#option-2-publisher-excluded-fixed-allocation)
5. [Option 3 (Publisher Excluded, Proportional Allocation)](#option-3-publisher-excluded-proportional-allocation)
6. [Option 4 (Stake Counting, Fixed Allocation)](#option-4-stake-counting-fixed-allocation)
7. [Option 5 (Stake Counting, Proportional Allocation)](#option-5-stake-counting-proportional-allocation)
8. [Why Options 4 and 5 Use Standard Doubling Arguments](#why-options-4-and-5-use-standard-doubling-arguments)
9. [Why Node Counting Fails Under Stake Assumptions](#why-node-counting-fails-under-stake-assumptions)

---

## Shared Assumptions and Definitions

**Definition (Gossip Property).** A broadcast protocol satisfies the *gossip
property* if: whenever an honest node delivers a message m, every honest node
eventually delivers m.

**Definition (Censorship Resistance).** A broadcast protocol is *censorship-resistant*
if: when an honest publisher publishes a message m, all honest nodes eventually
deliver m, regardless of Byzantine behaviour.

**Assumption (Tendermint Stake Model).** There are N nodes with positive integer
stakes s₁, …, sₙ. Let S = Σ sᵢ denote the total stake. The set of Byzantine
(malicious) nodes B satisfies:

```
stake(B)  =  Σᵢ∈B sᵢ  <  S / 3
```

Equivalently, honest nodes control more than 2S/3 of the total stake:
stake(H) > 2S/3, where H = {1, …, N} \ B is the set of honest nodes.

**Assumption (No Single Node ≥ 1/3).** For all nodes i: sᵢ < S/3. This is a
standard Tendermint operational requirement — any node with ≥ S/3 stake could
unilaterally halt consensus.

**Notation.**

| Symbol | Meaning |
|--------|---------|
| sₚ | Publisher's stake |
| S' | Non-publisher stake: S' = S − sₚ |
| B | Set of Byzantine nodes |
| B\_r | Byzantine receivers: B\_r = B ∩ {receivers} |
| H | Set of honest nodes |
| H\_r | Honest receivers: H\_r = H ∩ {receivers} |
| stake(X) | Σᵢ∈X sᵢ for any set X |

**Key derived facts** (used throughout):

- sₚ < S/3 (from "no single node ≥ 1/3").
- S' = S − sₚ > S − S/3 = 2S/3.
- When publisher ∈ B: stake(B\_r) = stake(B) − sₚ < S/3 − sₚ.
- When publisher ∉ B: stake(B\_r) = stake(B) < S/3.
- stake(H\_r) = S' − stake(B\_r).

---

## Propeller's Gossip Architecture

Propeller does **not** use re-gossip. Each shard has a single designated broadcaster,
and the `validate_origin` function rejects shards from any other sender. The only
propagation paths are:

1. Publisher → designated broadcaster (direct delivery of assigned shard).
2. Designated broadcaster → all N−2 other peers (one-hop gossip).
3. **Reconstruction cascade**: a peer who reconstructs the full message can extract and
   broadcast their own assigned shard, even if the publisher never sent it to them.

Path 3 is the cascade mechanism that makes the gossip property work.

**Honest publisher behaviour.** When the publisher is honest, it distributes every
shard to the correct designated broadcaster. Every honest receiver receives their
assigned shard directly from the publisher and broadcasts it to all N−2 peers.

**Byzantine publisher behaviour.** When the publisher is Byzantine, it may distribute
shards selectively, send corrupted shards, or not distribute at all.

---

## Option 1 (Node Counting)

### Setup

Each of the N−1 receivers gets exactly one shard. `num_data_shards` = d =
floor((N−1)/3). Build threshold: d shards. Receive threshold: 2d shards.

**Assumption:** At most f < N/3 nodes are malicious. (This is a node-counting
assumption, not a stake assumption. We include Option 1 for completeness.)

### Proof 1: Gossip Property (Byzantine Publisher)

**Theorem.** If a Byzantine publisher's message is delivered by an honest peer A,
all honest peers eventually deliver it.

**Proof.** The publisher may have distributed shards selectively. But A delivered, so
A received ≥ 2d validated shards from their designated broadcasters.

**Step 1.** At most f ≤ d of A's senders are Byzantine. So ≥ 2d − d = d of A's
senders are honest.

**Step 2.** Those d honest senders broadcast their shard to all N−2 peers. Every
honest peer B receives all d shards. Since d = build threshold, B reconstructs.

**Step 3 (Cascade).** Each honest peer extracts their own shard and broadcasts it.

**Step 4.** After cascade, every honest peer has broadcast. Honest receivers ≥
N−1−f ≥ N−1−d ≥ 2d. Every honest peer C has ≥ 2d shards = receive threshold.
All deliver. ∎

---

### Proof 2: Censorship Resistance (Honest Publisher)

**Theorem.** f malicious peers cannot prevent acceptance of a message published by
an honest publisher.

**Proof.** The honest publisher distributes all N−1 shards. Honest receivers (≥ N−1−f
≥ 2d) each broadcast their shard. Every honest peer receives these ≥ 2d shards,
meeting the receive threshold. Byzantine peers can suppress at most f ≤ d shards,
which is insufficient to prevent any honest peer from reconstructing (needs d) or
delivering (needs 2d, and honest senders provide ≥ 2d). ∎

---

## Option 2 (Publisher Excluded, Fixed Allocation)

### Setup

Each of the N−1 receivers gets exactly one shard. Thresholds are based on **receiver
stake only** (publisher stake excluded). Let S' = S − sₚ.

Reed-Solomon uses K' data shards where:

```
K'  =  min |C|  over receiver sets C  with  stake(C) ≥ S'/3
```

Build threshold: accumulated receiver stake ≥ S'/3.
Receive threshold: accumulated receiver stake ≥ **2S/3 − sₚ**.

Note: 2S/3 − sₚ = 2S'/3 − sₚ/3. This is the maximum receiver stake that
Byzantine peers can deny under the standard Tendermint assumption when the
publisher is honest. The receive threshold is set to exactly match what honest
receivers are guaranteed to provide, eliminating any gap without requiring a
strengthened assumption.

### Proof 1: Gossip Property (Byzantine Publisher)

**Theorem.** Under the Tendermint stake assumption (stake(B) < S/3) and sₚ < S/3:
if a Byzantine publisher's message is delivered by an honest receiver A, all honest
receivers eventually deliver it.

**Proof.** Publisher ∈ B, so sₚ ≤ stake(B). Byzantine receivers:
stake(B\_r) = stake(B) − sₚ. The publisher may distribute shards selectively.

**Step 1 (Honest sender stake).** A delivered, so A received shards from a set Rₐ
of receivers with stake(Rₐ) ≥ 2S/3 − sₚ (receive threshold). Partition into honest
Rₐᴴ and Byzantine Rₐᴮ:

```
stake(Rₐᴮ)  ≤  stake(B_r)  =  stake(B) − sₚ  <  S/3 − sₚ
```

Therefore:

```
stake(Rₐᴴ)  ≥  (2S/3 − sₚ) − (S/3 − sₚ)  =  S/3
```

We need stake(Rₐᴴ) ≥ S'/3 for building. Since S' < S, we have S/3 > S'/3. ✓

**Step 2 (Build).** Honest senders Rₐᴴ each broadcast to all N−2 peers. Every
honest receiver B receives these shards. stake(Rₐᴴ) ≥ S/3 > S'/3, so by definition
of K', |Rₐᴴ| ≥ K'. B has ≥ K' shards and can reconstruct.

**Step 3 (Cascade).** Each honest receiver reconstructs and broadcasts their shard.

**Step 4 (Deliver).** After cascade, every honest receiver has broadcast.

```
stake(H_r)  =  S' − stake(B_r)
            =  S' − (stake(B) − sₚ)
            >  S' − (S/3 − sₚ)
            =  (S − sₚ) − S/3 + sₚ
            =  S − S/3
            =  2S/3
```

Receive threshold is 2S/3 − sₚ. Since stake(H\_r) > 2S/3 > 2S/3 − sₚ, every
honest receiver C accumulates stake exceeding the receive threshold. All deliver. ∎

---

### Proof 2: Censorship Resistance (Honest Publisher)

**Theorem.** Under the Tendermint stake assumption (stake(B) < S/3) and sₚ < S/3:
f malicious peers cannot prevent delivery of a message published by an honest
publisher.

**Proof.** Publisher ∉ B, so stake(B\_r) = stake(B) < S/3.

**Step 1.** The honest publisher distributes all N−1 shards to designated
broadcasters. Every honest receiver receives their shard and broadcasts it.

**Step 2 (Build).** Every honest receiver B receives shards from all honest
receivers. stake(H\_r) = S' − stake(B\_r) > S' − S/3 = 2S/3 − sₚ.

We need stake(H\_r) ≥ S'/3 for building:

```
2S/3 − sₚ  ≥  (S − sₚ)/3
⟺  2S − 3sₚ  ≥  S − sₚ
⟺  S  ≥  2sₚ
```

Since sₚ < S/3 < S/2, this holds. So |H\_r| ≥ K' and every honest receiver can
reconstruct. ✓

**Step 3 (Deliver).** Honest receivers accumulate stake(H\_r) > 2S/3 − sₚ =
receive threshold. (More precisely, stake(H\_r) is strictly greater than 2S/3 − sₚ
since stake(B) is strictly less than S/3.)

All honest receivers deliver. ∎

**No strengthened assumption needed.** The standard Tendermint assumption
(stake(B) < S/3) suffices for both build and delivery. The key insight: we set
the receive threshold to 2S/3 − sₚ — exactly matching what honest receivers are
guaranteed to provide — eliminating the gap that would arise with a 2S'/3
threshold.

**Summary of assumptions for Option 2:**

| Property | Assumption | Implied by Tendermint alone? |
|----------|-----------|----------------------------|
| Gossip (Byzantine pub) | stake(B) < S/3 | **Yes** |
| Censorship resistance — build | stake(B) < S/3 | **Yes** |
| Censorship resistance — deliver | stake(B) < S/3 | **Yes** |

---

## Option 3 (Publisher Excluded, Proportional Allocation)

### Setup

T total shards allocated proportionally to non-publisher stake among N−1 receivers.
Receiver i gets nᵢ ≈ round(sᵢ × T / S') shards (at least 1). Let S' = S − sₚ.

Binding coalition: s\_coal' = min { Σ sᵢ over subsets C ⊆ receivers : Σ sᵢ ≥ S'/3 }.
num\_data\_shards ≈ (s\_coal' / S') × T.

Build threshold: accumulated receiver stake ≥ S'/3.
Receive threshold: accumulated receiver stake ≥ **2S/3 − sₚ**.

Any receiver set with stake ≥ S'/3 holds ≥ num\_data\_shards shards (by proportional
allocation). For delivery, a receiver set with stake ≥ 2S/3 − sₚ holds approximately
((2S/3 − sₚ) / S') × T shards, which is sufficient since shards are proportional to
stake and honest receivers exceed this threshold.

### Proof 1: Gossip Property (Byzantine Publisher)

**Theorem.** Under the Tendermint stake assumption and sₚ < S/3: if a Byzantine
publisher's message is delivered by an honest receiver A, all honest receivers
eventually deliver it.

**Proof.** Identical structure to Option 2 Proof 1. Publisher ∈ B, so
stake(B\_r) = stake(B) − sₚ < S/3 − sₚ.

**Step 1.** A delivered with accumulated receiver stake ≥ 2S/3 − sₚ. Subtracting
Byzantine:

```
stake(Rₐᴴ)  ≥  (2S/3 − sₚ) − (S/3 − sₚ)  =  S/3  >  S'/3
```

**Step 2.** Since shards are proportional to stake, Rₐᴴ with stake ≥ S'/3
holds ≥ num\_data\_shards shards. Honest senders broadcast to all N−2 peers.
Every honest receiver B receives these shards and can reconstruct.

**Step 3 (Cascade).** Each honest receiver reconstructs and broadcasts all their
assigned shards.

**Step 4.** stake(H\_r) > 2S/3 > 2S/3 − sₚ = receive threshold. Since shards are
proportional to stake, honest receivers collectively hold more than
((2S/3 − sₚ) / S') × T shards. All deliver. ∎

---

### Proof 2: Censorship Resistance (Honest Publisher)

**Theorem.** Under the Tendermint stake assumption and sₚ < S/3: f malicious peers
cannot prevent delivery of a message published by an honest publisher.

**Proof.** Identical structure and conclusion to Option 2 Proof 2. Publisher ∉ B, so
stake(B\_r) = stake(B) < S/3.

**Step 1.** Honest publisher distributes all T shards. Honest receivers broadcast
all their assigned shards.

**Step 2 (Build).** stake(H\_r) > 2S/3 − sₚ > S'/3 (since sₚ < S/2). Honest
receivers hold proportionally ≥ S'/3 worth of shards ≥ num\_data\_shards. Every
honest receiver can reconstruct. ✓

**Step 3 (Deliver).** stake(H\_r) > 2S/3 − sₚ = receive threshold. Since shards
are proportional to stake, honest receivers hold enough shards for delivery.
All deliver. ∎

**Summary of assumptions for Option 3:**

| Property | Assumption | Implied by Tendermint alone? |
|----------|-----------|----------------------------|
| Gossip (Byzantine pub) | stake(B) < S/3 | **Yes** |
| Censorship resistance — build | stake(B) < S/3 | **Yes** |
| Censorship resistance — deliver | stake(B) < S/3 | **Yes** |

---

## Option 4 (Stake Counting, Publisher in Pool, Fixed Allocation)

### Setup

Every node — including the publisher — gets exactly one shard (N total shards). The
publisher distributes each receiver's shard and **broadcasts their own shard** to all
N−1 receivers. Each receiver gossips their shard to N−2 peers.

Build threshold: accumulated stake ≥ S/3 (publisher's stake counted only when
publisher's shard is received — no "free" stake). Receive threshold: ≥ 2S/3.

K = min |C| over subsets C ⊆ {all N nodes} with stake(C) ≥ S/3.
num\_data\_shards = K. K is publisher-independent.

### Proof 1: Gossip Property (Byzantine Publisher)

**Theorem.** Under the Tendermint stake assumption and sₚ < S/3: if a Byzantine
publisher's message is delivered by an honest node A, all honest nodes eventually
deliver it.

**Proof.** Publisher ∈ B, sₚ ≤ stake(B) < S/3. The publisher may distribute
selectively and may not broadcast their own shard honestly.

**Step 1 (Honest sender stake).** A delivered with accumulated stake ≥ 2S/3. The
publisher's shard may or may not have been received by A, but the publisher is
Byzantine so sₚ ≤ stake(B). All shards received from Byzantine senders have
Byzantine owner stake: stake(Rₐᴮ) ≤ stake(B) < S/3 (this includes the
publisher's shard if it was received from the publisher). Therefore:

```
stake(Rₐᴴ)  ≥  2S/3 − S/3  =  S/3
```

**Step 2 (Build).** Honest senders Rₐᴴ each broadcast their shard to all peers.
Every honest node B receives these shards. stake(Rₐᴴ) ≥ S/3 = build threshold.
Since each node holds 1 shard and the minimum coalition with stake ≥ S/3 has K
members, |Rₐᴴ| ≥ K. B has ≥ K shards and can reconstruct. ✓

**Step 3 (Cascade).** Honest nodes reconstruct and broadcast their own shards.

**Step 4 (Deliver).** After cascade, every honest node has broadcast. stake(H) > 2S/3.
Every honest node C receives shards from all honest nodes:

```
C's accumulated stake  ≥  stake(H)  >  2S/3  =  receive threshold
```

All honest nodes deliver. ∎

---

### Proof 2: Censorship Resistance (Honest Publisher)

**Theorem.** Under the Tendermint stake assumption and sₚ < S/3, f malicious peers
cannot prevent acceptance of a message published by an honest publisher.

**Proof.** Publisher ∉ B, so stake(B) < S/3. Let B\_r = B (all Byzantine nodes are
receivers since publisher is honest). stake(B\_r) = stake(B) < S/3.

**Step 1.** The honest publisher distributes all N−1 receiver shards and broadcasts
their own shard to all N−1 receivers. Every honest receiver receives their own shard
and the publisher's shard.

**Step 2 (Build).** Honest receivers broadcast their shards to all peers. Every
honest node B receives shards from all honest nodes (honest receivers + honest
publisher). The honest nodes' combined stake:

```
stake(H)  =  S − stake(B)  >  S − S/3  =  2S/3
```

This includes both honest receivers' shards and the publisher's shard. Since
stake(H) > 2S/3 > S/3 = build threshold, B can reconstruct. ✓

**Step 3 (Deliver).** stake(H) > 2S/3 = receive threshold. Every honest node
delivers. ∎

**Note:** Unlike Options 2 and 3 (where the publisher has no shard), here the
publisher actively participates as a broadcaster. The proof is a standard doubling
argument because the publisher's stake is backed by actual shard data.

---

## Option 5 (Stake Counting, Publisher in Pool, Proportional Allocation)

### Setup

T total shards allocated proportionally to stake for all N nodes (including publisher).
The publisher holds nₚ ≈ sₚ T / S shards and **broadcasts them directly** to all N−1
receivers. Each receiver broadcasts their own shards to N−2 peers.

Build threshold: accumulated stake ≥ S/3. Publisher's stake is credited only when
publisher's actual shards are received (no "free" stake). Receive threshold: ≥ 2S/3.

### Proof 1: Gossip Property (Byzantine Publisher)

**Theorem.** Under the Tendermint stake assumption and sₚ < S/3: if a Byzantine
publisher's message is delivered by an honest node A, all honest nodes eventually
deliver it.

**Proof.** sₚ ≤ stake(B) < S/3. The publisher may distribute selectively or not at
all, and may not broadcast their own shards.

Every shard has an *owner* (whose stake it represents) and a *broadcaster* (who
gossips it). In Option 5 (no delegation), each node broadcasts only their own shards.
The publisher broadcasts the publisher's shards; each receiver broadcasts their own.
Therefore, every shard broadcast by a Byzantine node has a Byzantine owner:

```
owner stake broadcast by Byzantine nodes  ≤  stake(B)  <  S/3
```

**Step 1 (Build).** A delivered with accumulated owner stake ≥ 2S/3. Subtracting
Byzantine: owner stake from honest broadcasters ≥ 2S/3 − S/3 = S/3. Every honest
node B receives these (honest broadcasters gossip to all). B's accumulated owner
stake ≥ S/3 = build threshold. By the proportional allocation property, B can
reconstruct. ✓

**Step 2 (Cascade and delivery).** After reconstruction, honest nodes broadcast
their assigned shards. Since the publisher is Byzantine, its stake is part of the
< S/3 budget. Honest nodes' own shards carry stake(H) > 2S/3. Every honest node C
receives shards from all honest nodes:

```
C's accumulated owner stake  ≥  stake(H)  >  2S/3  =  receive threshold
```

All honest nodes deliver. ∎

---

### Proof 2: Censorship Resistance (Honest Publisher)

**Theorem.** Under the Tendermint stake assumption and sₚ < S/3, f malicious peers
cannot prevent acceptance of a message published by an honest publisher.

**Proof.** Publisher ∉ B, so stake(B\_r) = stake(B) < S/3. Publisher distributes all
receiver shards and broadcasts all publisher shards to N−1 receivers.

**Step 1.** Every honest receiver receives their shard from the publisher and
broadcasts it to N−2 peers. The honest publisher broadcasts their own shards to all
N−1 receivers.

**Step 2 (Build).** Every honest node B receives:
- Shards from all honest receivers: owner stake = stake(H\_r) = S' − stake(B\_r) > S' − S/3 = 2S/3 − sₚ.
- Shards from the honest publisher: owner stake = sₚ.

Total owner stake > (2S/3 − sₚ) + sₚ = 2S/3 > S/3 = build threshold. B can
reconstruct. ✓

**Step 3 (Deliver).** Total owner stake > 2S/3 = receive threshold. All honest
nodes deliver. ∎

**Note:** No cascade enhancement is needed for Option 5. Since the publisher
broadcasts their own shards directly to all receivers, honest receivers receive the
publisher's shards without needing any intermediary. The proof is a straightforward
doubling argument.

---

## Why Options 4 and 5 Use Standard Doubling Arguments

In Options 4 and 5, the publisher holds their own shard(s) and broadcasts them
directly. There is no "free" stake — the publisher's stake is backed by actual shard
data. This means the proofs follow the standard doubling argument:

- **Byzantine publisher:** The publisher's stake is part of stake(B) < S/3. All shards
  from Byzantine nodes (including the publisher's own shards) carry at most S/3 owner
  stake. Subtracting from 2S/3 gives S/3 honest stake — sufficient for build and cascade.
- **Honest publisher:** The publisher broadcasts their own shards to all receivers.
  Combined with honest receivers' shards, honest nodes accumulate > 2S/3 owner stake.

**How Options 2 and 3 differ.** Options 2 and 3 exclude the publisher entirely from the
stake distribution. Thresholds are against S' = S − sₚ. The build threshold is S'/3;
the receive threshold is 2S/3 − sₚ. The doubling argument works against S'/3 for the
Byzantine-publisher case. In the honest-publisher case, the receive threshold 2S/3 − sₚ
is exactly what honest receivers can guarantee.

---

## Why Node Counting Fails Under Stake Assumptions

The node-counting proof bounds malicious senders by f ≤ d. Under the stake assumption
(< 1/3 of stake is malicious), the **number** of malicious nodes is unbounded — an
attacker can create many low-stake identities.

Example: 30 malicious nodes each with 0.5% stake (total 15% < 33.3%). Among A's 2d = 42
shard-senders, up to 30 could be malicious. Honest senders: 42 − 30 = 12. But d = 21.
12 < 21, so honest peers cannot build. The cascade fails.

This is the fundamental reason Propeller must move to stake-weighted thresholds to align
with Tendermint's security model.

---

## Summary of Assumptions

| Option | Gossip (Byzantine publisher) | Censorship — build | Censorship — deliver |
|--------|-----------------------------|--------------------|---------------------|
| 1 | f < N/3 nodes ✓ | f < N/3 nodes ✓ | f < N/3 nodes ✓ |
| 2 | stake(B) < S/3 ✓ | stake(B) < S/3 ✓ | stake(B) < S/3 ✓ |
| 3 | stake(B) < S/3 ✓ | stake(B) < S/3 ✓ | stake(B) < S/3 ✓ |
| 4 | stake(B) < S/3 ✓ | stake(B) < S/3 ✓ | stake(B) < S/3 ✓ |
| 5 | stake(B) < S/3 ✓ | stake(B) < S/3 ✓ | stake(B) < S/3 ✓ |

All five options satisfy both the gossip property and censorship resistance under
the standard Tendermint stake assumption (stake(B) < S/3). No option requires a
strengthened assumption.

**Key design choice for Options 2 and 3:** The receive threshold is set to
2S/3 − sₚ (rather than 2S'/3 = 2S/3 − 2sₚ/3). This exactly matches what honest
receivers can guarantee under the standard Tendermint assumption, eliminating the
sₚ/3 gap that would arise with a 2S'/3 threshold. The build-to-receive ratio is
(2S/3 − sₚ) / (S'/3) = (2S − 3sₚ) / (S − sₚ), which ranges from 2:1 (at sₚ = 0)
to 3:2 (at sₚ = S/3). This provides meaningful erasure-coding benefit across all
practical publisher stakes.

**Key design choice for Options 4 and 5:** The publisher holds their own shard(s) and
broadcasts them directly to all N−1 receivers. This eliminates "free" stake and
makes the proofs follow the standard doubling argument. No cascade enhancement is
needed (unlike the old delegated-publisher design) because the publisher broadcasts
their own shards directly. The trade-off is higher publisher upload: the publisher
must both distribute non-publisher shards and broadcast their own shards.
