use cairo_lang_starknet_classes::contract_class::ContractEntryPoint;
use num_bigint::BigUint;
use pretty_assertions::assert_eq;
use starknet_api::core::EntryPointSelector;
use starknet_types_core::felt::Felt;

use crate::execution::native::utils::{
    contract_entrypoint_to_entrypoint_selector,
    decode_felts_as_str,
    encode_str_as_felts,
};

#[test]
fn test_contract_entrypoint_to_entrypoint_selector() {
    const NUM: u128 = 123;

    let entrypoint = ContractEntryPoint { selector: BigUint::from(NUM), function_idx: 0 };
    let expected_entrypoint_selector = EntryPointSelector(Felt::from(NUM));
    let actual_entrypoint_selector = contract_entrypoint_to_entrypoint_selector(&entrypoint);

    assert_eq!(actual_entrypoint_selector, expected_entrypoint_selector);
}

#[test]
fn test_encode_decode_str() {
    const STR: &str = "normal utf8 string:";

    let encoded_felt_array = encode_str_as_felts(STR);

    let decoded_felt_array = decode_felts_as_str(encoded_felt_array.as_slice());

    assert_eq!(&decoded_felt_array, STR);
}

#[test]
fn test_decode_non_utf8_str() {
    let v1 = Felt::from_dec_str("1234").unwrap();
    let v2_msg = "i am utf8";
    let v2 = Felt::from_bytes_be_slice(v2_msg.as_bytes());
    let v3 = Felt::from_dec_str("13299428").unwrap();
    let felts = [v1, v2, v3];

    assert_eq!(decode_felts_as_str(&felts), format!("[{}, {} ({}), {}]", v1, v2_msg, v2, v3))
}
