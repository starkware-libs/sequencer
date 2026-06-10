use std::sync::Arc;

use pretty_assertions::assert_eq;
use sha3::{Digest, Keccak256};
use starknet_types_core::felt::Felt;

use super::{
    concat_data_availability_mode,
    get_tip_resource_bounds_hash,
    get_transaction_hash,
    validate_transaction_hash,
    CONSTRUCTOR_ENTRY_POINT_SELECTOR,
    DEPLOY_ACCOUNT,
};
use crate::core::{ChainId, ClassHash, Nonce};
use crate::crypto::utils::HashChain;
use crate::data_availability::DataAvailabilityMode;
use crate::test_utils::{read_json_file, TransactionTestData};
use crate::transaction::fields::{
    Calldata,
    ContractAddressSalt,
    PaymasterData,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use crate::transaction::{
    CalculateContractAddress,
    DeployAccountTransaction,
    DeployAccountTransactionV3,
    Transaction,
    TransactionOptions,
    TransactionVersion,
};

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
            transaction_test_data.transaction_hash,
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
fn test_deploy_account_v3_hash_chains_nonce_before_da_mode() {
    // Regression test for the SNIP-8 field ordering of deploy_account V3 hashing: the common
    // fields must be chained as chain_id -> nonce -> data_availability_mode, matching invoke_v3,
    // declare_v3, and the Cairo OS `hash_tx_common_fields`. A non-L1 nonce DA mode makes the DA
    // felt non-zero, so swapping nonce and DA produces a different Poseidon hash — a hash
    // divergence between the Rust sequencer and the Cairo prover/consensus.
    let nonce = Nonce(Felt::from(7_u64));
    let resource_bounds = ValidResourceBounds::create_for_testing();
    let tip = Tip::default();
    let paymaster_data = PaymasterData(vec![Felt::from(9_u64)]);
    let constructor_calldata = Calldata(Arc::new(vec![Felt::from(1_u64), Felt::from(2_u64)]));
    let class_hash = ClassHash(Felt::from(0x111_u64));
    let contract_address_salt = ContractAddressSalt(Felt::from(0x222_u64));
    let nonce_data_availability_mode = DataAvailabilityMode::L2;
    let fee_data_availability_mode = DataAvailabilityMode::L1;

    let tx = DeployAccountTransactionV3 {
        resource_bounds,
        tip,
        signature: TransactionSignature::default(),
        nonce,
        class_hash,
        contract_address_salt,
        constructor_calldata: constructor_calldata.clone(),
        nonce_data_availability_mode,
        fee_data_availability_mode,
        paymaster_data: paymaster_data.clone(),
    };
    let chain_id = ChainId::Mainnet;

    let actual_hash = get_transaction_hash(
        &Transaction::DeployAccount(DeployAccountTransaction::V3(tx.clone())),
        &chain_id,
        &TransactionOptions::default(),
    )
    .unwrap();

    // Independently recompute the hash in the canonical SNIP-8 order (nonce before DA mode).
    let contract_address = tx.calculate_contract_address().unwrap();
    let tip_resource_bounds_hash = get_tip_resource_bounds_hash(&resource_bounds, &tip).unwrap();
    let paymaster_data_hash =
        HashChain::new().chain_iter(paymaster_data.0.iter()).get_poseidon_hash();
    let data_availability_mode =
        concat_data_availability_mode(&nonce_data_availability_mode, &fee_data_availability_mode);
    let constructor_calldata_hash =
        HashChain::new().chain_iter(constructor_calldata.0.iter()).get_poseidon_hash();
    let expected_hash = HashChain::new()
        .chain(&DEPLOY_ACCOUNT)
        .chain(&TransactionVersion::THREE.0)
        .chain(contract_address.0.key())
        .chain(&tip_resource_bounds_hash)
        .chain(&paymaster_data_hash)
        .chain(&Felt::try_from(&chain_id).unwrap())
        .chain(&nonce.0)
        .chain(&data_availability_mode)
        .chain(&constructor_calldata_hash)
        .chain(&class_hash.0)
        .chain(&contract_address_salt.0)
        .get_poseidon_hash();

    assert_eq!(actual_hash.0, expected_hash);
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
