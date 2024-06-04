use starknet_types_core::felt::Felt;

use super::concat_counts;
use crate::block::{
    BlockHash,
    BlockHeaderWithoutHash,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    GasPricePerToken,
    StarknetVersion,
};
use crate::block_hash::block_hash_calculator::{
    calculate_block_commitments,
    calculate_block_hash,
    TransactionHashingData,
};
use crate::block_hash::test_utils::{get_state_diff, get_transaction_output};
use crate::core::{ContractAddress, GlobalRoot, PatriciaKey, SequencerContractAddress};
use crate::data_availability::L1DataAvailabilityMode;
use crate::felt;
use crate::hash::{FeltConverter, TryIntoFelt};
use crate::transaction::{TransactionHash, TransactionSignature};

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
