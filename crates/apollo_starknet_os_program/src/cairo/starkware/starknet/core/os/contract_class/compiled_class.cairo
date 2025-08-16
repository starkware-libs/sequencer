from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.bool import FALSE, TRUE
from starkware.cairo.common.cairo_builtins import PoseidonBuiltin
from starkware.cairo.common.hash_state_poseidon import (
    HashState,
    hash_finalize,
    hash_init,
    hash_update_single,
    hash_update_with_nested_hash,
    poseidon_hash_many,
)
from starkware.cairo.common.math import assert_lt_felt
from starkware.cairo.common.poseidon_state import PoseidonBuiltinState
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
    CompiledClassFact,
)
from starkware.starknet.core.os.contract_class.blake_compiled_class_hash import (
    compiled_class_hash as blake_compiled_class_hash,
)

// Checks that the list of selectors is sorted.
func validate_entry_points{range_check_ptr}(
    n_entry_points: felt, entry_points: CompiledClassEntryPoint*
) {
    if (n_entry_points == 0) {
        return ();
    }

    return validate_entry_points_inner(
        n_entry_points=n_entry_points - 1,
        entry_points=&entry_points[1],
        prev_selector=entry_points[0].selector,
    );
}

// Inner function for validate_entry_points.
func validate_entry_points_inner{range_check_ptr}(
    n_entry_points: felt, entry_points: CompiledClassEntryPoint*, prev_selector
) {
    if (n_entry_points == 0) {
        return ();
    }

    assert_lt_felt(prev_selector, entry_points[0].selector);

    return validate_entry_points_inner(
        n_entry_points=n_entry_points - 1,
        entry_points=&entry_points[1],
        prev_selector=entry_points[0].selector,
    );
}

// Guesses the contract classes from the 'os_input' hint variable without validating their hashes.
// Returns CompiledClassFact list that maps a hash to a CompiledClass, and the builtin costs list
// which is appended to every contract.
//
// Note: `validate_compiled_class_facts` must be called eventually to complete the validation.
func guess_compiled_class_facts{poseidon_ptr: PoseidonBuiltin*, range_check_ptr}() -> (
    n_compiled_class_facts: felt, compiled_class_facts: CompiledClassFact*, builtin_costs: felt*
) {
    alloc_locals;

    local builtin_costs: felt* = new (
        PEDERSEN_GAS_COST,
        BITWISE_BUILTIN_GAS_COST,
        ECOP_GAS_COST,
        POSEIDON_GAS_COST,
        ADD_MOD_GAS_COST,
        MUL_MOD_GAS_COST,
    );
    local n_compiled_class_facts;
    local compiled_class_facts: CompiledClassFact*;
    %{ LoadClassesAndBuildBytecodeSegmentStructures %}

    return (
        n_compiled_class_facts=n_compiled_class_facts,
        compiled_class_facts=compiled_class_facts,
        builtin_costs=builtin_costs,
    );
}

// Validates the compiled class facts structure and hash after the execution.
// Uses the execution info to optimize hash computation.
func validate_compiled_class_facts_post_execution{poseidon_ptr: PoseidonBuiltin*, range_check_ptr}(
    n_compiled_class_facts, compiled_class_facts: CompiledClassFact*, builtin_costs: felt*
) {
    validate_compiled_class_facts(
        n_compiled_class_facts=n_compiled_class_facts,
        compiled_class_facts=compiled_class_facts,
        builtin_costs=builtin_costs,
    );

    return ();
}

// Validates the compiled class facts structure and hash, using the hint variable
// `bytecode_segment_structures` - a mapping from compilied class hash to the structure.
func validate_compiled_class_facts{poseidon_ptr: PoseidonBuiltin*, range_check_ptr}(
    n_compiled_class_facts, compiled_class_facts: CompiledClassFact*, builtin_costs: felt*
) {
    if (n_compiled_class_facts == 0) {
        return ();
    }
    alloc_locals;

    let compiled_class_fact = &compiled_class_facts[0];
    let compiled_class = compiled_class_fact.compiled_class;

    validate_entry_points(
        n_entry_points=compiled_class.n_external_functions,
        entry_points=compiled_class.external_functions,
    );

    validate_entry_points(
        n_entry_points=compiled_class.n_l1_handlers, entry_points=compiled_class.l1_handlers
    );
    // Compiled classes are expected to end with a `ret` opcode followed by a pointer to the
    // builtin costs.
    assert compiled_class.bytecode_ptr[compiled_class.bytecode_length] = 0x208b7fff7fff7ffe;
    assert compiled_class.bytecode_ptr[compiled_class.bytecode_length + 1] = cast(
        builtin_costs, felt
    );

    // Calculate the compiled class hash.
    // This hint enter a new scope that contains the bytecode segment structure.
    %{ EnterScopeWithBytecodeSegmentStructure %}
    let (hash) = blake_compiled_class_hash(compiled_class, full_contract=FALSE);
    %{
        vm_exit_scope()

        computed_hash = ids.hash
        expected_hash = ids.compiled_class_fact.hash
        assert computed_hash == expected_hash, (
            "Computed compiled_class_hash is inconsistent with the hash in the os_input. "
            f"Computed hash = {computed_hash}, Expected hash = {expected_hash}.")
    %}

    assert compiled_class_fact.hash = hash;

    return validate_compiled_class_facts(
        n_compiled_class_facts=n_compiled_class_facts - 1,
        compiled_class_facts=&compiled_class_facts[1],
        builtin_costs=builtin_costs,
    );
}
