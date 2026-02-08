use std::sync::Arc;

use apollo_network_types::network_types::PeerId;
use apollo_protobuf::authentication::{ChallengeAndIdentity, Signature};
use apollo_protobuf::converters::ProtobufConversionError;
use apollo_signature_manager::signature_manager::{verify_identity, SignatureVerificationError};
use apollo_signature_manager_types::{SharedSignatureManagerClient, SignatureManagerClientError};
use async_trait::async_trait;
use futures::SinkExt;
use rand::rngs::OsRng;
use rand::RngCore;
use starknet_api::core::ContractAddress;
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
use crate::committee_manager::behaviour::AddPeerSender;

pub type StarkAuthNegotiatorResult<T> = Result<T, StarkAuthNegotiatorError>;

#[async_trait]
pub trait ChallengeGenerator: Send + Sync {
    async fn generate(&self) -> Challenge;
}

pub struct OsRngChallengeGenerator;

#[async_trait]
impl ChallengeGenerator for OsRngChallengeGenerator {
    async fn generate(&self) -> Challenge {
        task::block_in_place(|| {
            let mut bytes = [0u8; 16];
            OsRng.fill_bytes(&mut bytes);
            Challenge(u128::from_be_bytes(bytes))
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
    my_staker_address: ContractAddress,
    my_public_key: PublicKey,
    signer: SharedSignatureManagerClient,
    challenge_generator: SharedChallengeGenerator,
    add_peer_sender: AddPeerSender,
}

impl StarkAuthNegotiator {
    pub fn new(
        my_staker_address: ContractAddress,
        my_public_key: PublicKey,
        signer: SharedSignatureManagerClient,
        challenge_generator: SharedChallengeGenerator,
        add_peer_sender: AddPeerSender,
    ) -> Self {
        Self { my_staker_address, my_public_key, signer, challenge_generator, add_peer_sender }
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
            staker_address: self.my_staker_address,
            public_key: self.my_public_key,
            challenge: self.challenge_generator.generate().await,
        };

        let msg = my_challenge_and_identity.into();
        let (_, other_stark_auth) =
            tokio::try_join!(connection_sender.send(msg), connection_receiver.receive())?;

        let ChallengeAndIdentity {
            staker_address: other_staker_address,
            public_key: other_public_key,
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
        if !verify_identity(other_peer_id, other_challenge, other_signature, other_public_key)? {
            return Err(StarkAuthNegotiatorError::VerificationFailure);
        }

        // 4. Ask CommitteeManager to map this staker to the peer (check + add).
        let (response_tx, response_rx) = futures::channel::oneshot::channel();
        self.add_peer_sender
            .send((other_staker_address, other_peer_id, response_tx))
            .await
            .map_err(|_| StarkAuthNegotiatorError::VerificationFailure)?;

        match response_rx.await.map_err(|_| StarkAuthNegotiatorError::VerificationFailure)? {
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
