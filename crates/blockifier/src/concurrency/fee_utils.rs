use std::collections::HashMap;

use num_traits::ToPrimitive;
use starknet_api::core::ContractAddress;
use starknet_api::transaction::fields::Fee;
use starknet_types_core::felt::Felt;

use crate::context::{BlockContext, TransactionContext};
use crate::execution::call_info::CallInfo;
use crate::fee::fee_utils::get_sequencer_balance_keys;
use crate::state::cached_state::{ContractClassMapping, StateMaps};
use crate::state::state_api::UpdatableState;
use crate::transaction::objects::TransactionExecutionInfo;
use crate::transaction::transaction_execution::Transaction;

#[cfg(test)]
#[path = "fee_utils_test.rs"]
mod test;

// We read account balance (sender), and sequencer balance (recipient). The balance is of type
// `Uint256`, consist of two felts (lsb, msb). Hence, storage read values =
// [account_balance, 0, sequencer_balance, 0]
pub(crate) const STORAGE_READ_SEQUENCER_BALANCE_INDICES: (usize, usize) = (2, 3);

// Completes the fee transfer flow if needed (if the transfer was made in concurrent mode).
pub fn complete_fee_transfer_flow(
    tx_context: &TransactionContext,
    tx_execution_info: &mut TransactionExecutionInfo,
    state_diff: &mut StateMaps,
    state: &mut impl UpdatableState,
    tx: &Transaction,
) {
    if tx_context.is_sequencer_the_sender() {
        // When the sequencer is the sender, we use the sequential (full) fee transfer.
        return;
    }

    if let Some(fee_transfer_call_info) = tx_execution_info.fee_transfer_call_info.as_mut() {
        let sequencer_balance = state
        .get_fee_token_balance(
            tx_context.block_context.block_info.sequencer_address,
            tx_context.fee_token_address()
        )
        // TODO(barak, 01/07/2024): Consider propagating the error.
        .unwrap_or_else(|error| {
            panic!(
                "Access to storage failed. Probably due to a bug in Papyrus. {error:?}: {error}"
            )
        });

        // Fix the transfer call info.
        fill_sequencer_balance_reads(fee_transfer_call_info, sequencer_balance);
        // Update the balance.
        add_fee_to_sequencer_balance(
            tx_context.fee_token_address(),
            state,
            tx_execution_info.receipt.fee,
            &tx_context.block_context,
            sequencer_balance,
            tx_context.tx_info.sender_address(),
            state_diff,
        );
    } else {
        // Sanity check.
        match tx {
            Transaction::Account(tx) => assert!(
                !tx.execution_flags.charge_fee || tx_execution_info.receipt.fee == Fee(0),
                "Transaction with no fee transfer info must not enforce a fee charge."
            ),
            // No fee transfer info for L1 handler transactions.
            Transaction::L1Handler(_) => {}
        };
    }
}

// Fixes the fee transfer call info to have the correct sequencer balance. In concurrency mode, the
// fee transfer is executed with a false (constant) sequencer balance. This affects the call info.
pub fn fill_sequencer_balance_reads(
    fee_transfer_call_info: &mut CallInfo,
    sequencer_balance: (Felt, Felt),
) {
    let storage_read_values = if fee_transfer_call_info.inner_calls.is_empty() {
        &mut fee_transfer_call_info.storage_access_tracker.storage_read_values
    } else
    // Proxy pattern.
    {
        assert_eq!(
            fee_transfer_call_info.inner_calls.len(),
            1,
            "Proxy pattern should have one inner call"
        );
        &mut fee_transfer_call_info.inner_calls[0].storage_access_tracker.storage_read_values
    };
    assert_eq!(storage_read_values.len(), 4, "Storage read values should have 4 elements");

    let (low_index, high_index) = STORAGE_READ_SEQUENCER_BALANCE_INDICES;
    for index in [low_index, high_index] {
        assert_eq!(storage_read_values[index], Felt::ZERO, "Sequencer balance should be zero");
    }
    let (low, high) = sequencer_balance;
    storage_read_values[low_index] = low;
    storage_read_values[high_index] = high;
}

pub fn add_fee_to_sequencer_balance(
    fee_token_address: ContractAddress,
    state: &mut impl UpdatableState,
    actual_fee: Fee,
    block_context: &BlockContext,
    sequencer_balance: (Felt, Felt),
    sender_address: ContractAddress,
    state_diff: &mut StateMaps,
) {
    assert_ne!(
        sender_address, block_context.block_info.sequencer_address,
        "The sender cannot be the sequencer."
    );
    let (low, high) = sequencer_balance;
    let sequencer_balance_low_as_u128 =
        low.to_u128().expect("sequencer balance low should be u128");
    let sequencer_balance_high_as_u128 =
        high.to_u128().expect("sequencer balance high should be u128");
    let (new_value_low, overflow_low) = sequencer_balance_low_as_u128.overflowing_add(actual_fee.0);
    let (new_value_high, overflow_high) =
        sequencer_balance_high_as_u128.overflowing_add(overflow_low.into());
    assert!(
        !overflow_high,
        "The sequencer balance overflowed when adding the fee. This should not happen."
    );
    let (sequencer_balance_key_low, sequencer_balance_key_high) =
        get_sequencer_balance_keys(block_context);
    let writes = StateMaps {
        storage: HashMap::from([
            ((fee_token_address, sequencer_balance_key_low), Felt::from(new_value_low)),
            ((fee_token_address, sequencer_balance_key_high), Felt::from(new_value_high)),
        ]),
        ..StateMaps::default()
    };

    // Modify state_diff to accurately reflect the post tx-execution state, after fee transfer to
    // the sequencer. We assume that a non-sequencer sender cannot reduce the sequencer's
    // balance—only increases are possible.

    if sequencer_balance_high_as_u128 != new_value_high {
        // Update the high balance only if it has changed.
        state_diff
            .storage
            .insert((fee_token_address, sequencer_balance_key_high), Felt::from(new_value_high));
    }

    if sequencer_balance_low_as_u128 != new_value_low {
        // Update the low balance only if it has changed.
        state_diff
            .storage
            .insert((fee_token_address, sequencer_balance_key_low), Felt::from(new_value_low));
    }
    state.apply_writes(&writes, &ContractClassMapping::default());
}
