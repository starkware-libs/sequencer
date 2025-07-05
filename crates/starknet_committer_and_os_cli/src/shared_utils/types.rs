use std::fmt::Debug;

use clap::Args;
use starknet_os::errors::StarknetOsError;

use crate::shared_utils::read::{read_input, write_to_file};

pub(crate) type PythonTestResult<E> = Result<String, PythonTestError<E>>;

#[derive(Debug, Args)]
pub(crate) struct IoArgs {
    /// File path to input.
    #[clap(long, short = 'i')]
    pub(crate) input_path: String,

    /// File path to output.
    #[clap(long, short = 'o', default_value = "stdout")]
    pub(crate) output_path: String,
}

#[derive(Debug, Args)]
pub(crate) struct PythonTestArg {
    // TODO(Amos): Make this optional.
    #[clap(flatten)]
    pub(crate) io_args: IoArgs,

    /// Test name.
    #[clap(long)]
    pub(crate) test_name: String,
}

/// Error type for PythonTest enum.
#[derive(Debug, thiserror::Error)]
pub enum PythonTestError<E> {
    #[error("Unknown test name: {0}")]
    UnknownTestName(String),
    #[error(transparent)]
    ParseInputError(#[from] serde_json::Error),
    #[error("None value found in input.")]
    NoneInputError,
    #[error(transparent)]
    SpecificError(E),
    #[error(transparent)]
    StarknetOs(#[from] StarknetOsError),
}

pub(crate) trait PythonTestRunner: TryFrom<String> {
    type SpecificError: Debug;

    /// Returns the input string if it's `Some`, or an error if it's `None`.
    fn non_optional_input(
        input: Option<&str>,
    ) -> Result<&str, PythonTestError<Self::SpecificError>> {
        input.ok_or(PythonTestError::NoneInputError)
    }

    async fn run(&self, input: Option<&str>) -> PythonTestResult<Self::SpecificError>;
}

pub(crate) async fn run_python_test<PT: PythonTestRunner>(python_test_arg: PythonTestArg)
where
    <PT as TryFrom<String>>::Error: Debug,
{
    // Create PythonTest from test_name.
    let test = PT::try_from(python_test_arg.test_name)
        .unwrap_or_else(|error| panic!("Failed to create PythonTest: {error:?}"));
    let input = read_input(python_test_arg.io_args.input_path);

    // Run relevant test.
    let output = test
        .run(Some(&input))
        .await
        .unwrap_or_else(|error| panic!("Failed to run test: {error:?}"));

    // Write test's output.
    write_to_file(&python_test_arg.io_args.output_path, &output);
}
