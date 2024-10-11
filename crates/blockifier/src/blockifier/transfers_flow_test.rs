use rstest::rstest;

use crate::blockifier::config::ConcurrencyConfig;
use crate::test_utils::transfers_generator::{
    RecipientGeneratorType,
    TransfersGenerator,
    TransfersGeneratorConfig,
};

#[rstest]
#[case::concurrency_enabled(ConcurrencyConfig{enabled: true, n_workers: 4, chunk_size: 100})]
#[case::concurrency_disabled(ConcurrencyConfig{enabled: false, n_workers: 0, chunk_size: 0})]
pub fn transfers_flow_test(#[case] concurrency_config: ConcurrencyConfig) {
    let transfers_generator_config = TransfersGeneratorConfig {
        recipient_generator_type: RecipientGeneratorType::DisjointFromSenders,
        concurrency_config,
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
