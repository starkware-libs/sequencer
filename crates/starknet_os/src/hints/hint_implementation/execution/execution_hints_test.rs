use rstest::rstest;

use super::utils::tx_name_as_felt;
use starknet_types_core::felt::Felt;


#[rstest]
#[case::invoke("INVOKE_FUNCTION", Felt::from_hex_unchecked("0x494e564f4b455f46554e4354494f4e"))]
#[case::l1_handler("L1_HANDLER", Felt::from_hex_unchecked("0x4c315f48414e444c4552"))]
#[case::deploy("DEPLOY_ACCOUNT", Felt::from_hex_unchecked("0x4445504c4f595f4143434f554e54"))]
#[case::declare("DECLARE", Felt::from_hex_unchecked("0x4445434c415245"))]
fn tx_type_as_hex_regression(#[case] tx_name: &'static str, #[case] expected_hex: Felt) {
    let tx_type = tx_name_as_felt(tx_name);
    assert_eq!(tx_type, expected_hex);
}
