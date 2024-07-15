use starknet_types_core::felt::Felt;

use super::concat_counts;
use crate::block::{
    BlockHash, BlockHeaderWithoutHash, BlockNumber, BlockTimestamp, GasPrice, GasPricePerToken,
    StarknetVersion,
};
use crate::block_hash::block_hash_calculator::{
    calculate_block_commitments, calculate_block_hash, BlockHeaderCommitments,
    TransactionHashingData,
};
use crate::block_hash::test_utils::{get_state_diff, get_transaction_output};
use crate::core::{
    ContractAddress, EventCommitment, GlobalRoot, PatriciaKey, ReceiptCommitment,
    SequencerContractAddress, StateDiffCommitment, TransactionCommitment,
};
use crate::data_availability::L1DataAvailabilityMode;
use crate::felt;
use crate::hash::PoseidonHash;
use crate::transaction::{TransactionHash, TransactionSignature};

/// Macro to test if changing any field in the header or commitments
/// results a change in the block hash.
/// The macro clones the original header and commitments, modifies each specified field,
/// and asserts that the block hash changes as a result.
macro_rules! test_hash_changes {
    ($header:expr, $commitments:expr, header_fields => { $($header_field:ident),* }, commitments_fields => { $($commitments_field:ident),* }) => {
        {
            let original_hash = calculate_block_hash($header.clone(), $commitments.clone());

            $(
                // Test changing the field in the header.
                let mut modified_header = $header.clone();
                modified_header.$header_field = Default::default();
                let new_hash = calculate_block_hash(modified_header, $commitments.clone());
                assert_ne!(original_hash, new_hash, concat!("Hash should change when ", stringify!($header_field), " is modified"));
            )*

            $(
                // Test changing the field in the commitments.
                let mut modified_commitments = $commitments.clone();
                modified_commitments.$commitments_field = Default::default();
                let new_hash = calculate_block_hash($header.clone(), modified_commitments);
                assert_ne!(original_hash, new_hash, concat!("Hash should change when ", stringify!($commitments_field), " is modified"));
            )*
        }
    };
}

#[test]
fn test_block_hash_regression() {
    let block_header = BlockHeaderWithoutHash {
        block_number: BlockNumber(1_u64),
        state_root: GlobalRoot(Felt::from(2_u8)),
        sequencer: SequencerContractAddress(ContractAddress(PatriciaKey::from(3_u8))),
        timestamp: BlockTimestamp(4),
        l1_da_mode: L1DataAvailabilityMode::Blob,
        l1_gas_price: GasPricePerToken { price_in_fri: GasPrice(6), price_in_wei: GasPrice(7) },
        l1_data_gas_price: GasPricePerToken {
            price_in_fri: GasPrice(10),
            price_in_wei: GasPrice(9),
        },
        starknet_version: StarknetVersion("10".to_owned()),
        parent_hash: BlockHash(Felt::from(11_u8)),
    };
    let transactions_data = vec![TransactionHashingData {
        transaction_signature: Some(TransactionSignature(vec![Felt::TWO, Felt::THREE])),
        transaction_output: get_transaction_output(),
        transaction_hash: TransactionHash(Felt::ONE),
    }];

    let state_diff = get_state_diff();
    let block_commitments =
        calculate_block_commitments(&transactions_data, &state_diff, block_header.l1_da_mode);

    let expected_hash = felt!("0x061e4998d51a248f1d0288d7e17f6287757b0e5e6c5e1e58ddf740616e312134");

    assert_eq!(BlockHash(expected_hash), calculate_block_hash(block_header, block_commitments),);
}

#[test]
fn concat_counts_test() {
    let concated = concat_counts(4, 3, 2, L1DataAvailabilityMode::Blob);
    let expected_felt = felt!("0x0000000000000004000000000000000300000000000000028000000000000000");
    assert_eq!(concated, expected_felt)
}

/// Test that if one of the input to block hash changes, the hash changes.
#[test]
fn change_field_of_hash_input() {
    let header = BlockHeaderWithoutHash {
        parent_hash: BlockHash(Felt::ONE),
        block_number: BlockNumber(1),
        l1_gas_price: GasPricePerToken { price_in_fri: GasPrice(1), price_in_wei: GasPrice(1) },
        l1_data_gas_price: GasPricePerToken {
            price_in_fri: GasPrice(1),
            price_in_wei: GasPrice(1),
        },
        state_root: GlobalRoot(Felt::ONE),
        sequencer: SequencerContractAddress(ContractAddress::from(1_u128)),
        timestamp: BlockTimestamp(1),
        l1_da_mode: L1DataAvailabilityMode::Blob,
        starknet_version: StarknetVersion("0.1.0".to_string()),
    };

    let block_commitments = BlockHeaderCommitments {
        transaction_commitment: TransactionCommitment(Felt::ONE),
        event_commitment: EventCommitment(Felt::ONE),
        receipt_commitment: ReceiptCommitment(Felt::ONE),
        state_diff_commitment: StateDiffCommitment(PoseidonHash(Felt::ONE)),
        concatenated_counts: Felt::ONE,
    };

    // Test that changing any of the fields in the header or the commitments changes the hash.
    test_hash_changes!(
        header,
        block_commitments,
        header_fields => {
            parent_hash,
            block_number,
            l1_gas_price,
            l1_data_gas_price,
            state_root,
            sequencer,
            timestamp,
            starknet_version
        },
        commitments_fields => {
            transaction_commitment,
            event_commitment,
            receipt_commitment,
            state_diff_commitment,
            concatenated_counts
        }
    );
    // TODO(Aviv, 10/06/2024): add tests that changes the first hash input, and the const zero.
}
