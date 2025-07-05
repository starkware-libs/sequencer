use std::collections::HashSet;

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use num_bigint::BigUint;
use starknet_api::abi::abi_utils::get_fee_token_var_address;
use starknet_api::block::{BlockInfo, FeeType, GasPriceVector};
use starknet_api::core::ContractAddress;
use starknet_api::execution_resources::{to_discounted_l1_gas, GasAmount, GasVector};
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::ValidResourceBounds::{AllResources, L1Gas};
use starknet_api::transaction::fields::{Fee, GasVectorComputationMode, Resource, Tip};
use starknet_types_core::felt::Felt;

use crate::blockifier_versioned_constants::VersionedConstants;
use crate::context::{BlockContext, TransactionContext};
use crate::fee::resources::TransactionFeeResult;
use crate::state::state_api::StateReader;
use crate::transaction::errors::TransactionFeeError;
use crate::transaction::objects::{ExecutionResourcesTraits, TransactionInfo};
use crate::utils::u64_from_usize;

#[cfg(test)]
#[path = "fee_test.rs"]
pub mod test;

/// A trait for converting a gas vector to L1 gas for fee. Converts both L1 data gas and L2 gas to
/// L1 gas units.
/// The trait is defined to allow implementing a method on GasVector, which is a starknet-api type.
pub trait GasVectorToL1GasForFee {
    fn to_l1_gas_for_fee(
        &self,
        gas_prices: &GasPriceVector,
        versioned_constants: &VersionedConstants,
    ) -> GasAmount;
}

impl GasVectorToL1GasForFee for GasVector {
    fn to_l1_gas_for_fee(
        &self,
        gas_prices: &GasPriceVector,
        versioned_constants: &VersionedConstants,
    ) -> GasAmount {
        // Discounted gas converts data gas to L1 gas. Add L2 gas using conversion ratio.
        let discounted_l1_gas = to_discounted_l1_gas(
            gas_prices.l1_gas_price,
            gas_prices.l1_data_gas_price.into(),
            self.l1_gas,
            self.l1_data_gas,
        );
        discounted_l1_gas
            .checked_add(versioned_constants.sierra_gas_to_l1_gas_amount_round_up(self.l2_gas))
            .unwrap_or_else(|| {
                panic!(
                    "L1 gas amount overflowed: addition of converted L2 gas ({}) to discounted \
                     gas ({}) resulted in overflow.",
                    self.l2_gas, discounted_l1_gas
                );
            })
    }
}

/// Calculates the gas consumed when submitting the underlying Cairo program to SHARP.
/// I.e., returns the heaviest Cairo resource weight (in terms of gas), as the size of
/// a proof is determined similarly - by the (normalized) largest segment.
/// The result can be in either L1 or L2 gas, according to the gas vector computation mode.
pub fn get_vm_resources_cost(
    versioned_constants: &VersionedConstants,
    vm_resource_usage: &ExecutionResources,
    n_reverted_steps: usize,
    computation_mode: &GasVectorComputationMode,
) -> GasVector {
    // TODO(Yoni, 1/7/2024): rename vm -> cairo.
    let vm_resource_fee_costs = versioned_constants.vm_resource_fee_cost();
    let builtin_usage_for_fee = vm_resource_usage.prover_builtins();

    // Validate used builtin resources.
    let used_builtins = HashSet::<&BuiltinName>::from_iter(builtin_usage_for_fee.keys());
    let known_builtins = HashSet::<&BuiltinName>::from_iter(vm_resource_fee_costs.builtins.keys());
    assert!(
        used_builtins.is_subset(&known_builtins),
        "{known_builtins:#?} should contain {used_builtins:#?}",
    );

    // Convert Cairo resource usage to L1 gas usage.
    // Do so by taking the maximum of the usage of each builtin + step usage.
    let vm_l1_gas_usage = vm_resource_fee_costs
        .builtins
        .iter()
        // Builtin costs and usage.
        .map(|(builtin, resource_cost)| {
            (*resource_cost, builtin_usage_for_fee.get(builtin).cloned().unwrap_or_default())
        })
        // Step costs and usage.
        .chain(vec![(
            vm_resource_fee_costs.n_steps,
            // TODO(AvivG): Compute memory_holes gas accurately while maintaining backward compatibility
            // Memory holes are slightly cheaper than actual steps, but we count them as such
            // for simplicity.
            vm_resource_usage.total_n_steps() + vm_resource_usage.n_memory_holes + n_reverted_steps,
        )])
        .map(|(cost, usage)| (cost * u64_from_usize(usage)).ceil().to_integer())
        .fold(0, u64::max).into();

    match computation_mode {
        GasVectorComputationMode::NoL2Gas => GasVector::from_l1_gas(vm_l1_gas_usage),
        GasVectorComputationMode::All => GasVector::from_l2_gas(
            versioned_constants.l1_gas_to_sierra_gas_amount_round_up(vm_l1_gas_usage),
        ),
    }
}

/// Converts the gas vector to a fee.
pub fn get_fee_by_gas_vector(
    block_info: &BlockInfo,
    gas_vector: GasVector,
    fee_type: &FeeType,
    tip: Tip,
) -> Fee {
    gas_vector.cost(block_info.gas_prices.gas_price_vector(fee_type), tip)
}

/// Returns the current fee balance and a boolean indicating whether the balance covers the fee.
pub fn get_balance_and_if_covers_fee(
    state: &mut dyn StateReader,
    tx_context: &TransactionContext,
    fee: Fee,
) -> TransactionFeeResult<(Felt, Felt, bool)> {
    let tx_info = &tx_context.tx_info;
    let (balance_low, balance_high) =
        state.get_fee_token_balance(tx_info.sender_address(), tx_context.fee_token_address())?;
    Ok((
        balance_low,
        balance_high,
        // TODO(Dori,1/10/2023): If/when fees can be more than 128 bit integers, this should be
        //   updated.
        balance_high > Felt::ZERO || balance_low >= Felt::from(fee.0),
    ))
}

/// Verifies that, given the current state, the account can cover the resource upper bounds.
/// Error may indicate insufficient balance, or some other error.
pub fn verify_can_pay_committed_bounds(
    state: &mut dyn StateReader,
    tx_context: &TransactionContext,
) -> TransactionFeeResult<()> {
    let tx_info = &tx_context.tx_info;
    let committed_fee = tx_context.max_possible_fee();
    let (balance_low, balance_high, can_pay) =
        get_balance_and_if_covers_fee(state, tx_context, committed_fee)?;
    if can_pay {
        Ok(())
    } else {
        Err(match tx_info {
            TransactionInfo::Current(context) => match &context.resource_bounds {
                L1Gas(l1_gas) => TransactionFeeError::GasBoundsExceedBalance {
                    resource: Resource::L1Gas,
                    max_amount: l1_gas.max_amount,
                    max_price: l1_gas.max_price_per_unit,
                    balance: balance_to_big_uint(&balance_low, &balance_high),
                },
                AllResources(bounds) => TransactionFeeError::ResourcesBoundsExceedBalance {
                    bounds: *bounds,
                    balance: balance_to_big_uint(&balance_low, &balance_high),
                },
            },
            TransactionInfo::Deprecated(context) => TransactionFeeError::MaxFeeExceedsBalance {
                max_fee: context.max_fee,
                balance: balance_to_big_uint(&balance_low, &balance_high),
            },
        })
    }
}

pub fn get_sequencer_balance_keys(block_context: &BlockContext) -> (StorageKey, StorageKey) {
    let sequencer_address = block_context.block_info.sequencer_address;
    get_address_balance_keys(sequencer_address)
}

pub fn get_address_balance_keys(address: ContractAddress) -> (StorageKey, StorageKey) {
    let balance_key_low = get_fee_token_var_address(address);
    let balance_key_high = balance_key_low.next_storage_key().unwrap_or_else(|_| {
        panic!("Failed to get balance_key_high for address: {:?}", address.0);
    });
    (balance_key_low, balance_key_high)
}

pub(crate) fn balance_to_big_uint(balance_low: &Felt, balance_high: &Felt) -> BigUint {
    let low = BigUint::from_bytes_be(&balance_low.to_bytes_be());
    let high = BigUint::from_bytes_be(&balance_high.to_bytes_be());
    (high << 128) + low
}
