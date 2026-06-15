use std::sync::Arc;

use rstest::rstest;
use starknet_types_core::felt::Felt;

use crate::core::Nonce;
use crate::executable_transaction::{L1HandlerTransaction, TransactionType};
use crate::transaction::fields::{Calldata, Fee};
use crate::transaction::{
    L1HandlerTransaction as RpcL1HandlerTransaction,
    TransactionHash,
    TransactionVersion,
};

#[rstest]
#[case::invoke(TransactionType::InvokeFunction, "0x494e564f4b455f46554e4354494f4e")]
#[case::l1_handler(TransactionType::L1Handler, "0x4c315f48414e444c4552")]
#[case::deploy(TransactionType::DeployAccount, "0x4445504c4f595f4143434f554e54")]
#[case::declare(TransactionType::Declare, "0x4445434c415245")]
fn tx_type_as_hex_regression(#[case] tx_type: TransactionType, #[case] expected_hex: &str) {
    assert_eq!(tx_type.tx_type_as_felt(), Felt::from_hex_unchecked(expected_hex));
}

/// `Calldata` has no non-empty invariant and `L1HandlerTransaction` derives `Deserialize`, so an
/// empty calldata is constructible. `payload_size` must not underflow (debug panic / release wrap
/// to usize::MAX); the payload of an empty calldata is just 0.
#[test]
fn l1_handler_payload_size_empty_calldata_does_not_underflow() {
    let tx = RpcL1HandlerTransaction {
        version: TransactionVersion::ZERO,
        nonce: Nonce::default(),
        contract_address: Default::default(),
        entry_point_selector: Default::default(),
        calldata: Calldata(Arc::new(vec![])),
    };
    let executable_tx =
        L1HandlerTransaction { tx, tx_hash: TransactionHash::default(), paid_fee_on_l1: Fee(0) };
    assert_eq!(executable_tx.payload_size(), 0);
}
