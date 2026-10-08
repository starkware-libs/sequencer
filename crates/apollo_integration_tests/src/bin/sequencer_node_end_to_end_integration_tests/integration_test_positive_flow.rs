use apollo_integration_tests::flows;

#[tokio::main]
async fn main() {
    flows::positive::run().await;
}
