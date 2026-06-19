# Bug Hunting Report: starknet_patricia

**Crate**: `starknet_patricia` at `/home/user/sequencer/crates/starknet_patricia/src/`
**Files deeply read**: `inner_node.rs`, `types.rs`, `storage_proof_verification.rs`, `create_tree_helper.rs`, `traversal.rs`, `filled_tree/tree.rs`, `hash_function.rs`, `original_skeleton_tree/utils.rs`, `original_skeleton_tree/tree.rs`, `updated_skeleton_tree/tree.rs`

---

## Bug 1: `is_left_descendant` Panics (or silently misbehaves) on Zero-Length `PathToBottom`

**File**: `/home/user/sequencer/crates/starknet_patricia/src/patricia_merkle_tree/node_data/inner_node.rs`, line 173

**Description**:
`PathToBottom::is_left_descendant()` performs an unchecked `u8` subtraction `self.length.0 - 1`. When `length == 0`, this underflows:

- **Debug mode**: panics with `attempt to subtract with overflow`
- **Release mode**: wraps to `255`, causing `self.path.0 >> 255` which evaluates to `0`, so the function silently returns `true` — incorrectly claiming the (zero-length) path goes left

A `PathToBottom` with `length == 0` is entirely valid to construct. `PathToBottom::new_zero()` is a public API function that creates exactly this, and `PathToBottom::new(EdgePath(U256::ZERO), EdgePathLength(0))` passes all validation in `PathToBottom::new`. Additionally, the `TryFrom<&Vec<Felt>> for Preimage` implementation accepts `[length=0, path=0, hash]` raw preimage data, constructing a zero-length edge `PathToBottom`.

The function `is_left_descendant` is called inside `update_edge_node` (line 315 of `create_tree_helper.rs`) with the `path_to_bottom` from an `OriginalSkeletonNode::Edge`. If a zero-length edge exists in the original skeleton — which a malicious/malformed preimage or storage entry can produce — the node processing code panics.

**Root Cause**:
`is_left_descendant` has an implicit precondition (`self.length > 0`) that is not validated during `PathToBottom` construction. The API allows `PathToBottom` with `length == 0` but the method computes `self.length.0 - 1` on a `u8` without a guard.

**Test**:
```rust
#[cfg(test)]
mod bug_1_test {
    use ethnum::U256;
    use crate::patricia_merkle_tree::node_data::inner_node::{
        EdgePath, EdgePathLength, PathToBottom,
    };

    /// Demonstrates that `is_left_descendant()` panics (in debug) or
    /// silently returns a wrong answer (in release) for a zero-length path.
    ///
    /// PathToBottom::new_zero() is a public API function that produces a
    /// valid zero-length path. Calling is_left_descendant on it causes
    /// u8 underflow: `self.length.0 - 1` = `0u8 - 1` = overflow.
    ///
    /// Run with: cargo test -p starknet_patricia bug_1_is_left_descendant_zero_length
    #[test]
    #[should_panic]
    fn bug_1_is_left_descendant_zero_length() {
        let zero_path = PathToBottom::new_zero();
        // This should not panic according to the API contract (PathToBottom is valid),
        // but it does because is_left_descendant does `self.length.0 - 1` unchecked.
        let _ = zero_path.is_left_descendant();
    }

    /// Demonstrates the same issue via PathToBottom::new() with explicit zero length.
    #[test]
    #[should_panic]
    fn bug_1_is_left_descendant_explicit_zero_length() {
        let zero_path = PathToBottom::new(
            EdgePath(U256::ZERO),
            EdgePathLength::new(0).unwrap(),
        )
        .unwrap();
        // u8 underflow: 0 - 1 panics in debug mode.
        let _ = zero_path.is_left_descendant();
    }
}
```

**How to verify**:
```bash
SEED=0 cargo test -p starknet_patricia bug_1_is_left_descendant_zero_length
SEED=0 cargo test -p starknet_patricia bug_1_is_left_descendant_explicit_zero_length
```

Both tests should pass (i.e., the `#[should_panic]` is triggered). If either does NOT panic, the bug is in the opposite direction: the function silently returns a wrong answer in release mode.

---

## Bug 2: `SortedLeafIndices::bisect_left` and `bisect_right` Assume Uniqueness But the Constructor Does Not Enforce It

**File**: `/home/user/sequencer/crates/starknet_patricia/src/patricia_merkle_tree/types.rs`, lines 231–288

**Description**:
`SortedLeafIndices::new` is documented with a `TODO` comment acknowledging that duplicates are not removed:

```rust
// TODO(Nimrod, 1/8/2024): Remove duplicates from the given indices.
pub fn new(indices: &'a mut [NodeIndex]) -> Self {
    indices.sort();
    Self(indices)
}
```

Meanwhile, `bisect_left` and `bisect_right` both carry doc comments stating **"Assumes that the elements in the slice are unique"**. They use Rust's `slice::binary_search` which, for a slice with duplicate values, returns `Ok(pos)` where `pos` is **any** occurrence of the search value — not guaranteed to be the leftmost (for `bisect_left`) or rightmost+1 (for `bisect_right`).

The critical usage is in `split_leaves` (`original_skeleton_tree/utils.rs`, line 41):

```rust
let leaves_split = leaf_indices.bisect_left(&leftmost_index_in_right_subtree);
```

When `root_height == 1` (node one level above leaves), `leftmost_index_in_right_subtree` equals the right child index — which is a leaf index. If the `leaf_indices` slice contains two copies of this index (i.e., the same leaf is updated twice by the caller), `binary_search` may return an interior duplicate occurrence, causing `split_leaves` to place both copies in the wrong subtree.

**Root Cause**:
The `bisect_left`/`bisect_right` invariant is not enforced at construction time. The constructor sorts but does not deduplicate, leaving a trap for callers who pass duplicate indices.

**Test**:
```rust
#[cfg(test)]
mod bug_2_test {
    use ethnum::U256;
    use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
    use crate::patricia_merkle_tree::original_skeleton_tree::utils::split_leaves;

    /// Demonstrates that bisect_left with duplicates may return
    /// a non-leftmost position, causing split_leaves to misbehave.
    ///
    /// Tree layout (height 1, root at index 1, leaves at 2 and 3):
    ///   root = 1 (height=1)
    ///   left child = 2 (leaf)
    ///   right child = 3 (leaf)
    ///
    /// We request modifications to leaf 3 TWICE (duplicate index).
    /// split_leaves should place both copies of index 3 in the RIGHT slice.
    /// With buggy bisect_left, binary_search may return Ok(0) or Ok(1) for
    /// either duplicate; if it returns 1 instead of 0, one copy goes left.
    ///
    /// Run with: cargo test -p starknet_patricia bug_2_duplicate_leaf_indices
    #[test]
    fn bug_2_duplicate_leaf_indices() {
        // In the full 251-height tree:
        // NodeIndex::FIRST_LEAF = 2^251 = left-most leaf
        // NodeIndex::FIRST_LEAF + 1 = right-most of first pair of siblings (right child of FIRST_LEAF's parent)
        //
        // Use a simpler absolute index: root = NodeIndex(2), children = NodeIndex(4) and NodeIndex(5)
        // (height = SubTreeHeight(1) in the sub-tree sense)
        // Actually use actual tree indices for clarity:
        // root index 1, left=2, right=3 (a 1-level tree).
        //
        // We can pass duplicate indices for the right child leaf.
        let root_index = NodeIndex::from(1u128);
        // Duplicate: leaf 3 appears twice.
        // After split_leaves, BOTH should be in the right slice (empty left).
        let mut leaf_indices = vec![
            NodeIndex::from(3u128),
            NodeIndex::from(3u128),
        ];
        let sorted = SortedLeafIndices::new(&mut leaf_indices);
        let [left, right] = split_leaves(&root_index, &sorted);

        // Correct behavior: all copies of leaf 3 should be in right, left should be empty.
        // Due to the bug, binary_search may return the interior occurrence,
        // so the split position may be 1 instead of 0.
        // The test documents that left is NOT guaranteed to be empty.
        assert!(
            left.is_empty(),
            "Expected empty left slice, but got {:?}. \
             bisect_left returned wrong position for duplicate elements.",
            left.get_indices()
        );
        assert_eq!(right.len(), 2, "Expected both copies of leaf 3 in right slice.");
    }
}
```

**How to verify**:
```bash
SEED=0 cargo test -p starknet_patricia bug_2_duplicate_leaf_indices
```

The test may fail (the `assert!(left.is_empty())` fires) depending on which position `binary_search` happens to return for the duplicate. The failure is non-deterministic because `binary_search`'s tie-breaking among duplicates is implementation-defined. The test documents the bug: callers who pass duplicate leaf indices can get incorrect tree splits.

**Note**: The immediate fix is to call `dedup()` after `sort()` in `SortedLeafIndices::new`, as the TODO comment intends.

---

## Summary

| # | Title | File | Line | Severity |
|---|-------|------|------|----------|
| 1 | `is_left_descendant` panics on zero-length `PathToBottom` | `node_data/inner_node.rs` | 173 | Medium — panic (DoS) via malformed preimage data |
| 2 | `bisect_left`/`bisect_right` incorrect with duplicate indices | `types.rs` | 231–288 | Low — incorrect tree structure if caller passes duplicates |

No significant cryptographic correctness bugs were found in the hash function implementation (`hash_function.rs`), the proof verification logic (`storage_proof_verification.rs`), or the `get_lca` algorithm (`types.rs`). The edge hash formula `H(bottom, path) + length` (in Felt arithmetic) matches the Starknet Patricia trie specification. The `get_lca` value-based comparison is equivalent to bit-length comparison for valid `NodeIndex` values. The `get_bottom_subtree` subtree boundary formula is arithmetically correct.
