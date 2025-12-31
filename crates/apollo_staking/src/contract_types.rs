use blockifier::execution::call_info::Retdata;
use starknet_api::core::ContractAddress;
use starknet_api::staking::StakingWeight;
use starknet_types_core::felt::Felt;
use thiserror::Error;

#[cfg(test)]
use crate::committee_provider::Staker;

pub(crate) const GET_STAKERS_ENTRY_POINT: &str = "get_stakers";
pub(crate) const EPOCH_LENGTH: u64 = 100; // Number of heights in an epoch.

/// Conversion from an [`Iterator`].
///
/// By implementing `TryFromIterator` for a type, you define how it will be
/// created from an iterator.
///
/// Used in this context to parse Cairo1 types returned by a contract as a vector of Felts.
pub trait TryFromIterator<Felt>: Sized {
    type Error;

    fn try_from_iter<T: Iterator<Item = Felt>>(iter: &mut T) -> Result<Self, Self::Error>;
}

// Represents a Cairo1 `Array` containing elements that can be deserialized to `T`.
// `T` must implement `TryFrom<[Felt; N]>`, where `N` is the size of `T`'s Cairo equivalent.
#[derive(Debug, PartialEq, Eq)]
struct ArrayRetdata<T>(Vec<T>);

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ContractStaker {
    pub(crate) contract_address: ContractAddress,
    pub(crate) staking_power: StakingWeight,
    pub(crate) public_key: Option<Felt>,
}

#[derive(Debug, Error)]
pub enum RetdataDeserializationError {
    #[error("Failed to convert Felt to ContractAddress: {address}")]
    ContractAddressConversionError { address: Felt },
    #[error("Failed to convert Felt to u128: {felt}")]
    U128ConversionError { felt: Felt },
    #[error("Failed to convert Felt to usize: {felt}")]
    USizeConversionError { felt: Felt },
    #[error("Invalid object length: {message}.")]
    InvalidObjectLength { message: String },
    #[error("Unexpected enum variant: {variant}")]
    UnexpectedEnumVariant { variant: usize },
}

impl ContractStaker {
    pub fn from_retdata_many(retdata: Retdata) -> Result<Vec<Self>, RetdataDeserializationError> {
        Ok(ArrayRetdata::try_from(retdata)?.0)
    }
}

impl TryFromIterator<Felt> for ContractStaker {
    type Error = RetdataDeserializationError;
    fn try_from_iter<T: Iterator<Item = Felt>>(iter: &mut T) -> Result<Self, Self::Error> {
        // Parse contract address.
        let raw_address = iter.next().ok_or(RetdataDeserializationError::InvalidObjectLength {
            message: "missing contract address.".to_string(),
        })?;
        let contract_address = ContractAddress::try_from(raw_address).map_err(|_| {
            RetdataDeserializationError::ContractAddressConversionError { address: raw_address }
        })?;

        // Parse staking power.
        let raw_staking_power =
            iter.next().ok_or(RetdataDeserializationError::InvalidObjectLength {
                message: "missing staking power.".to_string(),
            })?;
        let staking_power = StakingWeight(u128::try_from(raw_staking_power).map_err(|_| {
            RetdataDeserializationError::U128ConversionError { felt: raw_staking_power }
        })?);

        // Parse public key.
        let raw_option_variant =
            iter.next().ok_or(RetdataDeserializationError::InvalidObjectLength {
                message: "missing public key option variant.".to_string(),
            })?;
        let option_variant = usize::try_from(raw_option_variant).map_err(|_| {
            RetdataDeserializationError::USizeConversionError { felt: raw_option_variant }
        })?;
        let public_key = match option_variant {
            1 => None,
            0 => {
                let public_key =
                    iter.next().ok_or(RetdataDeserializationError::InvalidObjectLength {
                        message: "missing public key.".to_string(),
                    })?;
                Some(public_key)
            }
            _ => {
                return Err(RetdataDeserializationError::UnexpectedEnumVariant {
                    variant: option_variant,
                });
            }
        };

        Ok(Self { contract_address, staking_power, public_key })
    }
}

impl<T> TryFrom<Retdata> for ArrayRetdata<T>
where
    T: TryFromIterator<Felt, Error = RetdataDeserializationError>,
{
    type Error = RetdataDeserializationError;

    fn try_from(retdata: Retdata) -> Result<Self, Self::Error> {
        let mut iter = retdata.0.into_iter();

        // The first Felt in the Retdata must be the number of structs in the array.
        let raw_num_items =
            iter.next().ok_or(RetdataDeserializationError::InvalidObjectLength {
                message: "missing number of items in an array.".to_string(),
            })?;

        let num_items = usize::try_from(raw_num_items).map_err(|_| {
            RetdataDeserializationError::USizeConversionError { felt: raw_num_items }
        })?;

        let mut result = Vec::new();
        for _ in 0..num_items {
            let item = T::try_from_iter(&mut iter)?;
            result.push(item);
        }

        if iter.next().is_some() {
            return Err(RetdataDeserializationError::InvalidObjectLength {
                message: "Unconsumed elements found in retdata.".to_string(),
            });
        }

        Ok(ArrayRetdata(result))
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
impl From<&Staker> for ContractStaker {
    fn from(staker: &Staker) -> Self {
        Self {
            contract_address: staker.address,
            staking_power: staker.weight,
            public_key: Some(staker.public_key),
        }
    }
}
