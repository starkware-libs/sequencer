use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use rstest::{fixture, rstest};
use starknet_integration_tests::end_to_end_integration::end_to_end_integration;
use starknet_integration_tests::utils::{
    create_integration_test_tx_generator,
    run_integration_test,
};
use starknet_sequencer_infra::trace_util::configure_tracing;

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    create_integration_test_tx_generator()
}

#[rstest]
#[tokio::test]
async fn test_end_to_end_integration(tx_generator: MultiAccountTransactionGenerator) {
    if !run_integration_test() {
        return;
    }
    configure_tracing();
    end_to_end_integration(tx_generator).await;
}
