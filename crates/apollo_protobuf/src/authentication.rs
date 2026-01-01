use serde::{Deserialize, Serialize};
use starknet_api::core::ContractAddress;
use starknet_api::crypto::utils::{Challenge, PublicKey};
use starknet_types_core::felt::Felt;

// TODO(noam.s): Move this file/logic to the consensus manager crate once the whole stack is merged.
pub struct StarkAuthentication {
    pub message: StarkAuthenticationMessage,
}

#[derive(Debug)]
pub enum StarkAuthenticationMessage {
    ChallengeAndIdentity(ChallengeAndIdentity),
    Signature(Signature),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChallengeAndIdentity {
    pub staker_address: ContractAddress,
    pub public_key: PublicKey,
    pub challenge: Challenge,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Signature {
    pub signature: Vec<Felt>,
}
