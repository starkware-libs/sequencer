use rstest::rstest;

use crate::blockifier::config::ConcurrencyConfig;
use crate::test_utils::transfers_generator::{
    RecipientGeneratorType,
    TransfersGenerator,
    TransfersGeneratorConfig,
};
use crate::test_utils::{CairoVersion, RunnableCairo1};

#[rstest]
#[case::cairo1(CairoVersion::Cairo1(RunnableCairo1::Casm))]
#[cfg_attr(
    feature = "cairo_native",
    case::cairo1_native(CairoVersion::Cairo1(RunnableCairo1::Native))
)]
pub fn transfers_flow_test(
    #[values(true, false)] concurrency_enabled: bool,
    #[case] cairo_version: CairoVersion,
) {
    let concurrency_config = ConcurrencyConfig::create_for_testing(concurrency_enabled);
    let transfers_generator_config = TransfersGeneratorConfig {
        recipient_generator_type: RecipientGeneratorType::DisjointFromSenders,
        concurrency_config,
        cairo_version,
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
    transfers_generator.execute_transfers();
}
