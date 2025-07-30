from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.bool import FALSE
from starkware.cairo.common.cairo_blake2s.blake2s import encode_felt252_data_and_calc_blake_hash
from starkware.cairo.common.math import assert_lt_felt
from starkware.cairo.common.registers import get_fp_and_pc
from starkware.starknet.core.os.constants import (
    ADD_MOD_GAS_COST,
    BITWISE_BUILTIN_GAS_COST,
    ECOP_GAS_COST,
    MUL_MOD_GAS_COST,
    PEDERSEN_GAS_COST,
    POSEIDON_GAS_COST,
)
from starkware.starknet.core.os.contract_class.compiled_class_struct import (
    COMPILED_CLASS_VERSION,
    CompiledClass,
    CompiledClassEntryPoint,
)
from starkware.starknet.core.os.hash.hash_state_blake import (
    HashState,
    hash_finalize,
    hash_init,
    hash_update_single,
    hash_update_with_nested_hash,
)

// Computes the hash of the given compiled class.
// The full_contract argument is used to determine whether we
// are hashing the full contract or just the used segments.
func compiled_class_hash{range_check_ptr}(compiled_class: CompiledClass*, full_contract: felt) -> (
    hash: felt
) {
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
            data_ptr=compiled_class.bytecode_ptr,
            data_length=compiled_class.bytecode_length,
            full_contract=full_contract,
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
// The hash of a leaf is the Blake2s hash of the data.
// The hash of an internal node is `1 + blake2s(len0, hash0, len1, hash1, ...)` where
// len0 is the total length of the first segment, hash0 is the hash of the first segment, and so on.
//
// For each segment, the *prover* can choose whether to load or skip the segment.
// When full_contract is TRUE, all segments are loaded regardless of their usage.
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
func bytecode_hash_node{range_check_ptr}(
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
        let (hash) = encode_felt252_data_and_calc_blake_hash(data_len=data_length, data=data_ptr);
        return hash;
    }

    %{ bytecode_segments = iter(bytecode_segment_structure.segments) %}

    // Initialize Blake2s hash state for internal node.
    let hash_state: HashState = hash_init();
    with hash_state {
        bytecode_hash_internal_node(
            data_ptr=data_ptr, data_length=data_length, full_contract=full_contract
        );
    }

    let segmented_hash = hash_finalize(hash_state=hash_state);

    // Add 1 to segmented_hash to avoid collisions with the hash of a leaf (domain separation).
    return segmented_hash + 1;
}

// Helper function for bytecode_hash_node.
// Computes the hash of an internal node by adding its children to the hash state.
func bytecode_hash_internal_node{range_check_ptr, hash_state: HashState}(
    data_ptr: felt*, data_length: felt, full_contract: felt
) {
    if (data_length == 0) {
        %{ assert next(bytecode_segments, None) is None %}
        return ();
    }

    alloc_locals;
    local is_leaf_and_loaded;
    local load_segment;
    local segment_length;

    %{
        current_segment_info = next(bytecode_segments)

        should_load = ids.full_contract or is_segment_used_callback(
            ids.data_ptr, current_segment_info.segment_length
        )
        ids.load_segment = 1 if should_load else 0

        is_leaf_and_loaded = should_load and isinstance(current_segment_info.inner_structure, BytecodeLeaf)
        ids.is_leaf_and_loaded = 1 if is_leaf_and_loaded else 0

        ids.segment_length = current_segment_info.segment_length
        vm_enter_scope(new_scope_locals={
            "bytecode_segment_structure": current_segment_info.inner_structure,
            "is_segment_used_callback": is_segment_used_callback
        })
    %}

    if (is_leaf_and_loaded != FALSE) {
        // Repeat the code of bytecode_hash_node() for performance reasons, instead of calling it.
        let (current_segment_hash) = encode_felt252_data_and_calc_blake_hash(
            data_len=segment_length, data=data_ptr
        );
        tempvar range_check_ptr = range_check_ptr;
        tempvar current_segment_hash = current_segment_hash;
    } else {
        // The segment is at least partially loaded, and it is not a leaf.
        if (load_segment != FALSE) {
            let current_segment_hash = bytecode_hash_node(
                data_ptr=data_ptr, data_length=segment_length, full_contract=full_contract
            );
        } else {
            // If `full_contract` is true, this flow is not allowed.
            assert full_contract = FALSE;

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
            tempvar current_segment_hash = nondet %{ bytecode_segment_structure.hash() %};
        }
    }

    // Add the segment length and hash to the hash state.
    hash_update_single(item=segment_length);
    hash_update_single(item=current_segment_hash);

    %{ vm_exit_scope() %}

    return bytecode_hash_internal_node(
        data_ptr=&data_ptr[segment_length],
        data_length=data_length - segment_length,
        full_contract=full_contract,
    );
}

func hash_entry_points{hash_state: HashState, range_check_ptr: felt}(
    entry_points: CompiledClassEntryPoint*, n_entry_points: felt
) {
    let inner_hash_state = hash_init();
    hash_entry_points_inner{hash_state=inner_hash_state, range_check_ptr=range_check_ptr}(
        entry_points=entry_points, n_entry_points=n_entry_points
    );
    let hash: felt = hash_finalize{range_check_ptr=range_check_ptr}(hash_state=inner_hash_state);
    hash_update_single(item=hash);

    return ();
}

func hash_entry_points_inner{hash_state: HashState, range_check_ptr: felt}(
    entry_points: CompiledClassEntryPoint*, n_entry_points: felt
) {
    if (n_entry_points == 0) {
        return ();
    }

    hash_update_single(item=entry_points.selector);
    hash_update_single(item=entry_points.offset);

    // Hash builtins.
    hash_update_with_nested_hash{hash_state=hash_state, range_check_ptr=range_check_ptr}(
        data_ptr=entry_points.builtin_list, data_length=entry_points.n_builtins
    );

    return hash_entry_points_inner(
        entry_points=&entry_points[1], n_entry_points=n_entry_points - 1
    );
}
