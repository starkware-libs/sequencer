use std::fmt::Debug;
use std::sync::{Arc, LazyLock};
use std::time::{SystemTime, UNIX_EPOCH};

use apollo_network_types::network_types::PeerId;
use apollo_protobuf::protobuf::{PublicKeyAndChallenge, SignedChallengeAndIdentity};
use apollo_signature_manager::signature_manager::{verify_identity, SignatureVerificationError};
use apollo_signature_manager_types::{SharedSignatureManagerClient, SignatureManagerClientError};
use async_trait::async_trait;
use mockall::automock;
use rand::rngs::OsRng;
use rand::RngCore;
use starknet_api::core::Nonce;
use starknet_api::crypto::utils::{PublicKey, RawSignature};
use starknet_types_core::felt::Felt;
use thiserror::Error;
use tokio::task;

use crate::authentication::negotiator::{ConnectionEndpoint, Negotiator, NegotiatorOutput};

pub type StarkAuthNegotiatorResult<T> = Result<T, StarkAuthNegotiatorError>;

#[cfg_attr(any(feature = "testing", test), automock)]
#[async_trait]
pub trait ChallengeGenerator: Send + Sync {
    async fn generate(&self) -> Felt;
}

struct OsRngChallengeGenerator;

#[async_trait]
impl ChallengeGenerator for OsRngChallengeGenerator {
    async fn generate(&self) -> Felt {
        task::block_in_place(|| {
            let first_u64 = OsRng.next_u64();
            let second_u64 = OsRng.next_u64();
            let combined_u128 = (first_u64 as u128) << 64 | (second_u64 as u128);
            Felt::from(combined_u128)
        })
    }
}

struct TimeStampChallengeGenerator;

#[async_trait]
impl ChallengeGenerator for TimeStampChallengeGenerator {
    async fn generate(&self) -> Felt {
        // Return current system time in nanoseconds.
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time set to before UNIX EPOCH")
            .as_nanos();
        Felt::from(timestamp)
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
}

#[derive(Clone)]
pub struct StarkAuthNegotiator {
    my_stark_public_key: PublicKey,
    signer: SharedSignatureManagerClient,
    challenge_generator: SharedChallengeGenerator,
}

impl StarkAuthNegotiator {
    pub fn new(
        my_stark_public_key: PublicKey,
        signer: SharedSignatureManagerClient,
        challenge_generator: SharedChallengeGenerator,
    ) -> Self {
        Self { my_stark_public_key, signer, challenge_generator }
    }
}

impl StarkAuthNegotiator {
    async fn negotiate_connection(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection: &mut dyn ConnectionEndpoint,
    ) -> Result<NegotiatorOutput, StarkAuthNegotiatorError> {
        // 1. Send my public key and challenge.
        let my_challenge = self.challenge_generator.generate().await;
        let my_key_and_challenge = PublicKeyAndChallenge {
            public_key: Some(self.my_stark_public_key.0.into()),

            // TODO: Do we want challenge to be bytes which is more generic?  If so we need to
            // change identify() to take in bytes and not Nonce.
            challenge: Some(my_challenge.into()),
        };
        connection.send(my_key_and_challenge.into()).await?;

        // 2. Receive other's public key and challenge and verify they are valid.
        let other_key_and_challenge = connection.receive().await?;
        let other_key_and_challenge = PublicKeyAndChallenge::try_from(other_key_and_challenge)?;

        let other_public_key = PublicKey(
            other_key_and_challenge
                .public_key
                .ok_or(StarkAuthNegotiatorError::InvalidData("public_key".to_string()))?
                .try_into()
                .map_err(|e| {
                    StarkAuthNegotiatorError::InvalidData(
                        "PublicKeyAndChallenge::public_key".to_string(),
                    )
                })?,
        );

        let other_challenge = other_key_and_challenge
            .challenge
            .ok_or(StarkAuthNegotiatorError::InvalidData("challenge".to_string()))?
            .try_into()
            .map_err(|e| {
                StarkAuthNegotiatorError::InvalidData(
                    "PublicKeyAndChallenge::challenge".to_string(),
                )
            })?;

        // 3. Calculate and send my signature for the challenge.
        let signature = self.signer.identify(my_peer_id, Nonce(other_challenge)).await?;
        let signature_message = SignedChallengeAndIdentity {
            // Convert from Felt to Felt252 (proto type).
            signature: signature.0.iter().map(|stark_felt| (*stark_felt).into()).collect(),
        };
        connection.send(signature_message.into()).await?;

        // 4. Receive other's signature for the challenge.
        let other_signature = connection.receive().await?;
        let other_signature = SignedChallengeAndIdentity::try_from(other_signature)?;

        let other_raw_signature = RawSignature(
            other_signature
                .signature
                .iter()
                .map(|felt252| Felt::try_from(felt252.clone()))
                .collect::<Result<Vec<Felt>, _>>()
                .map_err(|e| {
                    StarkAuthNegotiatorError::InvalidData(
                        "SignedChallengeAndIdentity::signature".to_string(),
                    )
                })?,
        );

        // Verify other's signature.
        match verify_identity(
            other_peer_id,
            Nonce(other_challenge),
            other_raw_signature,
            other_public_key,
        )? {
            true => Ok(NegotiatorOutput::None),
            false => Err(StarkAuthNegotiatorError::VerificationFailure),
        }
    }
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
        self.negotiate_connection(my_peer_id, other_peer_id, connection).await
    }

    async fn negotiate_outgoing_connection(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection: &mut dyn ConnectionEndpoint,
    ) -> Result<NegotiatorOutput, Self::Error> {
        self.negotiate_connection(my_peer_id, other_peer_id, connection).await
    }
}
