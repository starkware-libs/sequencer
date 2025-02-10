use starknet_integration_tests::integration_test_utils::integration_test_setup;

#[tokio::main]
async fn main() {
    integration_test_setup("reset_flow").await;
    // TODO(noamsp): Add the reset flow test.
}
