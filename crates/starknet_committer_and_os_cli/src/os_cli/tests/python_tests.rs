use blake2s::encode_felts_to_u32s;
use starknet_os::test_utils::errors::OsSpecificTestError;
use starknet_types_core::felt::Felt;

use crate::os_cli::commands::{validate_os_input, AggregatorCliInput, OsCliInput};
use crate::os_cli::tests::aliases::aliases_test;
use crate::os_cli::tests::bls_field::test_bls_field;
use crate::os_cli::tests::types::{OsPythonTestError, OsPythonTestResult};
use crate::shared_utils::types::{PythonTestError, PythonTestRunner};

// Enum representing different Python tests.
pub enum OsPythonTestRunner {
    AliasesTest,
    AggregatorInputDeserialization,
    BlsFieldTest,
    OsInputDeserialization,
    EncodeFelts,
}

// Implements conversion from a string to the test runner.
impl TryFrom<String> for OsPythonTestRunner {
    type Error = OsPythonTestError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "aliases_test" => Ok(Self::AliasesTest),
            "aggregator_input_deserialization" => Ok(Self::AggregatorInputDeserialization),
            "bls_field_test" => Ok(Self::BlsFieldTest),
            "os_input_deserialization" => Ok(Self::OsInputDeserialization),
            "encode_felts" => Ok(Self::EncodeFelts),
            _ => Err(PythonTestError::UnknownTestName(value)),
        }
    }
}

impl PythonTestRunner for OsPythonTestRunner {
    type SpecificError = OsSpecificTestError;
    #[allow(clippy::result_large_err)]
    async fn run(&self, input: Option<&str>) -> OsPythonTestResult {
        match self {
            Self::AliasesTest => aliases_test(Self::non_optional_input(input)?),
            Self::AggregatorInputDeserialization => {
                aggregator_input_deserialization(Self::non_optional_input(input)?)
            }
            Self::BlsFieldTest => test_bls_field(Self::non_optional_input(input)?),
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
#[allow(clippy::result_large_err)]
fn os_input_deserialization(input_str: &str) -> OsPythonTestResult {
    let input = serde_json::from_str::<OsCliInput>(input_str)?;
    validate_os_input(&input.os_hints.os_input);
    Ok("Deserialization successful".to_string())
}

/// Deserialize the aggregator input string into an `AggregatorInput` struct.
#[allow(clippy::result_large_err)]
fn aggregator_input_deserialization(input_str: &str) -> OsPythonTestResult {
    let _input = serde_json::from_str::<AggregatorCliInput>(input_str)?;
    // TODO(Aner): Validate the aggregator input.
    Ok("Deserialization successful".to_string())
}
