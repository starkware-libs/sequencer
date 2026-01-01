use starknet_api::crypto::utils::{Challenge, PublicKey, CHALLENGE_LEN};

use super::common::missing;
use crate::authentication::{
    ChallengeAndIdentity,
    Signature,
    StarkAuthentication,
    StarkAuthenticationMessage,
};
use crate::converters::ProtobufConversionError;
use crate::protobuf;

// TODO(noam.s): Move this file/logic to the consensus manager crate once the whole stack is merged.
impl TryFrom<protobuf::StarkAuthentication> for ChallengeAndIdentity {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::StarkAuthentication) -> Result<Self, Self::Error> {
        let message = value.message.ok_or(missing("StarkAuthentication::message"))?;
        match message {
            protobuf::stark_authentication::Message::ChallengeAndIdentity(inner) => {
                inner.try_into()
            }
            other => Err(ProtobufConversionError::WrongEnumVariant {
                type_description: "StarkAuthenticationMessage",
                value_as_str: format!("{:?}", other),
                expected: "ChallengeAndIdentity",
            }),
        }
    }
}

impl TryFrom<protobuf::StarkAuthentication> for Signature {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::StarkAuthentication) -> Result<Self, Self::Error> {
        let message = value.message.ok_or(missing("StarkAuthentication::message"))?;
        match message {
            protobuf::stark_authentication::Message::Signature(inner) => inner.try_into(),
            other => Err(ProtobufConversionError::WrongEnumVariant {
                type_description: "StarkAuthenticationMessage",
                value_as_str: format!("{:?}", other),
                expected: "Signature",
            }),
        }
    }
}

impl From<StarkAuthenticationMessage> for protobuf::StarkAuthentication {
    fn from(value: StarkAuthenticationMessage) -> Self {
        protobuf::StarkAuthentication { message: Some(value.into()) }
    }
}

impl From<ChallengeAndIdentity> for protobuf::StarkAuthentication {
    fn from(value: ChallengeAndIdentity) -> Self {
        StarkAuthenticationMessage::ChallengeAndIdentity(value).into()
    }
}

impl From<Signature> for protobuf::StarkAuthentication {
    fn from(value: Signature) -> Self {
        StarkAuthenticationMessage::Signature(value).into()
    }
}

impl TryFrom<protobuf::stark_authentication::Message> for StarkAuthenticationMessage {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::stark_authentication::Message) -> Result<Self, Self::Error> {
        match value {
            protobuf::stark_authentication::Message::ChallengeAndIdentity(
                challenge_and_identity,
            ) => Ok(StarkAuthenticationMessage::ChallengeAndIdentity(
                challenge_and_identity.try_into()?,
            )),
            protobuf::stark_authentication::Message::Signature(signed_challenge_and_identity) => {
                Ok(StarkAuthenticationMessage::Signature(signed_challenge_and_identity.try_into()?))
            }
        }
    }
}

impl From<StarkAuthenticationMessage> for protobuf::stark_authentication::Message {
    fn from(value: StarkAuthenticationMessage) -> Self {
        match value {
            StarkAuthenticationMessage::ChallengeAndIdentity(challenge_and_identity) => {
                protobuf::stark_authentication::Message::ChallengeAndIdentity(
                    challenge_and_identity.into(),
                )
            }
            StarkAuthenticationMessage::Signature(signed_challenge_and_identity) => {
                protobuf::stark_authentication::Message::Signature(
                    signed_challenge_and_identity.into(),
                )
            }
        }
    }
}

impl TryFrom<protobuf::StarkAuthentication> for StarkAuthentication {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::StarkAuthentication) -> Result<Self, Self::Error> {
        let message = value.message.ok_or(missing("StarkAuthentication::message"))?;
        let message = message.try_into()?;
        Ok(StarkAuthentication { message })
    }
}

impl From<StarkAuthentication> for protobuf::StarkAuthentication {
    fn from(value: StarkAuthentication) -> Self {
        protobuf::StarkAuthentication { message: Some(value.message.into()) }
    }
}

impl TryFrom<protobuf::ChallengeAndIdentity> for ChallengeAndIdentity {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::ChallengeAndIdentity) -> Result<Self, Self::Error> {
        let staker_address = value
            .staker_address
            .ok_or(missing("ChallengeAndIdentity::staker_address"))?
            .try_into()?;
        let public_key = PublicKey(
            value.public_key.ok_or(missing("ChallengeAndIdentity::public_key"))?.try_into()?,
        );
        let challenge_bytes: [u8; CHALLENGE_LEN] =
            value.challenge.as_slice().try_into().map_err(|_| {
                ProtobufConversionError::BytesDataLengthMismatch {
                    type_description: "Challenge",
                    num_expected: CHALLENGE_LEN,
                    value: value.challenge.clone(),
                }
            })?;
        let challenge = Challenge(challenge_bytes);

        Ok(ChallengeAndIdentity { staker_address, public_key, challenge })
    }
}

impl From<ChallengeAndIdentity> for protobuf::ChallengeAndIdentity {
    fn from(value: ChallengeAndIdentity) -> Self {
        protobuf::ChallengeAndIdentity {
            staker_address: Some(value.staker_address.into()),
            public_key: Some(value.public_key.0.into()),
            challenge: value.challenge.0.to_vec(),
        }
    }
}

impl TryFrom<protobuf::Signature> for Signature {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::Signature) -> Result<Self, Self::Error> {
        let signature = value
            .signature
            .into_iter()
            .map(|felt| felt.try_into())
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Signature { signature })
    }
}

impl From<Signature> for protobuf::Signature {
    fn from(value: Signature) -> Self {
        let signature = value.signature.into_iter().map(Into::into).collect();

        protobuf::Signature { signature }
    }
}
