use std::collections::HashMap;

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use num_traits::Pow;
use serde::Serialize;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::transaction::{
    AccountDeploymentData,
    AllResourceBounds,
    Fee,
    PaymasterData,
    ResourceBounds,
    Tip,
    TransactionHash,
    TransactionSignature,
    TransactionVersion,
    ValidResourceBounds,
};
use starknet_types_core::felt::Felt;
use strum_macros::EnumIter;

use crate::abi::constants as abi_constants;
use crate::blockifier::block::BlockInfo;
use crate::context::TransactionContext;
use crate::execution::call_info::{CallInfo, ExecutionSummary, MessageL1CostInfo, OrderedEvent};
use crate::fee::actual_cost::TransactionReceipt;
use crate::fee::eth_gas_constants;
use crate::fee::fee_utils::{get_fee_by_gas_vector, get_vm_resources_cost};
use crate::fee::gas_usage::{
    get_consumed_message_to_l2_emissions_cost,
    get_da_gas_cost,
    get_log_message_to_l1_emissions_cost,
    get_onchain_data_segment_length,
};
use crate::state::cached_state::StateChangesCount;
use crate::transaction::constants;
use crate::transaction::errors::{
    TransactionExecutionError,
    TransactionFeeError,
    TransactionPreValidationError,
};
use crate::utils::{u128_div_ceil, u128_from_usize, usize_from_u128};
use crate::versioned_constants::VersionedConstants;

#[cfg(test)]
#[path = "objects_test.rs"]
pub mod objects_test;

pub type TransactionExecutionResult<T> = Result<T, TransactionExecutionError>;
pub type TransactionFeeResult<T> = Result<T, TransactionFeeError>;
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
        let version = self.version();
        if !self.only_query() {
            return version;
        }

        let query_version_base = Felt::TWO.pow(constants::QUERY_VERSION_BASE_BIT);
        let query_version = query_version_base + version.0;
        TransactionVersion(query_version)
    }

    pub fn enforce_fee(&self) -> bool {
        match self {
            TransactionInfo::Current(context) => match &context.resource_bounds {
                ValidResourceBounds::L1Gas(l1_bounds) => {
                    let max_amount: u128 = l1_bounds.max_amount.into();
                    max_amount * l1_bounds.max_price_per_unit > 0
                }
                ValidResourceBounds::AllResources(AllResourceBounds {
                    l1_gas,
                    l2_gas,
                    l1_data_gas,
                }) => {
                    u128::from(l1_gas.max_amount) * l1_gas.max_price_per_unit
                        + u128::from(l2_gas.max_amount) * l2_gas.max_price_per_unit
                        + u128::from(l1_data_gas.max_amount) * l1_data_gas.max_price_per_unit
                        > 0
                }
            },
            TransactionInfo::Deprecated(context) => context.max_fee != Fee(0),
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

impl CurrentTransactionInfo {
    /// Fetch the L1 resource bounds, if they exist.
    // TODO(Nimrod): Consider removing this function and add equivalent method to
    // `ValidResourceBounds`.
    pub fn l1_resource_bounds(&self) -> ResourceBounds {
        match self.resource_bounds {
            ValidResourceBounds::L1Gas(bounds) => bounds,
            ValidResourceBounds::AllResources(AllResourceBounds { l1_gas, .. }) => l1_gas,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeprecatedTransactionInfo {
    pub common_fields: CommonAccountFields,
    pub max_fee: Fee,
}

#[cfg_attr(feature = "transaction_serde", derive(serde::Deserialize))]
#[derive(
    derive_more::Add, derive_more::Sum, Clone, Copy, Debug, Default, Eq, PartialEq, Serialize,
)]
pub struct GasVector {
    pub l1_gas: u128,
    pub l1_data_gas: u128,
    pub l2_gas: u128,
}

impl GasVector {
    pub fn from_l1_gas(l1_gas: u128) -> Self {
        Self { l1_gas, ..Default::default() }
    }

    pub fn from_l1_data_gas(l1_data_gas: u128) -> Self {
        Self { l1_data_gas, ..Default::default() }
    }

    pub fn from_l2_gas(l2_gas: u128) -> Self {
        Self { l2_gas, ..Default::default() }
    }

    /// Computes the cost (in fee token units) of the gas vector (saturating on overflow).
    pub fn saturated_cost(&self, gas_price: u128, blob_gas_price: u128) -> Fee {
        let l1_gas_cost = self.l1_gas.checked_mul(gas_price).unwrap_or_else(|| {
            log::warn!(
                "L1 gas cost overflowed: multiplication of {} by {} resulted in overflow.",
                self.l1_gas,
                gas_price
            );
            u128::MAX
        });
        let l1_data_gas_cost = self.l1_data_gas.checked_mul(blob_gas_price).unwrap_or_else(|| {
            log::warn!(
                "L1 blob gas cost overflowed: multiplication of {} by {} resulted in overflow.",
                self.l1_data_gas,
                blob_gas_price
            );
            u128::MAX
        });
        let total = l1_gas_cost.checked_add(l1_data_gas_cost).unwrap_or_else(|| {
            log::warn!(
                "Total gas cost overflowed: addition of {} and {} resulted in overflow.",
                l1_gas_cost,
                l1_data_gas_cost
            );
            u128::MAX
        });
        Fee(total)
    }

    /// Compute l1_gas estimation from gas_vector using the following formula:
    /// One byte of data costs either 1 data gas (in blob mode) or 16 gas (in calldata
    /// mode). For gas price GP and data gas price DGP, the discount for using blobs
    /// would be DGP / (16 * GP).
    /// X non-data-related gas consumption and Y bytes of data, in non-blob mode, would
    /// cost (X + 16*Y) units of gas. Applying the discount ratio to the data-related
    /// summand, we get total_gas = (X + Y * DGP / GP).
    pub fn to_discounted_l1_gas(&self, tx_context: &TransactionContext) -> u128 {
        let gas_prices = &tx_context.block_context.block_info.gas_prices;
        let fee_type = tx_context.tx_info.fee_type();
        let gas_price = gas_prices.get_l1_gas_price_by_fee_type(&fee_type);
        let data_gas_price = gas_prices.get_l1_data_gas_price_by_fee_type(&fee_type);
        self.l1_gas + u128_div_ceil(self.l1_data_gas * u128::from(data_gas_price), gas_price)
    }
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

/// Contains the information gathered by the execution of a transaction.
#[cfg_attr(feature = "transaction_serde", derive(Serialize, serde::Deserialize))]
#[derive(Debug, Default, PartialEq)]
pub struct TransactionExecutionInfo {
    /// Transaction validation call info; [None] for `L1Handler`.
    pub validate_call_info: Option<CallInfo>,
    /// Transaction execution call info; [None] for `Declare`.
    pub execute_call_info: Option<CallInfo>,
    /// Fee transfer call info; [None] for `L1Handler`.
    pub fee_transfer_call_info: Option<CallInfo>,
    pub revert_error: Option<String>,
    /// The receipt of the transaction.
    /// Including the actual fee that was charged (in units of the relevant fee token),
    /// actual gas consumption the transaction is charged for data availability,
    /// actual execution resources the transaction is charged for
    /// (including L1 gas and additional OS resources estimation),
    /// and total gas consumed.
    pub receipt: TransactionReceipt,
}

impl TransactionExecutionInfo {
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
    pub fn summarize(&self) -> ExecutionSummary {
        self.non_optional_call_infos().map(|call_info| call_info.summarize()).sum()
    }
}

/// A mapping from a transaction execution resource to its actual usage.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ResourcesMapping(pub HashMap<String, usize>);

impl ResourcesMapping {
    #[cfg(test)]
    pub fn n_steps(&self) -> usize {
        *self.0.get(abi_constants::N_STEPS_RESOURCE).unwrap()
    }

    #[cfg(test)]
    pub fn gas_usage(&self) -> usize {
        *self.0.get(abi_constants::L1_GAS_USAGE).unwrap()
    }

    #[cfg(test)]
    pub fn blob_gas_usage(&self) -> usize {
        *self.0.get(abi_constants::BLOB_GAS_USAGE).unwrap()
    }
}

/// Contains all the L2 resources consumed by a transaction
#[cfg_attr(feature = "transaction_serde", derive(Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StarknetResources {
    pub calldata_length: usize,
    pub state_changes_for_fee: StateChangesCount,
    pub message_cost_info: MessageL1CostInfo,
    pub l1_handler_payload_size: Option<usize>,
    pub n_events: usize,
    signature_length: usize,
    code_size: usize,
    total_event_keys: u128,
    total_event_data_size: u128,
}

impl StarknetResources {
    pub fn new<'a>(
        calldata_length: usize,
        signature_length: usize,
        code_size: usize,
        state_changes_count: StateChangesCount,
        l1_handler_payload_size: Option<usize>,
        call_infos: impl Iterator<Item = &'a CallInfo> + Clone,
    ) -> Self {
        let (n_events, total_event_keys, total_event_data_size) =
            StarknetResources::calculate_events_resources(call_infos.clone());

        Self {
            calldata_length,
            signature_length,
            code_size,
            state_changes_for_fee: state_changes_count,
            l1_handler_payload_size,
            message_cost_info: MessageL1CostInfo::calculate(call_infos, l1_handler_payload_size),
            n_events,
            total_event_keys,
            total_event_data_size,
        }
    }

    /// Returns the gas cost of the starknet resources, summing all components.
    /// The L2 gas amount may be converted to L1 gas (depending on the gas vector computation mode).
    pub fn to_gas_vector(
        &self,
        versioned_constants: &VersionedConstants,
        use_kzg_da: bool,
        mode: &GasVectorComputationMode,
    ) -> GasVector {
        self.get_l2_archival_data_cost(versioned_constants, mode)
            + self.get_state_changes_cost(use_kzg_da)
            + self.get_messages_cost()
    }

    /// Returns the cost of the transaction's archival data, for example, calldata, signature, code,
    /// and events.
    pub fn get_l2_archival_data_cost(
        &self,
        versioned_constants: &VersionedConstants,
        mode: &GasVectorComputationMode,
    ) -> GasVector {
        let l2_resources_in_l1_gas = self.get_calldata_and_signature_cost(versioned_constants)
            + self.get_code_cost(versioned_constants)
            + self.get_events_cost(versioned_constants);
        match mode {
            GasVectorComputationMode::All => GasVector::from_l2_gas(
                versioned_constants.l1_to_l2_gas_price_conversion(l2_resources_in_l1_gas),
            ),
            GasVectorComputationMode::NoL2Gas => GasVector::from_l1_gas(l2_resources_in_l1_gas),
        }
    }

    /// Returns the cost for transaction calldata and transaction signature. Each felt costs a
    /// fixed and configurable amount of gas. This cost represents the cost of storing the
    /// calldata and the signature on L2.  The result is given in L1 gas units.
    // TODO(Nimrod, 1/10/2024): Calculate cost in L2 gas units.
    pub fn get_calldata_and_signature_cost(
        &self,
        versioned_constants: &VersionedConstants,
    ) -> u128 {
        // TODO(Avi, 20/2/2024): Calculate the number of bytes instead of the number of felts.
        let total_data_size = u128_from_usize(self.calldata_length + self.signature_length);
        (versioned_constants.l2_resource_gas_costs.gas_per_data_felt * total_data_size).to_integer()
    }

    /// Returns an estimation of the gas usage for processing L1<>L2 messages on L1. Accounts for
    /// Starknet contract only.
    fn get_messages_gas_usage(&self) -> GasVector {
        let n_l2_to_l1_messages = self.message_cost_info.l2_to_l1_payload_lengths.len();
        let n_l1_to_l2_messages = usize::from(self.l1_handler_payload_size.is_some());

        GasVector::from_l1_gas(
            // Starknet's updateState gets the message segment as an argument.
            u128_from_usize(
                self.message_cost_info.message_segment_length * eth_gas_constants::GAS_PER_MEMORY_WORD
                // Starknet's updateState increases a (storage) counter for each L2-to-L1 message.
                + n_l2_to_l1_messages * eth_gas_constants::GAS_PER_ZERO_TO_NONZERO_STORAGE_SET
                // Starknet's updateState decreases a (storage) counter for each L1-to-L2 consumed
                // message (note that we will probably get a refund of 15,000 gas for each consumed
                // message but we ignore it since refunded gas cannot be used for the current
                // transaction execution).
                + n_l1_to_l2_messages * eth_gas_constants::GAS_PER_COUNTER_DECREASE,
            ),
        ) + get_consumed_message_to_l2_emissions_cost(self.l1_handler_payload_size)
            + get_log_message_to_l1_emissions_cost(&self.message_cost_info.l2_to_l1_payload_lengths)
    }

    /// Returns an estimation of the gas usage for processing L1<>L2 messages on L1. Accounts for
    /// both Starknet and SHARP contracts.
    pub fn get_messages_cost(&self) -> GasVector {
        let starknet_gas_usage = self.get_messages_gas_usage();
        let sharp_gas_usage = GasVector::from_l1_gas(u128_from_usize(
            self.message_cost_info.message_segment_length
                * eth_gas_constants::SHARP_GAS_PER_MEMORY_WORD,
        ));

        starknet_gas_usage + sharp_gas_usage
    }

    /// Calculates the L1 resources used by L1<>L2 messages.
    /// Returns the total message segment length and the gas weight.
    pub fn calculate_message_l1_resources(&self) -> (usize, usize) {
        let message_segment_length = self.message_cost_info.message_segment_length;
        let gas_usage = self.get_messages_gas_usage();
        // TODO(Avi, 30/03/2024): Consider removing "l1_gas_usage" from actual resources.
        let gas_weight = usize_from_u128(gas_usage.l1_gas)
            .expect("This conversion should not fail as the value is a converted usize.");
        (message_segment_length, gas_weight)
    }

    /// Returns the cost of declared class codes in L1 gas units.
    // TODO(Nimrod, 1/10/2024): Calculate cost in L2 gas units.
    pub fn get_code_cost(&self, versioned_constants: &VersionedConstants) -> u128 {
        (versioned_constants.l2_resource_gas_costs.gas_per_code_byte
            * u128_from_usize(self.code_size))
        .to_integer()
    }

    /// Returns the gas cost of the transaction's state changes.
    pub fn get_state_changes_cost(&self, use_kzg_da: bool) -> GasVector {
        // TODO(Nimrod, 29/3/2024): delete `get_da_gas_cost` and move it's logic here.
        get_da_gas_cost(&self.state_changes_for_fee, use_kzg_da)
    }

    /// Returns the cost of the transaction's emmited events in L1 gas units.
    // TODO(Nimrod, 1/10/2024): Calculate cost in L2 gas units.
    pub fn get_events_cost(&self, versioned_constants: &VersionedConstants) -> u128 {
        let l2_resource_gas_costs = &versioned_constants.l2_resource_gas_costs;
        let (event_key_factor, data_word_cost) =
            (l2_resource_gas_costs.event_key_factor, l2_resource_gas_costs.gas_per_data_felt);
        (data_word_cost * (event_key_factor * self.total_event_keys + self.total_event_data_size))
            .to_integer()
    }

    pub fn get_onchain_data_segment_length(&self) -> usize {
        get_onchain_data_segment_length(&self.state_changes_for_fee)
    }

    /// Private and static method that calculates the n_events, total_event_keys and
    /// total_event_data_size fields according to the call_infos of a transaction.
    fn calculate_events_resources<'a>(
        call_infos: impl Iterator<Item = &'a CallInfo> + Clone,
    ) -> (usize, u128, u128) {
        let mut total_event_keys = 0;
        let mut total_event_data_size = 0;
        let mut n_events = 0;
        for call_info in call_infos.clone() {
            for inner_call in call_info.iter() {
                for OrderedEvent { event, .. } in inner_call.execution.events.iter() {
                    // TODO(barak: 18/03/2024): Once we start charging per byte
                    // change to num_bytes_keys
                    // and num_bytes_data.
                    total_event_data_size += u128_from_usize(event.data.0.len());
                    total_event_keys += u128_from_usize(event.keys.len());
                }
                n_events += inner_call.execution.events.len();
            }
        }
        (n_events, total_event_keys, total_event_data_size)
    }
}

#[cfg_attr(feature = "transaction_serde", derive(Serialize, serde::Deserialize))]
#[derive(Default, Clone, Debug, PartialEq)]
pub struct TransactionResources {
    pub starknet_resources: StarknetResources,
    pub vm_resources: ExecutionResources,
    pub n_reverted_steps: usize,
}

pub enum GasVectorComputationMode {
    All,
    NoL2Gas,
}

impl TransactionResources {
    /// Computes and returns the total gas consumption. The L2 gas amount may be converted
    /// to L1 gas (depending on the gas vector computation mode).
    pub fn to_gas_vector(
        &self,
        versioned_constants: &VersionedConstants,
        use_kzg_da: bool,
        computation_mode: &GasVectorComputationMode,
    ) -> TransactionFeeResult<GasVector> {
        Ok(self.starknet_resources.to_gas_vector(versioned_constants, use_kzg_da, computation_mode)
            + get_vm_resources_cost(
                versioned_constants,
                &self.vm_resources,
                self.n_reverted_steps,
                computation_mode,
            )?)
    }

    pub fn total_charged_steps(&self) -> usize {
        self.n_reverted_steps + self.vm_resources.n_steps
    }
}

pub trait ExecutionResourcesTraits {
    fn total_n_steps(&self) -> usize;
    fn to_resources_mapping(&self) -> ResourcesMapping;
    fn prover_builtins(&self) -> HashMap<BuiltinName, usize>;
    fn prover_builtins_by_name(&self) -> HashMap<String, usize>;
}

impl ExecutionResourcesTraits for ExecutionResources {
    fn total_n_steps(&self) -> usize {
        self.n_steps
            // Memory holes are slightly cheaper than actual steps, but we count them as such
            // for simplicity.
            + self.n_memory_holes
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

    fn prover_builtins(&self) -> HashMap<BuiltinName, usize> {
        let mut builtins = self.builtin_instance_counter.clone();

        // See "total_n_steps" documentation.
        builtins.remove(&BuiltinName::segment_arena);
        builtins
    }

    fn prover_builtins_by_name(&self) -> HashMap<String, usize> {
        self.prover_builtins()
            .iter()
            .map(|(builtin, value)| (builtin.to_str_with_suffix().to_string(), *value))
            .collect()
    }

    // TODO(Nimrod, 1/5/2024): Delete this function when it's no longer in use.
    fn to_resources_mapping(&self) -> ResourcesMapping {
        let mut map =
            HashMap::from([(abi_constants::N_STEPS_RESOURCE.to_string(), self.total_n_steps())]);
        map.extend(self.prover_builtins_by_name());

        ResourcesMapping(map)
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

    fn get_fee_by_gas_vector(&self, block_info: &BlockInfo, gas_vector: GasVector) -> Fee {
        get_fee_by_gas_vector(block_info, gas_vector, &self.fee_type())
    }
}

#[derive(Clone, Copy, Hash, EnumIter, Eq, PartialEq)]
pub enum FeeType {
    Strk,
    Eth,
}

pub trait TransactionInfoCreator {
    fn create_tx_info(&self) -> TransactionInfo;
}
