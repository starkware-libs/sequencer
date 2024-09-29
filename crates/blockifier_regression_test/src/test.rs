use blockifier::blockifier::transaction_executor::TransactionExecutor;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::state::StateDiff;

use crate::regression_test::{BlockifierRegressionTest, BlockifierRegressionTestConfig};
use crate::state_reader::rpc_test::{test_state_reader, tx};
use crate::state_reader::test_state_reader::TestStateReader;

#[fixture]
pub fn config() -> BlockifierRegressionTestConfig {
    BlockifierRegressionTestConfig { to_dump: false, dump_dir: None }
}

#[fixture]
pub fn block_before_execute() -> BlockNumber {
    BlockNumber(699999)
}

#[fixture]
pub fn prev_block_state_reader(block_before_execute: BlockNumber) -> TestStateReader {
    TestStateReader::new_for_testing(block_before_execute)
}

#[fixture]
pub fn rpc_executor(
    prev_block_state_reader: TestStateReader,
    test_state_reader: TestStateReader,
) -> TransactionExecutor<TestStateReader> {
    TestStateReader::get_transaction_executor(prev_block_state_reader, test_state_reader)
        .unwrap_or_else(|err| {
            panic!("Error creating transaction executor: {}", err);
        })
}

#[fixture]
pub fn rpc_blockifier_test(
    config: BlockifierRegressionTestConfig,
    rpc_executor: TransactionExecutor<TestStateReader>,
) -> BlockifierRegressionTest<TestStateReader> {
    BlockifierRegressionTest::new(rpc_executor, config, StateDiff::default())
}

#[rstest]
pub fn test_execute_tx(
    mut rpc_blockifier_test: BlockifierRegressionTest<TestStateReader>,
    tx: BlockifierTransaction,
) {
    let execution_result = rpc_blockifier_test.execute_txs(&[tx]);
    assert!(execution_result.is_ok(), "Transaction execution failed: {:?}", execution_result.err());
}
