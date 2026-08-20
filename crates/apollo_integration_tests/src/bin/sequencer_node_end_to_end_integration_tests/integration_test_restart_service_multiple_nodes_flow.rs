use apollo_integration_tests::flows;

#[tokio::main]
async fn main() {
    flows::restart_multiple_nodes::run().await;
}
