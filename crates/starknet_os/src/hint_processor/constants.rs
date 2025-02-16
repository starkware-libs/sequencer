use std::collections::HashMap;

use cairo_vm::vm::errors::hint_errors::HintError;
use starknet_types_core::felt::Felt;

#[derive(Clone, Copy)]
pub enum Constants {
    MerkleHeight,
}

impl Constants {
    /// Fetch the constant value from the VM constants map. Returns an error if the constant is not
    /// found.
    pub fn fetch<'a>(&self, constants: &'a HashMap<String, Felt>) -> Result<&'a Felt, HintError> {
        let identifier_str: &'static str = (*self).into();
        constants.get(identifier_str).ok_or(HintError::MissingConstant(Box::new(identifier_str)))
    }
}

impl From<Constants> for &'static str {
    fn from(id: Constants) -> &'static str {
        match id {
            Constants::MerkleHeight => "starkware.starknet.core.os.state.commitment.MERKLE_HEIGHT",
        }
    }
}
