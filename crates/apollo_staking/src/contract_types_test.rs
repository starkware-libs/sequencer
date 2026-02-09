use assert_matches::assert_matches;
use blockifier::execution::call_info::Retdata;
use rstest::rstest;
use starknet_api::block::BlockNumber;
use starknet_api::contract_address;
use starknet_api::core::{CONTRACT_ADDRESS_DOMAIN_SIZE, ContractAddress, PatriciaKey};
use starknet_api::staking::StakingWeight;
use starknet_types_core::felt::Felt;

use crate::contract_types::{ContractStaker, RetdataDeserializationError, TryFromIterator};
use crate::staking_manager::Epoch;

const STAKER_1: ContractStaker = ContractStaker {
    contract_address: ContractAddress(PatriciaKey::from_hex_unchecked("0x1")),
    staking_power: StakingWeight(1000),
    public_key: Some(Felt::ONE),
};
const STAKER_2: ContractStaker = ContractStaker {
    contract_address: ContractAddress(PatriciaKey::from_hex_unchecked("0x2")),
    staking_power: StakingWeight(2000),
    public_key: Some(Felt::TWO),
};
const STAKER_3: ContractStaker = ContractStaker {
    contract_address: ContractAddress(PatriciaKey::from_hex_unchecked("0x3")),
    staking_power: StakingWeight(3000),
    public_key: Some(Felt::THREE),
};
const STAKER_4: ContractStaker = ContractStaker {
    contract_address: ContractAddress(PatriciaKey::from_hex_unchecked("0x4")),
    staking_power: StakingWeight(4000),
    public_key: None,
};

#[rstest]
fn staker_try_from_valid() {
    let staker = ContractStaker::try_from_iter(
        &mut vec![Felt::ONE, Felt::TWO, Felt::ZERO, Felt::THREE].into_iter(),
    )
    .unwrap();
    assert_eq!(staker.contract_address, contract_address!("0x1"));
    assert_eq!(staker.staking_power, StakingWeight(2));
    assert_eq!(staker.public_key, Some(Felt::THREE));

    // A valid staker with no public key.
    let staker =
        ContractStaker::try_from_iter(&mut vec![Felt::ONE, Felt::TWO, Felt::ONE].into_iter())
            .unwrap();
    assert_eq!(staker.contract_address, contract_address!("0x1"));
    assert_eq!(staker.staking_power, StakingWeight(2));
    assert_eq!(staker.public_key, None);
}

#[rstest]
fn staker_try_from_invalid_address() {
    let err = ContractStaker::try_from_iter(
        &mut vec![CONTRACT_ADDRESS_DOMAIN_SIZE, Felt::ONE, Felt::ZERO, Felt::ONE].into_iter(),
    )
    .unwrap_err();
    assert_matches!(err, RetdataDeserializationError::ContractAddressConversionError { .. });
}

#[rstest]
fn staker_try_from_invalid_public_key() {
    let err = ContractStaker::try_from_iter(
        &mut vec![Felt::ONE, Felt::TWO, Felt::TWO, Felt::THREE].into_iter(),
    )
    .unwrap_err();
    assert_matches!(err, RetdataDeserializationError::UnexpectedEnumVariant { .. });
}

#[rstest]
fn staker_try_from_invalid_staked_amount() {
    let err = ContractStaker::try_from_iter(
        &mut vec![Felt::ONE, Felt::MAX, Felt::ZERO, Felt::ONE].into_iter(),
    )
    .unwrap_err(); // Felt::MAX is too big for u128
    assert_matches!(err, RetdataDeserializationError::U128ConversionError { .. });
}

#[rstest]
fn staker_array_retdata_try_from_valid() {
    // Case 1: No stakers.
    let retdata = Retdata(vec![Felt::ZERO]);
    assert!(ContractStaker::from_retdata_many(retdata).unwrap().is_empty());

    // Case 2: 4 Stakers, 1 with no public key.
    let expected_stakers: Vec<ContractStaker> = vec![STAKER_1, STAKER_2, STAKER_3, STAKER_4];
    let raw_felts = [
        [Felt::from(4)].as_slice(),
        expected_stakers.iter().map(Vec::<Felt>::from).collect::<Vec<_>>().concat().as_slice(),
    ]
    .concat();

    // A sanity check that the raw felts are constructed correctly.
    // 1 felt for the number of stakers (4) + 3 stakers with public key (4 felts) + 1 staker with no
    // public key (3 felts).
    assert_eq!(raw_felts.len(), 1 + 3 * 4 + 3);

    let result = ContractStaker::from_retdata_many(Retdata(raw_felts)).unwrap();
    assert_eq!(result, expected_stakers);
}

#[rstest]
#[case::empty_retdata(vec![])]
#[case::invalid_length_1(vec![Felt::ONE; 3])]
#[case::invalid_length_2(vec![Felt::ONE; 10])]
fn staker_array_retdata_try_from_invalid_length(#[case] raw_felts: Vec<Felt>) {
    let err = ContractStaker::from_retdata_many(Retdata(raw_felts)).unwrap_err();
    assert_matches!(err, RetdataDeserializationError::InvalidObjectLength { .. });
}

#[rstest]
fn epoch_try_from_valid() {
    let epoch = Epoch::try_from(Retdata(vec![Felt::ONE, Felt::TWO, Felt::THREE])).unwrap();
    assert_eq!(epoch, Epoch { epoch_id: 1, start_block: BlockNumber(2), epoch_length: 3 });
}

#[rstest]
fn epoch_try_from_invalid_length() {
    let err = Epoch::try_from(Retdata(vec![Felt::ONE, Felt::TWO])).unwrap_err();
    assert_matches!(err, RetdataDeserializationError::InvalidObjectLength { .. });

    let err =
        Epoch::try_from(Retdata(vec![Felt::ONE, Felt::TWO, Felt::THREE, Felt::ONE])).unwrap_err();
    assert_matches!(err, RetdataDeserializationError::InvalidObjectLength { .. });
}

#[rstest]
#[case::invalid_epoch_id(vec![Felt::MAX, Felt::TWO, Felt::THREE])]
#[case::invalid_start_block(vec![Felt::ONE, Felt::MAX, Felt::THREE])]
#[case::invalid_epoch_length(vec![Felt::ONE, Felt::TWO, Felt::MAX])]
fn epoch_try_from_conversion_errors(#[case] raw_felts: Vec<Felt>) {
    let err = Epoch::try_from(Retdata(raw_felts)).unwrap_err();
    assert_matches!(err, RetdataDeserializationError::U64ConversionError { .. });
}
