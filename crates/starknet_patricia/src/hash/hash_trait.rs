use serde::{Deserialize, Serialize};
use starknet_types_core::felt::{Felt, FromStrError};

use crate::impl_from_hex_for_felt_wrapper;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct HashOutput(pub Felt);

impl HashOutput {
    pub(crate) const ZERO: HashOutput = HashOutput(Felt::ZERO);
    pub const ROOT_OF_EMPTY_TREE: HashOutput = Self::ZERO;
}

impl_from_hex_for_felt_wrapper!(HashOutput);
