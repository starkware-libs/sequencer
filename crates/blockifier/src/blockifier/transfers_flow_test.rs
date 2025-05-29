use std::time::Duration;

use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use rstest::rstest;

use crate::blockifier::config::ConcurrencyConfig;
use crate::test_utils::transfers_generator::{
    RecipientGeneratorType,
    TransfersGenerator,
    TransfersGeneratorConfig,
};

const TIME_FOR_ZERO_TXS: Duration = Duration::ZERO;
const TIME_FOR_ALL_TXS: Duration = Duration::from_secs(100000000);

const N_TXS: usize = 200;

#[rstest]
#[case::cairo1(CairoVersion::Cairo1(RunnableCairo1::Casm))]
#[cfg_attr(
    feature = "cairo_native",
    case::cairo1_native(CairoVersion::Cairo1(RunnableCairo1::Native))
)]
pub fn transfers_flow_test(
    #[values(None, Some(TIME_FOR_ZERO_TXS), Some(TIME_FOR_ALL_TXS))] timeout: Option<Duration>,
    #[values(true, false)] concurrency_enabled: bool,
    #[values(RecipientGeneratorType::DisjointFromSenders, RecipientGeneratorType::RoundRobin)]
    recipient_generator_type: RecipientGeneratorType,
    #[case] cairo_version: CairoVersion,
) {
    let concurrency_config = ConcurrencyConfig::create_for_testing(concurrency_enabled);
    let transfers_generator_config = TransfersGeneratorConfig {
        recipient_generator_type,
        concurrency_config,
        cairo_version,
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
}
