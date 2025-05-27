use std::time::Duration;

use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use rstest::rstest;
use strum::IntoEnumIterator;

use super::transaction_executor::BlockExecutionSummary;
use crate::blockifier::config::ConcurrencyConfig;
use crate::test_utils::transfers_generator::{
    RecipientGeneratorType,
    TransfersGenerator,
    TransfersGeneratorConfig,
};
use crate::transaction::objects::TransactionExecutionInfo;

const TIME_FOR_ZERO_TXS: Duration = Duration::ZERO;
const TIME_FOR_ALL_TXS: Duration = Duration::from_secs(100000000);

const N_TXS: usize = 200;

#[rstest]
pub fn transfers_flow_test(
    #[values(None, Some(TIME_FOR_ZERO_TXS), Some(TIME_FOR_ALL_TXS))] timeout: Option<Duration>,
) {
    let mut expected_result = None;
    // Run the transfer test with/without concurrency, with/without Native, and make
    // sure the results are the same.
    for concurrency_enabled in [false, true] {
        for cairo1_version in RunnableCairo1::iter() {
            let result = transfers_flow_test_body(timeout, concurrency_enabled, cairo1_version);
            let Some((expected_tx_execution_infos, expected_block_summary)) = &expected_result
            else {
                expected_result = Some(result);
                continue;
            };
            let (tx_execution_infos, block_summary) = result;
            assert_eq!(
                &tx_execution_infos, expected_tx_execution_infos,
                "Transaction Results differ for concurrency_enabled: {}; cairo1_version: {:?}",
                concurrency_enabled, cairo1_version
            );
            assert_eq!(
                &block_summary, expected_block_summary,
                "Block Results differ for concurrency_enabled: {}; cairo1_version: {:?}",
                concurrency_enabled, cairo1_version
            );
        }
    }
}

pub fn transfers_flow_test_body(
    timeout: Option<Duration>,
    concurrency_enabled: bool,
    cairo1_version: RunnableCairo1,
) -> (Vec<TransactionExecutionInfo>, BlockExecutionSummary) {
    let concurrency_config = ConcurrencyConfig::create_for_testing(concurrency_enabled);
    let transfers_generator_config = TransfersGeneratorConfig {
        // TODO(Yoni): test scenarios with collisions.
        recipient_generator_type: RecipientGeneratorType::DisjointFromSenders,
        concurrency_config,
        cairo_version: CairoVersion::Cairo1(cairo1_version),
        n_txs: N_TXS,
        ..Default::default()
    };
    assert!(
        usize::from(transfers_generator_config.n_accounts)
            >= transfers_generator_config.concurrency_config.chunk_size,
        "The number of accounts must be at least the chunk size. Otherwise, the same account may \
         be used in multiple transactions in the same chunk, making the chunk not fully \
         independent."
    );
    let mut transfers_generator = TransfersGenerator::new(transfers_generator_config);

    let n_results = transfers_generator.execute_transfers(timeout);
    // Check that the number of results is as expected.
    match timeout {
        Some(TIME_FOR_ZERO_TXS) => {
            assert_eq!(n_results, 0);
        }
        Some(TIME_FOR_ALL_TXS) | None => {
            assert_eq!(n_results, N_TXS);
        }
        _ => {
            panic!("Unexpected timeout value: {:?}", timeout);
        }
    }

    transfers_generator.finalize()
}
