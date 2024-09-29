use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::concat_counts;
use crate::block::{
    BlockHash,
    BlockHeaderWithoutHash,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    GasPricePerToken,
};
use crate::block_hash::block_hash_calculator::{
    calculate_block_commitments,
    calculate_block_hash,
    BlockHashVersion,
    BlockHeaderCommitments,
    TransactionHashingData,
};
use crate::block_hash::test_utils::{get_state_diff, get_transaction_output};
use crate::core::{
    ContractAddress,
    EventCommitment,
    GlobalRoot,
    PatriciaKey,
    ReceiptCommitment,
    SequencerContractAddress,
    StateDiffCommitment,
    TransactionCommitment,
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
    (
        BlockHeaderWithoutHash { $($header_field:ident: $header_value:expr),* },
        BlockHeaderCommitments { $($commitments_field:ident: $commitments_value:expr),* }
    ) => {
        {
            let header = BlockHeaderWithoutHash {
                l1_da_mode: L1DataAvailabilityMode::Blob,
                $($header_field: $header_value),*
            };
            let commitments = BlockHeaderCommitments {
                $($commitments_field: $commitments_value),*
            };
            let original_hash = calculate_block_hash(header.clone(), commitments.clone());

            $(
                // Test changing the field in the header.
                let mut modified_header = header.clone();
                modified_header.$header_field = Default::default();
                let new_hash = calculate_block_hash(modified_header, commitments.clone());
                assert_ne!(original_hash, new_hash, concat!("Hash should change when ", stringify!($header_field), " is modified"));
            )*

            $(
                // Test changing the field in the commitments.
                let mut modified_commitments = commitments.clone();
                modified_commitments.$commitments_field = Default::default();
                let new_hash = calculate_block_hash(header.clone(), modified_commitments);
                assert_ne!(original_hash, new_hash, concat!("Hash should change when ", stringify!($commitments_field), " is modified"));
            )*
        }
    };
}

#[rstest]
fn test_block_hash_regression(
    #[values(BlockHashVersion::VO_13_2, BlockHashVersion::VO_13_3)]
    block_hash_version: BlockHashVersion,
) {
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
        l2_gas_price: GasPricePerToken { price_in_fri: GasPrice(11), price_in_wei: GasPrice(12) },
        starknet_version: block_hash_version.to_owned().into(),
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

    let expected_hash = match block_hash_version {
        BlockHashVersion::VO_13_2 => {
            felt!("0xe248d6ce583f8fa48d1d401d4beb9ceced3733e38d8eacb0d8d3669a7d901c")
        }
        BlockHashVersion::VO_13_3 => {
            felt!("0x65b653f5bc0939cdc39f98230affc8fbd1a01ea801e025271a4cfba912ba59a")
        }
    };

    assert_eq!(BlockHash(expected_hash), calculate_block_hash(block_header, block_commitments),);
}

#[test]
fn l2_gas_price_pre_v0_13_3() {
    let block_header = {
        |l2_gas_price| BlockHeaderWithoutHash {
            l2_gas_price: GasPricePerToken {
                price_in_fri: GasPrice(l2_gas_price),
                price_in_wei: GasPrice(l2_gas_price),
            },
            starknet_version: BlockHashVersion::VO_13_2.into(),
            ..Default::default()
        }
    };

    assert_eq!(
        calculate_block_hash(block_header(1), BlockHeaderCommitments::default()),
        calculate_block_hash(block_header(2), BlockHeaderCommitments::default())
    );
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
    // Set non-default values for the header and the commitments fields. Test that changing any of
    // these fields changes the hash.
    test_hash_changes!(
        BlockHeaderWithoutHash {
            parent_hash: BlockHash(Felt::ONE),
            block_number: BlockNumber(1),
            l1_gas_price: GasPricePerToken { price_in_fri: GasPrice(1), price_in_wei: GasPrice(1) },
            l1_data_gas_price: GasPricePerToken {
                price_in_fri: GasPrice(1),
                price_in_wei: GasPrice(1),
            },
            l2_gas_price: GasPricePerToken { price_in_fri: GasPrice(1), price_in_wei: GasPrice(1) },
            state_root: GlobalRoot(Felt::ONE),
            sequencer: SequencerContractAddress(ContractAddress::from(1_u128)),
            timestamp: BlockTimestamp(1),
            starknet_version: BlockHashVersion::VO_13_3.into()
        },
        BlockHeaderCommitments {
            transaction_commitment: TransactionCommitment(Felt::ONE),
            event_commitment: EventCommitment(Felt::ONE),
            receipt_commitment: ReceiptCommitment(Felt::ONE),
            state_diff_commitment: StateDiffCommitment(PoseidonHash(Felt::ONE)),
            concatenated_counts: Felt::ONE
        }
    );
    // TODO(Aviv, 10/06/2024): add tests that changes the first hash input, and the const zero.
}
