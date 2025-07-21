from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.bool import FALSE
from starkware.cairo.common.cairo_builtins import PoseidonBuiltin
from starkware.cairo.common.hash_state_poseidon import (
    HashState,
    hash_finalize,
    hash_init,
    hash_update_single,
    hash_update_with_nested_hash,
    poseidon_hash_many,
)
from starkware.cairo.common.poseidon_state import PoseidonBuiltinState
from starkware.starknet.core.os.contract_class.compiled_class_struct import (
    COMPILED_CLASS_VERSION,
    CompiledClass,
    CompiledClassEntryPoint,
)

func compiled_class_hash{range_check_ptr, poseidon_ptr: PoseidonBuiltin*}(
    compiled_class: CompiledClass*, full_contract: felt,
) -> (hash: felt) {
    alloc_locals;
    assert compiled_class.compiled_class_version = COMPILED_CLASS_VERSION;

    let hash_state: HashState = hash_init();
    with hash_state {
        hash_update_single(item=compiled_class.compiled_class_version);

        // Hash external entry points.
        hash_entry_points(
            entry_points=compiled_class.external_functions,
            n_entry_points=compiled_class.n_external_functions,
        );

        // Hash L1 handler entry points.
        hash_entry_points(
            entry_points=compiled_class.l1_handlers, n_entry_points=compiled_class.n_l1_handlers
        );

        // Hash constructor entry points.
        hash_entry_points(
            entry_points=compiled_class.constructors, n_entry_points=compiled_class.n_constructors
        );

        // Hash bytecode.
        let bytecode_hash = bytecode_hash_node(
            data_ptr=compiled_class.bytecode_ptr, data_length=compiled_class.bytecode_length, full_contract=full_contract
        );
        hash_update_single(item=bytecode_hash);
    }

    let hash: felt = hash_finalize(hash_state=hash_state);
    return (hash=hash);
}

// Returns the hash of the contract class bytecode according to its segments.
//
// The hash is computed according to a segment tree. Each segment may be either a leaf or divided
// into smaller segments (internal node).
// For example, the bytecode may be divided into functions and each function can be divided
// according to its branches.
//
// The hash of a leaf is the Poseidon hash the data.
// The hash of an internal node is `1 + poseidon(len0, hash0, len1, hash1, ...)` where
// len0 is the total length of the first segment, hash0 is the hash of the first segment, and so on.
//
// For each segment, the *prover* can choose whether to load or skip the segment.
//
// * Loaded segment:
//   For leaves, the data will be fully loaded into memory.
//   For internal nodes, the prover can choose to load/skip each of the children separately.
//
// * Skipped segment:
//   The inner structure of that segment is ignored.
//   The only guarantee is that the first field element is enforced to be -1.
//   The rest of the field elements are unconstrained.
//   The fact that a skipped segment is guaranteed to begin with -1 implies that the execution of
//   the program cannot visit the start of the segment, as -1 is not a valid Cairo opcode.
//
// In the example above of division according to functions and branches, a function may be skipped
// entirely or partially.
// As long as one function does not jump into the middle of another function and as long as there
// are no jumps into the middle of a branch segment, the loading process described above will be
// sound.
//
// Hint arguments:
// bytecode_segment_structure: A BytecodeSegmentStructure object that describes the bytecode
// structure.
// is_segment_used_callback: A callback that returns whether a segment is used.
func bytecode_hash_node{range_check_ptr, poseidon_ptr: PoseidonBuiltin*}(
    data_ptr: felt*, data_length: felt, full_contract: felt
) -> felt {
    alloc_locals;

    local is_leaf;

    %{
        from starkware.starknet.core.os.contract_class.compiled_class_hash_objects import (
            BytecodeLeaf,
        )
        ids.is_leaf = 1 if isinstance(bytecode_segment_structure, BytecodeLeaf) else 0
    %}

    // Guess if the bytecode is a leaf or an internal node in the tree.
    if (is_leaf != FALSE) {
        // If the bytecode is a leaf, it must be loaded into memory. Compute its hash.
        let (hash) = poseidon_hash_many(n=data_length, elements=data_ptr);
        return hash;
    }

    %{ bytecode_segments = iter(bytecode_segment_structure.segments) %}

    // Use the poseidon builtin directly for performance reasons.
    let poseidon_state = PoseidonBuiltinState(s0=0, s1=0, s2=0);
    bytecode_hash_internal_node{poseidon_state=poseidon_state}(
        data_ptr=data_ptr, data_length=data_length, full_contract=full_contract
    );

    // Pad input with [1, 0]. See implementation of poseidon_hash_many().
    assert poseidon_ptr.input = PoseidonBuiltinState(
        s0=poseidon_state.s0 + 1, s1=poseidon_state.s1, s2=poseidon_state.s2
    );
    let segmented_hash = poseidon_ptr.output.s0;
    let poseidon_ptr = &poseidon_ptr[1];

    // Add 1 to segmented_hash to avoid collisions with the hash of a leaf (domain separation).
    return segmented_hash + 1;
}

// Helper function for bytecode_hash_node.
// Computes the hash of an internal node by adding its children to the hash state.
func bytecode_hash_internal_node{
    range_check_ptr, poseidon_ptr: PoseidonBuiltin*, poseidon_state: PoseidonBuiltinState
}(data_ptr: felt*, data_length: felt, full_contract: felt) {
    if (data_length == 0) {
        %{ assert next(bytecode_segments, None) is None %}
        return ();
    }

    alloc_locals;
    local is_leaf;
    local is_segment_used;
    local segment_length;

    %{
        current_segment_info = next(bytecode_segments)

        is_used = is_segment_used_callback(ids.data_ptr, current_segment_info.segment_length)
        ids.is_segment_used = 1 if is_used else 0

        is_leaf = isinstance(current_segment_info.inner_structure, BytecodeLeaf)
        ids.is_leaf = 1 if is_leaf else 0

        ids.segment_length = current_segment_info.segment_length
        vm_enter_scope(new_scope_locals={
            "bytecode_segment_structure": current_segment_info.inner_structure,
            "is_segment_used_callback": is_segment_used_callback
        })
    %}

    tempvar is_segment_used = 1 - ((1 - full_contract) * (1 - is_segment_used));
    tempvar is_used_leaf = is_leaf * is_segment_used;

    if (is_used_leaf != FALSE) {
        // Repeat the code of bytecode_hash_node() for performance reasons, instead of calling it.
        let (current_segment_hash) = poseidon_hash_many(n=segment_length, elements=data_ptr);
        tempvar range_check_ptr = range_check_ptr;
        tempvar poseidon_ptr = poseidon_ptr;
        tempvar current_segment_hash = current_segment_hash;
    } else {
        if (is_segment_used != FALSE) {
            let current_segment_hash = bytecode_hash_node(
                data_ptr=data_ptr, data_length=segment_length, full_contract=full_contract
            );
        } else {
            // Set the first felt of the bytecode to -1 to make sure that the execution cannot jump
            // to this segment (-1 is an invalid opcode).
            // The hash in this case is guessed and the actual bytecode is unconstrained (except for
            // the first felt).
            %{
                # Sanity check.
                assert not is_accessed(ids.data_ptr), "The segment is skipped but was accessed."
                del memory.data[ids.data_ptr]
            %}
            assert data_ptr[0] = -1;

            assert [range_check_ptr] = segment_length;
            tempvar range_check_ptr = range_check_ptr + 1;
            tempvar poseidon_ptr = poseidon_ptr;
            tempvar current_segment_hash = nondet %{ bytecode_segment_structure.hash() %};
        }
    }

    // Add the segment length and hash to the hash state.
    // Use the poseidon builtin directly for performance reasons.
    assert poseidon_ptr.input = PoseidonBuiltinState(
        s0=poseidon_state.s0 + segment_length,
        s1=poseidon_state.s1 + current_segment_hash,
        s2=poseidon_state.s2,
    );
    let poseidon_state = poseidon_ptr.output;
    let poseidon_ptr = &poseidon_ptr[1];

    %{ vm_exit_scope() %}

    return bytecode_hash_internal_node(
        data_ptr=&data_ptr[segment_length], data_length=data_length - segment_length, full_contract=full_contract
    );
}

func hash_entry_points{poseidon_ptr: PoseidonBuiltin*, hash_state: HashState}(
    entry_points: CompiledClassEntryPoint*, n_entry_points: felt
) {
    let inner_hash_state = hash_init();
    hash_entry_points_inner{hash_state=inner_hash_state}(
        entry_points=entry_points, n_entry_points=n_entry_points
    );
    let hash: felt = hash_finalize(hash_state=inner_hash_state);
    hash_update_single(item=hash);

    return ();
}

func hash_entry_points_inner{poseidon_ptr: PoseidonBuiltin*, hash_state: HashState}(
    entry_points: CompiledClassEntryPoint*, n_entry_points: felt
) {
    if (n_entry_points == 0) {
        return ();
    }

    hash_update_single(item=entry_points.selector);
    hash_update_single(item=entry_points.offset);

    // Hash builtins.
    hash_update_with_nested_hash(
        data_ptr=entry_points.builtin_list, data_length=entry_points.n_builtins
    );

    return hash_entry_points_inner(
        entry_points=&entry_points[1], n_entry_points=n_entry_points - 1
    );
}
