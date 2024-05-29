use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;

use crate::StarknetApiError;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(try_from = "Deserializer")]
pub enum DataAvailabilityMode {
    L1 = 0,
    L2 = 1,
}

/// Deserialize a `DataAvailabilityMode` from a given `Deserializer`.
///
/// This implementation supports deserializing the `DataAvailabilityMode` enum from both numerical
/// and textual representations.
#[derive(Deserialize)]
#[serde(untagged)]
enum Deserializer {
    Num(u8),
    Text(String),
}

impl TryFrom<Deserializer> for DataAvailabilityMode {
    type Error = StarknetApiError;

    fn try_from(value: Deserializer) -> Result<Self, Self::Error> {
        match value {
            Deserializer::Num(0_u8) => Ok(DataAvailabilityMode::L1),
            Deserializer::Num(1_u8) => Ok(DataAvailabilityMode::L2),
            Deserializer::Text(text) if &text == "L1" => Ok(DataAvailabilityMode::L1),
            Deserializer::Text(text) if &text == "L2" => Ok(DataAvailabilityMode::L2),
            _ => Err(StarknetApiError::OutOfRange {
                string: "Data availability must be either 'L1' or '0' for L1, or 'L2' or '1' for \
                         L2."
                .to_string(),
            }),
        }
    }
}

impl TryFrom<Felt> for DataAvailabilityMode {
    type Error = StarknetApiError;

    fn try_from(felt: Felt) -> Result<Self, StarknetApiError> {
        if felt == Felt::ZERO {
            return Ok(DataAvailabilityMode::L1);
        }
        if felt == Felt::ONE {
            return Ok(DataAvailabilityMode::L2);
        }
        Err(StarknetApiError::OutOfRange {
            string: format!("Invalid data availability mode: {felt}."),
        })
    }
}

impl From<DataAvailabilityMode> for Felt {
    fn from(data_availability_mode: DataAvailabilityMode) -> Felt {
        match data_availability_mode {
            DataAvailabilityMode::L1 => Felt::ZERO,
            DataAvailabilityMode::L2 => Felt::ONE,
        }
    }
}

#[derive(
    Clone, Default, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum L1DataAvailabilityMode {
    #[default]
    Calldata,
    Blob,
}
