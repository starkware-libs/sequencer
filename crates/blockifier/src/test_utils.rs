pub mod cairo_compile;
pub mod contracts;
pub mod declare;
pub mod deploy_account;
pub mod dict_state_reader;
pub mod initial_test_state;
pub mod invoke;
pub mod l1_handler;
pub mod prices;
pub mod struct_impls;
pub mod syscall;
pub mod transfers_generator;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber, GasPrice, NonzeroGasPrice};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::execution_resources::{GasAmount, GasVector};
use starknet_api::hash::StarkHash;
use starknet_api::state::StorageKey;
use starknet_api::transaction::{
    Calldata,
    ContractAddressSalt,
    Fee,
    GasVectorComputationMode,
    TransactionVersion,
};
use starknet_api::{contract_address, felt};
use starknet_types_core::felt::Felt;

use crate::abi::abi_utils::{get_fee_token_var_address, selector_from_name};
use crate::abi::constants;
use crate::execution::call_info::ExecutionSummary;
use crate::execution::contract_class::TrackedResource;
use crate::execution::deprecated_syscalls::hint_processor::SyscallCounter;
use crate::execution::entry_point::CallEntryPoint;
use crate::execution::syscalls::SyscallSelector;
use crate::fee::resources::{StarknetResources, StateResources};
use crate::test_utils::contracts::FeatureContract;
use crate::transaction::transaction_types::TransactionType;
use crate::utils::{const_max, u64_from_usize};
use crate::versioned_constants::VersionedConstants;
// TODO(Dori, 1/2/2024): Remove these constants once all tests use the `contracts` and
//   `initial_test_state` modules for testing.
// Addresses.
pub const TEST_SEQUENCER_ADDRESS: &str = "0x1000";
pub const TEST_ERC20_CONTRACT_ADDRESS: &str = "0x1001";
pub const TEST_ERC20_CONTRACT_ADDRESS2: &str = "0x1002";

// Class hashes.
// TODO(Adi, 15/01/2023): Remove and compute the class hash corresponding to the ERC20 contract in
// starkgate once we use the real ERC20 contract.
pub const TEST_ERC20_CONTRACT_CLASS_HASH: &str = "0x1010";

// Paths.
pub const ERC20_CONTRACT_PATH: &str = "./ERC20/ERC20_Cairo0/ERC20_without_some_syscalls/ERC20/\
                                       erc20_contract_without_some_syscalls_compiled.json";

// TODO(Aviv, 14/7/2024): Move from test utils module, and use it in ContractClassVersionMismatch
// error.
#[derive(Clone, Hash, PartialEq, Eq, Copy, Debug)]
pub enum CairoVersion {
    Cairo0,
    Cairo1,
    #[cfg(feature = "cairo_native")]
    Native,
}

impl Default for CairoVersion {
    fn default() -> Self {
        Self::Cairo0
    }
}

impl From<isize> for CairoVersion {
    fn from(value: isize) -> Self {
        match value {
            0 => Self::Cairo0,
            1 => Self::Cairo1,
            #[cfg(feature = "cairo_native")]
            2 => Self::Native,
            _ => panic!("Invalid value for CairoVersion: {}", value),
        }
    }
}

impl CairoVersion {
    // A declare transaction of the given version, can be used to declare contracts of the returned
    // cairo version.
    // TODO: Make TransactionVersion an enum and use match here.
    pub fn from_declare_tx_version(tx_version: TransactionVersion) -> Self {
        if tx_version == TransactionVersion::ZERO || tx_version == TransactionVersion::ONE {
            CairoVersion::Cairo0
        } else if tx_version == TransactionVersion::TWO || tx_version == TransactionVersion::THREE {
            CairoVersion::Cairo1
        } else {
            panic!("Transaction version {:?} is not supported.", tx_version)
        }
    }

    pub fn other(&self) -> Self {
        match self {
            Self::Cairo0 => Self::Cairo1,
            Self::Cairo1 => Self::Cairo0,
            #[cfg(feature = "cairo_native")]
            Self::Native => todo!("who should be your other?"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompilerBasedVersion {
    CairoVersion(CairoVersion),
    OldCairo1,
}

impl CompilerBasedVersion {
    pub fn get_test_contract(&self) -> FeatureContract {
        match self {
            Self::CairoVersion(version) => FeatureContract::TestContract(*version),
            Self::OldCairo1 => FeatureContract::CairoStepsTestContract,
        }
    }

    /// Returns the context-free tracked resource of this contract (does not take caller contract
    /// and the transaction info into account).
    pub fn own_tracked_resource(&self) -> TrackedResource {
        match self {
            Self::CairoVersion(CairoVersion::Cairo0) | Self::OldCairo1 => {
                TrackedResource::CairoSteps
            }
            Self::CairoVersion(CairoVersion::Cairo1) => TrackedResource::SierraGas,
            #[cfg(feature = "cairo_native")]
            Self::CairoVersion(CairoVersion::Native) => TrackedResource::SierraGas,
        }
    }
}

// Storage keys.
pub fn test_erc20_sequencer_balance_key() -> StorageKey {
    get_fee_token_var_address(contract_address!(TEST_SEQUENCER_ADDRESS))
}

// The max_fee / resource bounds used for txs in this test.
// V3 transactions:
pub const DEFAULT_L1_GAS_AMOUNT: GasAmount = GasAmount(u64::pow(10, 6));
pub const DEFAULT_L1_DATA_GAS_MAX_AMOUNT: GasAmount = GasAmount(u64::pow(10, 6));
pub const DEFAULT_L2_GAS_MAX_AMOUNT: GasAmount = GasAmount(u64::pow(10, 9));
pub const MAX_L1_GAS_PRICE: NonzeroGasPrice = DEFAULT_STRK_L1_GAS_PRICE;
pub const MAX_L2_GAS_PRICE: NonzeroGasPrice = DEFAULT_STRK_L2_GAS_PRICE;
pub const MAX_L1_DATA_GAS_PRICE: NonzeroGasPrice = DEFAULT_STRK_L1_DATA_GAS_PRICE;

pub const DEFAULT_ETH_L1_GAS_PRICE: NonzeroGasPrice =
    NonzeroGasPrice::new_unchecked(GasPrice(100 * u128::pow(10, 9))); // Given in units of Wei.
pub const DEFAULT_STRK_L1_GAS_PRICE: NonzeroGasPrice =
    NonzeroGasPrice::new_unchecked(GasPrice(100 * u128::pow(10, 9))); // Given in units of STRK.
pub const DEFAULT_ETH_L1_DATA_GAS_PRICE: NonzeroGasPrice =
    NonzeroGasPrice::new_unchecked(GasPrice(u128::pow(10, 6))); // Given in units of Wei.
pub const DEFAULT_STRK_L1_DATA_GAS_PRICE: NonzeroGasPrice =
    NonzeroGasPrice::new_unchecked(GasPrice(u128::pow(10, 9))); // Given in units of STRK.
pub const DEFAULT_STRK_L2_GAS_PRICE: NonzeroGasPrice =
    NonzeroGasPrice::new_unchecked(GasPrice(u128::pow(10, 9)));

// Deprecated transactions:
pub const MAX_FEE: Fee = DEFAULT_L1_GAS_AMOUNT.nonzero_saturating_mul(DEFAULT_ETH_L1_GAS_PRICE);

// Commitment fee bounds.
const DEFAULT_L1_BOUNDS_COMMITTED_FEE: Fee =
    DEFAULT_L1_GAS_AMOUNT.nonzero_saturating_mul(DEFAULT_STRK_L1_GAS_PRICE);
const DEFAULT_ALL_BOUNDS_COMMITTED_FEE: Fee = DEFAULT_L1_BOUNDS_COMMITTED_FEE
    .saturating_add(DEFAULT_L2_GAS_MAX_AMOUNT.nonzero_saturating_mul(DEFAULT_STRK_L2_GAS_PRICE))
    .saturating_add(
        DEFAULT_L1_DATA_GAS_MAX_AMOUNT.nonzero_saturating_mul(DEFAULT_STRK_L1_DATA_GAS_PRICE),
    );
// The amount of test-token allocated to the account in this test, set to a multiple of the max
// amount deprecated / non-deprecated transactions commit to paying.
pub const BALANCE: Fee = Fee(10
    * const_max(
        const_max(DEFAULT_ALL_BOUNDS_COMMITTED_FEE.0, DEFAULT_L1_BOUNDS_COMMITTED_FEE.0),
        MAX_FEE.0,
    ));

// The block number of the BlockContext being used for testing.
pub const CURRENT_BLOCK_NUMBER: u64 = 2001;
pub const CURRENT_BLOCK_NUMBER_FOR_VALIDATE: u64 = 2000;

// The block timestamp of the BlockContext being used for testing.
pub const CURRENT_BLOCK_TIMESTAMP: u64 = 1072023;
pub const CURRENT_BLOCK_TIMESTAMP_FOR_VALIDATE: u64 = 1069200;

#[derive(Default)]
pub struct SaltManager {
    next_salt: u8,
}

impl SaltManager {
    pub fn next_salt(&mut self) -> ContractAddressSalt {
        let next_contract_address_salt = ContractAddressSalt(felt!(self.next_salt));
        self.next_salt += 1;
        next_contract_address_salt
    }
}

pub fn pad_address_to_64(address: &str) -> String {
    let trimmed_address = address.strip_prefix("0x").unwrap_or(address);
    String::from("0x") + format!("{trimmed_address:0>64}").as_str()
}

pub fn get_raw_contract_class(contract_path: &str) -> String {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), contract_path].iter().collect();
    fs::read_to_string(path).unwrap()
}

pub fn trivial_external_entry_point_new(contract: FeatureContract) -> CallEntryPoint {
    let address = contract.get_instance_address(0);
    trivial_external_entry_point_with_address(address)
}

pub fn trivial_external_entry_point_with_address(
    contract_address: ContractAddress,
) -> CallEntryPoint {
    CallEntryPoint {
        code_address: Some(contract_address),
        storage_address: contract_address,
        initial_gas: VersionedConstants::create_for_testing()
            .os_constants
            .gas_costs
            .default_initial_gas_cost,
        ..Default::default()
    }
}

#[macro_export]
macro_rules! check_inner_exc_for_custom_hint {
    ($inner_exc:expr, $expected_hint:expr) => {{
        use cairo_vm::vm::errors::hint_errors::HintError;
        use cairo_vm::vm::errors::vm_errors::VirtualMachineError;

        if let VirtualMachineError::Hint(hint) = $inner_exc {
            if let HintError::Internal(VirtualMachineError::Other(error)) = &hint.1 {
                assert_eq!(error.to_string(), $expected_hint.to_string());
            } else {
                panic!("Unexpected hint: {:?}", hint);
            }
        } else {
            panic!("Unexpected structure for inner_exc: {:?}", $inner_exc);
        }
    }};
}

#[macro_export]
macro_rules! check_inner_exc_for_invalid_scenario {
    ($inner_exc:expr) => {{
        use cairo_vm::vm::errors::vm_errors::VirtualMachineError;

        if let VirtualMachineError::DiffAssertValues(_) = $inner_exc {
        } else {
            panic!("Unexpected structure for inner_exc: {:?}", $inner_exc)
        }
    }};
}

#[macro_export]
macro_rules! check_entry_point_execution_error {
    ($error:expr, $expected_hint:expr $(,)?) => {{
        use cairo_vm::vm::errors::cairo_run_errors::CairoRunError;
        use cairo_vm::vm::errors::vm_exception::VmException;
        use $crate::execution::errors::EntryPointExecutionError;

        if let EntryPointExecutionError::CairoRunError(CairoRunError::VmException(VmException {
            inner_exc,
            ..
        })) = $error
        {
            match $expected_hint {
                Some(expected_hint) => {
                    $crate::check_inner_exc_for_custom_hint!(inner_exc, expected_hint)
                }
                None => $crate::check_inner_exc_for_invalid_scenario!(inner_exc),
            };
        } else {
            panic!("Unexpected structure for error: {:?}", $error);
        }
    }};
}

/// Checks that the given error is a `HintError::CustomHint` with the given hint.
#[macro_export]
macro_rules! check_entry_point_execution_error_for_custom_hint {
    ($error:expr, $expected_hint:expr $(,)?) => {
        $crate::check_entry_point_execution_error!($error, Some($expected_hint))
    };
}

#[macro_export]
macro_rules! check_tx_execution_error_inner {
    ($error:expr, $expected_hint:expr, $validate_constructor:expr $(,)?) => {{
        use $crate::execution::errors::ConstructorEntryPointExecutionError;
        use $crate::transaction::errors::TransactionExecutionError;

        if $validate_constructor {
            match $error {
                TransactionExecutionError::ContractConstructorExecutionFailed(
                    ConstructorEntryPointExecutionError::ExecutionError { error, .. },
                ) => {
                    $crate::check_entry_point_execution_error!(error, $expected_hint)
                }
                _ => panic!("Unexpected structure for error: {:?}", $error),
            }
        } else {
            match $error {
                TransactionExecutionError::ValidateTransactionError { error, .. } => {
                    $crate::check_entry_point_execution_error!(error, $expected_hint)
                }
                _ => panic!("Unexpected structure for error: {:?}", $error),
            }
        }
    }};
}

#[macro_export]
macro_rules! check_tx_execution_error_for_custom_hint {
    ($error:expr, $expected_hint:expr, $validate_constructor:expr $(,)?) => {
        $crate::check_tx_execution_error_inner!(
            $error,
            Some($expected_hint),
            $validate_constructor,
        );
    };
}

/// Checks that a given error is an assertion error with the expected message.
/// Formatted for test_validate_accounts_tx.
#[macro_export]
macro_rules! check_tx_execution_error_for_invalid_scenario {
    ($cairo_version:expr, $error:expr, $validate_constructor:expr $(,)?) => {
        match $cairo_version {
            CairoVersion::Cairo0 => {
                $crate::check_tx_execution_error_inner!(
                    $error,
                    None::<&str>,
                    $validate_constructor,
                );
            }
            CairoVersion::Cairo1 => {
                if let $crate::transaction::errors::TransactionExecutionError::ValidateTransactionError {
                    error, ..
                } = $error {
                    assert_eq!(
                        error.to_string(),
                        "Execution failed. Failure reason: 0x496e76616c6964207363656e6172696f \
                         ('Invalid scenario')."
                    )
                }
            }
            #[cfg(feature = "cairo_native")]
            CairoVersion::Native => {
                if let $crate::transaction::errors::TransactionExecutionError::ValidateTransactionError {
                    error, ..
                } = $error {
                    assert_eq!(
                        error.to_string(),
                        "Execution failed. Failure reason: 0x496e76616c6964207363656e6172696f \
                         ('Invalid scenario')."
                    )
                }
            }
        }
    };
}

pub fn get_syscall_resources(syscall_selector: SyscallSelector) -> ExecutionResources {
    let versioned_constants = VersionedConstants::create_for_testing();
    let syscall_counter: SyscallCounter = HashMap::from([(syscall_selector, 1)]);
    versioned_constants.get_additional_os_syscall_resources(&syscall_counter)
}

pub fn get_tx_resources(tx_type: TransactionType) -> ExecutionResources {
    let versioned_constants = VersionedConstants::create_for_testing();
    let starknet_resources = StarknetResources::new(
        1,
        0,
        0,
        StateResources::default(),
        None,
        ExecutionSummary::default(),
    );

    versioned_constants.get_additional_os_tx_resources(tx_type, &starknet_resources, false)
}

/// Creates the calldata for the Cairo function "test_deploy" in the featured contract TestContract.
/// The format of the calldata is:
/// [
///     class_hash,
///     contract_address_salt,
///     constructor_calldata_len,
///     *constructor_calldata,
///     deploy_from_zero
/// ]
pub fn calldata_for_deploy_test(
    class_hash: ClassHash,
    constructor_calldata: &[Felt],
    valid_deploy_from_zero: bool,
) -> Calldata {
    Calldata(
        [
            vec![
                class_hash.0,
                ContractAddressSalt::default().0,
                felt!(u64_from_usize(constructor_calldata.len())),
            ],
            constructor_calldata.into(),
            vec![felt!(if valid_deploy_from_zero { 0_u8 } else { 2_u8 })],
        ]
        .concat()
        .into(),
    )
}

/// Creates the calldata for the "__execute__" entry point in the featured contracts
/// AccountWithLongValidate and AccountWithoutValidations. The format of the returned calldata is:
/// [
///     contract_address,
///     entry_point_name,
///     calldata_length,
///     *calldata,
/// ]
/// The contract_address is the address of the called contract, the entry_point_name is the
/// name of the called entry point in said contract, and the calldata is the calldata for the called
/// entry point.
pub fn create_calldata(
    contract_address: ContractAddress,
    entry_point_name: &str,
    entry_point_args: &[Felt],
) -> Calldata {
    Calldata(
        [
            vec![
                *contract_address.0.key(),              // Contract address.
                selector_from_name(entry_point_name).0, // EP selector name.
                felt!(u64_from_usize(entry_point_args.len())),
            ],
            entry_point_args.into(),
        ]
        .concat()
        .into(),
    )
}

/// Calldata for a trivial entry point in the test contract.
pub fn create_trivial_calldata(test_contract_address: ContractAddress) -> Calldata {
    create_calldata(
        test_contract_address,
        "return_result",
        &[felt!(2_u8)], // Calldata: num.
    )
}

pub fn update_json_value(base: &mut serde_json::Value, update: serde_json::Value) {
    match (base, update) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(update_map)) => {
            base_map.extend(update_map);
        }
        _ => panic!("Both base and update should be of type serde_json::Value::Object."),
    }
}

pub fn gas_vector_from_vm_usage(
    vm_usage_in_l1_gas: GasAmount,
    computation_mode: &GasVectorComputationMode,
    versioned_constants: &VersionedConstants,
) -> GasVector {
    match computation_mode {
        GasVectorComputationMode::NoL2Gas => GasVector::from_l1_gas(vm_usage_in_l1_gas),
        GasVectorComputationMode::All => GasVector::from_l2_gas(
            versioned_constants.convert_l1_to_l2_gas_amount_round_up(vm_usage_in_l1_gas),
        ),
    }
}

pub fn get_vm_resource_usage() -> ExecutionResources {
    ExecutionResources {
        n_steps: 10000,
        n_memory_holes: 0,
        builtin_instance_counter: HashMap::from([
            (BuiltinName::pedersen, 10),
            (BuiltinName::range_check, 24),
            (BuiltinName::ecdsa, 1),
            (BuiltinName::bitwise, 1),
            (BuiltinName::poseidon, 1),
        ]),
    }
}

pub fn maybe_dummy_block_hash_and_number(block_number: BlockNumber) -> Option<BlockHashAndNumber> {
    if block_number.0 < constants::STORED_BLOCK_HASH_BUFFER {
        return None;
    }
    Some(BlockHashAndNumber {
        number: BlockNumber(block_number.0 - constants::STORED_BLOCK_HASH_BUFFER),
        hash: BlockHash(StarkHash::ONE),
    })
}
