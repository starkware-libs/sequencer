use serde::{Deserialize, Serialize};
use starknet_api::crypto::utils::{Challenge, PublicKey};
use starknet_types_core::felt::Felt;

// TODO(noam.s): Move this file/logic to the consensus manager crate once the whole stack is merged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarkAuthentication {
    pub message: StarkAuthenticationMessage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StarkAuthenticationMessage {
    ChallengeAndIdentity(ChallengeAndIdentity),
    Signature(Signature),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChallengeAndIdentity {
    pub operational_public_key: PublicKey,
    pub challenge: Challenge,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Signature {
    pub signature: Vec<Felt>,
}
