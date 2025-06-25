use std::collections::HashMap;

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::block::{BlockInfo, FeeType};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::execution_resources::GasVector;
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    Fee,
    GasVectorComputationMode,
    PaymasterData,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_api::transaction::{
    signed_tx_version,
    TransactionHash,
    TransactionOptions,
    TransactionVersion,
};

use crate::abi::constants as abi_constants;
use crate::blockifier_versioned_constants::VersionedConstants;
use crate::execution::call_info::{BuiltinCounterMap, CallInfo, ExecutionSummary};
use crate::execution::stack_trace::ErrorStack;
use crate::fee::fee_checks::FeeCheckError;
use crate::fee::fee_utils::get_fee_by_gas_vector;
use crate::fee::receipt::TransactionReceipt;
use crate::transaction::errors::{TransactionExecutionError, TransactionPreValidationError};

#[cfg(test)]
#[path = "objects_test.rs"]
pub mod objects_test;

pub type TransactionExecutionResult<T> = Result<T, TransactionExecutionError>;
pub type TransactionPreValidationResult<T> = Result<T, TransactionPreValidationError>;

macro_rules! implement_getters {
    ($(($field:ident, $field_type:ty)),*) => {
        $(pub fn $field(&self) -> $field_type {
            match self{
                Self::Current(context) => context.common_fields.$field,
                Self::Deprecated(context) => context.common_fields.$field,
            }
        })*
    };
}

/// Contains the account information of the transaction (outermost call).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransactionInfo {
    Current(CurrentTransactionInfo),
    Deprecated(DeprecatedTransactionInfo),
}

impl TransactionInfo {
    implement_getters!(
        (transaction_hash, TransactionHash),
        (version, TransactionVersion),
        (nonce, Nonce),
        (sender_address, ContractAddress),
        (only_query, bool)
    );

    pub fn signature(&self) -> TransactionSignature {
        match self {
            Self::Current(context) => context.common_fields.signature.clone(),
            Self::Deprecated(context) => context.common_fields.signature.clone(),
        }
    }

    pub fn is_v0(&self) -> bool {
        self.version() == TransactionVersion::ZERO
    }

    pub fn signed_version(&self) -> TransactionVersion {
        signed_tx_version(&self.version(), &TransactionOptions { only_query: self.only_query() })
    }

    pub fn enforce_fee(&self) -> bool {
        match self {
            TransactionInfo::Current(context) => {
                // Assumes that the tip is enabled, as it is in the OS.
                context.resource_bounds.max_possible_fee(context.tip) > Fee(0)
            }
            TransactionInfo::Deprecated(context) => context.max_fee != Fee(0),
        }
    }

    pub fn gas_mode(&self) -> GasVectorComputationMode {
        match self {
            TransactionInfo::Current(info) => {
                info.resource_bounds.get_gas_vector_computation_mode()
            }
            TransactionInfo::Deprecated(_) => GasVectorComputationMode::NoL2Gas,
        }
    }

    pub fn max_fee_for_execution_info_syscall(&self) -> Fee {
        match self {
            Self::Current(_) => Fee(0),
            Self::Deprecated(context) => context.max_fee,
        }
    }
}

impl HasRelatedFeeType for TransactionInfo {
    fn version(&self) -> TransactionVersion {
        self.version()
    }

    fn is_l1_handler(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentTransactionInfo {
    pub common_fields: CommonAccountFields,
    pub resource_bounds: ValidResourceBounds,
    pub tip: Tip,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
}

#[cfg(any(test, feature = "testing"))]
impl CurrentTransactionInfo {
    pub fn create_for_testing() -> Self {
        Self {
            common_fields: CommonAccountFields::default(),
            resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
            tip: Tip::default(),
            nonce_data_availability_mode: DataAvailabilityMode::L2,
            fee_data_availability_mode: DataAvailabilityMode::L2,
            paymaster_data: PaymasterData::default(),
            account_deployment_data: AccountDeploymentData::default(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeprecatedTransactionInfo {
    pub common_fields: CommonAccountFields,
    pub max_fee: Fee,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommonAccountFields {
    pub transaction_hash: TransactionHash,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub sender_address: ContractAddress,
    pub only_query: bool,
}

#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, derive_more::Display, PartialEq)]
pub enum RevertError {
    Execution(ErrorStack),
    PostExecution(FeeCheckError),
}

impl From<ErrorStack> for RevertError {
    fn from(stack: ErrorStack) -> Self {
        Self::Execution(stack)
    }
}

impl From<FeeCheckError> for RevertError {
    fn from(error: FeeCheckError) -> Self {
        Self::PostExecution(error)
    }
}

/// Contains the information gathered by the execution of a transaction.
#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Default, PartialEq)]
pub struct TransactionExecutionInfo {
    /// Transaction validation call info; [None] for `L1Handler`.
    pub validate_call_info: Option<CallInfo>,
    /// Transaction execution call info; [None] for `Declare`.
    pub execute_call_info: Option<CallInfo>,
    /// Fee transfer call info; [None] for `L1Handler`.
    pub fee_transfer_call_info: Option<CallInfo>,
    pub revert_error: Option<RevertError>,
    /// The receipt of the transaction.
    /// Including the actual fee that was charged (in units of the relevant fee token),
    /// actual gas consumption the transaction is charged for data availability,
    /// actual execution resources the transaction is charged for
    /// (including L1 gas and additional OS resources estimation),
    /// and total gas consumed.
    pub receipt: TransactionReceipt,
}

impl TransactionExecutionInfo {
    // TODO(Arni): Add a flag to non_optional_call_infos to indicate the transaction
    // type. Change the iteration order for `deploy_account` transactions.
    pub fn non_optional_call_infos(&self) -> impl Iterator<Item = &CallInfo> {
        self.validate_call_info
            .iter()
            .chain(self.execute_call_info.iter())
            .chain(self.fee_transfer_call_info.iter())
    }

    pub fn is_reverted(&self) -> bool {
        self.revert_error.is_some()
    }

    /// Returns a summary of transaction execution, including executed class hashes, visited storage
    /// entries, L2-to-L1_payload_lengths, and the number of emitted events.
    pub fn summarize(&self, versioned_constants: &VersionedConstants) -> ExecutionSummary {
        CallInfo::summarize_many(self.non_optional_call_infos(), versioned_constants)
    }
}
pub trait ExecutionResourcesTraits {
    fn total_n_steps(&self) -> usize;
    fn prover_builtins(&self) -> HashMap<BuiltinName, usize>;
    fn div_ceil(&self, rhs: usize) -> ExecutionResources;
}

impl ExecutionResourcesTraits for ExecutionResources {
    fn total_n_steps(&self) -> usize {
        self.n_steps
            // The "segment arena" builtin is not part of the prover (not in any proof layout);
            // It is transformed into regular steps by the OS program - each instance requires
            // approximately 10 steps.
            + abi_constants::N_STEPS_PER_SEGMENT_ARENA_BUILTIN
                * self
                    .builtin_instance_counter
                    .get(&BuiltinName::segment_arena)
                    .cloned()
                    .unwrap_or_default()
    }

    fn prover_builtins(&self) -> BuiltinCounterMap {
        let mut builtins = self.builtin_instance_counter.clone();

        // See "total_n_steps" documentation.
        builtins.remove(&BuiltinName::segment_arena);
        builtins
    }

    fn div_ceil(&self, rhs: usize) -> ExecutionResources {
        if rhs == 0 {
            panic!("Division by zero");
        }
        let mut new = self.clone();
        new.n_steps = self.n_steps.div_ceil(rhs);
        new.n_memory_holes = self.n_memory_holes.div_ceil(rhs);
        for counter in new.builtin_instance_counter.values_mut() {
            *counter = counter.div_ceil(rhs);
        }
        new
    }
}

pub trait HasRelatedFeeType {
    fn version(&self) -> TransactionVersion;

    fn is_l1_handler(&self) -> bool;

    fn fee_type(&self) -> FeeType {
        if self.is_l1_handler() || self.version() < TransactionVersion::THREE {
            FeeType::Eth
        } else {
            FeeType::Strk
        }
    }

    fn get_fee_by_gas_vector(
        &self,
        block_info: &BlockInfo,
        gas_vector: GasVector,
        tip: Tip,
    ) -> Fee {
        get_fee_by_gas_vector(block_info, gas_vector, &self.fee_type(), tip)
    }
}

pub trait TransactionInfoCreator {
    fn create_tx_info(&self) -> TransactionInfo;
}

pub trait TransactionInfoCreatorInner {
    fn create_tx_info(&self, only_query: bool) -> TransactionInfo;
}
