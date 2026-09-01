# Accessed Keys & Witnesses — Feature Manual

Covers the `get_accessed_keys` feature across the `sequencer` (Rust) and `starkware` (Python) repos.
Includes the recovery procedure for a network stuck on missing witnesses.

## 1. Background: the happy flow

### What are accessed keys?

Accessed keys are the trie leaves a block touches during execution.
The blockifier collects them while executing the block (`AccessedKeys`, `crates/blockifier/src/state/accessed_keys.rs`).

### What the Rust committer does with them

The batcher sends the committer the block's state diff plus its accessed keys.
The committer runs `read_paths_and_commit_block` (`crates/apollo_committer/src/committer.rs`).
It commits the block and collects the Patricia paths to the accessed keys, before and after the update.
It stores the merged paths, keyed by height.

The merged paths are the block's **state commitment infos**.
These are the **witnesses**.

### Why the OS needs them

The Starknet OS (`starkware` repo) re-executes the block in Cairo and verifies the trie update.
It gets the trie data as hints: the witnesses.

### How the witnesses reach the OS

When a height is decided, the proposer collects the ready witnesses that have not reached cende yet.
It puts them in the cende blob and writes the blob to the recorder (Aerospike).
The Python side (cende recorder, `src/starkware/starknet/services/cende/`) stores them.
The OS/prover pipeline reads them from there as part of the OS input.

## 2. The witness requirement in consensus

Witnesses are produced by whoever executed the block: the proposer, and validators that validated it.

Block `H+10` needs the witnesses of block `H` (`STORED_BLOCK_HASH_BUFFER = 10`, `crates/blockifier/src/abi/constants.rs`).
The witnesses must reach the recorder by then — in the blob of `H+10`, or in an earlier blob.
Before building a proposal, the proposer verifies the witnesses are available (`verify_retrospective_state_commitment_infos`, `crates/apollo_consensus_orchestrator/src/utils.rs`).
It checks its own batcher first, then the recorder's commitment-infos height offset.
If neither has the witnesses, the proposal build fails with `RetrospectiveStateCommitmentInfosError::NotStored`.
The round fails.

## 3. Who has no accessed keys: the sync flow

A node that gets a block from state sync does not execute it.
No execution means no call infos, and no call infos means no accessed keys.
The node can still commit the block: it applies the state diff and gets the correct roots.
But it cannot compute the witnesses.
So synced heights have no state commitment infos on that node.

## 4. Worst case: no node has witnesses for height H

Example: every node learned block `H` through sync (e.g., after an upgrade).
Then no batcher has `H`'s witnesses, and the recorder never receives them.
Block `H+10` cannot be produced.
Every proposer fails the check, every round fails, and the network is stuck.

# RECOVERY PROCEDURE

1. **Choose one node and revert it to a height before `H`.**
   Any node works. It must re-sync through `H` so it re-processes that block.
2. **Enable the accessed-keys config on that node:**
   ```
   consensus_manager_config.context_config.static_config.fetch_accessed_keys_from_centralized: true
   ```
   Defined in `ContextStaticConfig` (`crates/apollo_consensus_orchestrator_config/src/config.rs`). Default is `false`.
3. **Restart the node and let it sync.**
   With the flag on, every synced block triggers a call to the recorder's `/get_accessed_keys_input` endpoint via the cende ambassador (`crates/apollo_consensus_orchestrator/src/cende/mod.rs`).
   The recorder returns the block's proof facts and execution infos.
   The node computes the accessed keys locally (`compute_accessed_keys`) and passes them to the batcher with the sync block.
   The committer then produces and stores the witnesses for each synced block, including `H`.
4. **Expected result.**
   The node reaches the tip holding the witnesses of `H`.
   Its proposal for height `H+10` passes the retrospective check, so it proposes successfully.
   Its blob delivers `H`'s witnesses to the recorder.
   The network resumes.
5. **Revert the config change when the system is healthy.**
   Set the flag back to `false` and restart the node.

### Note

- The flag only affects the sync flow. Blocks the node executes itself are unaffected.
