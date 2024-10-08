use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::core::ContractAddress;
use starknet_api::execution_resources::{GasAmount, GasVector};
use starknet_api::transaction::fields::GasVectorComputationMode;

use crate::execution::call_info::{EventSummary, ExecutionSummary};
use crate::fee::eth_gas_constants;
use crate::fee::fee_utils::get_vm_resources_cost;
use crate::fee::gas_usage::{
    get_consumed_message_to_l2_emissions_cost,
    get_da_gas_cost,
    get_log_message_to_l1_emissions_cost,
    get_message_segment_length,
    get_onchain_data_segment_length,
};
use crate::state::cached_state::{StateChanges, StateChangesCount};
use crate::transaction::errors::TransactionFeeError;
use crate::utils::u64_from_usize;
use crate::versioned_constants::{ArchivalDataGasCosts, VersionedConstants};

pub type TransactionFeeResult<T> = Result<T, TransactionFeeError>;

#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default, Clone, Debug, PartialEq)]
pub struct TransactionResources {
    pub starknet_resources: StarknetResources,
    pub computation: ComputationResources,
}

impl TransactionResources {
    /// Computes and returns the total gas consumption. The L2 gas amount may be converted
    /// to L1 gas (depending on the gas vector computation mode).
    pub fn to_gas_vector(
        &self,
        versioned_constants: &VersionedConstants,
        use_kzg_da: bool,
        computation_mode: &GasVectorComputationMode,
    ) -> GasVector {
        let starknet_gas = self.starknet_resources.to_gas_vector(
            versioned_constants,
            use_kzg_da,
            computation_mode,
        );
        let computation_gas = self.computation.to_gas_vector(versioned_constants, computation_mode);
        starknet_gas.checked_add(computation_gas).unwrap_or_else(|| {
            panic!(
                "Transaction resources to gas vector overflowed: starknet gas cost is \
                 {starknet_gas:?}, computation gas is {computation_gas:?}",
            )
        })
    }
}

/// Contains all computation resources consumed by a transaction.
#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComputationResources {
    pub vm_resources: ExecutionResources,
    pub n_reverted_steps: usize,
    // TODO(Tzahi): add sierra_gas here.
}

impl ComputationResources {
    pub fn to_gas_vector(
        &self,
        versioned_constants: &VersionedConstants,
        computation_mode: &GasVectorComputationMode,
    ) -> GasVector {
        get_vm_resources_cost(
            versioned_constants,
            &self.vm_resources,
            self.n_reverted_steps,
            computation_mode,
        )
    }

    #[cfg(test)]
    pub fn total_charged_steps(&self) -> usize {
        self.n_reverted_steps + self.vm_resources.n_steps
    }
}

/// Contains all non-computation Starknet resources consumed by a transaction.
#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StarknetResources {
    pub archival_data: ArchivalDataResources,
    pub messages: MessageResources,
    pub state: StateResources,
}

impl StarknetResources {
    pub fn new(
        calldata_length: usize,
        signature_length: usize,
        code_size: usize,
        state_resources: StateResources,
        l1_handler_payload_size: Option<usize>,
        execution_summary_without_fee_transfer: ExecutionSummary,
    ) -> Self {
        // TODO(Yoni): store the entire summary.
        Self {
            archival_data: ArchivalDataResources {
                event_summary: execution_summary_without_fee_transfer.event_summary,
                calldata_length,
                signature_length,
                code_size,
            },
            messages: MessageResources::new(
                execution_summary_without_fee_transfer.l2_to_l1_payload_lengths,
                l1_handler_payload_size,
            ),
            state: state_resources,
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
        [
            self.archival_data.to_gas_vector(versioned_constants, mode),
            self.state.to_gas_vector(use_kzg_da),
            self.messages.to_gas_vector(),
        ]
        .iter()
        .fold(GasVector::ZERO, |accumulator, cost| {
            accumulator.checked_add(*cost).unwrap_or_else(|| {
                panic!(
                    "Starknet resources to gas vector overflowed: tried to add {accumulator:?} to \
                     {cost:?}",
                )
            })
        })
    }
}

#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StateResources {
    state_changes_for_fee: StateChangesCount,
}

impl StateResources {
    pub fn new(
        state_changes: &StateChanges,
        sender_address: Option<ContractAddress>,
        fee_token_address: ContractAddress,
    ) -> Self {
        Self {
            state_changes_for_fee: state_changes
                .count_for_fee_charge(sender_address, fee_token_address),
        }
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn new_for_testing(state_changes_for_fee: StateChangesCount) -> Self {
        Self { state_changes_for_fee }
    }

    /// Returns the gas cost of the transaction's state changes.
    pub fn to_gas_vector(&self, use_kzg_da: bool) -> GasVector {
        // TODO(Nimrod, 29/3/2024): delete `get_da_gas_cost` and move it's logic here.
        get_da_gas_cost(&self.state_changes_for_fee, use_kzg_da)
    }

    pub fn get_onchain_data_segment_length(&self) -> usize {
        get_onchain_data_segment_length(&self.state_changes_for_fee)
    }
}

#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArchivalDataResources {
    pub event_summary: EventSummary,
    pub calldata_length: usize,
    signature_length: usize,
    code_size: usize,
}

impl ArchivalDataResources {
    /// Returns the cost of the transaction's archival data, for example, calldata, signature, code,
    /// and events.
    pub fn to_gas_vector(
        &self,
        versioned_constants: &VersionedConstants,
        mode: &GasVectorComputationMode,
    ) -> GasVector {
        let archival_gas_costs = match mode {
            // Computation is in L2 gas units.
            GasVectorComputationMode::All => &versioned_constants.archival_data_gas_costs,
            // Computation is in L1 gas units.
            GasVectorComputationMode::NoL2Gas => {
                &versioned_constants.deprecated_l2_resource_gas_costs
            }
        };
        let gas_amount = [
            self.get_calldata_and_signature_gas_cost(archival_gas_costs),
            self.get_code_gas_cost(archival_gas_costs),
            self.get_events_gas_cost(archival_gas_costs),
        ]
        .into_iter()
        .fold(GasAmount::ZERO, |accumulator, cost| {
            accumulator.checked_add(cost).unwrap_or_else(|| {
                panic!(
                    "Archival data resources to gas vector overflowed: tried to add \
                     {accumulator:?} gas to {cost:?} gas.",
                )
            })
        });
        match mode {
            GasVectorComputationMode::All => GasVector::from_l2_gas(gas_amount),
            GasVectorComputationMode::NoL2Gas => GasVector::from_l1_gas(gas_amount),
        }
    }

    /// Returns the cost for transaction calldata and transaction signature. Each felt costs a
    /// fixed and configurable amount of gas. This cost represents the cost of storing the
    /// calldata and the signature on L2.  The result is given in L1/L2 gas units, depending on the
    /// mode.
    fn get_calldata_and_signature_gas_cost(
        &self,
        archival_gas_costs: &ArchivalDataGasCosts,
    ) -> GasAmount {
        // TODO(Avi, 20/2/2024): Calculate the number of bytes instead of the number of felts.
        let total_data_size = u64_from_usize(self.calldata_length + self.signature_length);
        (archival_gas_costs.gas_per_data_felt * total_data_size).to_integer().into()
    }

    /// Returns the cost of declared class codes in L1/L2 gas units, depending on the mode.
    fn get_code_gas_cost(&self, archival_gas_costs: &ArchivalDataGasCosts) -> GasAmount {
        (archival_gas_costs.gas_per_code_byte * u64_from_usize(self.code_size)).to_integer().into()
    }

    /// Returns the cost of the transaction's emmited events in L1/L2 gas units, depending on the
    /// mode.
    fn get_events_gas_cost(&self, archival_gas_costs: &ArchivalDataGasCosts) -> GasAmount {
        (archival_gas_costs.gas_per_data_felt
            * (archival_gas_costs.event_key_factor * self.event_summary.total_event_keys
                + self.event_summary.total_event_data_size))
            .to_integer()
            .into()
    }
}

/// Contains L1->L2 and L2->L1 message resources.
#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MessageResources {
    pub l2_to_l1_payload_lengths: Vec<usize>,
    pub message_segment_length: usize,
    pub l1_handler_payload_size: Option<usize>,
}

impl MessageResources {
    pub fn new(
        l2_to_l1_payload_lengths: Vec<usize>,
        l1_handler_payload_size: Option<usize>,
    ) -> Self {
        let message_segment_length =
            get_message_segment_length(&l2_to_l1_payload_lengths, l1_handler_payload_size);
        Self { l2_to_l1_payload_lengths, message_segment_length, l1_handler_payload_size }
    }
    /// Returns an estimation of the gas usage for processing L1<>L2 messages on L1. Accounts for
    /// both Starknet and SHARP contracts.
    pub fn to_gas_vector(&self) -> GasVector {
        let starknet_gas_usage = self.get_starknet_gas_cost();
        let sharp_gas_usage = GasVector::from_l1_gas(
            u64_from_usize(
                self.message_segment_length * eth_gas_constants::SHARP_GAS_PER_MEMORY_WORD,
            )
            .into(),
        );

        starknet_gas_usage.checked_add(sharp_gas_usage).unwrap_or_else(|| {
            panic!(
                "Message resources to gas vector overflowed: starknet gas cost is \
                 {starknet_gas_usage:?}, SHARP gas cost is {sharp_gas_usage:?}.",
            )
        })
    }

    /// Returns an estimation of the gas usage for processing L1<>L2 messages on L1. Accounts for
    /// Starknet contract only.
    pub fn get_starknet_gas_cost(&self) -> GasVector {
        let n_l2_to_l1_messages = self.l2_to_l1_payload_lengths.len();
        let n_l1_to_l2_messages = usize::from(self.l1_handler_payload_size.is_some());

        [
            GasVector::from_l1_gas(
                // Starknet's updateState gets the message segment as an argument.
                u64_from_usize(
                    self.message_segment_length * eth_gas_constants::GAS_PER_MEMORY_WORD
                // Starknet's updateState increases a (storage) counter for each L2-to-L1 message.
                + n_l2_to_l1_messages * eth_gas_constants::GAS_PER_ZERO_TO_NONZERO_STORAGE_SET
                // Starknet's updateState decreases a (storage) counter for each L1-to-L2 consumed
                // message (note that we will probably get a refund of 15,000 gas for each consumed
                // message but we ignore it since refunded gas cannot be used for the current
                // transaction execution).
                + n_l1_to_l2_messages * eth_gas_constants::GAS_PER_COUNTER_DECREASE,
                )
                .into(),
            ),
            get_consumed_message_to_l2_emissions_cost(self.l1_handler_payload_size),
            get_log_message_to_l1_emissions_cost(&self.l2_to_l1_payload_lengths),
        ]
        .iter()
        .fold(GasVector::ZERO, |accumulator, cost| {
            accumulator.checked_add(*cost).unwrap_or_else(|| {
                panic!(
                    "Message resources to starknet gas cost overflowed: tried to add \
                     {accumulator:?} to {cost:?}"
                )
            })
        })
    }
}
