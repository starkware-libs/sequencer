use starknet_types_core::felt::Felt as StarknetTypesFelt;

#[derive(Eq, PartialEq)]
pub(crate) struct Felt(StarknetTypesFelt);

impl From<StarknetTypesFelt> for Felt {
    fn from(felt: StarknetTypesFelt) -> Self {
        Self(felt)
    }
}

impl From<Felt> for StarknetTypesFelt {
    fn from(felt: Felt) -> Self {
        felt.0
    }
}

impl Felt {
    pub const ZERO: Felt = Felt(StarknetTypesFelt::ZERO);
}
