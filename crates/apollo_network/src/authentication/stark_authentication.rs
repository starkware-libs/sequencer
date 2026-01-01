use std::sync::Arc;

use apollo_network_types::network_types::PeerId;
use apollo_protobuf::authentication::{ChallengeAndIdentity, Signature};
use apollo_protobuf::converters::ProtobufConversionError;
use apollo_signature_manager::signature_manager::{verify_identity, SignatureVerificationError};
use apollo_signature_manager_types::{SharedSignatureManagerClient, SignatureManagerClientError};
use async_trait::async_trait;
use rand::rngs::OsRng;
use rand::RngCore;
use starknet_api::crypto::utils::{Challenge, PublicKey, RawSignature};
use thiserror::Error;
use tokio::task;

use crate::authentication::negotiator::{
    ConnectionReceiver,
    ConnectionSender,
    NegotiationSide,
    Negotiator,
    NegotiatorOutput,
};

pub type StarkAuthNegotiatorResult<T> = Result<T, StarkAuthNegotiatorError>;

#[async_trait]
pub trait ChallengeGenerator: Send + Sync {
    async fn generate(&self) -> Challenge;
}

// Default implementation for production use.
#[allow(dead_code)]
pub struct OsRngChallengeGenerator;

#[async_trait]
impl ChallengeGenerator for OsRngChallengeGenerator {
    async fn generate(&self) -> Challenge {
        task::block_in_place(|| {
            let mut bytes = [0u8; 16];
            OsRng.fill_bytes(&mut bytes);
            Challenge(bytes)
        })
    }
}

type SharedChallengeGenerator = Arc<dyn ChallengeGenerator>;

#[derive(Debug, Error)]
pub enum StarkAuthNegotiatorError {
    #[error("Other side sent invalid data: {0}")]
    InvalidData(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
    #[error(transparent)]
    SignatureManager(#[from] SignatureManagerClientError),
    #[error(transparent)]
    SignatureVerification(#[from] SignatureVerificationError),
    #[error("Verification failed")]
    VerificationFailure,
    #[error(transparent)]
    ProtobufConversion(#[from] ProtobufConversionError),
}

#[derive(Clone)]
pub struct StarkAuthNegotiator {
    my_public_key: PublicKey,
    signer: SharedSignatureManagerClient,
    challenge_generator: SharedChallengeGenerator,
}

impl StarkAuthNegotiator {
    pub fn new(
        my_public_key: PublicKey,
        signer: SharedSignatureManagerClient,
        challenge_generator: SharedChallengeGenerator,
    ) -> Self {
        Self { my_public_key, signer, challenge_generator }
    }
}

impl StarkAuthNegotiator {
    async fn negotiate_connection(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection_sender: &mut dyn ConnectionSender<
            apollo_protobuf::protobuf::StarkAuthentication,
        >,
        connection_receiver: &mut dyn ConnectionReceiver<
            apollo_protobuf::protobuf::StarkAuthentication,
        >,
        _side: NegotiationSide,
    ) -> Result<NegotiatorOutput, StarkAuthNegotiatorError> {
        // 1. Send my challenge and identity and receive other's challenge and identity.
        let my_challenge_and_identity = ChallengeAndIdentity {
            operational_public_key: self.my_public_key,
            challenge: self.challenge_generator.generate().await,
        };

        let msg = my_challenge_and_identity.into();
        let (_, other_stark_auth) =
            tokio::try_join!(connection_sender.send(msg), connection_receiver.receive())?;

        let ChallengeAndIdentity {
            operational_public_key: other_public_key,
            challenge: other_challenge,
        } = ChallengeAndIdentity::try_from(other_stark_auth)?;

        // 2. Send my signature for the challenge and receive other's signature for the challenge.
        let signature = self.signer.sign_identification(my_peer_id, other_challenge).await?;
        let signed_challenge_and_identity = Signature { signature: signature.0 };
        let (_, other_signature) = tokio::try_join!(
            connection_sender.send(signed_challenge_and_identity.into()),
            connection_receiver.receive()
        )?;

        let other_signature = RawSignature(Signature::try_from(other_signature)?.signature);

        // 3. Verify other's signature.
        match verify_identity(other_peer_id, other_challenge, other_signature, other_public_key)? {
            true => Ok(NegotiatorOutput::Success),
            false => Err(StarkAuthNegotiatorError::VerificationFailure),
        }
    }
}

#[async_trait]
impl Negotiator for StarkAuthNegotiator {
    type WireMessage = apollo_protobuf::protobuf::StarkAuthentication;
    type Error = StarkAuthNegotiatorError;

    fn protocol_name(&self) -> &'static str {
        "verify_staker"
    }

    async fn negotiate_connection(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection_sender: &mut dyn ConnectionSender<Self::WireMessage>,
        connection_receiver: &mut dyn ConnectionReceiver<Self::WireMessage>,
        side: NegotiationSide,
    ) -> Result<NegotiatorOutput, Self::Error> {
        self.negotiate_connection(
            my_peer_id,
            other_peer_id,
            connection_sender,
            connection_receiver,
            side,
        )
        .await
    }
}
