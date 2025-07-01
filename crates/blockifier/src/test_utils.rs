pub mod contracts;
pub mod dict_state_reader;
pub mod initial_test_state;
pub mod l1_handler;
pub mod prices;
pub mod struct_impls;
pub mod syscall;
#[cfg(test)]
pub mod test_templates;
pub mod transfers_generator;
use std::collections::HashMap;
use std::slice::Iter;

use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::abi::abi_utils::{get_fee_token_var_address, selector_from_name};
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::executable_transaction::TransactionType;
use starknet_api::execution_resources::{GasAmount, GasVector};
use starknet_api::hash::StarkHash;
use starknet_api::state::StorageKey;
use starknet_api::test_utils::{
    DEFAULT_L1_DATA_GAS_MAX_AMOUNT,
    DEFAULT_L1_GAS_AMOUNT,
    DEFAULT_L2_GAS_MAX_AMOUNT,
    DEFAULT_STRK_L1_DATA_GAS_PRICE,
    DEFAULT_STRK_L1_GAS_PRICE,
    DEFAULT_STRK_L2_GAS_PRICE,
    MAX_FEE,
    TEST_SEQUENCER_ADDRESS,
};
use starknet_api::transaction::fields::{
    Calldata,
    ContractAddressSalt,
    Fee,
    GasVectorComputationMode,
};
use starknet_api::{contract_address, felt};
use starknet_types_core::felt::Felt;
use strum::EnumCount;
use strum_macros::EnumCount as EnumCountMacro;

use crate::abi::constants;
use crate::blockifier_versioned_constants::VersionedConstants;
use crate::execution::call_info::ExecutionSummary;
use crate::execution::contract_class::TrackedResource;
use crate::execution::entry_point::CallEntryPoint;
use crate::execution::syscalls::vm_syscall_utils::{
    SyscallSelector,
    SyscallUsage,
    SyscallUsageMap,
};
use crate::fee::resources::{StarknetResources, StateResources};
use crate::utils::{const_max, u64_from_usize};
// Class hashes.
// TODO(Adi, 15/01/2023): Remove and compute the class hash corresponding to the ERC20 contract in
// starkgate once we use the real ERC20 contract.
pub const TEST_ERC20_CONTRACT_CLASS_HASH: &str = "0x1010";

// Paths.
pub const ERC20_CONTRACT_PATH: &str = "../blockifier_test_utils/resources/ERC20/ERC20_Cairo0/\
                                       ERC20_without_some_syscalls/ERC20/\
                                       erc20_contract_without_some_syscalls_compiled.json";

#[derive(Clone, Copy, EnumCountMacro, PartialEq, Eq, Debug)]
pub enum CompilerBasedVersion {
    CairoVersion(CairoVersion),
    OldCairo1,
}

impl From<CompilerBasedVersion> for CairoVersion {
    fn from(compiler_based_version: CompilerBasedVersion) -> Self {
        match compiler_based_version {
            CompilerBasedVersion::CairoVersion(version) => version,
            CompilerBasedVersion::OldCairo1 => CairoVersion::Cairo1(RunnableCairo1::Casm),
        }
    }
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
            Self::CairoVersion(CairoVersion::Cairo1(_)) => TrackedResource::SierraGas,
        }
    }

    /// Returns an iterator over all of the enum variants.
    pub fn iter() -> Iter<'static, Self> {
        assert_eq!(Self::COUNT, 2);
        static VERSIONS: [CompilerBasedVersion; 3] = [
            CompilerBasedVersion::CairoVersion(CairoVersion::Cairo0),
            CompilerBasedVersion::OldCairo1,
            CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Casm)),
        ];
        VERSIONS.iter()
    }

    pub fn is_cairo_native(&self) -> bool {
        match self {
            Self::CairoVersion(version) => version.is_cairo_native(),
            Self::OldCairo1 => false,
        }
    }
}

// Storage keys.
pub fn test_erc20_sequencer_balance_key() -> StorageKey {
    get_fee_token_var_address(contract_address!(TEST_SEQUENCER_ADDRESS))
}

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
            .base
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

        match $error {
            EntryPointExecutionError::CairoRunError(boxed_error) => {
                if let CairoRunError::VmException(VmException { inner_exc, .. }) =
                    &*(boxed_error.as_ref())
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
            }
            _ => panic!("Unexpected structure for error: {:?}", $error),
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
                    $crate::check_entry_point_execution_error!(&*(error.as_ref()), $expected_hint)
                }
                _ => panic!("Unexpected structure for error: {:?}", $error),
            }
        } else {
            match $error {
                TransactionExecutionError::ValidateTransactionError { error, .. } => {
                    $crate::check_entry_point_execution_error!(&*(error.as_ref()), $expected_hint)
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

            CairoVersion::Cairo1(_) => {
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

/// Returns the const syscall resources for the given syscall selector.
pub fn get_const_syscall_resources(syscall_selector: SyscallSelector) -> ExecutionResources {
    let versioned_constants = VersionedConstants::create_for_testing();
    let syscalls_usage: SyscallUsageMap =
        HashMap::from([(syscall_selector, SyscallUsage::new(1, 0))]);
    versioned_constants.get_additional_os_syscall_resources(&syscalls_usage)
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

pub fn create_deploy_entry_point(
    class_hash: ClassHash,
    constructor_calldata: &[Felt],
    valid_deploy_from_zero: bool,
    deployer_contract: FeatureContract,
) -> CallEntryPoint {
    let calldata =
        calldata_for_deploy_test(class_hash, constructor_calldata, valid_deploy_from_zero);
    CallEntryPoint {
        entry_point_selector: selector_from_name("test_deploy"),
        calldata,
        ..trivial_external_entry_point_new(deployer_contract)
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
            versioned_constants.l1_gas_to_sierra_gas_amount_round_up(vm_usage_in_l1_gas),
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
