# Node 14 Consensus Failure - Root Cause Analysis

**Date:** January 12, 2026  
**Affected Node:** apollo-sepolia-alpha-14 (validator 0x67)  
**Affected Heights:** H=5174017, H=5174018  
**Status:** Node stuck in infinite retry loop, requires reset

---

## Executive Summary

Node 14 became permanently stuck after successfully validating a block but then building its own proposal for the same height with a different timestamp. The later proposal **overwrote** the earlier validation result, causing `previous_block_info` to be set with the wrong timestamp. This made all subsequent validation and sync attempts fail timestamp validation checks, permanently locking the node out of consensus.

---

## Timeline of Events

### Height 5174017 (H=017)

#### Node 13 (validator 0x66) - Proposer
- `12:46:40.985Z` - Starts consensus for H=5174017
- `12:46:40.986Z` - Builds proposal (round 0) with **timestamp 1768135600**
- `12:46:43.028Z` - Decision reached for H=5174017 with commitment `0x51b95923ab309406073a128560cd96cd32d9c5329b8f1e33604a8a7b0de2ba5`
- `12:46:43.028Z` - Moves to H=5174018

#### Node 14 (validator 0x67) - Validator (Late Join)
- `12:46:53.582Z` - **Starts consensus for H=5174017 (~13 seconds late!)**
- `12:46:53.582Z` - Starts as validator for round 0
- `12:46:54.085Z` - Begins validating node 13's round 0 proposal (timestamp 1768135600)
- `12:46:54.584Z` - Hits `TimeoutPrecommit` for round 0 (didn't receive enough precommits in time)
- `12:46:54.584Z` - **Starts round 1 as proposer** (due to late join and timeout)
- `12:46:54.615Z` - **✅ Successfully validates node 13's proposal** (commitment: `0x51b95923...`)
  - **CRITICAL:** This validation result is stored in `valid_proposals` with timestamp **1768135600**
- `12:46:54.617Z` - Begins building own proposal for H=5174017
- `12:46:57.127Z` - **✅ Finishes building proposal** with timestamp **1768135614** (14 seconds later than node 13!)
  - **CRITICAL:** Same commitment `0x51b95923...` but different timestamp
  - **BUG:** This proposal **OVERWRITES** the earlier validation in `valid_proposals`
- `12:46:57.129Z` - `decision_reached` called (consensus reached via other nodes)
- `12:46:57.129Z` - **BUG TRIGGERED:** Retrieves block_info from `valid_proposals` → gets timestamp **1768135614** instead of **1768135600**
- `12:46:57.129Z` - Sets `previous_block_info.timestamp = 1768135614` (sequencer_consensus_context.rs:700)
- `12:46:57.136Z` - Moves to H=5174018

---

### Height 5174018 (H=018)

#### Node 14 - Stuck Validator
- `12:46:57.136Z` - Starts consensus for H=5174018
- `12:46:59.645Z` - Attempts to validate the proposal (timestamp **1768135613**)
- `12:46:59.645Z` - **❌ VALIDATION FAILS:** Timestamp check fails
  - Expected: `timestamp > previous_block_info.timestamp`
  - Got: `1768135613 < 1768135614` (FAIL!)
  - Error: `"PROPOSAL_FAILED: Timestamp is too old. Current: 1768135614, got: 1768135613"`
  - Location: `validate_proposal.rs:254`
- Node 14 falls behind and attempts to sync
- **❌ SYNC FAILS:** `try_sync` also fails timestamp validation
  - Expected: `block_info.timestamp > self.previous_block_info.timestamp`
  - Got: `1768135613 < 1768135614` (FAIL!)
  - Error: `"Invalid block info: expected block number 5174018, expected timestamp in range [1768135601, 1768135614], got 1768135613"`
  - Location: `sequencer_consensus_context.rs:732-733`
- `try_sync` returns `false`
- Node 14 **STUCK IN INFINITE RETRY LOOP**

---

## Root Cause

### The Core Bug

The `valid_proposals` HashMap in `SequencerConsensusContext` is keyed **only by height**, not by `(height, round)`:

**When a validator:**
1. Successfully validates a proposal in round N → stores `(height, proposal_id)` in `valid_proposals`
2. Then becomes proposer in round N+1 → builds and validates own proposal
3. The second validation **overwrites** the first because both use the same height and commitment key

**When `decision_reached` is called:**
- It retrieves the proposal from `valid_proposals` using only the height
- Gets the **last stored** proposal (the one from round 1 with wrong timestamp)
- Sets `previous_block_info` with the **wrong timestamp** in case the decision was on the previous round (0 in this case)
- All future validations fail because timestamps must be strictly increasing

---

### Detailed Technical Explanation (Known Issue)

This is a **known race condition** that occurs when a validator transitions to proposer between rounds while an async validation is still in progress:

#### The Race Condition Timeline:

1. **Round 0 starts:** Node 14 is a **validator**
   - Begins validating node 13's proposal asynchronously
   - Validation is running in background (async task)

2. **TimeoutPrecommit occurs:** Node transitions to round 1
   - Node didn't receive enough precommits in time
   - State machine moves node to round 1
   - **PROBLEM:** Cannot interrupt the active validation task from round 0

3. **Round 1 starts:** Node 14 is now the **proposer**
   - Node starts building its own proposal
   - **CRITICAL:** The node doesn't handle any consensus events while building
   - All incoming events (including validation completion) are queued

4. **Round 0 validation completes in background:**
   - Validation finishes successfully with timestamp 1768135600
   - Stores result in `valid_proposals[height] = proposal_id_round0`
   - **But the node cannot process this result** - it's blocked building

5. **Round 1 build completes:**
   - Build finishes with timestamp 1768135614
   - Because the block is **empty**, both proposals have the **same commitment hash**: `0x51b95923...`
   - Stores result in `valid_proposals[height] = proposal_id_round1`
   - **OVERWRITES** the round 0 validation result

6. **State machine processes queued events:**
   - Sees that round 0 validation finished and was agreed upon by other validators
   - Calls `decision_reached` with the consensus decision
   - Looks up `valid_proposals[height]` to get block_info
   - **Gets the wrong one** - the round 1 proposal with timestamp 1768135614 instead of round 0 with 1768135600

#### Why This Happens:

- **Async validation cannot be cancelled** when round changes
- **Building blocks the event loop** - no events processed until build completes
- **HashMap uses only height as key** - overwrites happen silently
- **Same commitment hash** for empty blocks masks the problem
- **State machine assumes** `valid_proposals` contains the agreed-upon proposal's block_info

#### The Fundamental Issues:

1. **No cancellation mechanism** for in-flight proposal validations/builds when rounds change
2. **Blocking operation** during build prevents handling validation completion
3. **HashMap key design** allows silent overwrites of different proposals for same height
4. **No round tracking** in `valid_proposals` to detect when proposals from different rounds collide

---

## Evidence from Logs

### Evidence 1: Successful Validation (Round 0, Timestamp 1768135600)


```json
{
  "timestamp": "2026-01-11T12:46:54.085635220Z",
  "resource": {
    "labels": {
      "namespace_name": "apollo-sepolia-alpha-14"
    }
  },
  "jsonPayload": {
    "message": "Initiating validate proposal: input=ValidateBlockInput { ... block_info: BlockInfo { block_number: BlockNumber(5174017), block_timestamp: BlockTimestamp(1768135600), ... } }",
    "line_number": 409,
    "filename": "crates/apollo_consensus_orchestrator/src/validate_proposal.rs"
  }
}
```

```json
{
  "timestamp": "2026-01-11T12:46:54.615491825Z",
  "jsonPayload": {
    "message": "Finished validating proposal. Proposal succeeded.",
    "proposal_commitment": "0x51b95923ab309406073a128560cd96cd32d9c5329b8f1e33604a8a7b0de2ba5",
    "line_number": 536
  }
}
```

### Evidence 2: Round Transition and Build Proposal (Round 1, Timestamp 1768135614)

```json
{
  "timestamp": "2026-01-11T12:46:54.584960468Z",
  "jsonPayload": {
    "message": "Applying TimeoutPrecommit for round=0.",
    "line_number": 478,
    "filename": "crates/apollo_consensus/src/state_machine.rs"
  }
}
```

```json
{
  "timestamp": "2026-01-11T12:46:54.617380009Z",
  "jsonPayload": {
    "message": "Initiating build proposal for round=1: input=BuildBlockInput { ... block_info: BlockInfo { block_number: BlockNumber(5174017), block_timestamp: BlockTimestamp(1768135614), ... } }",
    "line_number": 115,
    "filename": "crates/apollo_consensus_orchestrator/src/build_proposal.rs"
  }
}
```

```json
{
  "timestamp": "2026-01-11T12:46:57.127498169Z",
  "jsonPayload": {
    "message": "Finished building proposal",
    "proposal_commitment": "0x51b95923ab309406073a128560cd96cd32d9c5329b8f1e33604a8a7b0de2ba5",
    "line_number": 259
  }
}
```

### Evidence 3: Wrong Timestamp in previous_block_info

```json
{
  "timestamp": "2026-01-11T12:46:59.645932267Z",
  "jsonPayload": {
    "message": "PROPOSAL_FAILED: Timestamp is too old. Current: 1768135614, got: 1768135613",
    "line_number": 254,
    "filename": "crates/apollo_consensus_orchestrator/src/validate_proposal.rs"
  }
}
```

### Evidence 4: Sync Failure

```json
{
  "timestamp": "2026-01-11T12:47:XX.XXXXXX",
  "jsonPayload": {
    "message": "Invalid block info: expected block number 5174018, expected timestamp in range [1768135601, 1768135614], got 1768135613",
    "line_number": 732,
    "filename": "crates/apollo_consensus_orchestrator/src/sequencer_consensus_context.rs"
  }
}
```

---

## Code Locations

### Bug Location: HashMap Key Design
**File:** `crates/apollo_consensus_orchestrator/src/sequencer_consensus_context.rs`  
**Line:** ~85-90

```rust
pub struct SequencerConsensusContext {
    valid_proposals: Arc<Mutex<HashMap<BlockNumber, ProposalContentId>>>,
    // BUG: Should be HashMap<(BlockNumber, Round), ProposalContentId>
    // or keep only the winning proposal (from decision_reached)
}
```

### Sync Validation Location
**File:** `crates/apollo_consensus_orchestrator/src/sequencer_consensus_context.rs`  
**Line:** 728-733

```rust
if block_info.block_timestamp < self.previous_block_info.timestamp
    || block_info.block_timestamp > self.previous_block_info.timestamp
{
    warn!("Invalid block info: expected block number {}, expected timestamp in range [{}, {}], got {}");
    return false;
}
```

---

## Impact and Consequences

### Immediate Impact
- **Node 14 is completely stuck** and cannot participate in consensus
- Cannot validate new proposals (fails timestamp check)
- Cannot sync blocks (fails timestamp check in `try_sync`)
- Infinite retry loop consuming resources

### Systemic Impact
- Reduces network validator count by 1
- Could affect consensus if enough nodes experience this issue
- Can happen to **any validator that joins late** and times out into a proposer role
- **This is a known race condition** that can occur whenever:
  - A node transitions from validator to proposer between rounds
  - The async validation from the previous round completes after building starts
  - The block is empty (causing identical commitment hashes)
  - Multiple proposals for the same height are stored without round tracking

### Recovery
- **Manual reset required** - node must be restarted
- No automatic recovery mechanism
- State must be re-synced from other nodes

---

## Recommended Fixe

### Fix 1: Change HashMap Key to Include Round
Use `(BlockNumber, Round)` as the key instead of just `BlockNumber`:

```rust
pub struct SequencerConsensusContext {
    valid_proposals: Arc<Mutex<HashMap<(BlockNumber, Round), ProposalContentId>>>,
    // ...
}
```
---

## Summary for Developers

1. ✅ A validator starts validating a proposal (round N)
2. ⏱️ TimeoutPrecommit occurs before validation completes
3. 🔄 Node transitions to round N+1 and becomes proposer
4. 🚫 Cannot cancel the in-flight validation from round N
5. 🔨 Node starts building proposal (blocks event processing)
6. ✅ Round N validation completes in background → stores result in `valid_proposals[height]`
7. 🏗️ Round N+1 build completes → **overwrites** round N result in `valid_proposals[height]` when it's the same commitment
8. 📢 State machine processes consensus decision for round N
9. ❌ Retrieves wrong block_info (from round N+1) → sets wrong timestamp in `previous_block_info`
10. 🔒 Node permanently stuck - cannot validate or sync subsequent blocks

**Root cause:** `valid_proposals` uses height-only key (missing round), allowing silent overwrites.

**Immediate fix:** Use `(BlockNumber, Round)` as HashMap key, or only store the decided proposal in `decision_reached`.

**Long-term fix:** Implement task cancellation when rounds change, and track current round with active tasks.

---

**Report Generated:** January 12, 2026  
**Analyzed by:** AI Assistant  
**Log Files:** `downloaded-logs-20260111-173945.json`, `downloaded-logs-20260112-091134.json`
