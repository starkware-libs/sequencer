use starknet_os::hints::enum_definition::{Hint, HintExtension};
use starknet_os::hints::types::HintEnum;
use strum::IntoEnumIterator;
use strum_macros::Display;
use thiserror;

use crate::shared_utils::types::{PythonTestError, PythonTestResult, PythonTestRunner};

pub type OsPythonTestError = PythonTestError<OsSpecificTestError>;
type OsPythonTestResult = PythonTestResult<OsSpecificTestError>;

// Enum representing different Python tests.
pub enum OsPythonTestRunner {
    OutputAllHints,
}

// Implements conversion from a string to the test runner.
impl TryFrom<String> for OsPythonTestRunner {
    type Error = OsPythonTestError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "hint_compatibility_test" => Ok(Self::OutputAllHints),
            _ => Err(PythonTestError::UnknownTestName(value)),
        }
    }
}

#[derive(Debug, thiserror::Error, Display)]
pub enum OsSpecificTestError {
    PlaceHolder,
}

impl PythonTestRunner for OsPythonTestRunner {
    type SpecificError = OsSpecificTestError;
    async fn run(&self, input: Option<&str>) -> OsPythonTestResult {
        match self {
            Self::OutputAllHints => output_all_hints(input).await,
        }
    }
}

async fn output_all_hints(input: Option<&str>) -> OsPythonTestResult {
    assert!(input.is_none(), "No input is expected for hint compatibility test.");
    let mut hint_strings = Hint::iter()
        .map(|hint| hint.to_str())
        .chain(HintExtension::iter().map(|hint| hint.to_str()))
        .collect::<Vec<_>>();
    hint_strings.sort();
    Ok(serde_json::to_string(&hint_strings)?)
}
