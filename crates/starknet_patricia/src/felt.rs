use ethnum::U256;
use starknet_types_core::felt::Felt;

// TODO(Nimrod): Move this to sn-api crate.
#[macro_export]
macro_rules! impl_from_hex_for_felt_wrapper {
    ($wrapper:ty) => {
        impl $wrapper {
            pub fn from_hex(hex_string: &str) -> Result<Self, FromStrError> {
                Ok(Self(Felt::from_hex(hex_string)?))
            }
        }
    };
}

#[macro_export]
macro_rules! impl_from {
    ($to:ty, $from:ty, $($other_from: ty),+) => {
        $crate::impl_from!($to, $from);
        $crate::impl_from!($to $(, $other_from)*);
    };
    ($to:ty, $from:ty) => {
        impl From<$from> for $to {
            fn from(value: $from) -> Self {
                Self(value.into())
            }
        }
    };
}

pub fn u256_from_felt(felt: &Felt) -> U256 {
    U256::from_be_bytes(felt.to_bytes_be())
}
