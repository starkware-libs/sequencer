%builtins output pedersen range_check ecdsa bitwise ec_op keccak poseidon range_check96 add_mod mul_mod

from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.bool import TRUE
from starkware.cairo.common.cairo_builtins import (
    BitwiseBuiltin,
    EcOpBuiltin,
    HashBuiltin,
    KeccakBuiltin,
    ModBuiltin,
    PoseidonBuiltin,
)
from starkware.cairo.common.dict import dict_new
from starkware.cairo.common.dict_access import DictAccess
from starkware.cairo.common.math import assert_not_equal
from starkware.cairo.common.registers import get_label_location
from starkware.cairo.common.segments import relocate_segment
from starkware.starknet.core.os.block_context import (
    BlockContext,
    CompiledClassFactsBundle,
    OsGlobalContext,
    VirtualOsConfig,
    get_block_context,
)
from starkware.starknet.core.os.builtins import get_builtin_params
from starkware.starknet.core.os.contract_class.compiled_class import (
    guess_compiled_class_facts,
    validate_compiled_class_facts_post_execution,
)
from starkware.starknet.core.os.contract_class.deprecated_compiled_class import (
    deprecated_load_compiled_class_facts,
)
from starkware.starknet.core.os.execution.deprecated_execute_syscalls import (
    execute_deprecated_syscalls,
)
from starkware.starknet.core.os.execution.execute_syscalls import execute_syscalls
from starkware.starknet.core.os.execution.execute_transactions import execute_transactions
from starkware.starknet.core.os.os_config.os_config import (
    StarknetOsConfig,
    get_public_keys_hash,
    get_starknet_os_config_hash,
)
from starkware.starknet.core.os.os_utils import (
    get_block_os_output_header,
    get_virtual_os_config,
    pre_process_block,
    process_os_output,
)
from starkware.starknet.core.os.output import (
    MessageToL1Header,
    MessageToL2Header,
    OsCarriedOutputs,
    OsOutput,
)
from starkware.starknet.core.os.state.state import OsStateUpdate, state_update

// The main entry point of the Starknet OS.
//
// This function orchestrates the execution of Starknet blocks. It performs the following key steps:
// 1.  **Initialization**: Sets up the OS global context, including public keys and configuration.
// 2.  **Block Execution**: Iterates through the provided blocks (`n_blocks`). For each block, it:
//     - Executes transactions.
//     - Updates the state.
//     - Generates an OS output for the block.
// 3.  **Fact Validation**: Validates that the compiled class facts (CASM) used during execution
//     match the expected hashes.
// 4.  **Output Processing**: Aggregates the per-block outputs into a final OS output and
//     serializes it to the output segment.
// 5.  **Safety Checks**: Verifies that the range check builtin pointer has advanced correctly,
//     ensuring that internal OS range checks were not compromised by transaction execution.
func main{
    output_ptr: felt*,
    pedersen_ptr: HashBuiltin*,
    range_check_ptr,
    ecdsa_ptr,
    bitwise_ptr: BitwiseBuiltin*,
    ec_op_ptr: EcOpBuiltin*,
    keccak_ptr: KeccakBuiltin*,
    poseidon_ptr: PoseidonBuiltin*,
    range_check96_ptr: felt*,
    add_mod_ptr: ModBuiltin*,
    mul_mod_ptr: ModBuiltin*,
}() {
    alloc_locals;
    %{ StarknetOsInput %}

    // Reserve the initial range check for self validation.
    // Note that this must point to the first range check used by the OS.
    let initial_range_check_ptr = range_check_ptr;
    let range_check_ptr = range_check_ptr + 1;

    local public_keys: felt*;
    local n_public_keys: felt;
    %{ GetPublicKeys %}

    // Build OS global context.
    let os_global_context = get_os_global_context(
        n_public_keys=n_public_keys, public_keys=public_keys
    );

    // Execute blocks.
    local n_blocks = nondet %{ len(os_input.block_inputs) %};
    let (local os_outputs: OsOutput*) = alloc();
    %{ InitStateUpdatePointers %}
    local initial_txs_range_check_ptr = nondet %{ segments.add_temp_segment() %};
    let txs_range_check_ptr = initial_txs_range_check_ptr;
    with txs_range_check_ptr {
        execute_blocks(
            n_blocks=n_blocks,
            os_output_per_block_dst=os_outputs,
            os_global_context=os_global_context,
        );
    }

    // Validate the guessed compile class facts.
    let compiled_class_facts_bundle = os_global_context.compiled_class_facts_bundle;
    validate_compiled_class_facts_post_execution(
        n_compiled_class_facts=compiled_class_facts_bundle.n_compiled_class_facts,
        compiled_class_facts=compiled_class_facts_bundle.compiled_class_facts,
        builtin_costs=compiled_class_facts_bundle.builtin_costs,
    );

    // Process and serialize the OS output to the output segment.
    process_os_output(
        n_blocks=n_blocks,
        os_outputs=os_outputs,
        n_public_keys=n_public_keys,
        public_keys=public_keys,
    );

    // The following code deals with the problem that untrusted code (contract code) could
    // potentially move builtin pointers backward, compromising the soundness of those builtins.
    //
    // The check that the pointers can only move forward is done in `validate_builtins`,
    // but `validate_builtins` itself relies on range checks.
    //
    // To guarantee the validity of this mechanism, we split range checks into:
    //   * OS range checks (used internally by the OS, and in particular by `validate_builtins`).
    //   * Transaction range checks (used by contracts).
    //
    // The OS range checks are located at `[initial_range_check_ptr, reserved_range_checks_end)` and
    // the transaction range checks are located at `[reserved_range_checks_end, range_check_ptr)`.
    //
    // The following `assert_not_equal` guarantees that at least one value
    // (`[initial_range_check_ptr]`) is range-checked by ensuring
    //   `range_check_ptr != initial_range_check_ptr`,
    // which implies
    //   `range_check_ptr >= initial_range_check_ptr + 1`,
    // since the bootloader checks `range_check_ptr >= initial_range_check_ptr`.
    // Combined with the next assertion, we establish that
    //   `range_check_ptr >= reserved_range_checks_end`,
    // which confirms that all OS range checks are sound.
    //
    // Since `validate_builtins` relies on OS range checks, proving their validity ensures that all
    // calls to `validate_builtins` remain sound, thereby maintaining the integrity of all the
    // builtins.
    let reserved_range_checks_end = range_check_ptr;
    // Relocate the range checks used by the transactions after the range checks used by the OS.
    relocate_segment(
        src_ptr=cast(initial_txs_range_check_ptr, felt*),
        dest_ptr=cast(reserved_range_checks_end, felt*),
    );
    let range_check_ptr = txs_range_check_ptr;

    assert_not_equal(initial_range_check_ptr, range_check_ptr);
    assert [initial_range_check_ptr] = range_check_ptr - reserved_range_checks_end;

    return ();
}

// Executes `n_blocks` blocks. For each block,
//   * Runs the transactions,
//   * Updates the state,
//   * Produces the OS output and writes it to `os_output_per_block_dst`.
//
// Hint arguments:
// state_update_pointers - A class that manages the pointers of the squashed state changes.
// block_input_iterator - an iterator for Block-related input, such as the transaction to execute.
// global_hints - hints that are used to create the execution helper.
func execute_blocks{
    output_ptr: felt*,
    pedersen_ptr: HashBuiltin*,
    range_check_ptr,
    ecdsa_ptr,
    bitwise_ptr: BitwiseBuiltin*,
    ec_op_ptr: EcOpBuiltin*,
    keccak_ptr: KeccakBuiltin*,
    poseidon_ptr: PoseidonBuiltin*,
    range_check96_ptr: felt*,
    add_mod_ptr: ModBuiltin*,
    mul_mod_ptr: ModBuiltin*,
    txs_range_check_ptr,
}(n_blocks: felt, os_output_per_block_dst: OsOutput*, os_global_context: OsGlobalContext*) {
    %{ LogRemainingBlocks %}
    if (n_blocks == 0) {
        return ();
    }
    alloc_locals;

    %{ CreateBlockAdditionalHints %}

    // Initialize the carried outputs and the state changes dictionaries.
    let (messages_to_l1: MessageToL1Header*) = alloc();
    let (messages_to_l2: MessageToL2Header*) = alloc();
    tempvar initial_carried_outputs = new OsCarriedOutputs(
        messages_to_l1=messages_to_l1, messages_to_l2=messages_to_l2
    );
    let (
        contract_state_changes: DictAccess*, contract_class_changes: DictAccess*
    ) = initialize_state_changes();
    let contract_state_changes_start = contract_state_changes;
    let contract_class_changes_start = contract_class_changes;

    // Build block context.
    let (block_context: BlockContext*) = get_block_context(os_global_context=os_global_context);

    // Pre-process the block.
    with contract_state_changes, contract_class_changes {
        pre_process_block(block_context=block_context);
    }

    // Execute transactions.
    let outputs = initial_carried_outputs;
    with contract_state_changes, contract_class_changes, outputs {
        execute_transactions(block_context=block_context);
    }
    let final_carried_outputs = outputs;

    // Update the state.
    %{ EnterScopeWithAliases %}
    let (squashed_os_state_update, state_update_output) = state_update{hash_ptr=pedersen_ptr}(
        os_state_update=OsStateUpdate(
            contract_state_changes_start=contract_state_changes_start,
            contract_state_changes_end=contract_state_changes,
            contract_class_changes_start=contract_class_changes_start,
            contract_class_changes_end=contract_class_changes,
        ),
        should_allocate_aliases=TRUE,
    );
    %{ vm_exit_scope() %}

    // Write the OS block output.
    let os_output_header = get_block_os_output_header(
        block_context=block_context,
        state_update_output=state_update_output,
        os_global_context=os_global_context,
    );
    assert os_output_per_block_dst[0] = OsOutput(
        header=os_output_header,
        squashed_os_state_update=squashed_os_state_update,
        initial_carried_outputs=initial_carried_outputs,
        final_carried_outputs=final_carried_outputs,
    );

    return execute_blocks(
        n_blocks=n_blocks - 1,
        os_output_per_block_dst=&os_output_per_block_dst[1],
        os_global_context=os_global_context,
    );
}

// Initializes state changes dictionaries.
func initialize_state_changes() -> (
    contract_state_changes: DictAccess*, contract_class_changes: DictAccess*
) {
    %{ InitializeStateChanges %}
    // A dictionary from contract address to a dict of storage changes of type StateEntry.
    let (contract_state_changes: DictAccess*) = dict_new();

    %{ InitializeClassHashes %}
    // A dictionary from class hash to compiled class hash (Casm).
    let (contract_class_changes: DictAccess*) = dict_new();

    return (
        contract_state_changes=contract_state_changes, contract_class_changes=contract_class_changes
    );
}

// Returns an OsGlobalContext instance.
//
// Note: the compiled class facts are guessed here, and must be validated post-execution.
func get_os_global_context{
    pedersen_ptr: HashBuiltin*, range_check_ptr, poseidon_ptr: PoseidonBuiltin*
}(n_public_keys: felt, public_keys: felt*) -> OsGlobalContext* {
    alloc_locals;
    // Compiled class facts bundle.
    let (n_compiled_class_facts, compiled_class_facts, builtin_costs) = guess_compiled_class_facts(
        );
    let (
        n_deprecated_compiled_class_facts, deprecated_compiled_class_facts
    ) = deprecated_load_compiled_class_facts();

    // Starknet OS config.
    let (public_keys_hash) = get_public_keys_hash{hash_ptr=pedersen_ptr}(
        n_public_keys=n_public_keys, public_keys=public_keys
    );
    tempvar starknet_os_config = new StarknetOsConfig(
        chain_id=nondet %{ os_hints_config.starknet_os_config.chain_id %},
        fee_token_address=nondet %{ os_hints_config.starknet_os_config.fee_token_address %},
        public_keys_hash=public_keys_hash,
    );
    let (starknet_os_config_hash) = get_starknet_os_config_hash{hash_ptr=pedersen_ptr}(
        starknet_os_config=starknet_os_config
    );

    // Function pointers.
    let (execute_syscalls_ptr) = get_label_location(label_value=execute_syscalls);
    let (execute_deprecated_syscalls_ptr) = get_label_location(
        label_value=execute_deprecated_syscalls
    );

    let virtual_os_config = get_virtual_os_config();
    tempvar os_global_context: OsGlobalContext* = new OsGlobalContext(
        starknet_os_config=[starknet_os_config],
        starknet_os_config_hash=starknet_os_config_hash,
        virtual_os_config=virtual_os_config,
        compiled_class_facts_bundle=CompiledClassFactsBundle(
            n_compiled_class_facts=n_compiled_class_facts,
            compiled_class_facts=compiled_class_facts,
            builtin_costs=builtin_costs,
            n_deprecated_compiled_class_facts=n_deprecated_compiled_class_facts,
            deprecated_compiled_class_facts=deprecated_compiled_class_facts,
        ),
        builtin_params=get_builtin_params(),
        execute_syscalls_ptr=execute_syscalls_ptr,
        execute_deprecated_syscalls_ptr=execute_deprecated_syscalls_ptr,
    );
    return os_global_context;
}
