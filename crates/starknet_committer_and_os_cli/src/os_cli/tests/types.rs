use starknet_os::hints::error::OsHintError;
use starknet_os::test_utils::errors::Cairo0EntryPointRunnerError;
use strum::Display;

use crate::shared_utils::types::{PythonTestError, PythonTestResult};

pub type OsPythonTestError = PythonTestError<OsSpecificTestError>;
pub type OsPythonTestResult = PythonTestResult<OsSpecificTestError>;

#[derive(Debug, thiserror::Error, Display)]
pub enum OsSpecificTestError {
    Cairo0EntryPointRunner(#[from] Cairo0EntryPointRunnerError),
    OsHintError(#[from] OsHintError),
}
