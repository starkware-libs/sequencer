use std::collections::HashMap;

use cairo_vm::hint_processor::hint_processor_definition::{HintProcessor, HintReference};
use cairo_vm::serde::deserialize_program::ApTracking;
use cairo_vm::types::exec_scope::ExecutionScopes;
use cairo_vm::vm::vm_core::VirtualMachine;
use indoc::indoc;
use starknet_types_core::felt::Felt;

use crate::hints::block_context::{
    block_number,
    block_timestamp,
    bytecode_segment_structure,
    chain_id,
    deprecated_fee_token_address,
    elements_ge_10,
    elements_ge_2,
    fee_token_address,
    get_block_mapping,
    is_leaf,
    load_class,
    load_class_facts,
    load_class_inner,
    sequencer_address,
    write_use_kzg_da_to_memory,
};
use crate::hints::error::{HintExtensionResult, HintResult, OsHintError};
use crate::hints::types::{HintEnum, HintExtensionImplementation, HintImplementation};
use crate::{define_hint_enum, define_hint_extension_enum};

define_hint_enum!(
    Hint,
    (
        LoadClassFacts,
        load_class_facts,
        indoc! {r#"
    ids.compiled_class_facts = segments.add()
    ids.n_compiled_class_facts = len(os_input.compiled_classes)
    vm_enter_scope({
        'compiled_class_facts': iter(os_input.compiled_classes.items()),
        'compiled_class_visited_pcs': os_input.compiled_class_visited_pcs,
    })"#}
    ),
    (
        LoadClassInner,
        load_class_inner,
        indoc! {r#"
    from starkware.starknet.core.os.contract_class.compiled_class_hash import (
        create_bytecode_segment_structure,
        get_compiled_class_struct,
    )

    compiled_class_hash, compiled_class = next(compiled_class_facts)

    bytecode_segment_structure = create_bytecode_segment_structure(
        bytecode=compiled_class.bytecode,
        bytecode_segment_lengths=compiled_class.bytecode_segment_lengths,
        visited_pcs=compiled_class_visited_pcs[compiled_class_hash],
    )

    cairo_contract = get_compiled_class_struct(
        identifiers=ids._context.identifiers,
        compiled_class=compiled_class,
        bytecode=bytecode_segment_structure.bytecode_with_skipped_segments()
    )
    ids.compiled_class = segments.gen_arg(cairo_contract)"#}
    ),
    (
        BytecodeSegmentStructure,
        bytecode_segment_structure,
        indoc! {r#"
    vm_enter_scope({
        "bytecode_segment_structure": bytecode_segment_structure
    })"#}
    ),
    (
        BlockNumber,
        block_number,
        "memory[ap] = to_felt_or_relocatable(syscall_handler.block_info.block_number)"
    ),
    (
        BlockTimestamp,
        block_timestamp,
        "memory[ap] = to_felt_or_relocatable(syscall_handler.block_info.block_timestamp)"
    ),
    (
        ChainId,
        chain_id,
        "memory[ap] = to_felt_or_relocatable(os_input.general_config.chain_id.value)"
    ),
    (
        FeeTokenAddress,
        fee_token_address,
        "memory[ap] = to_felt_or_relocatable(os_input.general_config.fee_token_address)"
    ),
    (
        DeprecatedFeeTokenAddress,
        deprecated_fee_token_address,
        "memory[ap] = to_felt_or_relocatable(os_input.general_config.deprecated_fee_token_address)"
    ),
    (
        SequencerAddress,
        sequencer_address,
        "memory[ap] = to_felt_or_relocatable(syscall_handler.block_info.sequencer_address)"
    ),
    (
        GetBlockMapping,
        get_block_mapping,
        indoc! {r#"
    ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[
        ids.BLOCK_HASH_CONTRACT_ADDRESS
    ]"#}
    ),
    (
        ElementsGe10,
        elements_ge_10,
        "memory[ap] = to_felt_or_relocatable(ids.elements_end - ids.elements >= 10)"
    ),
    (
        ElementsGe2,
        elements_ge_2,
        "memory[ap] = to_felt_or_relocatable(ids.elements_end - ids.elements >= 2)"
    ),
    (
        IsLeaf,
        is_leaf,
        indoc! {r#"
    from starkware.starknet.core.os.contract_class.compiled_class_hash_objects import (
        BytecodeLeaf,
    )
    ids.is_leaf = 1 if isinstance(bytecode_segment_structure, BytecodeLeaf) else 0"#}
    ),
    (
        WriteUseKzgDaToMemory,
        write_use_kzg_da_to_memory,
        indoc! {r#"
    memory[fp + 18] = to_felt_or_relocatable(syscall_handler.block_info.use_kzg_da and (
        not os_input.full_output
    ))"#}
    )
);

define_hint_extension_enum!(
    HintExtension,
    (
        LoadClass,
        load_class,
        indoc! {r#"
    computed_hash = ids.compiled_class_fact.hash
    expected_hash = compiled_class_hash
    assert computed_hash == expected_hash, (
        "Computed compiled_class_hash is inconsistent with the hash in the os_input. "
        f"Computed hash = {computed_hash}, Expected hash = {expected_hash}.")

    vm_load_program(
        compiled_class.get_runnable_program(entrypoint_builtins=[]),
        ids.compiled_class.bytecode_ptr
    )"#
        }
    ),
);
