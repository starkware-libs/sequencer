pub enum Constants {
    MerkleHeight,
}

impl From<Constants> for &'static str {
    fn from(id: Constants) -> &'static str {
        match id {
            Constants::MerkleHeight => "starkware.starknet.core.os.state.commitment.MERKLE_HEIGHT",
        }
    }
}
