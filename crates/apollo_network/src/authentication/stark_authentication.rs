use std::fmt::Debug;
use std::sync::{Arc, LazyLock};
use std::time::{SystemTime, UNIX_EPOCH};

use apollo_network_types::network_types::PeerId;
use apollo_protobuf::protobuf::{Felt252, PublicKeyAndChallenge, SignedChallengeAndIdentity};
use apollo_signature_manager::signature_manager::{verify_identity, SignatureVerificationError};
use apollo_signature_manager_types::{
    SharedSignatureManagerClient,
    SignatureManagerClientError,
    SignatureManagerError,
};
use async_trait::async_trait;
use mockall::automock;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use starknet_api::core::Nonce;
use starknet_api::crypto::utils::{PublicKey, RawSignature};
use starknet_types_core::felt::{Felt, NonZeroFelt};
use thiserror::Error;
use tokio::task;
use tokio::time::Instant;

use crate::authentication::negotiator::{ConnectionEndpoint, Negotiator, NegotiatorOutput};

// #[cfg(test)]
// #[path = "stark_authentication_test.rs"]
// pub mod stark_authentication_test;

pub type StarkAuthNegotiatorResult<T> = Result<T, StarkAuthNegotiatorError>;
pub type AuthCommunicationResult<T> = Result<T, AuthCommunicationError>;

#[derive(Debug, Error)]
pub enum AuthCommunicationError {
    #[error(transparent)]
    Connection(#[from] ConnectionError),
    #[error(transparent)]
    Serialize(#[from] bincode::Error),
}

// macro_rules! impl_bytes_conversion {
//     ($t:ty) => {
//         impl TryInto<Bytes> for $t {
//             type Error = bincode::Error;

//             fn try_into(self) -> Result<Bytes, Self::Error> {
//                 bincode::serialize(&self)
//             }
//         }

//         impl TryFrom<Bytes> for $t {
//             type Error = bincode::Error;

//             fn try_from(value: Bytes) -> Result<Self, Self::Error> {
//                 bincode::deserialize(&value)
//             }
//         }
//     };
// }

// // Stark authentication protocol messages.
// // The protocol is serial: each message is consumed and sent by one peer; the other one returns
// // next message.

// #[derive(Debug, Serialize, Deserialize)]
// pub struct PublicKeyMessage {
//     pub public_key: PublicKey,
// }

// impl_bytes_conversion!(PublicKeyMessage);

// impl PublicKeyMessage {
//     pub async fn receive<C: ConnectionEnd>(connection: &mut C) -> AuthCommunicationResult<Self> {
//         Ok(connection.receive_message().await?.try_into()?)
//     }

//     pub async fn communicate<C: ConnectionEnd>(
//         self,
//         connection: &mut C,
//     ) -> AuthCommunicationResult<NonceAndPublicKeyMessage> {
//         // Send my public key.
//         let raw_message = self.try_into()?;
//         connection.send_message(raw_message).await?;

//         // Receive other's nonce and public key.
//         let other_nonce_and_public_key = connection.receive_message().await?;

//         Ok(other_nonce_and_public_key.try_into()?)
//     }
// }

// #[derive(Debug, Serialize, Deserialize)]
// pub struct NonceAndPublicKeyMessage {
//     pub nonce: Nonce,
//     pub public_key: PublicKey,
// }

// impl_bytes_conversion!(NonceAndPublicKeyMessage);

// impl NonceAndPublicKeyMessage {
//     pub async fn communicate<C: ConnectionEnd>(
//         self,
//         connection: &mut C,
//     ) -> AuthCommunicationResult<NonceAndSignatureMessage> {
//         // Send my nonce and public key.
//         let raw_message = self.try_into()?;
//         connection.send_message(raw_message).await?;

//         // Receive other's signature and message payload.
//         let other_nonce_and_signature = connection.receive_message().await?;

//         Ok(other_nonce_and_signature.try_into()?)
//     }
// }

// #[derive(Debug, Serialize, Deserialize)]
// pub struct NonceAndSignatureMessage {
//     pub nonce: Nonce,
//     pub signature: RawSignature,
// }

// impl_bytes_conversion!(NonceAndSignatureMessage);

// impl NonceAndSignatureMessage {
//     pub async fn communicate<C: ConnectionEnd>(
//         self,
//         connection: &mut C,
//     ) -> AuthCommunicationResult<SignatureMessage> {
//         // Send my nonce and signature.
//         let raw_message = self.try_into()?;
//         connection.send_message(raw_message).await?;

//         // Receive other's message payload.
//         let other_message_payload = connection.receive_message().await?;

//         Ok(other_message_payload.try_into()?)
//     }
// }

// #[derive(Debug, Serialize, Deserialize)]
// pub struct SignatureMessage {
//     pub signature: RawSignature,
// }

// impl_bytes_conversion!(SignatureMessage);

// impl SignatureMessage {
//     pub async fn communicate<C: ConnectionEnd>(
//         self,
//         connection: &mut C,
//     ) -> AuthCommunicationResult<()> {
//         // Send my signature.
//         let raw_message = self.try_into()?;
//         connection.send_message(raw_message).await?;

//         Ok(())
//     }
// }

#[cfg_attr(any(feature = "testing", test), automock)]
#[async_trait]
pub trait ChallengeGenerator: Send + Sync {
    async fn generate(&self) -> u64;
}

struct OsRngChallengeGenerator;

#[async_trait]
impl ChallengeGenerator for OsRngChallengeGenerator {
    async fn generate(&self) -> u64 {
        task::block_in_place(|| OsRng.next_u64())
    }
}

struct TimeStampChallengeGenerator;

#[async_trait]
impl ChallengeGenerator for TimeStampChallengeGenerator {
    async fn generate(&self) -> u64 {
// For now we use the bottom 64 bits of the timestamp
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time set to before UNIX EPOCH")
            .as_nanos() as u64;
    }
}

#[derive(Clone)]
pub struct StarkAuthNegotiator {
    // pub peer_id: PeerId,
    my_public_key: PublicKey,
    // pub nonce: Nonce,
    signer: SharedSignatureManagerClient,
}

impl StarkAuthNegotiator {
    pub fn new(my_public_key: PublicKey, signer: SharedSignatureManagerClient) -> Self {
        Self { my_public_key, signer }
    }
}

#[derive(Debug, Error)]
pub enum StarkAuthNegotiatorError {
    #[error(transparent)]
    AuthCommunication(#[from] AuthCommunicationError),
    #[error(transparent)]
    AuthConnection(#[from] ConnectionError),
    #[error(transparent)]
    Serialize(#[from] bincode::Error),
    #[error(transparent)]
    SignatureManager(#[from] SignatureManagerClientError),
    #[error(transparent)]
    SignatureManagerError(#[from] SignatureManagerError),
    #[error(transparent)]
    SignatureVerification(#[from] SignatureVerificationError),
}

#[async_trait]
impl Negotiator for StarkAuthNegotiator {
    type Error = StarkAuthNegotiatorError;

    fn protocol_name(&self) -> &'static str {
        "verify_staker"
    }

    async fn negotiate_incoming_connection(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection: &mut dyn ConnectionEndpoint,
    ) -> Result<NegotiatorOutput, Self::Error> {
        static START_TIME: LazyLock<Instant> = LazyLock::new(|| Instant::now());

        // 1. Send my public key and challenge.
        let mut my_key_and_challenge = PublicKeyAndChallenge {
            public_key: Some(self.my_public_key.0.into()),
            // TODO(guy.f): Think if we want a more "random" challenge than time or is not repeating
            // enough.
            challenge: START_TIME.elapsed().as_nanos() as u64,
        };
        connection.send(my_key_and_challenge.into()).await?;

        // 2. Receive other's public key and challenge.
        let other_key_and_challenge = connection.receive().await?;
        let other_key_and_challenge = PublicKeyAndChallenge::try_from(other_key_and_challenge)?;

        // 3. Calculate and send my signature for the challenge.
        //
        // TODO: Identify should also prepend the message "Identify" to make sure random bytes are
        // never valid messages.
        let signature = self
            .signer
            .identify(my_peer_id, Nonce(other_key_and_challenge.challenge.into()))
            .await?;
        let signature_message = SignedChallengeAndIdentity {
            signature: signature.0.iter().map(|stark_felt| (*stark_felt).into()).collect(),
        };
        connection.send(signature_message.into()).await?;

        // 4. Receive other's signature for the challenge.
        let other_signature = connection.receive().await?;
        let other_signature = SignedChallengeAndIdentity::try_from(other_signature)?;

        // Verify other's signature.

        Ok(NegotiatorOutput::None)
    }

    // async fn negotiate_incoming_connection<C: ConnectionEnd>(
    //     &mut self,
    //     self_peer_id: PeerId,
    //     other_peer_id: PeerId,
    //     connection: &mut C,
    // ) -> StarkAuthNegotiatorResult<bool> {

    //     // Receive other's public key.
    //     let PublicKeyMessage { public_key: other_public_key } =
    //         PublicKeyMessage::receive(connection).await?;

    //     // Send my nonce and public key; receive other's signature and message payload.
    //     let my_nonce_and_public_key =
    //         NonceAndPublicKeyMessage { nonce: self.nonce, public_key: self.public_key };
    //     let NonceAndSignatureMessage { nonce: other_nonce, signature: other_signature } =
    //         my_nonce_and_public_key.communicate(connection).await?;

    //     // Sign my peer ID and other's nonce; send my signature.
    //     let signature = self.signer.identify(self_peer_id, other_nonce).await?;
    //     SignatureMessage { signature }.communicate(connection).await?;

    //     // Verify other's signature.
    //     let is_valid =
    //         verify_identity(other_peer_id, other_nonce, other_signature, other_public_key)?;

    //     Ok(is_valid)
    // }

    async fn negotiate_outgoing_connection(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection: &mut dyn ConnectionEndpoint,
    ) -> Result<NegotiatorOutput, Self::Error> {
        Ok(NegotiatorOutput::None)
    }

    // async fn negotiate_outgoing_connection<C: ConnectionEnd>(
    //     &mut self,
    //     self_peer_id: PeerId,
    //     other_peer_id: PeerId,
    //     connection: &mut C,
    // ) -> StarkAuthNegotiatorResult<bool> {
    //     // Send my public key; receive other's nonce and public key.
    //     let my_public_key = PublicKeyMessage { public_key: self.public_key };
    //     let NonceAndPublicKeyMessage { nonce: other_nonce, public_key: other_public_key } =
    //         my_public_key.communicate(connection).await?;

    //     // Sign my peer ID and other's nonce.
    //     let signature = self.signer.identify(self_peer_id, other_nonce).await?;

    //     // Send my signature, together with my nonce; receive other's message payload.
    //     let my_nonce_and_signature = NonceAndSignatureMessage { nonce: self.nonce, signature };
    //     let SignatureMessage { signature: other_signature } =
    //         my_nonce_and_signature.communicate(connection).await?;

    //     // Verify other's signature.
    //     let is_valid =
    //         verify_identity(other_peer_id, other_nonce, other_signature, other_public_key)?;

    //     Ok(is_valid)
    // }

    // fn protocol_name(&self) -> &'static str {
    //     "stark_authentication"
    // }
}
