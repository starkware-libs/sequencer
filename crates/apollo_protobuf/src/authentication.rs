use serde::{Deserialize, Serialize};
use starknet_api::core::ContractAddress;
use starknet_api::crypto::utils::{Challenge, PublicKey};
use starknet_types_core::felt::Felt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChallengeAndIdentity {
    pub staker_address: ContractAddress,
    pub public_key: PublicKey,
    pub challenge: Challenge,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SignedChallengeAndIdentity {
    pub signature: Vec<Felt>,
}
