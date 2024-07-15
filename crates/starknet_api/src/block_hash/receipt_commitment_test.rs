use starknet_types_core::felt::Felt;
use starknet_types_core::hash::Poseidon;

use super::calculate_messages_sent_hash;
use crate::block_hash::receipt_commitment::{
    calculate_receipt_commitment, calculate_receipt_hash, get_revert_reason_hash, ReceiptElement,
};
use crate::block_hash::test_utils::{generate_message_to_l1, get_transaction_output};
use crate::core::ReceiptCommitment;
use crate::felt;
use crate::transaction::{
    RevertedTransactionExecutionStatus, TransactionExecutionStatus, TransactionHash,
};

#[test]
fn test_receipt_hash_regression() {
    let transaction_receipt = ReceiptElement {
        transaction_hash: TransactionHash(Felt::from(1234_u16)),
        transaction_output: get_transaction_output(),
    };

    let expected_hash = felt!("0x6276abf21e7c68b2eecfdc8a845b11b44401901f5f040efe10c60d625049646");
    assert_eq!(calculate_receipt_hash(&transaction_receipt), expected_hash);

    let expected_root = ReceiptCommitment(felt!(
        "0x31963cb891ebb825e83514deb748c89b6967b5368cbc48a9b56193a1464ca87"
    ));
    assert_eq!(calculate_receipt_commitment::<Poseidon>(&[transaction_receipt]), expected_root);
}

#[test]
fn test_messages_sent_regression() {
    let messages_sent = vec![generate_message_to_l1(0), generate_message_to_l1(1)];
    let messages_hash = calculate_messages_sent_hash(&messages_sent);
    let expected_hash = felt!("0x00c89474a9007dc060aed76caf8b30b927cfea1ebce2d134b943b8d7121004e4");
    assert_eq!(messages_hash, expected_hash);
}

#[test]
fn test_revert_reason_hash_regression() {
    let execution_succeeded = TransactionExecutionStatus::Succeeded;
    assert_eq!(get_revert_reason_hash(&execution_succeeded), Felt::ZERO);
    let execution_reverted =
        TransactionExecutionStatus::Reverted(RevertedTransactionExecutionStatus {
            revert_reason: "ABC".to_string(),
        });
    let expected_hash = felt!("0x01629b9dda060bb30c7908346f6af189c16773fa148d3366701fbaa35d54f3c8");
    assert_eq!(get_revert_reason_hash(&execution_reverted), expected_hash);
}
