use starknet_api::crypto::utils::{Challenge, PublicKey};

use super::common::missing;
use crate::authentication::{
    ChallengeAndIdentity,
    SignedChallengeAndIdentity,
    StarkAuthentication,
    StarkAuthenticationMessage,
};
use crate::converters::ProtobufConversionError;
use crate::protobuf;

macro_rules! try_from_stark_auth_message {
    ($value:expr, $variant:ident, $expected:literal) => {{
        let message = $value.message.ok_or(missing("StarkAuthentication::message"))?;
        match message {
            protobuf::stark_authentication::Message::$variant(inner) => inner.try_into(),
            other => Err(ProtobufConversionError::WrongEnumVariant {
                type_description: "StarkAuthenticationMessage",
                value_as_str: format!("{:?}", other),
                expected: $expected,
            }),
        }
    }};
}

impl TryFrom<protobuf::StarkAuthentication> for ChallengeAndIdentity {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::StarkAuthentication) -> Result<Self, Self::Error> {
        try_from_stark_auth_message!(value, ChallengeAndIdentity, "ChallengeAndIdentity")
    }
}

impl TryFrom<protobuf::StarkAuthentication> for SignedChallengeAndIdentity {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::StarkAuthentication) -> Result<Self, Self::Error> {
        try_from_stark_auth_message!(
            value,
            SignedChallengeAndIdentity,
            "SignedChallengeAndIdentity"
        )
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

impl From<SignedChallengeAndIdentity> for protobuf::StarkAuthentication {
    fn from(value: SignedChallengeAndIdentity) -> Self {
        StarkAuthenticationMessage::SignedChallengeAndIdentity(value).into()
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
            protobuf::stark_authentication::Message::SignedChallengeAndIdentity(
                signed_challenge_and_identity,
            ) => Ok(StarkAuthenticationMessage::SignedChallengeAndIdentity(
                signed_challenge_and_identity.try_into()?,
            )),
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
            StarkAuthenticationMessage::SignedChallengeAndIdentity(
                signed_challenge_and_identity,
            ) => protobuf::stark_authentication::Message::SignedChallengeAndIdentity(
                signed_challenge_and_identity.into(),
            ),
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
        let challenge = Challenge(u128::from(
            value.challenge.ok_or(missing("ChallengeAndIdentity::challenge"))?,
        ));

        Ok(ChallengeAndIdentity { staker_address, public_key, challenge })
    }
}

impl From<ChallengeAndIdentity> for protobuf::ChallengeAndIdentity {
    fn from(value: ChallengeAndIdentity) -> Self {
        protobuf::ChallengeAndIdentity {
            staker_address: Some(value.staker_address.into()),
            public_key: Some(value.public_key.0.into()),
            challenge: Some(value.challenge.0.into()),
        }
    }
}

impl TryFrom<protobuf::SignedChallengeAndIdentity> for SignedChallengeAndIdentity {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::SignedChallengeAndIdentity) -> Result<Self, Self::Error> {
        let signature = value
            .signature
            .into_iter()
            .map(|felt| felt.try_into())
            .collect::<Result<Vec<_>, _>>()?;

        Ok(SignedChallengeAndIdentity { signature })
    }
}

impl From<SignedChallengeAndIdentity> for protobuf::SignedChallengeAndIdentity {
    fn from(value: SignedChallengeAndIdentity) -> Self {
        let signature = value.signature.into_iter().map(Into::into).collect();

        protobuf::SignedChallengeAndIdentity { signature }
    }
}
