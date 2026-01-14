// Virtual OS version of storage_patricia.cairo.
// Only traverses the previous tree (skips write validation).

from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_builtins import HashBuiltin
from starkware.cairo.common.dict import DictAccess
from starkware.cairo.common.patricia import traverse_prev_tree
from starkware.cairo.common.patricia_utils import PatriciaUpdateConstants

// Perform storage Patricia verification.
// In the virtual OS, this only traverses the previous tree (skips write validation).
func storage_patricia_update{hash_ptr: HashBuiltin*, range_check_ptr}(
    patricia_update_constants: PatriciaUpdateConstants*,
    update_ptr: DictAccess*,
    n_updates: felt,
    height: felt,
    prev_root: felt,
    new_root: felt,
) {
    if (n_updates == 0) {
        return ();
    }

    alloc_locals;

    // Traverse the tree to verify the read values.
    // Use prev_root as new_root for read-only validation.
    let (siblings) = alloc();
    traverse_prev_tree{siblings=siblings}(
        patricia_update_constants=patricia_update_constants,
        update_ptr=update_ptr,
        n_updates=n_updates,
        height=height,
        prev_root=prev_root,
        new_root=prev_root,
    );

    return ();
}
