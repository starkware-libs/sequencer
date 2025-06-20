use pretty_assertions::assert_eq;
use sha3::{Digest, Keccak256};
use starknet_types_core::felt::Felt;

use super::{get_transaction_hash, validate_transaction_hash, CONSTRUCTOR_ENTRY_POINT_SELECTOR};
use crate::test_utils::{read_json_file, TransactionTestData};
use crate::transaction::{Transaction, TransactionOptions};

#[test]
fn test_constructor_selector() {
    let mut keccak = Keccak256::default();
    keccak.update(b"constructor");
    let mut constructor_bytes: [u8; 32] = keccak.finalize().into();
    constructor_bytes[0] &= 0b00000011_u8; // Discard the six MSBs.
    let constructor_felt = Felt::from_bytes_be(&constructor_bytes);
    assert_eq!(constructor_felt, CONSTRUCTOR_ENTRY_POINT_SELECTOR);
}

#[test]
fn test_transaction_hash() {
    // The details were taken from Starknet Mainnet. You can find the transactions by hash in:
    // https://alpha-mainnet.starknet.io/feeder_gateway/get_transaction?transactionHash=<transaction_hash>
    let transactions_test_data_vec: Vec<TransactionTestData> =
        read_json_file("transaction_hash.json");

    for transaction_test_data in transactions_test_data_vec {
        assert!(
            validate_transaction_hash(
                &transaction_test_data.transaction,
                &transaction_test_data.block_number,
                &transaction_test_data.chain_id,
                transaction_test_data.transaction_hash,
                &TransactionOptions::default(),
            )
            .unwrap(),
            "expected transaction hash {}",
            transaction_test_data.transaction_hash
        );
        let actual_transaction_hash = get_transaction_hash(
            &transaction_test_data.transaction,
            &transaction_test_data.chain_id,
            &TransactionOptions::default(),
        )
        .unwrap();
        assert_eq!(
            actual_transaction_hash, transaction_test_data.transaction_hash,
            "expected_transaction_hash: {:?}",
            transaction_test_data.transaction_hash
        );
    }
}

#[test]
fn test_deprecated_transaction_hash() {
    // The details were taken from Starknet Mainnet. You can find the transactions by hash in:
    // https://alpha-mainnet.starknet.io/feeder_gateway/get_transaction?transactionHash=<transaction_hash>
    let transaction_test_data_vec: Vec<TransactionTestData> =
        read_json_file("deprecated_transaction_hash.json");

    for transaction_test_data in transaction_test_data_vec {
        assert!(
            validate_transaction_hash(
                &transaction_test_data.transaction,
                &transaction_test_data.block_number,
                &transaction_test_data.chain_id,
                transaction_test_data.transaction_hash,
                &TransactionOptions::default(),
            )
            .unwrap(),
            "expected_transaction_hash: {:?}",
            transaction_test_data.transaction_hash
        );
    }
}

#[test]
fn test_only_query_transaction_hash() {
    let transactions_test_data_vec: Vec<TransactionTestData> =
        read_json_file("transaction_hash.json");

    for transaction_test_data in transactions_test_data_vec {
        // L1Handler only-query transactions are not supported.
        if let Transaction::L1Handler(_) = transaction_test_data.transaction {
            continue;
        }

        let actual_transaction_hash = get_transaction_hash(
            &transaction_test_data.transaction,
            &transaction_test_data.chain_id,
            &TransactionOptions { only_query: true },
        )
        .unwrap();
        assert_eq!(
            actual_transaction_hash,
            transaction_test_data.only_query_transaction_hash.unwrap(),
        );
    }
}
