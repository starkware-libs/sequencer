use std::sync::Arc;

use rstest::rstest;
use starknet_api::core::{is_pedersen_reachable_address, ContractAddress};
use starknet_api::test_utils::deploy_account::deploy_account_tx;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt};
use starknet_api::transaction::{
    Transaction as StarknetApiTransaction,
    TransactionHash,
    TransactionVersion,
};
use starknet_api::{class_hash, deploy_account_tx_args, nonce};
use starknet_types_core::felt::Felt;

use super::Transaction;
use crate::transaction::account_transaction::ExecutionFlags as AccountExecutionFlags;

/// Without an externally supplied address, `from_api` must derive it with the scheme the
/// transaction version implies -- Blake2 for v4, not the Pedersen default.
#[rstest]
fn test_from_api_derives_v4_address_with_blake2() {
    // Frozen vector: deployer = 0, salt = 771, class_hash = 0x4242,
    // constructor_calldata = [42, 2^63, 1337], escaping after one increment.
    let tx = deploy_account_tx(
        deploy_account_tx_args! {
            version: TransactionVersion::FOUR,
            class_hash: class_hash!("0x4242"),
            contract_address_salt: ContractAddressSalt(Felt::from(771_u16)),
            constructor_calldata: Calldata(Arc::new(vec![
                Felt::from(42_u8),
                Felt::from(1_u64 << 63),
                Felt::from(1337_u16),
            ])),
        },
        nonce!(0_u8),
    );

    let tx = Transaction::from_api(
        StarknetApiTransaction::DeployAccount(tx),
        TransactionHash::default(),
        None,
        None,
        None,
        AccountExecutionFlags::default(),
    )
    .unwrap();

    let expected_address = ContractAddress::try_from(Felt::from_hex_unchecked(
        "0x566c3e328f3fd5a311267250cadc3c1c4de799db54180fcf862fe90b622571d",
    ))
    .unwrap();
    assert_eq!(tx.sender_address(), expected_address);
    assert!(!is_pedersen_reachable_address(expected_address.0.key()));
}
