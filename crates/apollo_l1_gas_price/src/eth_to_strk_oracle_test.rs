use apollo_l1_gas_price_types::EthToStrkOracleClientTrait;
use serde_json::json;
use tokio;
use url::Url;

use crate::eth_to_strk_oracle::EthToStrkOracleClient;

#[tokio::test]
async fn eth_to_fri_rate() {
    let expected_rate = 123456;
    let expected_rate_hex = format!("0x{:x}", expected_rate);
    let timestamp = 1234567890;
    // Create a mock HTTP server.
    let mut server = mockito::Server::new_async().await;

    // Define a mock response for a GET request with a specific timestamp in the path
    let _m = server
        .mock("GET", format!("/{}", timestamp).as_str())
        .with_header("Content-Type", "application/json")
        .with_body(
            json!({
                "price": expected_rate_hex,
                "decimals": 18
            })
            .to_string(),
        )
        .create();

    // Construct the base URL from the mock server
    let base_url = Url::parse(&server.url()).unwrap();

    let client = EthToStrkOracleClient::new(base_url, None, 0);
    let rate = client.eth_to_fri_rate(timestamp).await.unwrap();

    assert_eq!(rate, expected_rate);
}
