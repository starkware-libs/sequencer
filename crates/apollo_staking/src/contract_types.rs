use blockifier::execution::call_info::Retdata;
use starknet_api::core::ContractAddress;
use starknet_api::staking::StakingWeight;
use starknet_types_core::felt::Felt;
use thiserror::Error;

use crate::committee_provider::Staker;

pub(crate) const GET_STAKERS_ENTRY_POINT: &str = "get_stakers";
pub(crate) const EPOCH_LENGTH: u64 = 100; // Number of heights in an epoch.

// Represents a Cairo1 `Array` containing elements that can be deserialized to `T`.
// `T` must implement `TryFrom<[Felt; N]>`, where `N` is the size of `T`'s Cairo equivalent.
#[derive(Debug, PartialEq, Eq)]
struct ArrayRetdata<const N: usize, T>(Vec<T>);

#[derive(Debug, Error)]
pub enum RetdataDeserializationError {
    #[error("Failed to convert Felt to ContractAddress: {address}")]
    ContractAddressConversionError { address: Felt },
    #[error("Failed to convert Felt to u128: {felt}")]
    U128ConversionError { felt: Felt },
    #[error(
        "Invalid retdata length: expected 1 Felt followed by {num_structs} (number of structs) *
         {struct_size} (number of Felts per struct), but received {length} Felts."
    )]
    InvalidArrayLength { length: usize, num_structs: usize, struct_size: usize },
}

impl Staker {
    pub const CAIRO_OBJECT_NUM_FELTS: usize = 3;

    pub fn from_retdata_many(retdata: Retdata) -> Result<Vec<Self>, RetdataDeserializationError> {
        Ok(ArrayRetdata::<{ Self::CAIRO_OBJECT_NUM_FELTS }, Staker>::try_from(retdata)?.0)
    }
}

impl TryFrom<[Felt; Self::CAIRO_OBJECT_NUM_FELTS]> for Staker {
    type Error = RetdataDeserializationError;

    fn try_from(felts: [Felt; Self::CAIRO_OBJECT_NUM_FELTS]) -> Result<Self, Self::Error> {
        let [address, weight, public_key] = felts;
        let address = ContractAddress::try_from(address)
            .map_err(|_| RetdataDeserializationError::ContractAddressConversionError { address })?;
        let weight = StakingWeight(
            u128::try_from(weight)
                .map_err(|_| RetdataDeserializationError::U128ConversionError { felt: weight })?,
        );
        Ok(Self { address, weight, public_key })
    }
}

#[cfg(test)]
impl From<&Staker> for Vec<Felt> {
    fn from(staker: &Staker) -> Self {
        vec![Felt::from(staker.address), Felt::from(staker.weight.0), staker.public_key]
    }
}

impl<const N: usize, T> TryFrom<Retdata> for ArrayRetdata<N, T>
where
    T: TryFrom<[Felt; N], Error = RetdataDeserializationError>,
{
    type Error = RetdataDeserializationError;

    fn try_from(retdata: Retdata) -> Result<Self, Self::Error> {
        let data = retdata.0;

        // The first Felt in the Retdata must be the number of structs in the array.
        if data.is_empty() {
            return Err(RetdataDeserializationError::InvalidArrayLength {
                length: data.len(),
                num_structs: 0,
                struct_size: N,
            });
        }

        // Split the remaining Felts into chunks of N Felts, each is a struct in the array.
        let data_chunks = data[1..].chunks_exact(N);

        // Verify that the number of structs in the array matches the number of chunks.
        let num_structs = usize::try_from(data[0]).expect("num_structs should fit in usize.");
        if data_chunks.len() != num_structs || !data_chunks.remainder().is_empty() {
            return Err(RetdataDeserializationError::InvalidArrayLength {
                length: data.len(),
                num_structs,
                struct_size: N,
            });
        }

        // Convert each chunk to T.
        let result = data_chunks
            .map(|chunk| {
                T::try_from(
                    chunk.try_into().unwrap_or_else(|_| panic!("chunk size must be N: {N}.")),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ArrayRetdata(result))
    }
}
