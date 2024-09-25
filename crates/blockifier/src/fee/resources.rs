use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use serde::Serialize;
use starknet_api::transaction::Fee;

use crate::context::TransactionContext;
use crate::execution::call_info::{CallInfo, EventSummary, MessageL1CostInfo};
use crate::fee::eth_gas_constants;
use crate::fee::fee_utils::get_vm_resources_cost;
use crate::fee::gas_usage::{
    get_consumed_message_to_l2_emissions_cost,
    get_da_gas_cost,
    get_log_message_to_l1_emissions_cost,
    get_message_segment_length,
    get_onchain_data_segment_length,
};
use crate::state::cached_state::StateChangesCount;
use crate::transaction::errors::TransactionFeeError;
use crate::transaction::objects::HasRelatedFeeType;
use crate::utils::{u128_div_ceil, u128_from_usize, usize_from_u128};
use crate::versioned_constants::{ArchivalDataGasCosts, VersionedConstants};

pub type TransactionFeeResult<T> = Result<T, TransactionFeeError>;

#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default, Clone, Debug, PartialEq)]
pub struct TransactionResources {
    pub starknet_resources: StarknetResources,
    pub vm_resources: ExecutionResources,
    pub n_reverted_steps: usize,
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

/// Contains all non-computation Starknet resources consumed by a transaction.
#[cfg_attr(feature = "transaction_serde", derive(Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StarknetResources {
    pub archival_data_resources: ArchivalDataResources,
    pub state_changes_for_fee: StateChangesCount,
    pub message_cost_info: MessageL1CostInfo,
    pub l1_handler_payload_size: Option<usize>,
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
        // TODO(Yoni): store the entire summary.
        let execution_summary_without_fee_transfer = CallInfo::summarize_many(call_infos.clone());
        let l2_to_l1_payload_lengths =
            execution_summary_without_fee_transfer.l2_to_l1_payload_lengths;
        let message_segment_length =
            get_message_segment_length(&l2_to_l1_payload_lengths, l1_handler_payload_size);
        Self {
            archival_data_resources: ArchivalDataResources {
                event_summary: execution_summary_without_fee_transfer.event_summary,
                calldata_length,
                signature_length,
                code_size,
            },
            state_changes_for_fee: state_changes_count,
            // TODO(Yoni, 1/10/2024): remove.
            message_cost_info: MessageL1CostInfo {
                l2_to_l1_payload_lengths,
                message_segment_length,
            },
            l1_handler_payload_size,
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
        self.archival_data_resources.to_gas_vector(versioned_constants, mode)
            + self.get_state_changes_cost(use_kzg_da)
            + self.get_messages_total_gas_cost()
    }

    /// Returns an estimation of the gas usage for processing L1<>L2 messages on L1. Accounts for
    /// Starknet contract only.
    fn get_messages_starknet_gas_cost(&self) -> GasVector {
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
    pub fn get_messages_total_gas_cost(&self) -> GasVector {
        let starknet_gas_usage = self.get_messages_starknet_gas_cost();
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
        let gas_usage = self.get_messages_starknet_gas_cost();
        // TODO(Avi, 30/03/2024): Consider removing "l1_gas_usage" from actual resources.
        let gas_weight = usize_from_u128(gas_usage.l1_gas)
            .expect("This conversion should not fail as the value is a converted usize.");
        (message_segment_length, gas_weight)
    }

    /// Returns the gas cost of the transaction's state changes.
    pub fn get_state_changes_cost(&self, use_kzg_da: bool) -> GasVector {
        // TODO(Nimrod, 29/3/2024): delete `get_da_gas_cost` and move it's logic here.
        get_da_gas_cost(&self.state_changes_for_fee, use_kzg_da)
    }

    pub fn get_onchain_data_segment_length(&self) -> usize {
        get_onchain_data_segment_length(&self.state_changes_for_fee)
    }
}

#[cfg_attr(feature = "transaction_serde", derive(Serialize, serde::Deserialize))]
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
        .sum();
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
    ) -> u128 {
        // TODO(Avi, 20/2/2024): Calculate the number of bytes instead of the number of felts.
        let total_data_size = u128_from_usize(self.calldata_length + self.signature_length);
        (archival_gas_costs.gas_per_data_felt * total_data_size).to_integer()
    }

    /// Returns the cost of declared class codes in L1/L2 gas units, depending on the mode.
    fn get_code_gas_cost(&self, archival_gas_costs: &ArchivalDataGasCosts) -> u128 {
        (archival_gas_costs.gas_per_code_byte * u128_from_usize(self.code_size)).to_integer()
    }

    /// Returns the cost of the transaction's emmited events in L1/L2 gas units, depending on the
    /// mode.
    fn get_events_gas_cost(&self, archival_gas_costs: &ArchivalDataGasCosts) -> u128 {
        (archival_gas_costs.gas_per_data_felt
            * (archival_gas_costs.event_key_factor * self.event_summary.total_event_keys
                + self.event_summary.total_event_data_size))
            .to_integer()
    }
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

#[derive(Debug, PartialEq)]
pub enum GasVectorComputationMode {
    All,
    NoL2Gas,
}
