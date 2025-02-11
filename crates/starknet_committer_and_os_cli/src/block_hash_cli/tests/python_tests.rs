use starknet_api::block_hash::block_hash_calculator::{
    TransactionHashingData,
    TransactionOutputForHash,
};
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionExecutionStatus;

use crate::block_hash_cli::tests::objects::{
    get_thin_state_diff,
    get_transaction_output_for_hash,
    get_tx_data,
};
use crate::shared_utils::types::{PythonTestError, PythonTestResult, PythonTestRunner};

pub type BlockHashPythonTestError = PythonTestError<()>;
pub type BlockHashPythonTestResult = PythonTestResult<()>;

// Enum representing different Python tests.
pub enum BlockHashPythonTestRunner {
    ParseTxOutput,
    ParseStateDiff,
    ParseTxData,
}

/// Implements conversion from a string to the test runner.
impl TryFrom<String> for BlockHashPythonTestRunner {
    type Error = BlockHashPythonTestError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "parse_tx_output_test" => Ok(Self::ParseTxOutput),
            "parse_state_diff_test" => Ok(Self::ParseStateDiff),
            "parse_tx_data_test" => Ok(Self::ParseTxData),
            _ => Err(PythonTestError::UnknownTestName(value)),
        }
    }
}

impl PythonTestRunner for BlockHashPythonTestRunner {
    type SpecificError = ();
    /// Runs the test with the given arguments.
    async fn run(&self, input: Option<&str>) -> BlockHashPythonTestResult {
        match self {
            Self::ParseTxOutput => {
                let tx_output: TransactionOutputForHash =
                    serde_json::from_str(Self::non_optional_input(input)?)?;
                Ok(parse_tx_output_test(tx_output))
            }
            Self::ParseStateDiff => {
                let tx_output: ThinStateDiff =
                    serde_json::from_str(Self::non_optional_input(input)?)?;
                Ok(parse_state_diff_test(tx_output))
            }
            Self::ParseTxData => {
                let tx_data: TransactionHashingData =
                    serde_json::from_str(Self::non_optional_input(input)?)?;
                Ok(parse_tx_data_test(tx_data))
            }
        }
    }
}

pub(crate) fn parse_tx_output_test(tx_execution_info: TransactionOutputForHash) -> String {
    let expected_object = get_transaction_output_for_hash(&tx_execution_info.execution_status);
    is_success_string(expected_object == tx_execution_info)
}

pub(crate) fn parse_state_diff_test(state_diff: ThinStateDiff) -> String {
    let expected_object = get_thin_state_diff();
    is_success_string(expected_object == state_diff)
}

pub(crate) fn parse_tx_data_test(tx_data: TransactionHashingData) -> String {
    let expected_object = get_tx_data(&TransactionExecutionStatus::Succeeded);
    is_success_string(expected_object == tx_data)
}

fn is_success_string(is_success: bool) -> String {
    match is_success {
        true => "Success",
        false => "Failure",
    }
    .to_owned()
}
