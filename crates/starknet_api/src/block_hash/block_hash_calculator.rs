use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::Poseidon;

use super::event_commitment::{calculate_event_commitment, EventLeafElement};
use super::receipt_commitment::{calculate_receipt_commitment, ReceiptElement};
use super::state_diff_hash::calculate_state_diff_hash;
use super::transaction_commitment::{calculate_transaction_commitment, TransactionLeafElement};
use crate::block::{BlockHash, BlockHeaderWithoutHash, GasPricePerToken, StarknetVersion};
use crate::core::{
    ascii_as_felt,
    EventCommitment,
    ReceiptCommitment,
    StateDiffCommitment,
    TransactionCommitment,
};
use crate::crypto::utils::HashChain;
use crate::data_availability::L1DataAvailabilityMode;
use crate::execution_resources::GasVector;
use crate::state::ThinStateDiff;
use crate::transaction::fields::{Fee, TransactionSignature};
use crate::transaction::{Event, MessageToL1, TransactionExecutionStatus, TransactionHash};
use crate::{StarknetApiError, StarknetApiResult};

#[cfg(test)]
#[path = "block_hash_calculator_test.rs"]
mod block_hash_calculator_test;

static STARKNET_BLOCK_HASH0: LazyLock<Felt> = LazyLock::new(|| {
    ascii_as_felt("STARKNET_BLOCK_HASH0").expect("ascii_as_felt failed for 'STARKNET_BLOCK_HASH0'")
});
static STARKNET_BLOCK_HASH1: LazyLock<Felt> = LazyLock::new(|| {
    ascii_as_felt("STARKNET_BLOCK_HASH1").expect("ascii_as_felt failed for 'STARKNET_BLOCK_HASH1'")
});
static STARKNET_GAS_PRICES0: LazyLock<Felt> = LazyLock::new(|| {
    ascii_as_felt("STARKNET_GAS_PRICES0").expect("ascii_as_felt failed for 'STARKNET_GAS_PRICES0'")
});

#[allow(non_camel_case_types)]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd)]
pub enum BlockHashVersion {
    V0_13_2,
    V0_13_4,
}

impl From<BlockHashVersion> for StarknetVersion {
    fn from(value: BlockHashVersion) -> Self {
        match value {
            BlockHashVersion::V0_13_2 => StarknetVersion::V0_13_2,
            BlockHashVersion::V0_13_4 => StarknetVersion::V0_13_4,
        }
    }
}

impl TryFrom<StarknetVersion> for BlockHashVersion {
    type Error = StarknetApiError;

    fn try_from(value: StarknetVersion) -> StarknetApiResult<Self> {
        if value < Self::V0_13_2.into() {
            Err(StarknetApiError::BlockHashVersion { version: value.to_string() })
        } else if value < Self::V0_13_4.into() {
            // Starknet versions 0.13.2 and 0.13.3 both have the same block hash mechanism.
            Ok(Self::V0_13_2)
        } else {
            Ok(Self::V0_13_4)
        }
    }
}

// The prefix constant for the block hash calculation.
type BlockHashConstant = Felt;

impl From<BlockHashVersion> for BlockHashConstant {
    fn from(block_hash_version: BlockHashVersion) -> Self {
        match block_hash_version {
            BlockHashVersion::V0_13_2 => *STARKNET_BLOCK_HASH0,
            BlockHashVersion::V0_13_4 => *STARKNET_BLOCK_HASH1,
        }
    }
}

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
    pub transaction_signature: TransactionSignature,
    pub transaction_output: TransactionOutputForHash,
    pub transaction_hash: TransactionHash,
}

/// Commitments of a block.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockHeaderCommitments {
    pub transaction_commitment: TransactionCommitment,
    pub event_commitment: EventCommitment,
    pub receipt_commitment: ReceiptCommitment,
    pub state_diff_commitment: StateDiffCommitment,
    pub concatenated_counts: Felt,
}

/// Poseidon (
///     block_hash_constant, block_number, global_state_root, sequencer_address,
///     block_timestamp, concat_counts, state_diff_hash, transaction_commitment,
///     event_commitment, receipt_commitment, gas_prices, starknet_version, 0, parent_block_hash
/// ).
pub fn calculate_block_hash(
    header: BlockHeaderWithoutHash,
    block_commitments: BlockHeaderCommitments,
) -> StarknetApiResult<BlockHash> {
    // Replace match with a map_none. is Ok?
    let starknet_version = match header.starknet_version {
        Some(starknet_version) => starknet_version,
        None => {
            tracing::debug!(
                "Calculating block hash for an unsupported Starknet version. Probably For \
                 Starknet version before 0_9_1."
            );
            return Err(StarknetApiError::InvalidStarknetVersion(vec![]));
        }
    };
    let block_hash_version: BlockHashVersion = starknet_version.try_into()?;
    Ok(BlockHash(
        HashChain::new()
            .chain(&block_hash_version.clone().into())
            .chain(&header.block_number.0.into())
            .chain(&header.state_root.0)
            .chain(&header.sequencer.0)
            .chain(&header.timestamp.0.into())
            .chain(&block_commitments.concatenated_counts)
            .chain(&block_commitments.state_diff_commitment.0.0)
            .chain(&block_commitments.transaction_commitment.0)
            .chain(&block_commitments.event_commitment.0)
            .chain(&block_commitments.receipt_commitment.0)
            .chain_iter(
                gas_prices_to_hash(
                    &header.l1_gas_price,
                    &header.l1_data_gas_price,
                    &header.l2_gas_price,
                    &block_hash_version,
                )
                .iter(),
            )
            .chain(&ascii_as_felt(&starknet_version.to_string()).expect("Expect ASCII version"))
            .chain(&Felt::ZERO)
            .chain(&header.parent_hash.0)
            .get_poseidon_hash(),
    ))
}

/// Calculates the commitments of the transactions data for the block hash.
pub fn calculate_block_commitments(
    transactions_data: &[TransactionHashingData],
    state_diff: &ThinStateDiff,
    l1_da_mode: L1DataAvailabilityMode,
    starknet_version: &StarknetVersion,
) -> BlockHeaderCommitments {
    let transaction_leaf_elements: Vec<TransactionLeafElement> = transactions_data
        .iter()
        .map(|tx_leaf| {
            let mut tx_leaf_element = TransactionLeafElement::from(tx_leaf);
            if starknet_version < &BlockHashVersion::V0_13_4.into()
                && tx_leaf.transaction_signature.0.is_empty()
            {
                tx_leaf_element.transaction_signature =
                    TransactionSignature(vec![Felt::ZERO].into());
            }
            tx_leaf_element
        })
        .collect();
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

// For starknet version >= 0.13.3, returns:
// [Poseidon (
//     "STARKNET_GAS_PRICES0", gas_price_wei, gas_price_fri, data_gas_price_wei, data_gas_price_fri,
//     l2_gas_price_wei, l2_gas_price_fri
// )].
// Otherwise, returns:
// [gas_price_wei, gas_price_fri, data_gas_price_wei, data_gas_price_fri].
// TODO(Ayelet): add l2_gas_consumed, next_l2_gas_price after 0.14.0.
fn gas_prices_to_hash(
    l1_gas_price: &GasPricePerToken,
    l1_data_gas_price: &GasPricePerToken,
    l2_gas_price: &GasPricePerToken,
    block_hash_version: &BlockHashVersion,
) -> Vec<Felt> {
    if block_hash_version >= &BlockHashVersion::V0_13_4 {
        vec![
            HashChain::new()
                .chain(&STARKNET_GAS_PRICES0)
                .chain(&l1_gas_price.price_in_wei.0.into())
                .chain(&l1_gas_price.price_in_fri.0.into())
                .chain(&l1_data_gas_price.price_in_wei.0.into())
                .chain(&l1_data_gas_price.price_in_fri.0.into())
                .chain(&l2_gas_price.price_in_wei.0.into())
                .chain(&l2_gas_price.price_in_fri.0.into())
                .get_poseidon_hash(),
        ]
    } else {
        vec![
            l1_gas_price.price_in_wei.0.into(),
            l1_gas_price.price_in_fri.0.into(),
            l1_data_gas_price.price_in_wei.0.into(),
            l1_data_gas_price.price_in_fri.0.into(),
        ]
    }
}
