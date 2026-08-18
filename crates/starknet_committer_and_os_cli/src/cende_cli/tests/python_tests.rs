use apollo_consensus_orchestrator::cende::python_compat::parse_accessed_keys_input_test;

use crate::shared_utils::types::{PythonTestError, PythonTestResult, PythonTestRunner};

pub type CendePythonTestError = PythonTestError<()>;
pub type CendePythonTestResult = PythonTestResult<()>;

/// Enum representing the cende Python tests.
pub enum CendePythonTestRunner {
    ParseAccessedKeysInput,
}

impl TryFrom<String> for CendePythonTestRunner {
    type Error = CendePythonTestError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "parse_accessed_keys_input_test" => Ok(Self::ParseAccessedKeysInput),
            _ => Err(PythonTestError::UnknownTestName(value)),
        }
    }
}

impl PythonTestRunner for CendePythonTestRunner {
    type SpecificError = ();

    async fn run(&self, input: Option<&str>) -> CendePythonTestResult {
        match self {
            Self::ParseAccessedKeysInput => {
                Ok(parse_accessed_keys_input_test(Self::non_optional_input(input)?)?)
            }
        }
    }
}
