use cairo_lang_starknet_classes::contract_class::ContractEntryPoint;
use num_bigint::BigUint;
use pretty_assertions::assert_eq;
use starknet_api::core::EntryPointSelector;
use starknet_types_core::felt::Felt;

use crate::execution::execution_utils::format_panic_data;
use crate::execution::native::utils::{
    contract_entrypoint_to_entrypoint_selector,
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
fn test_encode_small_str() {
    const STR: &str = "I fit in a felt :)";

    let encoded_felt_array = encode_str_as_felts(STR);

    let decoded_felt_array = format_panic_data(&encoded_felt_array);

    assert_eq!(
        &decoded_felt_array,
        "0x492066697420696e20612066656c74203a2900000000000000000000000000 ('I fit in a felt :)')"
    );
}

#[test]
fn test_encode_large_str() {
    const STR: &str =
        "Three sad tigers ate wheat. Two tigers were full. The other tiger not so much";

    let encoded_felt_array = encode_str_as_felts(STR);

    let decoded_felt_array = format_panic_data(&encoded_felt_array);

    assert_eq!(
        &decoded_felt_array,
        "(0x54687265652073616420746967657273206174652077686561742e2054776f ('Three sad tigers ate \
         wheat. Two'), 0x2074696765727320776572652066756c6c2e20546865206f74686572207469 (' tigers \
         were full. The other ti'), \
         0x676572206e6f7420736f206d75636800000000000000000000000000000000 ('ger not so much'))"
    );
}
