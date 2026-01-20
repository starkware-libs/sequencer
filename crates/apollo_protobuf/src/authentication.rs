use serde::{Deserialize, Serialize};
use starknet_api::core::ContractAddress;
use starknet_types_core::felt::Felt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StakerAddress {
    pub staker_address: ContractAddress,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Challenge {
    pub challenge: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SignedChallengeAndIdentity {
    pub signature: Vec<Felt>,
}
