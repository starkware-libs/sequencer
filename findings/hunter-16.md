# Bug Findings: starknet_committer

Crate audited: `/home/user/sequencer/crates/starknet_committer/src/`

Files read deeply:
- `hash_function/hash.rs`
- `patricia_merkle_tree/types.rs`
- `patricia_merkle_tree/leaf/leaf_impl.rs`
- `patricia_merkle_tree/leaf/leaf_serde.rs`
- `block_committer/commit.rs`
- `block_committer/input.rs`
- `block_committer/state_diff_generator.rs`
- `forest/original_skeleton_forest.rs`
- `forest/updated_skeleton_forest.rs`
- `forest/filled_forest.rs`
- `forest/deleted_nodes.rs`
- `db/trie_traversal.rs`

---

## Bug 1: `get_nodes_count` inflates measurement by counting contract-state leaves as inner trie nodes

**File**: `/home/user/sequencer/crates/starknet_committer/src/patricia_merkle_tree/types.rs`, line 256

**Description**:
`StarknetForestProofs::get_nodes_count()` includes `contracts_trie_proof.leaves.len()` in its
total. `contracts_trie_proof.leaves` is a `HashMap<ContractAddress, ContractState>` — it holds
leaf contract states, not inner trie nodes (preimage entries). The other summands
(`classes_trie_proof.len()`, `contracts_trie_proof.nodes.len()`, storage proof lens) are all
`PreimageMap` lengths, i.e., counts of inner trie nodes. Mixing leaf counts into an "inner node"
count inflates every measurement by the number of accessed contracts.

**Root Cause**:
The field `contracts_trie_proof` is a `ContractsTrieProof { nodes: PreimageMap, leaves: HashMap<ContractAddress, ContractState> }`. The `.nodes` field holds inner node preimages; `.leaves` holds contract state leaves. `get_nodes_count()` sums both `.nodes.len()` and `.leaves.len()`, conflating two different things under the same metric.

The result is passed directly to `measurements.record_measurement(Action::FetchWitnessesFirstPass, patricia_proofs.get_nodes_count(), ...)` and `measurements.attempt_to_stop_measurement(Action::FetchWitnessesSecondPass, proof_after.get_nodes_count())` in `commit.rs` (lines 165 and 189).

**Test**:
```rust
// Place in crates/starknet_committer/src/patricia_merkle_tree/ (test file)
#[cfg(test)]
mod get_nodes_count_test {
    use std::collections::HashMap;
    use starknet_api::core::ContractAddress;
    use starknet_api::hash::HashOutput;
    use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
        BinaryData, Preimage, PreimageMap,
    };
    use starknet_types_core::felt::Felt;

    use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
    use crate::patricia_merkle_tree::types::{ContractsTrieProof, StarknetForestProofs};

    /// Demonstrates that get_nodes_count() over-counts by including contract leaves.
    #[test]
    fn test_get_nodes_count_excludes_contract_leaves() {
        let dummy_hash = HashOutput(Felt::from(1_u64));
        let dummy_preimage = Preimage::Binary(BinaryData {
            left_data: dummy_hash,
            right_data: dummy_hash,
        });

        // One inner node in the classes trie, zero inner nodes in the contracts trie.
        let mut classes_trie_proof = PreimageMap::new();
        classes_trie_proof.insert(dummy_hash, dummy_preimage);

        let contracts_inner_nodes = PreimageMap::new(); // zero inner nodes

        // Two contract-state leaves (these are NOT inner nodes).
        let mut contract_leaves: HashMap<ContractAddress, ContractState> = HashMap::new();
        contract_leaves.insert(
            ContractAddress::try_from(Felt::from(10_u64)).unwrap(),
            ContractState::default(),
        );
        contract_leaves.insert(
            ContractAddress::try_from(Felt::from(11_u64)).unwrap(),
            ContractState::default(),
        );

        let proofs = StarknetForestProofs {
            classes_trie_proof,
            contracts_trie_proof: ContractsTrieProof {
                nodes: contracts_inner_nodes,
                leaves: contract_leaves,
            },
            contracts_trie_storage_proofs: HashMap::new(),
        };

        // There is exactly 1 inner node (in the classes trie).
        // get_nodes_count() returns 1 + 0 + 2 (contract leaves) = 3.
        // The correct answer is 1.
        let reported = proofs.get_nodes_count();
        assert_eq!(
            reported, 1,
            "get_nodes_count() should count only inner trie nodes (1), \
             but it counts contract leaves too and reports {}",
            reported
        );
    }
}
```

**How to verify**:
```bash
cargo test -p starknet_committer test_get_nodes_count_excludes_contract_leaves
```
The test will fail: `get_nodes_count()` reports 3 instead of 1 because it adds the two
contract leaves to the inner-node count.

**Fix**: Remove `+ self.contracts_trie_proof.leaves.len()` from `get_nodes_count()`, or rename
the method to `get_entries_count()` and document that it intentionally includes leaf entries.

---

## Bug 2: `StateDiff::is_empty()` returns `true` when `storage_updates` has a contract address with zero storage slots

**File**: `/home/user/sequencer/crates/starknet_committer/src/block_committer/input.rs`, line 110

**Description**:
`StateDiff::len()` sums the number of inner storage-slot entries across all contracts. It does
NOT count contracts that appear in `storage_updates` with an empty inner map. Therefore
`is_empty()` (which delegates to `len() == 0`) can return `true` even when `storage_updates`
contains a contract address pointing to an empty `HashMap`.

In contrast, `accessed_addresses()` DOES include that address (because it iterates
`storage_updates.keys()`). The result is a contradiction: `is_empty()` says the state diff is
empty, but `accessed_addresses()` says there is one contract to process.

Concretely, this matters because `StateDiff::actual_storage_updates()` calls
`accessed_addresses()`, so a state diff with `{ addr: {} }` would spin up a storage trie for
`addr` (creating entries in `ForestSortedIndices`, `UpdatedSkeletonForest`, etc.) even though
`is_empty()` claims there is nothing to do.

**Root Cause**:
`len()` is defined as the total number of individual modification entries. An address present in
`storage_updates` with an empty inner map contributes 0 to `len()` but IS present in the
`storage_updates` key set.

**Test**:
```rust
// Add to crates/starknet_committer/src/block_committer/input_test.rs
#[cfg(test)]
mod state_diff_is_empty_test {
    use std::collections::HashMap;
    use starknet_api::core::ContractAddress;
    use starknet_types_core::felt::Felt;

    use crate::block_committer::input::StateDiff;

    #[test]
    fn test_is_empty_with_empty_inner_storage_map() {
        let addr = ContractAddress::try_from(Felt::from(42_u64)).unwrap();
        let state_diff = StateDiff {
            address_to_class_hash: HashMap::new(),
            address_to_nonce: HashMap::new(),
            class_hash_to_compiled_class_hash: HashMap::new(),
            // Contract address present but no storage keys — contributes 0 to len().
            storage_updates: HashMap::from([(addr, HashMap::new())]),
        };

        // accessed_addresses() includes `addr`.
        let accessed = state_diff.accessed_addresses();
        assert!(!accessed.is_empty(), "Contract should be in accessed_addresses()");

        // Bug: is_empty() returns true even though the state diff "touches" addr.
        assert!(
            !state_diff.is_empty(),
            "is_empty() should return false when a contract address is present in \
             storage_updates, but it returns true. accessed_addresses() = {:?}",
            accessed
        );
    }
}
```

**How to verify**:
```bash
cargo test -p starknet_committer test_is_empty_with_empty_inner_storage_map
```
The assertion `!state_diff.is_empty()` fails because `is_empty()` returns `true`.

**Fix**:
```rust
pub fn is_empty(&self) -> bool {
    self.address_to_class_hash.is_empty()
        && self.address_to_nonce.is_empty()
        && self.class_hash_to_compiled_class_hash.is_empty()
        && self.storage_updates.is_empty()
}
```
(The revised version checks whether any contract address is present at all, rather than
whether any storage slot is modified.)

---

## Bug 3: `DeletedNodes::is_empty()` is inconsistent with `storage_tries` containing phantom entries

**File**: `/home/user/sequencer/crates/starknet_committer/src/forest/deleted_nodes.rs`, line 32

**Description**:
`DeletedNodes::is_empty()` is:
```rust
pub fn is_empty(&self) -> bool {
    self.classes_trie.is_empty()
        && self.contracts_trie.is_empty()
        && self.storage_tries.values().all(|leaves| leaves.is_empty())
}
```

If `storage_tries` is `HashMap { addr: HashSet::new() }`, `is_empty()` returns `true` because
`.all(|leaves| leaves.is_empty())` is satisfied by an empty inner set. But
`storage_tries.is_empty()` itself returns `false`. A caller iterating
`deleted_nodes.storage_tries.keys()` would see a phantom `addr` entry, while `is_empty()`
would have promised that nothing was deleted.

`len()` has the same blindspot — it sums inner set sizes (each 0) and also returns 0 for this
state. So both `len()` and `is_empty()` are mutually consistent with each other, but both
diverge from the structural emptiness of the outer `storage_tries` map.

**Root Cause**:
`is_empty()` and `len()` measure the number of deleted node indices, not the structural state
of the `storage_tries` HashMap. An entry `{ addr: {} }` has zero deleted nodes but is structurally
non-empty. In practice, `find_deleted_nodes()` avoids this via the
`if deleted_leaves_indices.is_empty() { continue; }` guard, so production code does not create
phantom entries today. But the invariant is not enforced on the type, making it a latent bug.

**Test**:
```rust
// Add to crates/starknet_committer/src/forest/deleted_nodes_test.rs
#[cfg(test)]
mod deleted_nodes_consistency_test {
    use std::collections::{HashMap, HashSet};
    use starknet_api::core::ContractAddress;
    use starknet_types_core::felt::Felt;

    use crate::forest::deleted_nodes::DeletedNodes;

    #[test]
    fn test_is_empty_with_phantom_storage_entry() {
        let addr = ContractAddress::try_from(Felt::from(99_u64)).unwrap();
        let deleted = DeletedNodes {
            classes_trie: HashSet::new(),
            contracts_trie: HashSet::new(),
            // Contract address present, but no deleted node indices.
            storage_tries: HashMap::from([(addr, HashSet::new())]),
        };

        // Structural check: the outer map is NOT empty.
        assert!(
            !deleted.storage_tries.is_empty(),
            "storage_tries has one entry, so it is not structurally empty"
        );

        // Bug: is_empty() returns true despite storage_tries being non-empty.
        assert!(
            !deleted.is_empty(),
            "is_empty() should return false when storage_tries has a phantom entry, \
             but it returns true (storage_tries = {:?})",
            deleted.storage_tries
        );
    }
}
```

**How to verify**:
```bash
cargo test -p starknet_committer test_is_empty_with_phantom_storage_entry
```
The assertion `!deleted.is_empty()` fails.

**Fix**:
```rust
pub fn is_empty(&self) -> bool {
    self.classes_trie.is_empty()
        && self.contracts_trie.is_empty()
        && self.storage_tries.is_empty()
}
```
This also aligns semantically with how `is_empty()` works for `HashSet` and `HashMap` in Rust.

---

## Summary

Three bugs were found in the `starknet_committer` crate:

| # | Title | File | Severity |
|---|-------|------|----------|
| 1 | `get_nodes_count` inflates measurement by counting contract leaves as inner nodes | `patricia_merkle_tree/types.rs:256` | Medium (incorrect metrics/measurements) |
| 2 | `is_empty()` returns `true` when `storage_updates` has a contract with zero slots | `block_committer/input.rs:110` | Low-Medium (semantic inconsistency vs `accessed_addresses()`) |
| 3 | `DeletedNodes::is_empty()` inconsistent when `storage_tries` has phantom entries | `forest/deleted_nodes.rs:32` | Low (structural inconsistency, guarded in practice) |

No cryptographic correctness bugs were found. The Pedersen hash order for contract state leaf
hashes (`H(H(H(class_hash, storage_root), nonce), version)`) matches the Starknet protocol
specification. The Poseidon hash for compiled class leaf hashes
(`Poseidon(CONTRACT_CLASS_LEAF_V0, compiled_class_hash)`) also matches the spec. The
`updated_contract_skeleton_leaf` logic correctly handles contracts with zero nonce and zero
class hash when their storage becomes empty. The `fetch_patricia_paths` traversal correctly
inserts `L::default()` (the zero state) for deleted contracts via the `empty_leaves_indices`
mechanism, so the witness collection for deleted contracts is correct.
