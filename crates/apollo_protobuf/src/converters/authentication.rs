use prost::Message;
use starknet_api::crypto::utils::{Challenge, PublicKey};

use super::common::missing;
use crate::authentication::{ChallengeAndIdentity, SignedChallengeAndIdentity};
use crate::converters::ProtobufConversionError;
use crate::{auto_impl_into_and_try_from_vec_u8, protobuf};

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
        let challenge = Challenge(value.challenge);

        Ok(ChallengeAndIdentity { staker_address, public_key, challenge })
    }
}

impl From<ChallengeAndIdentity> for protobuf::ChallengeAndIdentity {
    fn from(value: ChallengeAndIdentity) -> Self {
        protobuf::ChallengeAndIdentity {
            staker_address: Some(value.staker_address.into()),
            public_key: Some(value.public_key.0.into()),
            challenge: value.challenge.0,
        }
    }
}

auto_impl_into_and_try_from_vec_u8!(ChallengeAndIdentity, protobuf::ChallengeAndIdentity);

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
