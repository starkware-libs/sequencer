use blake2s::encode_felts_to_u32s;
use starknet_os::test_utils::errors::OsSpecificTestError;
use starknet_types_core::felt::Felt;

use crate::os_cli::commands::{validate_os_input, AggregatorCliInput, OsCliInput};
use crate::os_cli::tests::types::{OsPythonTestError, OsPythonTestResult};
use crate::shared_utils::types::{PythonTestError, PythonTestRunner};

// Enum representing different Python tests.
pub enum OsPythonTestRunner {
    AggregatorInputDeserialization,
    OsInputDeserialization,
    EncodeFelts,
}

// Implements conversion from a string to the test runner.
impl TryFrom<String> for OsPythonTestRunner {
    type Error = OsPythonTestError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "aggregator_input_deserialization" => Ok(Self::AggregatorInputDeserialization),
            "os_input_deserialization" => Ok(Self::OsInputDeserialization),
            "encode_felts" => Ok(Self::EncodeFelts),
            _ => Err(PythonTestError::UnknownTestName(value)),
        }
    }
}

impl PythonTestRunner for OsPythonTestRunner {
    type SpecificError = OsSpecificTestError;
    async fn run(&self, input: Option<&str>) -> OsPythonTestResult {
        match self {
            Self::AggregatorInputDeserialization => {
                aggregator_input_deserialization(Self::non_optional_input(input)?)
            }
            Self::OsInputDeserialization => {
                os_input_deserialization(Self::non_optional_input(input)?)
            }
            Self::EncodeFelts => {
                let felts: Vec<Felt> = serde_json::from_str(Self::non_optional_input(input)?)?;
                Ok(format!("{:?}", encode_felts_to_u32s(felts)))
            }
        }
    }
}

/// Deserialize the OS input string into an `OsInput` struct.
fn os_input_deserialization(input_str: &str) -> OsPythonTestResult {
    let input = serde_json::from_str::<OsCliInput>(input_str)?;
    validate_os_input(&input.os_hints.os_input);
    Ok("Deserialization successful".to_string())
}

/// Deserialize the aggregator input string into an `AggregatorInput` struct.
fn aggregator_input_deserialization(input_str: &str) -> OsPythonTestResult {
    let _input = serde_json::from_str::<AggregatorCliInput>(input_str)?;
    // TODO(Aner): Validate the aggregator input.
    Ok("Deserialization successful".to_string())
}
