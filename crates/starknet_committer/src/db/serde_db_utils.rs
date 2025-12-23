use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_patricia_storage::errors::DeserializationError;
use starknet_patricia_storage::storage_trait::DbValue;
use starknet_types_core::felt::Felt;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Hash, Serialize)]
pub struct DbBlockNumber(pub BlockNumber);

impl DbBlockNumber {
    pub fn serialize(&self) -> [u8; 8] {
        self.0.0.to_be_bytes()
    }

    pub fn deserialize(value: &[u8]) -> Result<Self, DeserializationError> {
        let array_value: [u8; 8] =
            value.try_into().map_err(|error| DeserializationError::ValueError(Box::new(error)))?;
        Ok(Self(BlockNumber(u64::from_be_bytes(array_value))))
    }
}

pub fn serialize_felt(felt: Felt) -> DbValue {
    DbValue(felt.to_bytes_be().to_vec())
}

pub fn deserialize_felt(value: &DbValue) -> Felt {
    Felt::from_bytes_be_slice(&value.0)
}
