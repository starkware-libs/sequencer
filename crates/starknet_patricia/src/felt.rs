use ethnum::U256;
use serde::{Deserialize, Serialize};
use starknet_types_core::felt::{Felt as StarknetTypesFelt, FromStrError};

#[derive(
    Eq,
    PartialEq,
    Clone,
    Copy,
    Default,
    Hash,
    derive_more::Add,
    derive_more::Sub,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
)]
pub struct Felt(pub StarknetTypesFelt);

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
impl_from!(Felt, StarknetTypesFelt, u128, u8);

impl From<Felt> for StarknetTypesFelt {
    fn from(felt: Felt) -> Self {
        felt.0
    }
}

impl From<&Felt> for U256 {
    fn from(felt: &Felt) -> Self {
        U256::from_be_bytes(felt.to_bytes_be())
    }
}

impl std::ops::Mul for Felt {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        Self(self.0 * rhs.0)
    }
}

impl core::fmt::Debug for Felt {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", U256::from(self))
    }
}

impl Felt {
    pub const ZERO: Felt = Felt(StarknetTypesFelt::ZERO);
    #[allow(dead_code)]
    pub const ONE: Felt = Felt(StarknetTypesFelt::ONE);
    #[allow(dead_code)]
    pub const TWO: Felt = Felt(StarknetTypesFelt::TWO);
    #[allow(dead_code)]
    pub const THREE: Felt = Felt(StarknetTypesFelt::THREE);
    pub const MAX: Felt = Felt(StarknetTypesFelt::MAX);

    pub fn from_bytes_be_slice(bytes: &[u8]) -> Self {
        Self(StarknetTypesFelt::from_bytes_be_slice(bytes))
    }

    /// Raises `self` to the power of `exponent`.
    #[allow(dead_code)]
    pub(crate) fn pow(&self, exponent: impl Into<u128>) -> Self {
        Self(self.0.pow(exponent.into()))
    }

    #[allow(dead_code)]
    pub(crate) fn bits(&self) -> u8 {
        self.0
            .bits()
            .try_into()
            // Should not fail as it takes less than 252 bits to represent a felt.
            .expect("Unexpected error occurred when extracting bits of a Felt.")
    }

    pub fn from_bytes_be(bytes: &[u8; 32]) -> Self {
        StarknetTypesFelt::from_bytes_be(bytes).into()
    }

    pub fn to_bytes_be(self) -> [u8; 32] {
        self.0.to_bytes_be()
    }

    /// Parse a hex-encoded number into `Felt`.
    pub fn from_hex(hex_string: &str) -> Result<Self, FromStrError> {
        Ok(StarknetTypesFelt::from_hex(hex_string)?.into())
    }

    pub fn to_hex(&self) -> String {
        self.0.to_hex_string()
    }

    // Convert to a 64-character hexadecimal string without the "0x" prefix.
    pub fn to_fixed_hex_string(&self) -> String {
        // Zero-pad the remaining string
        self.0.to_fixed_hex_string().strip_prefix("0x").unwrap_or("0").to_string()
    }
}
