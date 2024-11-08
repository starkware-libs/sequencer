use rstest::rstest;
use starknet_sequencer_infra::trace_util::configure_tracing;
use tracing::info;

#[rstest]
#[tokio::test]
async fn test_end_to_end_integration() {
    configure_tracing();
    info!("Running integration test setup.");
}
