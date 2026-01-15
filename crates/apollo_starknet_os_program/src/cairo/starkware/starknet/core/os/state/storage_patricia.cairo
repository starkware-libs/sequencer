// Storage Patricia update logic.
// This file is swapped with storage_patricia__virtual.cairo in virtual OS builds.

from starkware.cairo.common.cairo_builtins import HashBuiltin
from starkware.cairo.common.dict import DictAccess
from starkware.cairo.common.patricia import (
    patricia_update_read_optimized,
    patricia_update_using_update_constants,
)
from starkware.cairo.common.patricia_utils import PatriciaUpdateConstants

// Perform storage Patricia update.
// Call patricia_update_using_update_constants() (or the read-optimized variant) instead of
// patricia_update() in order not to repeat globals_pow2 calculation.
func storage_patricia_update{hash_ptr: HashBuiltin*, range_check_ptr}(
    patricia_update_constants: PatriciaUpdateConstants*,
    update_ptr: DictAccess*,
    n_updates: felt,
    height: felt,
    prev_root: felt,
    new_root: felt,
) {
    tempvar should_use_read_optimized: felt;
    %{ ShouldUseReadOptimizedPatriciaUpdate %}
    if (should_use_read_optimized != 0) {
        patricia_update_read_optimized(
            patricia_update_constants=patricia_update_constants,
            update_ptr=update_ptr,
            n_updates=n_updates,
            height=height,
            prev_root=prev_root,
            new_root=new_root,
        );
    } else {
        // Call patricia_update_using_update_constants() instead of patricia_update()
        // in order not to repeat globals_pow2 calculation.
        patricia_update_using_update_constants(
            patricia_update_constants=patricia_update_constants,
            update_ptr=update_ptr,
            n_updates=n_updates,
            height=height,
            prev_root=prev_root,
            new_root=new_root,
        );
    }

    return ();
}
