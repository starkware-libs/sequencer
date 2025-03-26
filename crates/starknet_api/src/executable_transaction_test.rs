use rstest::rstest;
use starknet_types_core::felt::Felt;

use crate::executable_transaction::TransactionType;

#[rstest]
#[case::invoke(
    TransactionType::InvokeFunction,
    Felt::from_hex_unchecked("0x494e564f4b455f46554e4354494f4e")
)]
#[case::l1_handler(TransactionType::L1Handler, Felt::from_hex_unchecked("0x4c315f48414e444c4552"))]
#[case::deploy(
    TransactionType::DeployAccount,
    Felt::from_hex_unchecked("0x4445504c4f595f4143434f554e54")
)]
#[case::declare(TransactionType::Declare, Felt::from_hex_unchecked("0x4445434c415245"))]
fn tx_type_as_hex_regression(#[case] tx_type: TransactionType, #[case] expected_hex: Felt) {
    assert_eq!(tx_type.tx_name_as_felt(), expected_hex);
}
