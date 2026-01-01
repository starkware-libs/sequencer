use prost::Message;

use super::common::missing;
use crate::authentication::{Challenge, PublicKey, SignedChallengeAndIdentity};
use crate::converters::ProtobufConversionError;
use crate::{auto_impl_into_and_try_from_vec_u8, protobuf};

impl TryFrom<protobuf::PublicKey> for PublicKey {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::PublicKey) -> Result<Self, Self::Error> {
        let public_key = value.public_key.ok_or(missing("PublicKey::public_key"))?.try_into()?;

        Ok(PublicKey { public_key })
    }
}

impl From<PublicKey> for protobuf::PublicKey {
    fn from(value: PublicKey) -> Self {
        protobuf::PublicKey { public_key: Some(value.public_key.into()) }
    }
}

auto_impl_into_and_try_from_vec_u8!(PublicKey, protobuf::PublicKey);

impl TryFrom<protobuf::Challenge> for Challenge {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::Challenge) -> Result<Self, Self::Error> {
        Ok(Challenge { challenge: value.challenge })
    }
}

impl From<Challenge> for protobuf::Challenge {
    fn from(value: Challenge) -> Self {
        protobuf::Challenge { challenge: value.challenge }
    }
}

auto_impl_into_and_try_from_vec_u8!(Challenge, protobuf::Challenge);

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

auto_impl_into_and_try_from_vec_u8!(
    SignedChallengeAndIdentity,
    protobuf::SignedChallengeAndIdentity
);
