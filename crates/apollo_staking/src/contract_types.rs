use apollo_cairo_utils::{CairoArray, RetdataDeserializationError, TryFromIterator};
use blockifier::execution::call_info::Retdata;
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::staking::StakingWeight;
use starknet_types_core::felt::Felt;

use crate::committee_provider::Staker;
use crate::staking_manager::Epoch;

#[cfg(test)]
#[path = "contract_types_test.rs"]
mod contract_types_test;

pub(crate) const GET_STAKERS_ENTRY_POINT: &str = "get_stakers";
pub(crate) const GET_CURRENT_EPOCH_DATA_ENTRY_POINT: &str = "get_current_epoch_data";
pub(crate) const GET_PREVIOUS_EPOCH_DATA_ENTRY_POINT: &str = "get_previous_epoch_data";
#[allow(dead_code)]
pub(crate) const EPOCH_LENGTH: u64 = 100; // Number of heights in an epoch.

// Represents a Cairo1 `Array` containing elements that can be deserialized to `T`.
// `T` must implement `TryFrom<[Felt; N]>`, where `N` is the size of `T`'s Cairo equivalent.
#[derive(Debug, PartialEq, Eq)]
// TODO(Dafna): Remove this when we have a CairoStakingContract implementation.
#[allow(dead_code)]
struct ArrayRetdata<T>(Vec<T>);

#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct ContractStaker {
    pub(crate) contract_address: ContractAddress,
    pub(crate) staking_power: StakingWeight,
    pub(crate) public_key: Option<Felt>,
}

fn felt_to_u64(felt: Felt) -> Result<u64, RetdataDeserializationError> {
    u64::try_from(felt).map_err(|_| RetdataDeserializationError::U64ConversionError { felt })
}

impl TryFromIterator<Felt> for ContractStaker {
    type Error = RetdataDeserializationError;

    // Parses a single `ContractStaker` from a stream of Felts.
    //
    // The iterator is expected to yield the following values, in order:
    // 1. Contract Address (1 Felt)
    // 2. Staking Power (1 Felt)
    // 3. Public Key option variant (1 Felt):
    //    - 0 => Some
    //    - 1 => None
    // 4. Public Key (1 Felt), only if the option variant is `Some`
    fn try_from_iter<T: Iterator<Item = Felt>>(iter: &mut T) -> Result<Self, Self::Error> {
        // Parse contract address.
        let raw_address = Felt::try_from_iter(iter)?;
        let contract_address = ContractAddress::try_from(raw_address).map_err(|_| {
            RetdataDeserializationError::ContractAddressConversionError { address: raw_address }
        })?;

        // Parse staking power.
        let raw_staking_power = Felt::try_from_iter(iter)?;
        let staking_power = StakingWeight(u128::try_from(raw_staking_power).map_err(|_| {
            RetdataDeserializationError::U128ConversionError { felt: raw_staking_power }
        })?);

        // Parse public key.
        let public_key = Option::<Felt>::try_from_iter(iter)?;

        Ok(Self { contract_address, staking_power, public_key })
    }
}

impl TryFromIterator<Felt> for Epoch {
    type Error = RetdataDeserializationError;

    // Parses a single `Epoch` from a stream of Felts.
    //
    // The iterator is expected to yield the following values, in order:
    // 1. Epoch ID (1 Felt)
    // 2. Start Block (1 Felt)
    // 3. Epoch Length (1 Felt)
    fn try_from_iter<T: Iterator<Item = Felt>>(iter: &mut T) -> Result<Self, Self::Error> {
        let epoch_id = felt_to_u64(Felt::try_from_iter(iter)?)?;
        let start_block = BlockNumber(felt_to_u64(Felt::try_from_iter(iter)?)?);
        let epoch_length = felt_to_u64(Felt::try_from_iter(iter)?)?;
        Ok(Epoch { epoch_id, start_block, epoch_length })
    }
}

impl ContractStaker {
    pub fn from_retdata_many(retdata: Retdata) -> Result<Vec<Self>, RetdataDeserializationError> {
        Ok(CairoArray::try_from(retdata)?.0)
    }
}

impl TryFrom<Retdata> for Epoch {
    type Error = RetdataDeserializationError;

    fn try_from(retdata: Retdata) -> Result<Self, Self::Error> {
        let mut iter = retdata.0.into_iter();
        let epoch = Epoch::try_from_iter(&mut iter)?;
        if iter.next().is_some() {
            return Err(RetdataDeserializationError::InvalidObjectLength {
                message: "unconsumed elements in Epoch retdata.".to_string(),
            });
        }
        Ok(epoch)
    }
}

impl From<&ContractStaker> for Staker {
    /// # Panics
    ///
    /// Panics if `public_key` is `None`.
    fn from(contract_staker: &ContractStaker) -> Self {
        Self {
            address: contract_staker.contract_address,
            weight: contract_staker.staking_power,
            public_key: contract_staker.public_key.expect("public key is required."),
        }
    }
}

#[cfg(test)]
impl From<&ContractStaker> for Vec<Felt> {
    fn from(staker: &ContractStaker) -> Self {
        let public_key = match staker.public_key {
            Some(public_key) => vec![Felt::ZERO, public_key],
            None => vec![Felt::ONE],
        };
        [
            [Felt::from(staker.contract_address), Felt::from(staker.staking_power.0)].as_slice(),
            public_key.as_slice(),
        ]
        .concat()
    }
}

#[cfg(test)]
impl From<&Epoch> for Vec<Felt> {
    fn from(epoch: &Epoch) -> Self {
        vec![
            Felt::from(epoch.epoch_id),
            Felt::from(epoch.start_block.0),
            Felt::from(epoch.epoch_length),
        ]
    }
}

#[cfg(test)]
impl From<&Staker> for ContractStaker {
    fn from(staker: &Staker) -> Self {
        Self {
            contract_address: staker.address,
            staking_power: staker.weight,
            public_key: Some(staker.public_key),
        }
    }
}
