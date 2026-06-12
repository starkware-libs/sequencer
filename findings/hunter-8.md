# Bug Hunter 8 Findings

## Files Examined

- `crates/starknet_patricia/src/patricia_merkle_tree/storage_proof_verification.rs` — proof verification logic
- `crates/starknet_patricia/src/patricia_merkle_tree/node_data/inner_node.rs` — `PathToBottom`, `EdgePath`, `EdgePathLength`, node data types, hash flattening
- `crates/starknet_patricia/src/patricia_merkle_tree/updated_skeleton_tree/hash_function.rs` — tree hash computation
- `crates/starknet_patricia/src/patricia_merkle_tree/types.rs` — `NodeIndex`, `SortedLeafIndices`
- `crates/starknet_patricia/src/patricia_merkle_tree/updated_skeleton_tree/create_tree_helper.rs` — skeleton tree update logic
- `crates/starknet_patricia/src/patricia_merkle_tree/updated_skeleton_tree/tree.rs` — updated skeleton tree
- `crates/starknet_patricia/src/patricia_merkle_tree/filled_tree/tree.rs` — filled tree + hash computation
- `crates/starknet_patricia/src/patricia_merkle_tree/original_skeleton_tree/utils.rs` — `split_leaves`, node height
- `crates/starknet_patricia/src/patricia_merkle_tree/traversal.rs` — subtree traversal trait
- `crates/starknet_committer/src/db/facts_db/node_serde.rs` — node serialization/deserialization
- `crates/starknet_committer/src/hash_function/hash.rs` — production hash functions
- `crates/starknet_committer/src/db/trie_traversal.rs` — trie traversal for committer

---

## Bug 1

**File**: `crates/starknet_patricia/src/patricia_merkle_tree/storage_proof_verification.rs`
**Location**: `fn build_proof_index_maps`, line ~95 (indirect — panic originates in `NodeIndex::compute_bottom_index` → `NodeIndex::new`)

**Description**: `build_proof_index_maps` (and by extension `verify_patricia_proof`) panic when the supplied `preimages` map contains an edge node whose path length is valid in isolation (`length <= 251`) but whose computed `bottom_index = (parent_index << length) + path` overflows `NodeIndex::MAX = 2^252 - 1`.

This can happen whenever an edge preimage is anchored at any internal node other than the root. For example, a node at index 2 (the root's left child) with an edge of length 251 yields `bottom_index = 2 << 251 = 2^252`, which exceeds `NodeIndex::MAX = 2^252 - 1`. The `NodeIndex::new` constructor unconditionally asserts `index <= MAX`, so this panics the caller process.

**Root Cause**: `PathToBottom::new` validates only that `path` fits within `length` bits — it does not validate that `parent_node_index << length` stays within the legal node-index range. `build_proof_index_maps` applies the unsanitised `path_to_bottom` from external preimage data to whatever the current queue index is, without bounds-checking the resulting `bottom_index` before calling `NodeIndex::new`.

`NodeIndex::compute_bottom_index` and all callers assume the preimage is well-formed with respect to tree depth, which is a valid assumption for honest proofs but not for adversarially crafted ones.

**Impact**: Any caller that passes externally-supplied (untrusted) data as the `preimages` argument to `verify_patricia_proof` or `build_proof_index_maps` can trigger an unconditional `assert!` failure (panic / process abort). Both functions are `pub` in a library crate (`starknet_patricia`). The bug is a denial-of-service via panic, not a hash-break.

**Failing Test**:

```rust
// Add to: crates/starknet_patricia/src/patricia_merkle_tree/storage_proof_verification.rs
// (or any integration test that imports the crate)
#[cfg(test)]
mod proof_verification_tests {
    use std::collections::HashMap;

    use starknet_api::hash::HashOutput;
    use starknet_types_core::felt::Felt;

    use crate::patricia_merkle_tree::node_data::inner_node::{
        BinaryData, EdgeData, EdgePathLength, EdgePath, PathToBottom, Preimage, PreimageMap,
    };
    use crate::patricia_merkle_tree::storage_proof_verification::build_proof_index_maps;
    use crate::patricia_merkle_tree::types::NodeIndex;
    // TestTreeHashFunction and MockLeaf live in internal_test_utils / external_test_utils:
    use crate::patricia_merkle_tree::internal_test_utils::TestTreeHashFunction;
    use crate::patricia_merkle_tree::external_test_utils::MockLeaf;

    /// A crafted edge preimage anchored at NodeIndex(2) with length=251 causes
    /// `bottom_index = 2^252` which exceeds `NodeIndex::MAX = 2^252 - 1`.
    /// The `NodeIndex::new` assertion fires → process panic.
    ///
    /// With the mock addition hash H(a,b)=a+b:
    ///   edge_hash  = H(0, 0) + 251 = 251
    ///   root_hash  = H(251, 0)     = 251      (binary with left=251, right=0)
    #[test]
    #[should_panic]
    fn test_build_proof_index_maps_overflowing_edge_panics() {
        // edge preimage: anchored at index 2, path=0, length=251, bottom_hash=0
        let edge_path =
            PathToBottom::new(EdgePath(ethnum::U256::ZERO.into()), EdgePathLength::new(251).unwrap())
                .unwrap();
        let edge_hash = HashOutput(Felt::from(251_u128)); // H(0,0)+251 = 251

        // binary root: left child hash = edge_hash, right child hash = 0
        let root_hash = HashOutput(Felt::from(251_u128)); // H(251,0) = 251

        let preimages: PreimageMap = HashMap::from([
            // root at index 1: binary node
            (
                root_hash,
                Preimage::Binary(BinaryData {
                    left_data: edge_hash,
                    right_data: HashOutput(Felt::ZERO),
                }),
            ),
            // left child at index 2: edge of length 251 → bottom_index = 2^252, overflows MAX
            (
                edge_hash,
                Preimage::Edge(EdgeData {
                    bottom_data: HashOutput(Felt::ZERO),
                    path_to_bottom: edge_path,
                }),
            ),
        ]);

        // This call panics instead of returning an error.
        let _ = build_proof_index_maps::<MockLeaf, TestTreeHashFunction>(root_hash, &preimages);
    }
}
```

**How to Verify**: `SEED=0 cargo test -p starknet_patricia test_build_proof_index_maps_overflowing_edge_panics`

The test is annotated `#[should_panic]` to document the current (buggy) behaviour. A correct fix would make `build_proof_index_maps` return `Err(ProofVerificationError::HashMismatch { ... })` instead of panicking when the preimage edge would produce an out-of-range node index.

**Suggested Fix**: In `build_proof_index_maps`, before calling `edge.path_to_bottom.bottom_index(index)`, check that the resulting index would not exceed `NodeIndex::MAX`:

```rust
Preimage::Edge(edge) => {
    // Guard: reject malformed edges that would overflow the node-index space.
    // A node at bit-length B with an edge of length L produces bottom at bit-length B+L.
    // NodeIndex::BITS = 252, so B+L must not exceed 252.
    let parent_bit_length = index.bit_length();
    let edge_length = u8::from(edge.path_to_bottom.length);
    if u16::from(parent_bit_length) + u16::from(edge_length) > u16::from(NodeIndex::BITS) {
        return Err(ProofVerificationError::HashMismatch {
            index,
            proof_value: hash,
            actual: hash, // placeholder — could use a dedicated error variant
        });
    }
    let bottom_index = edge.path_to_bottom.bottom_index(index);
    register_child(&mut hash_by_index, &mut queue, bottom_index, edge.bottom_data)?;
}
```

(Or add a dedicated `ProofVerificationError::MalformedEdge` variant for clarity.)

---

## What Was Checked and Found Correct

- **Binary node hash ordering** (`H(left, right)`): correct, left and right are passed in the right order.
- **Edge node hash formula** (`H(bottom_hash, path) + length`): correct per the Starknet spec.
- **`EdgeData::flatten` / `TryFrom<&Vec<Felt>>`**: the `[length, path, bottom]` round-trip is consistent.
- **`PathToBottom::remove_first_edges`**: the bit-masking is correct for all edge cases including `n_edges = 0` and `n_edges = length`.
- **`PathToBottom::concat_paths`**: correct for valid inputs; potential u8 overflow only if total length exceeds 251, which is impossible in a valid 251-depth tree.
- **`PathToBottom::is_left_descendant`**: the u8-1 underflow for length=0 is unreachable in practice — all call sites guard against length=0.
- **`SortedLeafIndices::bisect_left` / `bisect_right`**: correct assuming unique elements (the TODO about duplicates is a known limitation, not a latent bug in any current code path).
- **`split_leaves`**: correct; height-0 edge cases are guarded by `is_leaf()` checks upstream.
- **`NodeIndex::get_lca`**: correct for all tested cases.
- **`NodeIndex::get_path_to_descendant`**: correct; `distance <= 251` is guaranteed by the bit-length bounds.
- **`FilledTreeImpl::compute_filled_tree_rec`**: correct use of left/right indices and hash ordering.
- **DB serialisation format** (`[bottom, path, length]` bytes) vs. proof preimage format (`[length, path, bottom]` Felts): these are intentionally different representations; each serialiser/deserialiser pair is internally consistent.
- **Production hash functions** (`PedersenHashFunction`, `PoseidonHashFunction`): correct wrappers for `starknet-types-core`.
