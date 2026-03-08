use blockifier::execution::call_info::Retdata;
use starknet_types_core::felt::Felt;
use thiserror::Error;

/// Conversion from an [`Iterator`].
///
/// By implementing `TryFromIterator` for a type, you define how it will be
/// created from an iterator.
///
/// Used in this context to parse Cairo1 types returned by a contract as a vector of Felts.
pub trait TryFromIterator<Item>: Sized {
    type Error;

    fn try_from_iter<T: Iterator<Item = Item>>(iter: &mut T) -> Result<Self, Self::Error>;
}

// Represents a Cairo1 `Array` containing elements that can be deserialized to `T`.
// `T` must implement `TryFromIterator<Felt>`.
#[derive(Debug, PartialEq, Eq)]
pub struct CairoArray<T>(pub Vec<T>);

// Represents a Cairo1 `Option` containing an element that can be deserialized to `T`.
#[derive(Debug)]
pub struct CairoOption<T>(pub Option<T>);

#[derive(Debug, Error)]
pub enum RetdataDeserializationError {
    #[error("Failed to convert Felt to ContractAddress: {address}")]
    ContractAddressConversionError { address: Felt },
    #[error("Failed to convert Felt to u128: {felt}")]
    U128ConversionError { felt: Felt },
    #[error("Failed to convert Felt to usize: {felt}")]
    USizeConversionError { felt: Felt },
    #[error("Failed to convert Felt to u64: {felt}")]
    U64ConversionError { felt: Felt },
    #[error("Invalid object length: {message}.")]
    InvalidObjectLength { message: String },
    #[error("Unexpected enum variant: {variant}")]
    UnexpectedEnumVariant { variant: usize },
}

impl TryFromIterator<Felt> for Felt {
    type Error = RetdataDeserializationError;

    // Consumes the next felt from the iterator.
    fn try_from_iter<T: Iterator<Item = Felt>>(iter: &mut T) -> Result<Self, Self::Error> {
        iter.next().ok_or(RetdataDeserializationError::InvalidObjectLength {
            message: "missing felt value.".to_string(),
        })
    }
}

impl<V> TryFromIterator<Felt> for Option<V>
where
    V: TryFromIterator<Felt, Error = RetdataDeserializationError>,
{
    type Error = RetdataDeserializationError;

    // Parses a Cairo `Option<V>` from a stream of Felts.
    //
    // The iterator is expected to yield the following values, in order:
    // 1. Option variant (1 Felt):
    //    - 0 => Some
    //    - 1 => None
    // 2. Value (N Felts), only if the option variant is `Some`
    fn try_from_iter<T: Iterator<Item = Felt>>(iter: &mut T) -> Result<Self, Self::Error> {
        let raw_variant = Felt::try_from_iter(iter)?;
        let variant = usize::try_from(raw_variant)
            .map_err(|_| RetdataDeserializationError::USizeConversionError { felt: raw_variant })?;
        match variant {
            0 => Ok(Some(V::try_from_iter(iter)?)),
            1 => Ok(None),
            _ => Err(RetdataDeserializationError::UnexpectedEnumVariant { variant }),
        }
    }
}

impl<T> TryFrom<Retdata> for CairoArray<T>
where
    T: TryFromIterator<Felt, Error = RetdataDeserializationError>,
{
    type Error = RetdataDeserializationError;

    fn try_from(retdata: Retdata) -> Result<Self, Self::Error> {
        let mut iter = retdata.0.into_iter();

        // The first Felt in the Retdata must be the number of structs in the array.
        let raw_num_items = Felt::try_from_iter(&mut iter)?;

        let num_items = usize::try_from(raw_num_items).map_err(|_| {
            RetdataDeserializationError::USizeConversionError { felt: raw_num_items }
        })?;

        let mut result = Vec::with_capacity(num_items);
        for _ in 0..num_items {
            let item = T::try_from_iter(&mut iter)?;
            result.push(item);
        }

        if iter.next().is_some() {
            return Err(RetdataDeserializationError::InvalidObjectLength {
                message: "Unconsumed elements found in retdata.".to_string(),
            });
        }

        Ok(CairoArray(result))
    }
}

impl<T> TryFrom<Retdata> for CairoOption<T>
where
    T: TryFromIterator<Felt, Error = RetdataDeserializationError>,
{
    type Error = RetdataDeserializationError;

    /// Deserializes a Cairo `Option<T>` from retdata.
    fn try_from(retdata: Retdata) -> Result<Self, Self::Error> {
        let mut iter = retdata.0.into_iter();
        let result = Option::<T>::try_from_iter(&mut iter)?;
        if iter.next().is_some() {
            return Err(RetdataDeserializationError::InvalidObjectLength {
                message: "unconsumed elements in Option retdata.".to_string(),
            });
        }
        Ok(CairoOption(result))
    }
}
