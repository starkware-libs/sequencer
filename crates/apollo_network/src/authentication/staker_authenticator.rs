use std::sync::Arc;
use std::time::Duration;

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
use tokio::time::timeout;

use crate::authentication::negotiator::{
    ConnectionReceiver,
    ConnectionSender,
    NegotiationSide,
    Negotiator,
    NegotiatorOutput,
};

#[async_trait]
pub trait ChallengeGenerator: Send + Sync {
    async fn generate(&self) -> Result<Challenge, StakerAuthenticatorError>;
}

#[async_trait]
pub trait AllowListChecker: Send + Sync {
    async fn is_allowed(&self, public_key: &PublicKey) -> bool;
}

// TODO(noam.s): Verify with @albert-starkware that OsRng is cryptographically secure enough for
// challenge generation. Also check which RNG libp2p uses in its default noise implementation.
#[allow(dead_code)]
pub struct OsRngChallengeGenerator;

#[async_trait]
impl ChallengeGenerator for OsRngChallengeGenerator {
    async fn generate(&self) -> Result<Challenge, StakerAuthenticatorError> {
        task::spawn_blocking(|| {
            let mut bytes = [0u8; 16];
            OsRng.fill_bytes(&mut bytes);
            Challenge(bytes)
        })
        .await
        .map_err(StakerAuthenticatorError::ChallengeGenerationPanicked)
    }
}

// TODO(noam.s): Consider making this configurable.
const NEGOTIATION_EXCHANGE_TIMEOUT: Duration = Duration::from_secs(10);

type SharedChallengeGenerator = Arc<dyn ChallengeGenerator>;
type SharedAllowListChecker = Arc<dyn AllowListChecker>;

#[derive(Debug, Error)]
pub enum StakerAuthenticatorError {
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
    #[error("Signature verification failed for peer {0}")]
    VerificationFailure(PeerId),
    #[error("Peer's public key {0:?} is not in the allow list")]
    PeerNotAllowed(PublicKey),
    #[error(transparent)]
    ProtobufConversion(#[from] ProtobufConversionError),
    #[error("spawn_blocking panicked during challenge generation: {0}")]
    ChallengeGenerationPanicked(tokio::task::JoinError),
    #[error("spawn_blocking panicked during signature verification: {0}")]
    VerificationPanicked(tokio::task::JoinError),
    #[error("{step} timed out after {timeout:?}")]
    Timeout { step: &'static str, timeout: Duration },
}

#[derive(Clone)]
pub struct StakerAuthenticator {
    my_operational_public_key: PublicKey,
    signer_client: SharedSignatureManagerClient,
    challenge_generator: SharedChallengeGenerator,
    allow_list_checker: SharedAllowListChecker,
}

impl StakerAuthenticator {
    pub fn new(
        my_operational_public_key: PublicKey,
        signer_client: SharedSignatureManagerClient,
        challenge_generator: SharedChallengeGenerator,
        allow_list_checker: SharedAllowListChecker,
    ) -> Self {
        Self { my_operational_public_key, signer_client, challenge_generator, allow_list_checker }
    }
}

#[async_trait]
impl Negotiator for StakerAuthenticator {
    type WireMessage = apollo_protobuf::protobuf::StarkAuthentication;
    type Error = StakerAuthenticatorError;

    fn protocol_name(&self) -> &'static str {
        "verify_staker"
    }

    async fn negotiate_connection(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection_sender: &mut dyn ConnectionSender<Self::WireMessage>,
        connection_receiver: &mut dyn ConnectionReceiver<Self::WireMessage>,
        // TODO(Andrew): remove the side parameter from the Negotiator trait.
        _side: NegotiationSide,
    ) -> Result<NegotiatorOutput, Self::Error> {
        // NOTE: Some steps below (e.g. signing then sending, receiving then verifying) could be
        // parallelized with try_join!, but we run them sequentially to prefer readability over
        // performance.

        // 1. Send my challenge and identity and receive other's challenge and identity.
        let my_challenge = self.challenge_generator.generate().await?;
        let my_challenge_and_identity = ChallengeAndIdentity {
            operational_public_key: self.my_operational_public_key,
            challenge: my_challenge,
        };

        // Both send and receive must complete before advancing: the next step requires the
        // other side's challenge (from receive) to sign, and the other side needs our challenge
        // (from send) to produce their signature.
        let (_, other_stark_auth) = timeout(NEGOTIATION_EXCHANGE_TIMEOUT, async {
            tokio::try_join!(
                connection_sender.send(my_challenge_and_identity.into()),
                connection_receiver.receive()
            )
        })
        .await
        .map_err(|_| StakerAuthenticatorError::Timeout {
            step: "challenge and identity exchange",
            timeout: NEGOTIATION_EXCHANGE_TIMEOUT,
        })??;

        let ChallengeAndIdentity {
            operational_public_key: other_public_key,
            challenge: other_challenge,
        } = ChallengeAndIdentity::try_from(other_stark_auth)?;

        // 2. Verify the peer is in the allow list before signing anything.
        if !self.allow_list_checker.is_allowed(&other_public_key).await {
            return Err(StakerAuthenticatorError::PeerNotAllowed(other_public_key));
        }

        // 3. Send my signature for the challenge and receive other's signature for the challenge.
        let signature = self.signer_client.sign_identification(my_peer_id, other_challenge).await?;
        let wire_signature = Signature { signature: signature.0 };
        let (_, other_signature) = timeout(NEGOTIATION_EXCHANGE_TIMEOUT, async {
            tokio::try_join!(
                connection_sender.send(wire_signature.into()),
                connection_receiver.receive()
            )
        })
        .await
        .map_err(|_| StakerAuthenticatorError::Timeout {
            step: "signature exchange",
            timeout: NEGOTIATION_EXCHANGE_TIMEOUT,
        })??;

        let other_signature = RawSignature(Signature::try_from(other_signature)?.signature);

        // 4. Verify other's signature on a blocking thread since it involves CPU-heavy
        //    cryptographic operations.
        let peer_id_for_error = other_peer_id;
        let verification_result = task::spawn_blocking(move || {
            verify_identity(other_peer_id, my_challenge, other_signature, other_public_key)
        })
        .await
        .map_err(StakerAuthenticatorError::VerificationPanicked)?;

        match verification_result? {
            true => Ok(NegotiatorOutput::Success),
            false => Err(StakerAuthenticatorError::VerificationFailure(peer_id_for_error)),
        }
    }
}
