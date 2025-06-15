use starknet_os::test_utils::errors::OsSpecificTestError;

use crate::shared_utils::types::{PythonTestError, PythonTestResult};

pub type OsPythonTestError = PythonTestError<OsSpecificTestError>;
pub type OsPythonTestResult = PythonTestResult<OsSpecificTestError>;
