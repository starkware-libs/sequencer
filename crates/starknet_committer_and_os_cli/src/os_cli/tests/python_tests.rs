use crate::shared_utils::types::{PythonTestError, PythonTestRunner};
use thiserror;

pub type OsPythonTestError = PythonTestError<OsSpecificTestError>;

// Enum representing different Python tests.
pub enum OsPythonTestRunner {
    HintCompatibility,
}

/// Implements conversion from a string to the test runner.
impl TryFrom<String> for OsPythonTestError {
    type Error = OsPythonTestError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "hint_compatibility_test" => Ok(Self::HintCompatibility),
            _ => Err(PythonTestError::UnknownTestName(value)),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OsSpecificTestError {
    HintMismatchError,
}

impl PythonTestRunner for OsPythonTestRunner {
    type SpecificError = OsPythonTestError;
    /// Runs the test with the given arguments.
    async fn run(&self, input: Option<&str>) -> Result<String, OsPythonTestError> {
        match self {
            // FIXME: Implement the test runner for the hint compatibility test.
            Self::HintCompatibility => {
                let tx_data: TransactionHashingData =
                    serde_json::from_str(Self::non_optional_input(input)?)?;
                Ok(parse_tx_data_test(tx_data))
            }
        }
    }
}

fn test_hints_match(python_side_hints: set<str>, rust_side_hints: set<str>) {}
