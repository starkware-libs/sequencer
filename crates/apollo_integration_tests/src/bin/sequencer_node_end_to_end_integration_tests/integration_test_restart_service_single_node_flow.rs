use apollo_integration_tests::flows;

#[tokio::main]
async fn main() {
    flows::restart_single_node::run().await;
}
