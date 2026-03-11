---
name: consensus-and-block-building
description: Use this skill for work in `apollo_consensus*`, `apollo_batcher`, `apollo_gateway`, `apollo_mempool`, `apollo_committer`, or any task involving heights, rounds, proposals, votes, block building, transaction admission, or decision flow. This skill should also trigger when a change spans gateway -> mempool -> batcher -> consensus.
---

# Consensus and Block Building

<purpose>
Preserve the sequencer's height/round/proposal invariants while changing the hottest business-logic path in the repo.
</purpose>

<context>
- `apollo_consensus::run_consensus` drives multi-height consensus.
- `apollo_consensus_orchestrator::SequencerConsensusContext` bridges consensus to batcher, state sync, gas price, and network broadcasts.
- `apollo_batcher::Batcher` owns active height, active proposal, executed proposals, and proposal streams.
- Gateway performs stateless validation, transaction conversion, and stateful validation before enqueueing to mempool.
</context>

<procedure>
1. Decide which layer owns the behavior:
   - consensus state machine -> `apollo_consensus`
   - proposal orchestration / network bridge -> `apollo_consensus_orchestrator`
   - block execution / proposal streams -> `apollo_batcher`
   - admission / validation -> `apollo_gateway` and `apollo_mempool`
2. Trace data by invariant, not only by file:
   - height
   - round
   - proposal id / commitment
   - transaction batches
3. If you touch `ProposalPart`, `Vote`, or any protobuf-backed consensus message, also load `network-and-protobuf`.
4. Preserve async lifecycle rules:
   - one tracked proposal per `(height, round)` in the stored proposal map
   - transaction batches stay batched for reproposal paths
   - cancellation and queued proposal behavior must remain consistent
5. Verify with crate tests first, then integration flows when admission/build/proposal paths cross crate boundaries.
</procedure>

<patterns>
<do>
- Keep proposal content and commitment checks aligned.
- Treat `decision_reached` and reproposal paths as first-class behavior, not edge cases.
- Follow the existing separation between stateless validation, stateful validation, mempool admission, and batch execution.
</do>
<dont>
- Don't change consensus or proposal wire messages without protobuf/network review.
- Don't flatten transaction batches if reproposal or streaming logic depends on batch boundaries.
- Don't "fix" a height/round bug by bypassing stored-proposal consistency checks.
</dont>
</patterns>

<examples>
Example: proposal flow anchors
```text
apollo_consensus::run_consensus
-> apollo_consensus_orchestrator::SequencerConsensusContext
-> apollo_batcher::Batcher
-> apollo_committer / apollo_state_sync
```
</examples>

<troubleshooting>
| Symptom | Cause | Fix |
|---------|-------|-----|
| Proposal commitment mismatch | stored proposal content and decision path diverged | trace insertion/update paths in `SequencerConsensusContext` |
| Proposal validation fails mid-stream | invalid `ProposalPart` ordering or batch handling | inspect `validate_proposal.rs` and the sender path |
| Transactions accepted but never built | gateway/mempool path diverged from batcher expectations | trace add-tx -> mempool -> batcher request flow |
</troubleshooting>

<references>
- `crates/apollo_consensus/src/manager.rs`: multi-height consensus loop
- `crates/apollo_consensus_orchestrator/src/sequencer_consensus_context.rs`: bridge to the sequencer
- `crates/apollo_consensus_orchestrator/src/validate_proposal.rs`: streamed proposal validation
- `crates/apollo_batcher/src/batcher.rs`: block-building state owner
- `crates/apollo_gateway/src/gateway.rs`: transaction admission path
</references>
