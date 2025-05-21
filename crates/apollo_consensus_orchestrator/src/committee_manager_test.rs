use std::convert::TryFrom;

use assert_matches::assert_matches;
use blockifier::execution::call_info::Retdata;
use rstest::rstest;
use starknet_api::contract_address;
use starknet_api::core::CONTRACT_ADDRESS_DOMAIN_SIZE;
use starknet_api::staking::StakingWeight;
use starknet_types_core::felt::Felt;

use crate::committee_manager::{RetdataDeserializationError, Staker};

// --- TryFrom tests for Staker and ArrayRetdata ---

#[rstest]
fn staker_try_from_valid() {
    let staker = Staker::try_from([Felt::ONE, Felt::ONE, Felt::ONE]).unwrap();
    assert_eq!(staker.address, contract_address!("0x1"));
    assert_eq!(staker.weight, StakingWeight(1));
    assert_eq!(staker.public_key, Felt::ONE);
}

#[rstest]
fn staker_try_from_invalid_address() {
    let err = Staker::try_from([CONTRACT_ADDRESS_DOMAIN_SIZE, Felt::ONE, Felt::ONE]).unwrap_err();
    assert_matches!(err, RetdataDeserializationError::ContractAddressConversionError { .. });
}

#[rstest]
fn staker_try_from_invalid_staked_amount() {
    let err = Staker::try_from([Felt::ONE, Felt::MAX, Felt::ONE]).unwrap_err(); // Felt::MAX is too big for u128
    assert_matches!(err, RetdataDeserializationError::U128ConversionError { .. });
}

#[rstest]
fn staker_array_retdata_try_from_valid() {
    const NUM_ELEMENTS: usize = 2;
    let valid_retdata = [
        [Felt::from(NUM_ELEMENTS)].as_slice(),
        [Felt::ONE; Staker::CAIRO_OBJECT_LENGTH * NUM_ELEMENTS].as_slice(),
    ]
    .concat();

    let result = Staker::from_retdata_many(Retdata(valid_retdata)).unwrap();
    assert_eq!(result.len(), NUM_ELEMENTS);
}

#[rstest]
#[case::empty_retdata(vec![])]
#[case::missing_num_elements(vec![Felt::ONE; Staker::CAIRO_OBJECT_LENGTH * 2])]
#[case::invalid_staker_length(vec![Felt::ONE; Staker::CAIRO_OBJECT_LENGTH - 1])]
fn staker_array_retdata_try_from_invalid_length(#[case] retdata: Vec<Felt>) {
    let err = Staker::from_retdata_many(Retdata(retdata)).unwrap_err();
    assert_matches!(err, RetdataDeserializationError::InvalidRetdataLength { .. });
}
