use prost::Message;

use super::common::missing;
use crate::authentication::{Challenge, SignedChallengeAndIdentity, StakerAddress};
use crate::converters::ProtobufConversionError;
use crate::{auto_impl_into_and_try_from_vec_u8, protobuf};

impl TryFrom<protobuf::StakerAddress> for StakerAddress {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::StakerAddress) -> Result<Self, Self::Error> {
        let staker_address =
            value.staker_address.ok_or(missing("StakerAddress::staker_address"))?.try_into()?;

        Ok(StakerAddress { staker_address })
    }
}

impl From<StakerAddress> for protobuf::StakerAddress {
    fn from(value: StakerAddress) -> Self {
        protobuf::StakerAddress { staker_address: Some(value.staker_address.into()) }
    }
}

auto_impl_into_and_try_from_vec_u8!(StakerAddress, protobuf::StakerAddress);

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
