use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::Poseidon;

use super::event_commitment::{calculate_event_commitment, EventLeafElement};
use super::receipt_commitment::{calculate_receipt_commitment, ReceiptElement};
use super::state_diff_hash::calculate_state_diff_hash;
use super::transaction_commitment::{calculate_transaction_commitment, TransactionLeafElement};
use crate::block::{BlockHash, BlockHeaderWithoutHash};
use crate::core::{EventCommitment, ReceiptCommitment, StateDiffCommitment, TransactionCommitment};
use crate::crypto::utils::HashChain;
use crate::data_availability::L1DataAvailabilityMode;
use crate::state::ThinStateDiff;
use crate::transaction::{
    Event, Fee, GasVector, MessageToL1, TransactionExecutionStatus, TransactionHash,
    TransactionSignature,
};
use crate::transaction_hash::ascii_as_felt;

#[cfg(test)]
#[path = "block_hash_calculator_test.rs"]
mod block_hash_calculator_test;

static STARKNET_BLOCK_HASH0: Lazy<Felt> = Lazy::new(|| {
    ascii_as_felt("STARKNET_BLOCK_HASH0").expect("ascii_as_felt failed for 'STARKNET_BLOCK_HASH0'")
});

/// The common fields of transaction output types.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct TransactionOutputForHash {
    pub actual_fee: Fee,
    pub events: Vec<Event>,
    pub execution_status: TransactionExecutionStatus,
    pub gas_consumed: GasVector,
    pub messages_sent: Vec<MessageToL1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct TransactionHashingData {
    pub transaction_signature: Option<TransactionSignature>,
    pub transaction_output: TransactionOutputForHash,
    pub transaction_hash: TransactionHash,
}

/// Commitments of a block.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockHeaderCommitments {
    pub transaction_commitment: TransactionCommitment,
    pub event_commitment: EventCommitment,
    pub receipt_commitment: ReceiptCommitment,
    pub state_diff_commitment: StateDiffCommitment,
    pub concatenated_counts: Felt,
}

/// Poseidon (
///     “STARKNET_BLOCK_HASH0”, block_number, global_state_root, sequencer_address,
///     block_timestamp, concat_counts, state_diff_hash, transaction_commitment,
///     event_commitment, receipt_commitment, gas_price_wei, gas_price_fri,
///     data_gas_price_wei, data_gas_price_fri, starknet_version, 0, parent_block_hash
/// ).
pub fn calculate_block_hash(
    header: BlockHeaderWithoutHash,
    block_commitments: BlockHeaderCommitments,
) -> BlockHash {
    BlockHash(
        HashChain::new()
            .chain(&STARKNET_BLOCK_HASH0)
            .chain(&header.block_number.0.into())
            .chain(&header.state_root.0)
            .chain(&header.sequencer.0)
            .chain(&header.timestamp.0.into())
            .chain(&block_commitments.concatenated_counts)
            .chain(&block_commitments.state_diff_commitment.0.0)
            .chain(&block_commitments.transaction_commitment.0)
            .chain(&block_commitments.event_commitment.0)
            .chain(&block_commitments.receipt_commitment.0)
            .chain(&header.l1_gas_price.price_in_wei.0.into())
            .chain(&header.l1_gas_price.price_in_fri.0.into())
            .chain(&header.l1_data_gas_price.price_in_wei.0.into())
            .chain(&header.l1_data_gas_price.price_in_fri.0.into())
            .chain(&ascii_as_felt(&header.starknet_version.0).expect("Expect ASCII version"))
            .chain(&Felt::ZERO)
            .chain(&header.parent_hash.0)
            .get_poseidon_hash(),
    )
}

/// Calculates the commitments of the transactions data for the block hash.
pub fn calculate_block_commitments(
    transactions_data: &[TransactionHashingData],
    state_diff: &ThinStateDiff,
    l1_da_mode: L1DataAvailabilityMode,
) -> BlockHeaderCommitments {
    let transaction_leaf_elements: Vec<TransactionLeafElement> =
        transactions_data.iter().map(TransactionLeafElement::from).collect();
    let transaction_commitment =
        calculate_transaction_commitment::<Poseidon>(&transaction_leaf_elements);

    let event_leaf_elements: Vec<EventLeafElement> = transactions_data
        .iter()
        .flat_map(|transaction_data| {
            transaction_data.transaction_output.events.iter().map(|event| EventLeafElement {
                event: event.clone(),
                transaction_hash: transaction_data.transaction_hash,
            })
        })
        .collect();
    let event_commitment = calculate_event_commitment::<Poseidon>(&event_leaf_elements);

    let receipt_elements: Vec<ReceiptElement> =
        transactions_data.iter().map(ReceiptElement::from).collect();
    let receipt_commitment = calculate_receipt_commitment::<Poseidon>(&receipt_elements);
    let state_diff_commitment = calculate_state_diff_hash(state_diff);
    let concatenated_counts = concat_counts(
        transactions_data.len(),
        event_leaf_elements.len(),
        state_diff.len(),
        l1_da_mode,
    );
    BlockHeaderCommitments {
        transaction_commitment,
        event_commitment,
        receipt_commitment,
        state_diff_commitment,
        concatenated_counts,
    }
}

// A single felt: [
//     transaction_count (64 bits) | event_count (64 bits) | state_diff_length (64 bits)
//     | L1 data availability mode: 0 for calldata, 1 for blob (1 bit) | 0 ...
// ].
fn concat_counts(
    transaction_count: usize,
    event_count: usize,
    state_diff_length: usize,
    l1_data_availability_mode: L1DataAvailabilityMode,
) -> Felt {
    let l1_data_availability_byte: u8 = match l1_data_availability_mode {
        L1DataAvailabilityMode::Calldata => 0,
        L1DataAvailabilityMode::Blob => 0b10000000,
    };
    let concat_bytes = [
        to_64_bits(transaction_count).as_slice(),
        to_64_bits(event_count).as_slice(),
        to_64_bits(state_diff_length).as_slice(),
        &[l1_data_availability_byte],
        &[0_u8; 7], // zero padding
    ]
    .concat();
    Felt::from_bytes_be_slice(concat_bytes.as_slice())
}

fn to_64_bits(num: usize) -> [u8; 8] {
    let sized_transaction_count: u64 = num.try_into().expect("Expect usize is at most 8 bytes");
    sized_transaction_count.to_be_bytes()
}
