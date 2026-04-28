# Dynamic L2 Gas Pricing: Adjusting Base Fee to STRK Price

## Problem

Starknet's L2 gas base fee is currently a manually adjusted value. Sequencer operating costs
(compute, proving, infrastructure) are denominated in USD, but users pay in STRK. When STRK price
moves:

- **STRK rises**: users overpay in USD terms, dApp budgets break
- **STRK drops**: sequencers receive less USD revenue while costs stay fixed

Today, the team manually updates `override_l2_gas_price_fri` or `min_l2_gas_price_per_height` in
the node config. This is infrequent, fully trusted, and reactive. The goal is to automate this so
the base fee tracks a USD cost target continuously.

This document surveys how the current system works, what constraints exist, what other chains do,
and what options are available for implementation. It is intended as a starting point for design
discussions. See also [SNIP-35](#snip-35-summary) for the long-term decentralized proposal.

---

## How L2 Gas Pricing Works Today

### Fee market (EIP-1559 style)

The L2 gas price is tracked as internal state (`self.l2_gas_price`) in the consensus context. After
each block, it is updated by `calculate_next_l2_gas_price_for_fin()` in
`apollo_consensus_orchestrator/src/fee_market/mod.rs`:

1. If `override_l2_gas_price_fri` is set, return that value immediately (bypasses EIP-1559)
2. Look up `min_gas_price` from `min_l2_gas_price_per_height` config (or fall back to
   `VersionedConstants::min_gas_price` = 8 gwei)
3. If current price < min: gradually increase (max +0.3% per block)
4. Otherwise: standard EIP-1559 adjustment based on gas usage vs target (1.5B gas target,
   denominator 48 = ~2% max change per block)

The result is written into the block header as `next_l2_gas_price` and becomes the price for the
next block.

### Manual adjustment mechanisms

| Mechanism | How it works | Effect |
|---|---|---|
| `override_l2_gas_price_fri` | Config value, updated via config manager | Bypasses EIP-1559 entirely; returns this fixed value |
| `min_l2_gas_price_per_height` | Height-indexed list of minimum prices | Sets a floor; EIP-1559 can go higher but not lower |

Both are in `ContextDynamicConfig` and can be updated at runtime via the config manager (file
watch + hot reload).

### How L1 gas prices work (for comparison)

L1 gas prices use a different approach:

1. **L1 scraper** fetches Ethereum block headers, extracts `base_fee` and `blob_fee`
2. **L1 provider** computes a rolling mean over 300 blocks
3. **ETH/STRK oracle** (Pragma, HTTP endpoint) provides the conversion rate
4. The orchestrator converts L1 gas prices from wei to FRI using the oracle rate
5. **3-tier fallback**: fresh data -> previous block -> hardcoded defaults

The ETH/STRK oracle returns JSON: `GET /endpoint?timestamp=<u64>` ->
`{"price": "0x...", "decimals": 18}`. It has LRU caching, multi-URL failover, and configurable
timeouts.

---

## The Consensus Constraint

This is the critical constraint that any implementation must respect.

### L2 gas price: exact equality required

When a validator receives a proposal, it checks the L2 gas price with **exact equality**
(`validate_proposal.rs:281`):

```rust
init_proposed.l2_gas_price_fri == proposal_init_validation.l2_gas_price_fri
```

No margin, no tolerance. If two nodes compute different L2 gas prices, they reject each other's
proposals and consensus breaks.

### L1 gas prices: 10% margin

In contrast, L1 gas prices allow a tolerance (`validate_proposal.rs:305`):

```rust
within_margin(l1_gas_price_fri_proposed, l1_gas_price_fri, l1_gas_price_margin_percent)
// l1_gas_price_margin_percent = 10 (from versioned constants)
// Also allows absolute difference of 1 wei
```

### Why this matters

The L2 gas price (`self.l2_gas_price`) is **deterministic local state** derived from processing
blocks. All nodes process the same blocks with the same EIP-1559 formula, so they converge on the
same value. The `override_l2_gas_price_fri` is from shared config, so all nodes also agree on that.

If we introduce an oracle-derived floor that nodes compute independently, and two nodes get
different oracle responses (due to timing, transient failures, or cache state), their
`self.l2_gas_price` diverges. Once diverged, it stays diverged permanently because each subsequent
block's price depends on the previous one.

**Any implementation that feeds independently-fetched oracle data into the deterministic L2 gas
price calculation will break consensus.**

### What `next_l2_gas_price` in the Fin IS and ISN'T

The `ProposalFinPayload` contains `next_l2_gas_price_fri`, but this value is **not validated** by
the receiver. It is accepted as-is and used to set `self.l2_gas_price` for the next block. This
means:
- The proposer's calculated next price is accepted by all validators
- Validators don't independently verify the next price
- But the next block's `ProposalInit` uses this value, and THAT is checked with exact equality

So while the Fin payload itself doesn't cause rejection, any divergence in the next price will
surface as a consensus failure on the subsequent block.

---

## What Other Blockchains Do

### The universal pattern: avoid independent oracle fetches

Every major blockchain either (a) computes gas prices deterministically from on-chain state, or (b)
feeds external data through a shared deterministic channel. None allow nodes to independently fetch
from an oracle and hope they agree.

### Ethereum (EIP-1559)

- Execution gas base fee is a **pure function** of the parent block header (gas used vs gas target)
- Since Dencun (March 2024), there is a separate **blob gas** fee market for L2 data (EIP-4844),
  also deterministic from the parent block header. Pectra (May 2025) doubled the blob target.
- No oracle, no external input. A block with the wrong base fee is invalid
- Disagreement is impossible: every node computes the same value from the same parent block

### Solana

- Base fee is a **protocol constant** (5000 lamports per signature). SIMD-0096 proposed dynamic
  base fees; check whether it has shipped to mainnet
- Priority fees are user-set bids; there is no consensus-level minimum, but they are processed
  deterministically by the runtime
- No oracle needed

### Avalanche (C-Chain)

- Modified EIP-1559, **deterministic from parent block**
- As of the **Octane upgrade (ACP-176, April 2025)**: fee mechanism was overhauled with
  **dynamic validator-signaled gas targets** replacing the old static target. Validators signal
  preferred gas consumption rate; effective target converges to 50th percentile of stake-weighted
  votes
- Minimum base fee: **1 nAVAX** (reduced from 25 nAVAX by ACP-125 in Dec 2024; ACP-176 spec sets
  the theoretical floor at 1 wei)
- No oracle; still fully deterministic from on-chain state

### Arbitrum

- L2 execution: **Multi-window, multi-dimensional pricing** (since ArbOS Dia, Jan 2026). Replaced
  single-window EIP-1559 with 6 target-window pairs for burst dampening and long-run constraints.
  ArbOS 60 Elara (March 2026) tracks gas across 5 resource dimensions (compute, state access,
  state growth, history growth, calldata). Still **deterministic from L2 state**
- L1 data cost: derived from **batch posting reports** fed through the trustless Arbitrum inbox.
  When a batch is posted to L1, the contract sends a report into the delayed inbox with the L1
  basefee and cost. All nodes process the same inbox in the same order -> **deterministic**.
  Supports blob transactions (EIP-4844) since ArbOS 20
- On stale data: surplus/deficit tracker auto-adjusts fees. The equilibration mechanism dampens
  fluctuations and self-corrects when batch posting reports are delayed
- Neither chain has decentralized sequencing yet

### Optimism (OP Stack)

- L2 execution: **EIP-1559** foundation, extended by Isthmus (April 2025) with an **Operator Fee**
  component and by Jovian (Dec 2025) with a **configurable minimum base fee** and DA footprint
  block limit (ties DA usage into base fee updates)
- L1 data cost: L1 block headers are fed into L2 via **deposit transactions** by the L1 Attributes
  Depositor. Ecotone added blob-aware pricing with `blob_base_fee_scalar`; Fjord added FastLZ
  compression estimation for per-transaction sizing
- Every verifier derives the same L2 state from the same L1 data -> **deterministic**
- If the sequencer loses L1 connection, it **continues producing blocks** with larger epochs
  (more L2 blocks per L1 block), bounded by `max_sequencer_drift`. "Unsafe" height advances while
  "safe" and "finalized" heights stall
- Neither chain has decentralized sequencing yet

### Cosmos / Slinky (dYdX v4)

- Validators submit price observations via **Vote Extensions** (ABCI++ feature)
- Block proposer aggregates using **stake-weighted median** from 2/3+ of voting power
- Result is committed to the block -> all nodes use the same aggregated value
- If not enough validators report: price stays at last known value (stale). dYdX experienced a
  real-world stale-oracle incident in October 2024 after a chain halt, causing incorrect
  liquidations
- If a validator's oracle is down: it submits empty vote extensions, continues in consensus
- Stake-weighted median resists price manipulation up to ~1/2 of stake; <1/3 malicious stake
  cannot disrupt oracle updates (standard BFT guarantee)
- Note: Skip archived the upstream "Connect" (formerly Slinky) project in March 2025. dYdX
  maintains its own active fork (`dydxprotocol/slinky`)

### Summary table

| Chain | Price source | External oracle? | Disagreement prevention |
|---|---|---|---|
| Ethereum | Deterministic (parent block) | No | Same computation, same result |
| Solana | Protocol constant + user bids | No | Nothing to disagree on |
| Avalanche | Deterministic (parent block + validator signals) | No | Same computation, same result |
| Arbitrum | Deterministic L2 state + L1 inbox reports | No (L1 data is deterministic) | Same inbox, same derivation |
| Optimism | Deterministic L2 state + L1 deposit txs | No (L1 data is deterministic) | Same derivation pipeline |
| Cosmos/Slinky | Vote extensions -> stake-weighted median | Yes, but aggregated in consensus | Committed to block |

---

## Implementation Options

### Option A: External config updater (sidecar / cron job)

An external service fetches STRK/USD, computes `target_usd / strk_price`, and writes the result to
the shared node config as `override_l2_gas_price_fri`. The config manager pushes the same value to
all nodes.

**Pros:**
- Zero changes to consensus-critical code
- All nodes get the same value (shared config)
- Trivially reversible (stop the service, set the override manually)
- Works today with existing infrastructure

**Cons:**
- Another service to operate and monitor
- Latency: config file watch + reload cycle (seconds to minutes)
- Not suitable for the multi-sequencer future (centralized trust in the config service)
- The `override` bypasses EIP-1559 entirely, losing congestion-based pricing

### Option B: On-chain price feed (L2 contract)

Deploy a STRK/USD price feed contract on Starknet L2 (e.g., Pragma on-chain oracle). All nodes
read the price from L2 state, which is deterministic -- every node processing the same block sees
the same contract state.

The oracle-derived floor is computed from on-chain state and fed into the fee market as a dynamic
minimum.

**Pros:**
- Fully deterministic: all nodes read the same L2 state
- No consensus changes needed
- Decentralization-friendly (the oracle contract can be governed independently)
- Congestion-based pricing still works (EIP-1559 on top of the floor)

**Cons:**
- Circular dependency risk: the gas price depends on a contract whose execution costs gas at that
  price. Need to ensure the oracle update transaction can always be included.
- Oracle update latency: the on-chain price is only as fresh as the last oracle update transaction
- Requires an external service to push price updates to the contract (similar to Option A, but the
  trust is in the oracle contract, not the node config)
- Reading contract state during block production may add complexity to the proposal flow

### Option C: Embed in block header (SNIP-35 approach)

Each proposer includes a `Fee_proposal` value in the block header. `Fee_actual` (the effective
floor) is the median of the last 10 proposers' `Fee_proposal` values. Consensus enforces that
`Fee_proposal` is within +/-0.2% of the current `Fee_actual`.

**Pros:**
- Designed for multi-sequencer decentralization
- Resistant to minority manipulation (median requires 6/10 to agree)
- Resilient to oracle failures (proposer reports `Fee_actual` if oracle is down = freeze)
- No external dependency in the consensus path (the price is committed to the block)

**Cons:**
- Protocol change: new field in block header, changes block hash computation
- Slow convergence: a 50% price swing takes ~340+ blocks to fully track
- Requires a full node upgrade
- More complex implementation (median computation, initiation period, new validation rules)
- The 0.2% bound and 10-block window are parameters that need tuning

### Option D: Tolerance margin for L2 gas price validation

Add a margin (like L1's 10%) to the L2 gas price validation check. Then nodes can independently
fetch oracle data and compute slightly different floors without rejecting each other.

**Pros:**
- Relatively simple code change
- Allows independent oracle fetches

**Cons:**
- Divergence accumulates: if nodes use slightly different floors every block, `self.l2_gas_price`
  drifts apart over time. The margin would need to grow to accommodate, eventually becoming
  meaningless
- Does not solve the fundamental problem, just masks it temporarily
- Changes consensus validation rules (protocol change)

### Option E: Hybrid -- external config updater now, SNIP-35 later

Use Option A as an interim solution while SNIP-35 is developed. The external service automates what
the team does manually today. When SNIP-35 is approved and implemented, the external service is
retired and the in-protocol mechanism takes over.

**Pros:**
- Immediate value with zero consensus risk
- Clean migration path to the decentralized solution
- Each phase is independently useful

**Cons:**
- Two implementations over time (but the interim one is trivial)
- Interim solution doesn't benefit from congestion-based pricing (unless we use
  `min_l2_gas_price_per_height` instead of `override`)

---

## Option A+: External updater with EIP-1559 preserved

A refinement of Option A that preserves congestion-based pricing: instead of setting
`override_l2_gas_price_fri` (which bypasses EIP-1559), the external service writes to
`min_l2_gas_price_per_height` with the current block height + oracle-derived floor. This sets the
**floor** while allowing EIP-1559 to raise the price during congestion.

This requires knowing the current block height, which the service can get from the node's RPC.

**Pros:**
- Preserves EIP-1559 congestion pricing
- Oracle price acts as a floor, not a fixed override
- Still zero consensus code changes
- All nodes get the same floor via shared config

**Cons:**
- `min_l2_gas_price_per_height` is an append-only list that could grow unboundedly. May need
  cleanup logic or a rolling window.
- Need to handle race conditions between the service writing config and the node processing blocks

---

## Config Synchronization: A Pre-existing Risk

An important finding from exploring the codebase: **the current `min_l2_gas_price_per_height` and
`override_l2_gas_price_fri` mechanisms already have the same cross-node disagreement risk** we are
concerned about for oracle pricing. They work in practice because all nodes are operated by the same
team and share config -- not because the protocol guarantees agreement.

### How config updates propagate

1. Each node has its own config file on its own filesystem (K8s `ReadWriteOnce` volumes)
2. `ConfigManagerRunner` watches the local file via `notify` (filesystem events) with a 60-second
   polling fallback
3. On change, the runner calls `SetNodeDynamicConfig` which updates an `Arc<RwLock<>>` in the
   local node
4. The consensus context fetches the latest config at `set_height_and_round()` time

There is **no cross-node synchronization**:
- No shared filesystem
- No config epoch or versioning
- No consensus on which config applies at which height
- No mechanism to ensure all nodes apply a config change at the same block

### What this means

If an external service writes a new L2 gas price to two nodes' config files, node A might apply it
at block N while node B applies it at block N+2. During blocks N and N+1, the nodes disagree on the
L2 gas price.

In practice, this is mitigated by:
- Config changes being infrequent (manual, not per-block)
- The `override_l2_gas_price_fri` taking effect immediately with no gradual transition
- Block times being short (~2.6s), so the window of disagreement is brief
- All nodes being operated by the same team with coordinated deployments

For an automated system that updates the price frequently (e.g., every few minutes), this timing
gap becomes a real concern. The more frequently the price changes, the higher the probability of a
transient disagreement causing a consensus failure.

### Implication for option selection

This finding is relevant to Options A and A+ (external config updater):
- They inherit this pre-existing synchronization gap
- For infrequent updates (e.g., every 10-30 minutes), the risk is low (same as today)
- For frequent updates (e.g., every block), the risk is significant
- A safe middle ground: update every N minutes, ensuring N is much larger than the config
  propagation delay (~60s polling + processing time)

Options B (on-chain feed) and C (SNIP-35) avoid this entirely by making the price deterministic
from shared state.

---

## Safety Considerations

Regardless of which option is chosen, these safety properties should hold:

| Property | Mechanism |
|---|---|
| Never charge below USD cost target | Oracle floor = `target_usd / strk_price` |
| Bounded oracle error impact | Hard min/max clamps on the oracle-derived floor |
| Oracle failure resilience | Freeze at last known value; don't lower the price |
| Emergency manual control | `override_l2_gas_price_fri` always available as kill switch |
| Feature toggle | Config flag to disable and revert to manual mode |
| No consensus breakage | Price must reach nodes through a deterministic channel |

---

## Recommendation

Given the constraints, the options break down into two tiers:

**Available now (no protocol changes):**
- **Option A/A+**: External config updater. Automates what the team does manually. Zero consensus
  risk. Works because all nodes share config from the same operator. Use `override_l2_gas_price_fri`
  for simplicity (Option A) or `min_l2_gas_price_per_height` to preserve EIP-1559 (Option A+).
  Update frequency should be conservative (every 10-30 minutes) to stay well above the config
  propagation delay.

**Requires protocol work:**
- **Option C (SNIP-35)**: The proper decentralized solution. Needs a block header change, new
  validation rules, and a full node upgrade. Worth investing in if multi-sequencer decentralization
  is on a near-term timeline.
- **Option B (on-chain feed)**: A middle ground that gives determinism without protocol changes to
  the block header. Worth exploring if the circular dependency can be resolved cleanly.

The pragmatic path is **Option A+ now, Option C later**. The external updater provides immediate
value and can be built in days. SNIP-35 provides the long-term architecture and can be developed in
parallel.

---

## SNIP-35 Summary

SNIP-35 proposes the long-term decentralized solution (Option C above). Key parameters:

- Each proposer publishes `Fee_proposal` in the block header
- `Fee_actual` = median of last 10 `Fee_proposal` values (average of 5th and 6th)
- Consensus enforces: `Fee_actual / 1.002 <= Fee_proposal <= Fee_actual * 1.002`
- On oracle failure: `Fee_proposal = Fee_actual` (freeze)
- Initiation: first 10 blocks use the pre-SNIP base fee
- Convergence: ~6 blocks for the median to start moving; ~340+ blocks for a 50% price change

See the full SNIP for rationale and security analysis.

---

## Open Questions

### Oracle source and availability

1. Which STRK/USD oracle endpoint should we use? Pragma has been confirmed to provide it in the same
   format as the existing ETH/STRK oracle. What is the expected uptime SLA? How often is the price
   updated?

2. Should we support multiple oracle sources (Pragma + CEX APIs as fallback)? The existing
   ETH/STRK oracle client already supports multi-URL failover.

3. Can we derive STRK/USD from existing data? We already have ETH/STRK. If we add ETH/USD (from
   the L1 base fee or a separate feed), we get STRK/USD = ETH/USD / ETH/STRK. This avoids adding
   a new oracle dependency but compounds errors from two feeds.

### Consensus and multi-node behavior

4. For Option A/A+ (external config updater): how do we ensure all nodes receive the config update
   atomically? If node 1 gets the new floor at block N and node 2 gets it at block N+1, will this
   cause a transient disagreement?

   **Partial answer from research**: Config propagation is NOT atomic. Each node watches its own
   file independently with a 60-second polling fallback. Config changes applied at
   `set_height_and_round()` time. A safe mitigation: update infrequently (every 10-30 minutes) so
   the propagation delay (~60s) is a small fraction of the update interval.

5. For Option B (on-chain feed): how do we handle the circular dependency? The oracle update
   transaction pays gas at the price that depends on the oracle. Can we exempt oracle updates from
   the dynamic floor, or use a system transaction?

6. For Option C (SNIP-35): the 0.2% per-block cap means large price swings take hundreds of blocks
   to track. Is this acceptable? What is the worst-case USD loss during convergence on a sudden
   STRK crash?

7. Should we add a tolerance margin to L2 gas price validation regardless of which option we
   choose? L1 gas prices already allow 10%. Even a small margin (1-2%) for L2 would make the
   system more resilient to transient differences.

   **Note from research**: `min_l2_gas_price_per_height` already has this same disagreement risk
   today. It works because updates are infrequent and coordinated. Adding a margin would make the
   system strictly more robust, though it is a consensus rule change.

### Economics and parameters

8. What is the current USD cost target per L2 gas unit? This determines the `target_usd_per_l2_gas`
   config value and is the anchor for all oracle-derived price calculations.

9. What min/max bounds should we set on the oracle-derived floor? The min prevents charging too
   little if the oracle reports an absurdly high STRK/USD. The max prevents charging too much if
   the oracle reports an absurdly low STRK/USD. What are reasonable values?

10. How do we handle the transition? When switching from manual override to dynamic pricing, the
    current price and the oracle-derived price may differ. Should we ramp gradually or switch
    instantly?

### Implementation sequencing

11. Should we implement Option A/A+ first (immediate value, zero risk) and Option C (SNIP-35) later?
    Or invest directly in Option C if the timeline is short?

12. If we go with Option A+, should the external service be a Python script, a Rust binary, or
    integrated into an existing monitoring/deployment tool?

13. For Option C (SNIP-35), what is the expected timeline for approval and multi-sequencer
    deployment? This affects whether an interim solution is worth building.
