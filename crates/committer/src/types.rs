use starknet_types_core::felt::{Felt as StarknetTypesFelt, FromStrError};

#[derive(Eq, PartialEq, Clone, Copy, Debug, Default, Hash, derive_more::Add)]
pub(crate) struct Felt(StarknetTypesFelt);

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

impl std::ops::Mul for Felt {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        Self(self.0 * rhs.0)
    }
}

#[allow(dead_code)]
impl Felt {
    pub(crate) const ZERO: Felt = Felt(StarknetTypesFelt::ZERO);
    pub(crate) const ONE: Felt = Felt(StarknetTypesFelt::ONE);
    pub(crate) const TWO: Felt = Felt(StarknetTypesFelt::TWO);
    pub(crate) const THREE: Felt = Felt(StarknetTypesFelt::THREE);

    /// Raises `self` to the power of `exponent`.
    pub(crate) fn pow(&self, exponent: impl Into<u128>) -> Self {
        Self(self.0.pow(exponent.into()))
    }

    /// Parse a hex-encoded number into `Felt`.
    pub(crate) fn from_hex(hex_string: &str) -> Result<Self, FromStrError> {
        Ok(StarknetTypesFelt::from_hex(hex_string)?.into())
    }
}
