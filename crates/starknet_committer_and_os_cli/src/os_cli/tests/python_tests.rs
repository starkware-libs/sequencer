use starknet_os::hints::enum_definition::{Hint, HintExtension};
use strum_macros::Display;
use thiserror;

use crate::shared_utils::types::{PythonTestError, PythonTestRunner};

pub type OsPythonTestError = PythonTestError<OsSpecificTestError>;

// Enum representing different Python tests.
pub enum OsPythonTestRunner {
    HintCompatibility,
}

/// Implements conversion from a string to the test runner.
impl TryFrom<String> for OsPythonTestRunner {
    type Error = OsPythonTestError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "hint_compatibility_test" => Ok(Self::HintCompatibility),
            _ => Err(PythonTestError::UnknownTestName(value)),
        }
    }
}

#[derive(Debug, thiserror::Error, Display)]
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
                let hint_strings =
                    Hint::iter().map(|hint| hint.to_str()).collect::<Vec<_>>().sort();
                let hint_extension_strings =
                    HintExtension::iter().map(|hint| hint.to_str()).collect::<Vec<_>>().sort();
                Ok(hint_strings.join("\n") + "\n" + hint_extension_strings.join("\n"))
            }
        }
    }
}
