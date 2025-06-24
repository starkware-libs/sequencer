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
#[case::zero_txs(Some(TIME_FOR_ZERO_TXS), RecipientGeneratorType::DisjointFromSenders)]
#[case::all_txs_disjoint(Some(TIME_FOR_ALL_TXS), RecipientGeneratorType::DisjointFromSenders)]
#[case::all_txs_round_robin(None, RecipientGeneratorType::RoundRobin)]
pub fn transfers_flow_test(
    #[case] timeout: Option<Duration>,
    #[case] recipient_generator_type: RecipientGeneratorType,
) {
    let mut expected_result = None;
    // Run the transfer test with/without concurrency, with/without Native, and make
    // sure the results are the same.
    for concurrency_enabled in [false, true] {
        for cairo1_version in RunnableCairo1::iter() {
            let mut result = transfers_flow_test_body(
                timeout,
                concurrency_enabled,
                cairo1_version,
                recipient_generator_type,
            );
            for execution_info in &mut result.0 {
                execution_info.clear_call_infos_nonessential_fields_for_comparison();
            }
            let Some((expected_tx_execution_infos, mut expected_block_summary)) =
                expected_result.take()
            else {
                expected_result = Some(result);
                continue;
            };

            let (tx_execution_infos, mut block_summary) = result;

            assert_eq!(
                &tx_execution_infos, &expected_tx_execution_infos,
                "Transaction Results differ for concurrency_enabled: {}; cairo1_version: {:?}",
                concurrency_enabled, cairo1_version
            );

            block_summary.clear_nonessential_fields_for_comparison();
            expected_block_summary.clear_nonessential_fields_for_comparison();
            use pretty_assertions::assert_eq;
            assert_eq!(
                &block_summary, &expected_block_summary,
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
    recipient_generator_type: RecipientGeneratorType,
) -> (Vec<TransactionExecutionInfo>, BlockExecutionSummary) {
    let concurrency_config = ConcurrencyConfig::create_for_testing(concurrency_enabled);
    let transfers_generator_config = TransfersGeneratorConfig {
        recipient_generator_type,
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

    let (block_summary, tx_execution_infos) =
        transfers_generator.execute_block_of_transfers(timeout);
    let n_results = tx_execution_infos.len();
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

    (tx_execution_infos, block_summary)
}
