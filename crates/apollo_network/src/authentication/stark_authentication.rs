use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use starknet_api::core::Nonce;
use starknet_api::crypto::utils::{PublicKey, RawSignature};
use thiserror::Error;

use crate::authentication::negotiator::{ConnectionEnd, ConnectionError};

pub type Bytes = Vec<u8>;

pub type AuthCommunicationResult<T> = Result<T, AuthCommunicationError>;

#[derive(Debug, Error)]
pub enum AuthCommunicationError {
    #[error(transparent)]
    Connection(#[from] ConnectionError),
    #[error(transparent)]
    Serialize(#[from] bincode::Error),
}

macro_rules! impl_bytes_conversion {
    ($t:ty) => {
        impl TryInto<Bytes> for $t {
            type Error = bincode::Error;

            fn try_into(self) -> Result<Bytes, Self::Error> {
                bincode::serialize(&self)
            }
        }

        impl TryFrom<Bytes> for $t {
            type Error = bincode::Error;

            fn try_from(value: Bytes) -> Result<Self, Self::Error> {
                bincode::deserialize(&value)
            }
        }
    };
}

// Stark authentication protocol messages.
// The protocol is serial: each message is consumed and sent by one peer; the other one returns the
// next message.

#[derive(Debug, Serialize, Deserialize)]
pub struct PublicKeyMessage {
    pub public_key: PublicKey,
}

impl_bytes_conversion!(PublicKeyMessage);

impl PublicKeyMessage {
    pub async fn receive<C: ConnectionEnd>(connection: &mut C) -> AuthCommunicationResult<Self> {
        Ok(connection.receive_message().await?.try_into()?)
    }

    pub async fn communicate<C: ConnectionEnd>(
        self,
        connection: &mut C,
    ) -> AuthCommunicationResult<NonceAndPublicKeyMessage> {
        // Send my public key.
        let raw_message = self.try_into()?;
        connection.send_message(raw_message).await?;

        // Receive other's nonce and public key.
        let other_nonce_and_public_key = connection.receive_message().await?;

        Ok(other_nonce_and_public_key.try_into()?)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NonceAndPublicKeyMessage {
    pub nonce: Nonce,
    pub public_key: PublicKey,
}

impl_bytes_conversion!(NonceAndPublicKeyMessage);

impl NonceAndPublicKeyMessage {
    pub async fn communicate<C: ConnectionEnd>(
        self,
        connection: &mut C,
    ) -> AuthCommunicationResult<NonceAndSignatureMessage> {
        // Send my nonce and public key.
        let raw_message = self.try_into()?;
        connection.send_message(raw_message).await?;

        // Receive other's signature and message payload.
        let other_nonce_and_signature = connection.receive_message().await?;

        Ok(other_nonce_and_signature.try_into()?)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NonceAndSignatureMessage {
    pub nonce: Nonce,
    pub signature: RawSignature,
}

impl_bytes_conversion!(NonceAndSignatureMessage);

impl NonceAndSignatureMessage {
    pub async fn communicate<C: ConnectionEnd>(
        self,
        connection: &mut C,
    ) -> AuthCommunicationResult<SignatureMessage> {
        // Send my nonce and signature.
        let raw_message = self.try_into()?;
        connection.send_message(raw_message).await?;

        // Receive other's message payload.
        let other_message_payload = connection.receive_message().await?;

        Ok(other_message_payload.try_into()?)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SignatureMessage {
    pub signature: RawSignature,
}

impl_bytes_conversion!(SignatureMessage);

impl SignatureMessage {
    pub async fn communicate<C: ConnectionEnd>(
        self,
        connection: &mut C,
    ) -> AuthCommunicationResult<()> {
        // Send my signature.
        let raw_message = self.try_into()?;
        connection.send_message(raw_message).await?;

        Ok(())
    }
}
