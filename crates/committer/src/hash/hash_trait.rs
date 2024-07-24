use starknet_types_core::felt::FromStrError;

use crate::felt::Felt;
use crate::impl_from_hex_for_felt_wrapper;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct HashOutput(pub Felt);

impl HashOutput {
    pub(crate) const ZERO: HashOutput = HashOutput(Felt::ZERO);
    pub(crate) const ROOT_OF_EMPTY_TREE: HashOutput = Self::ZERO;
}

impl_from_hex_for_felt_wrapper!(HashOutput);
