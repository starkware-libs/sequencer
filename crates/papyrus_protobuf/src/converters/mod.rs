mod class;
mod common;
// TODO(matan): Internalize once we remove the dependency on the protobuf crate.
pub mod consensus;
mod event;
mod header;
mod receipt;
mod rpc_transaction;
mod state_diff;
#[cfg(test)]
mod test_instances;
mod transaction;

use prost::DecodeError;
use starknet_api::StarknetApiError;

#[derive(thiserror::Error, PartialEq, Debug, Clone)]
pub enum ProtobufConversionError {
    #[error("Type `{type_description}` got out of range value {value_as_str}")]
    OutOfRangeValue { type_description: &'static str, value_as_str: String },
    #[error("Missing field `{field_description}`")]
    MissingField { field_description: &'static str },
    #[error("Type `{type_description}` should be {num_expected} bytes but it got {value:?}.")]
    BytesDataLengthMismatch { type_description: &'static str, num_expected: usize, value: Vec<u8> },
    #[error(transparent)]
    DecodeError(#[from] DecodeError),
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
}

#[macro_export]
macro_rules! auto_impl_into_and_try_from_vec_u8 {
    ($T:ty, $ProtobufT:ty) => {
        impl From<$T> for Vec<u8> {
            fn from(value: $T) -> Self {
                let protobuf_value = <$ProtobufT>::from(value);
                protobuf_value.encode_to_vec()
            }
        }
        $crate::auto_impl_try_from_vec_u8!($T, $ProtobufT);
    };
}

// TODO(shahak): Remove this macro once all types implement both directions.
#[macro_export]
macro_rules! auto_impl_try_from_vec_u8 {
    ($T:ty, $ProtobufT:ty) => {
        impl TryFrom<Vec<u8>> for $T {
            type Error = ProtobufConversionError;
            fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
                let protobuf_value = <$ProtobufT>::decode(&value[..])?;
                <$T>::try_from(protobuf_value)
            }
        }
    };
}
