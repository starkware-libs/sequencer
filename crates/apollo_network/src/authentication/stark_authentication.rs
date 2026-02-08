use std::fmt::Debug;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use apollo_network_types::network_types::PeerId;
use apollo_protobuf::authentication::{Challenge, SignedChallengeAndIdentity, StakerAddress};
use apollo_protobuf::converters::ProtobufConversionError;
use apollo_signature_manager::signature_manager::{verify_identity, SignatureVerificationError};
use apollo_signature_manager_types::{SharedSignatureManagerClient, SignatureManagerClientError};
use async_trait::async_trait;
use futures::SinkExt;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use rand::rngs::OsRng;
use rand::RngCore;
use starknet_api::crypto::utils::{PublicKey, RawSignature};
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

#[cfg_attr(any(feature = "testing", test), automock)]
#[async_trait]
pub trait ChallengeGenerator: Send + Sync {
    async fn generate(&self) -> Vec<u8>;
}

pub struct OsRngChallengeGenerator;

#[async_trait]
#[allow(clippy::as_conversions)]
impl ChallengeGenerator for OsRngChallengeGenerator {
    async fn generate(&self) -> Vec<u8> {
        task::block_in_place(|| {
            let first_u64 = OsRng.next_u64();
            let second_u64 = OsRng.next_u64();
            let combined_u128 = (first_u64 as u128) << 64 | (second_u64 as u128);

            // TODO(noam.s): should this be little endian?
            combined_u128.to_be_bytes().to_vec()
        })
    }
}

#[allow(dead_code)]
struct TimeStampChallengeGenerator;

#[async_trait]
impl ChallengeGenerator for TimeStampChallengeGenerator {
    async fn generate(&self) -> Vec<u8> {
        // Return current system time in nanoseconds.
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time set to before UNIX EPOCH")
            .as_nanos();
        // TODO(noam.s): should this be little endian?
        timestamp.to_be_bytes().to_vec()
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
    my_staker_address: StakerAddress,
    signer: SharedSignatureManagerClient,
    challenge_generator: SharedChallengeGenerator,
    add_peer_sender: AddPeerSender,
}

impl StarkAuthNegotiator {
    pub fn new(
        my_staker_address: StakerAddress,
        signer: SharedSignatureManagerClient,
        challenge_generator: SharedChallengeGenerator,
        add_peer_sender: AddPeerSender,
    ) -> Self {
        Self { my_staker_address, signer, challenge_generator, add_peer_sender }
    }
}

impl StarkAuthNegotiator {
    async fn negotiate_connection(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection_sender: &mut dyn ConnectionSender,
        connection_receiver: &mut dyn ConnectionReceiver,
        _side: NegotiationSide,
    ) -> Result<NegotiatorOutput, StarkAuthNegotiatorError> {
        // 1. Send my staker address and receive other's staker address.
        let my_staker_address = self.my_staker_address.clone();
        let (_, other_staker_address) = tokio::try_join!(
            connection_sender.send(my_staker_address.into()),
            connection_receiver.receive()
        )?;

        let other_staker_address = StakerAddress::try_from(other_staker_address)?;

        // 2. Send my challenge and receive other's challenge.
        let my_challenge = self.challenge_generator.generate().await;
        let (_, other_challenge) =
            tokio::try_join!(connection_sender.send(my_challenge), connection_receiver.receive())?;

        let other_challenge = Challenge::try_from(other_challenge)?.challenge;

        // 3. Send my signature for the challenge and receive other's signature for the challenge.
        let signature = self.signer.identify(my_peer_id, other_challenge.clone()).await?;
        let signature_message = SignedChallengeAndIdentity { signature: signature.0 };
        let (_, other_signature) = tokio::try_join!(
            connection_sender.send(signature_message.into()),
            connection_receiver.receive()
        )?;

        let other_signature =
            RawSignature(SignedChallengeAndIdentity::try_from(other_signature)?.signature);

        // 4. Verify other's signature.
        if !verify_identity(
            other_peer_id,
            other_challenge,
            other_signature,
            PublicKey(other_staker_address.staker_address.into()),
        )? {
            return Err(StarkAuthNegotiatorError::VerificationFailure);
        }

        // 5. Ask CommitteeManager to map this staker to the peer (check + add).
        let staker_id = other_staker_address.staker_address;
        let (response_tx, response_rx) = futures::channel::oneshot::channel();
        self.add_peer_sender
            .send((staker_id, other_peer_id, response_tx))
            .await
            .map_err(|_| StarkAuthNegotiatorError::VerificationFailure)?;

        let accepted =
            response_rx.await.map_err(|_| StarkAuthNegotiatorError::VerificationFailure)?;
        if !accepted {
            return Err(StarkAuthNegotiatorError::VerificationFailure);
        }

        Ok(NegotiatorOutput::Success)
    }
}

#[async_trait]
impl Negotiator for StarkAuthNegotiator {
    type Error = StarkAuthNegotiatorError;

    fn protocol_name(&self) -> &'static str {
        "verify_staker"
    }

    async fn negotiate_connection(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection_sender: &mut dyn ConnectionSender,
        connection_receiver: &mut dyn ConnectionReceiver,
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
