from starkware.cairo.common.cairo_builtins import HashBuiltin, PoseidonBuiltin
from starkware.cairo.common.math import unsigned_div_rem
from starkware.cairo.common.registers import get_fp_and_pc
from starkware.starknet.common.new_syscalls import BlockInfo
from starkware.starknet.core.os.builtins import BuiltinParams, get_builtin_params
from starkware.starknet.core.os.constants import (
    VALIDATE_BLOCK_NUMBER_ROUNDING,
    VALIDATE_TIMESTAMP_ROUNDING,
)
from starkware.starknet.core.os.contract_class.compiled_class import CompiledClassFact
from starkware.starknet.core.os.contract_class.deprecated_compiled_class import (
    DeprecatedCompiledClassFact,
)
from starkware.starknet.core.os.os_config.os_config import StarknetOsConfig

// Configuration for the virtual OS.
// Virtual OS is a restricted mode for client-side proving, allowing the execution of transactions
// over an existing Starknet block.
// This mode should be disabled in the Starknet sequencer.
struct VirtualOsConfig {
    enabled: felt,
}

// Struct to group compiled class facts parameters.
struct CompiledClassFactsBundle {
    n_compiled_class_facts: felt,
    compiled_class_facts: CompiledClassFact*,
    builtin_costs: felt*,
    n_deprecated_compiled_class_facts: felt,
    deprecated_compiled_class_facts: DeprecatedCompiledClassFact*,
}

// Holds global context for the OS execution.
struct OsGlobalContext {
    // OS config available globally for all blocks.
    starknet_os_config: StarknetOsConfig,
    starknet_os_config_hash: felt,

    // Configuration for the virtual OS.
    virtual_os_config: VirtualOsConfig,

    // Compiled class facts available globally for all blocks.
    compiled_class_facts_bundle: CompiledClassFactsBundle,

    // Parameters for select_builtins.
    builtin_params: BuiltinParams*,
    // A function pointer to the 'execute_syscalls' function.
    execute_syscalls_ptr: felt*,
    // A function pointer to the 'execute_deprecated_syscalls' function.
    execute_deprecated_syscalls_ptr: felt*,
}

// Represents information that is the same throughout the block.
struct BlockContext {
    os_global_context: OsGlobalContext,
    // Information about the block.
    block_info_for_execute: BlockInfo*,
    // A version of `block_info` that will be returned by the 'get_execution_info'
    // syscall during '__validate__'.
    // Some of the fields, which cannot be used in validate mode, are zeroed out.
    block_info_for_validate: BlockInfo*,
}

// Returns a BlockContext instance.
//
// 'syscall_handler' should be passed as a hint variable.
func get_block_context{range_check_ptr}(os_global_context: OsGlobalContext*) -> (
    block_context: BlockContext*
) {
    alloc_locals;
    tempvar block_number = nondet %{ syscall_handler.block_info.block_number %};
    tempvar block_timestamp = nondet %{ syscall_handler.block_info.block_timestamp %};
    let (divided_block_number, _) = unsigned_div_rem(block_number, VALIDATE_BLOCK_NUMBER_ROUNDING);
    tempvar block_number_for_validate = divided_block_number * VALIDATE_BLOCK_NUMBER_ROUNDING;
    let (divided_block_timestamp, _) = unsigned_div_rem(
        block_timestamp, VALIDATE_TIMESTAMP_ROUNDING
    );
    tempvar block_timestamp_for_validate = divided_block_timestamp * VALIDATE_TIMESTAMP_ROUNDING;
    let compiled_class_facts_bundle = os_global_context.compiled_class_facts_bundle;
    local block_context: BlockContext = BlockContext(
        os_global_context=[os_global_context],
        block_info_for_execute=new BlockInfo(
            block_number=block_number,
            block_timestamp=block_timestamp,
            sequencer_address=nondet %{ syscall_handler.block_info.sequencer_address %},
        ),
        block_info_for_validate=new BlockInfo(
            block_number=block_number_for_validate,
            block_timestamp=block_timestamp_for_validate,
            sequencer_address=0,
        ),
    );

    let (__fp__, _) = get_fp_and_pc();
    return (block_context=&block_context);
}
