use std::path::PathBuf;

use apollo_infra_utils::cairo0_compiler::cairo0_format;
use apollo_infra_utils::compile_time_cargo_manifest_dir;
use blockifier::blockifier_versioned_constants::{OsConstants, VersionedConstants};
use blockifier::execution::syscalls::vm_syscall_utils::SyscallSelector;
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector};
use starknet_types_core::felt::Felt;

const CONSTANTS_CONTENTS: &str = include_str!("cairo/starkware/starknet/core/os/constants.cairo");

fn selector_to_hex(selector: &EntryPointSelector) -> String {
    format!("{:#?}", selector.0)
}

fn contract_address_to_hex(address: &ContractAddress) -> String {
    format!("{:#?}", address.0.key())
}

fn base_only_syscall_cost(selector: SyscallSelector, os_constants: &OsConstants) -> u64 {
    let syscall_cost =
        os_constants.gas_costs.syscalls.get_syscall_gas_cost(&selector).unwrap_or_else(|error| {
            panic!("Selector {selector:?} should have a gas cost. Error: {error}")
        });
    assert_eq!(syscall_cost.linear_syscall_cost(), 0, "Syscall {selector:?} has a linear cost.");
    syscall_cost.base_syscall_cost()
}

fn quote_string(s: &str) -> String {
    format!("'{s}'")
}

/// Create constants from a list of class hashes. Example:
/// ```
/// let expected = #"
/// X_0 = 0x1;
/// X_1 = 0x2;
/// X_LEN = 2;
/// "#;
/// assert_eq!(stringify_class_hash_list("X", &[ClassHash(1), ClassHash(2)]), expected);
/// ```
fn stringify_class_hash_list(name: &str, class_hashes: &[ClassHash]) -> String {
    class_hashes
        .iter()
        .enumerate()
        .map(|(i, class_hash)| {
            // If the line ends up longer than 100 chars, wrap the value in parenthesis, so the
            // formatter can split the lines.
            let line = format!("const {name}_{i} = {:#064x};", class_hash.0);
            if line.len() > 100 {
                format!("const {name}_{i} = ({:#064x});", class_hash.0)
            } else {
                line
            }
        })
        .chain(std::iter::once(format!("const {name}_LEN = {};", class_hashes.len())))
        .collect::<Vec<String>>()
        .join("\n")
}

fn generate_constants_file() -> String {
    let os_constants = &VersionedConstants::latest_constants().os_constants;

    // Replace the template with the actual values.
    let unformatted = format!(
        include_str!("cairo/starkware/starknet/core/os/constants_template.txt"),
        // Miscellaneous constants.
        NOP_ENTRY_POINT_OFFSET = os_constants.nop_entry_point_offset,
        STORED_BLOCK_HASH_BUFFER = os_constants.stored_block_hash_buffer,
        L1_HANDLER_VERSION = os_constants.l1_handler_version,
        L1_HANDLER_L2_GAS_MAX_AMOUNT = os_constants.l1_handler_max_amount_bounds.l2_gas.0,
        SIERRA_ARRAY_LEN_BOUND = os_constants.sierra_array_len_bound,
        // Entry point types.
        ENTRY_POINT_TYPE_EXTERNAL = os_constants.entry_point_type_external,
        ENTRY_POINT_TYPE_L1_HANDLER = os_constants.entry_point_type_l1_handler,
        ENTRY_POINT_TYPE_CONSTRUCTOR = os_constants.entry_point_type_constructor,
        // Entry point selectors.
        CONSTRUCTOR_ENTRY_POINT_SELECTOR =
            selector_to_hex(&os_constants.constructor_entry_point_selector),
        EXECUTE_ENTRY_POINT_SELECTOR = selector_to_hex(&os_constants.execute_entry_point_selector),
        VALIDATE_ENTRY_POINT_SELECTOR =
            selector_to_hex(&os_constants.validate_entry_point_selector),
        VALIDATE_DECLARE_ENTRY_POINT_SELECTOR =
            selector_to_hex(&os_constants.validate_declare_entry_point_selector),
        VALIDATE_DEPLOY_ENTRY_POINT_SELECTOR =
            selector_to_hex(&os_constants.validate_deploy_entry_point_selector),
        TRANSFER_ENTRY_POINT_SELECTOR =
            selector_to_hex(&os_constants.transfer_entry_point_selector),
        DEFAULT_ENTRY_POINT_SELECTOR = selector_to_hex(&os_constants.default_entry_point_selector),
        // OS addresses.
        BLOCK_HASH_CONTRACT_ADDRESS = contract_address_to_hex(
            &os_constants.os_contract_addresses.block_hash_contract_address()
        ),
        ALIAS_CONTRACT_ADDRESS =
            contract_address_to_hex(&os_constants.os_contract_addresses.alias_contract_address()),
        RESERVED_CONTRACT_ADDRESS = contract_address_to_hex(
            &os_constants.os_contract_addresses.reserved_contract_address()
        ),
        // Base costs.
        STEP_GAS_COST = os_constants.gas_costs.base.step_gas_cost,
        MEMORY_HOLE_GAS_COST = os_constants.gas_costs.base.memory_hole_gas_cost,
        // Builtin costs.
        RANGE_CHECK_GAS_COST = os_constants.gas_costs.builtins.range_check,
        RANGE_CHECK96_GAS_COST = os_constants.gas_costs.builtins.range_check96,
        KECCAK_BUILTIN_GAS_COST = os_constants.gas_costs.builtins.keccak,
        PEDERSEN_GAS_COST = os_constants.gas_costs.builtins.pedersen,
        BITWISE_BUILTIN_GAS_COST = os_constants.gas_costs.builtins.bitwise,
        ECOP_GAS_COST = os_constants.gas_costs.builtins.ecop,
        POSEIDON_GAS_COST = os_constants.gas_costs.builtins.poseidon,
        ADD_MOD_GAS_COST = os_constants.gas_costs.builtins.add_mod,
        MUL_MOD_GAS_COST = os_constants.gas_costs.builtins.mul_mod,
        ECDSA_GAS_COST = os_constants.gas_costs.builtins.ecdsa,
        // Initial costs and gas limits.
        DEFAULT_INITIAL_GAS_COST = os_constants.default_initial_gas_cost,
        VALIDATE_MAX_SIERRA_GAS = os_constants.validate_max_sierra_gas,
        EXECUTE_MAX_SIERRA_GAS = os_constants.execute_max_sierra_gas,
        ENTRY_POINT_INITIAL_BUDGET = os_constants.entry_point_initial_budget,
        // Syscall costs.
        // Costs without a linear factor use `base_only_syscall_cost`; costs with a linear factor
        // use `get_syscall_cost(0)` for the base cost (0 linear factor), and `linear_syscall_cost`
        // for the linear factor.
        SYSCALL_BASE_GAS_COST = os_constants.gas_costs.base.syscall_base_gas_cost,
        CALL_CONTRACT_GAS_COST =
            base_only_syscall_cost(SyscallSelector::CallContract, os_constants),
        DEPLOY_GAS_COST = os_constants.gas_costs.syscalls.deploy.get_syscall_cost(0),
        DEPLOY_CALLDATA_FACTOR_GAS_COST =
            os_constants.gas_costs.syscalls.deploy.linear_syscall_cost(),
        GET_BLOCK_HASH_GAS_COST =
            base_only_syscall_cost(SyscallSelector::GetBlockHash, os_constants),
        GET_CLASS_HASH_AT_GAS_COST =
            base_only_syscall_cost(SyscallSelector::GetClassHashAt, os_constants),
        GET_EXECUTION_INFO_GAS_COST =
            base_only_syscall_cost(SyscallSelector::GetExecutionInfo, os_constants),
        LIBRARY_CALL_GAS_COST = base_only_syscall_cost(SyscallSelector::LibraryCall, os_constants),
        REPLACE_CLASS_GAS_COST =
            base_only_syscall_cost(SyscallSelector::ReplaceClass, os_constants),
        STORAGE_READ_GAS_COST = base_only_syscall_cost(SyscallSelector::StorageRead, os_constants),
        STORAGE_WRITE_GAS_COST =
            base_only_syscall_cost(SyscallSelector::StorageWrite, os_constants),
        EMIT_EVENT_GAS_COST = base_only_syscall_cost(SyscallSelector::EmitEvent, os_constants),
        SEND_MESSAGE_TO_L1_GAS_COST =
            base_only_syscall_cost(SyscallSelector::SendMessageToL1, os_constants),
        META_TX_V0_GAS_COST = os_constants.gas_costs.syscalls.meta_tx_v0.get_syscall_cost(0),
        META_TX_V0_CALLDATA_FACTOR_GAS_COST =
            os_constants.gas_costs.syscalls.meta_tx_v0.linear_syscall_cost(),
        SECP256K1_ADD_GAS_COST =
            base_only_syscall_cost(SyscallSelector::Secp256k1Add, os_constants),
        SECP256K1_GET_POINT_FROM_X_GAS_COST =
            base_only_syscall_cost(SyscallSelector::Secp256k1GetPointFromX, os_constants),
        SECP256K1_GET_XY_GAS_COST =
            base_only_syscall_cost(SyscallSelector::Secp256k1GetXy, os_constants),
        SECP256K1_MUL_GAS_COST =
            base_only_syscall_cost(SyscallSelector::Secp256k1Mul, os_constants),
        SECP256K1_NEW_GAS_COST =
            base_only_syscall_cost(SyscallSelector::Secp256k1New, os_constants),
        SECP256R1_ADD_GAS_COST =
            base_only_syscall_cost(SyscallSelector::Secp256r1Add, os_constants),
        SECP256R1_GET_POINT_FROM_X_GAS_COST =
            base_only_syscall_cost(SyscallSelector::Secp256r1GetPointFromX, os_constants),
        SECP256R1_GET_XY_GAS_COST =
            base_only_syscall_cost(SyscallSelector::Secp256r1GetXy, os_constants),
        SECP256R1_MUL_GAS_COST =
            base_only_syscall_cost(SyscallSelector::Secp256r1Mul, os_constants),
        SECP256R1_NEW_GAS_COST =
            base_only_syscall_cost(SyscallSelector::Secp256r1New, os_constants),
        KECCAK_GAS_COST = base_only_syscall_cost(SyscallSelector::Keccak, os_constants),
        KECCAK_ROUND_COST_GAS_COST =
            base_only_syscall_cost(SyscallSelector::KeccakRound, os_constants),
        SHA256_PROCESS_BLOCK_GAS_COST =
            base_only_syscall_cost(SyscallSelector::Sha256ProcessBlock, os_constants),
        // Short-strings.
        ERROR_BLOCK_NUMBER_OUT_OF_RANGE =
            quote_string(&os_constants.error_block_number_out_of_range),
        ERROR_OUT_OF_GAS = quote_string(&os_constants.error_out_of_gas),
        ERROR_ENTRY_POINT_FAILED = quote_string(&os_constants.error_entry_point_failed),
        ERROR_ENTRY_POINT_NOT_FOUND = quote_string(&os_constants.error_entry_point_not_found),
        ERROR_INVALID_INPUT_LEN = quote_string(&os_constants.error_invalid_input_len),
        ERROR_INVALID_ARGUMENT = quote_string(&os_constants.error_invalid_argument),
        VALIDATED = quote_string(&os_constants.validated),
        // Resource bounds.
        L1_GAS = quote_string(&os_constants.l1_gas),
        L2_GAS = quote_string(&os_constants.l2_gas),
        L1_DATA_GAS = quote_string(&os_constants.l1_data_gas),
        L1_GAS_INDEX = os_constants.l1_gas_index,
        L2_GAS_INDEX = os_constants.l2_gas_index,
        L1_DATA_GAS_INDEX = os_constants.l1_data_gas_index,
        // Syscall rounding.
        VALIDATE_BLOCK_NUMBER_ROUNDING =
            os_constants.validate_rounding_consts.validate_block_number_rounding,
        VALIDATE_TIMESTAMP_ROUNDING =
            os_constants.validate_rounding_consts.validate_timestamp_rounding,
        // Backward compatibility accounts.
        V1_BOUND_ACCOUNTS_CAIRO0 = stringify_class_hash_list(
            "V1_BOUND_ACCOUNTS_CAIRO0",
            &os_constants.v1_bound_accounts_cairo0
        ),
        V1_BOUND_ACCOUNTS_CAIRO1 = stringify_class_hash_list(
            "V1_BOUND_ACCOUNTS_CAIRO1",
            &os_constants.v1_bound_accounts_cairo1
        ),
        V1_BOUND_ACCOUNTS_MAX_TIP =
            format!("{:#?}", Felt::from(os_constants.v1_bound_accounts_max_tip)),
        DATA_GAS_ACCOUNTS =
            stringify_class_hash_list("DATA_GAS_ACCOUNTS", &os_constants.data_gas_accounts),
    );

    // Format and return.
    cairo0_format(&unformatted)
}

/// Test that `constants.cairo` generated from the values in the versioned constants matches the
/// existing file. To fix this test, run:
/// ```bash
/// FIX_OS_CONSTANTS=1 cargo test -p apollo_starknet_os_program test_os_constants
/// ```
#[test]
fn test_os_constants() {
    // Generate `constants.cairo` from the current OS constants.
    let generated = generate_constants_file();
    let fix = std::env::var("FIX_OS_CONSTANTS").is_ok();
    if fix {
        // Write the generated contents to the file.
        let path = PathBuf::from(compile_time_cargo_manifest_dir!())
            .join("src/cairo/starkware/starknet/core/os/constants.cairo");
        std::fs::write(path, &generated).expect("Failed to write generated constants file.");
    } else {
        assert_eq!(
            CONSTANTS_CONTENTS, generated,
            "Generated constants file does not match the expected contents. Please run \
             `FIX_OS_CONSTANTS=1 cargo test -p apollo_starknet_os_program test_os_constants` to \
             fix the test."
        );
    }
}
