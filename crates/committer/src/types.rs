use starknet_types_core::felt::Felt as StarknetTypesFelt;

#[derive(Eq, PartialEq, Clone, Copy, Debug, Default, Hash, derive_more::Add)]
pub(crate) struct Felt(StarknetTypesFelt);

impl From<StarknetTypesFelt> for Felt {
    fn from(felt: StarknetTypesFelt) -> Self {
        Self(felt)
    }
}

impl From<u128> for Felt {
    fn from(value: u128) -> Self {
        Self(value.into())
    }
}

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
    pub const ZERO: Felt = Felt(StarknetTypesFelt::ZERO);
    pub const ONE: Felt = Felt(StarknetTypesFelt::ONE);
    pub const TWO: Felt = Felt(StarknetTypesFelt::TWO);

    /// Raises `self` to the power of `exponent`.
    pub fn pow(&self, exponent: impl Into<u128>) -> Self {
        Self(self.0.pow(exponent.into()))
    }
}
