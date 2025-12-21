use std::collections::BTreeMap;
use std::time::Duration;

use apollo_config::converters::UrlAndHeaders;
use apollo_l1_gas_price_types::errors::EthToStrkOracleClientError;
use apollo_l1_gas_price_types::EthToStrkOracleClientTrait;
use mockito::{Mock, ServerGuard};
use serde_json::json;
use tokio::{self};
use url::Url;

use crate::eth_to_strk_oracle::{EthToStrkOracleClient, EthToStrkOracleConfig};

async fn make_server(server: &mut ServerGuard, body: serde_json::Value) -> Mock {
    server
        .mock("GET", mockito::Matcher::Any) // Match any GET request.
        .with_header("Content-Type", "application/json")
        .with_body(body.to_string())
        .create()
}

#[tokio::test]
async fn eth_to_fri_rate_uses_cache_on_quantized_hit() {
    const EXPECTED_RATE: u128 = 123456;
    let expected_rate_hex = format!("0x{EXPECTED_RATE:x}");
    const TIMESTAMP1: u64 = 1234567890;
    const TIMESTAMP_OFFSET: u64 = 10;
    // Still in the same quantized bucket.
    const TIMESTAMP2: u64 = TIMESTAMP1 + TIMESTAMP_OFFSET;
    const LAG_INTERVAL_SECONDS: u64 = 60;

    let quantized_timestamp = (TIMESTAMP1 - LAG_INTERVAL_SECONDS) / LAG_INTERVAL_SECONDS;
    let adjusted_timestamp = quantized_timestamp * LAG_INTERVAL_SECONDS;

    let mut server = mockito::Server::new_async().await;

    // Define a mock response for a GET request with a specific adjusted_timestamp in the path
    let _mock_response = server
        .mock("GET", "/") // Match the base path only.
        .match_query(mockito::Matcher::UrlEncoded("timestamp".into(), adjusted_timestamp.to_string()))
        .with_header("Content-Type", "application/json")
        .with_body(
            json!({
                "price": expected_rate_hex,
                "decimals": 18
            })
            .to_string(),
        )
        .create();
    let url_and_headers = UrlAndHeaders {
        url: Url::parse(&server.url()).unwrap(),
        headers: BTreeMap::new(), // No additional headers needed for this test.
    };
    let url_header_list = Some(vec![url_and_headers.into()]);
    let config = EthToStrkOracleConfig {
        url_header_list,
        lag_interval_seconds: LAG_INTERVAL_SECONDS,
        ..Default::default()
    };
    let client = EthToStrkOracleClient::new(config.clone());

    // First request should fail because the cache is empty.
    assert!(client.eth_to_fri_rate(TIMESTAMP1).await.is_err());
    // Wait for the query to resolve.
    while client.eth_to_fri_rate(TIMESTAMP1).await.is_err() {
        tokio::task::yield_now().await; // Don't block the executor.
    }
    let rate1 = client.eth_to_fri_rate(TIMESTAMP1).await.unwrap();
    let rate2 = client
        .eth_to_fri_rate(TIMESTAMP2)
        .await
        .expect("Should resolve immediately due to the cache");
    assert_eq!(rate1, rate2);
}

#[tokio::test]
async fn eth_to_fri_rate_uses_prev_cache_when_query_not_ready() {
    const EXPECTED_RATE: u128 = 123456;
    let expected_rate_hex = format!("0x{EXPECTED_RATE:x}");
    let different_rate = EXPECTED_RATE * 2;
    let different_rate_hex = format!("0x{:x}", different_rate);
    const LAG_INTERVAL_SECONDS: u64 = 60;

    const TIMESTAMP1: u64 = 1234567890;
    const TIMESTAMP2: u64 = TIMESTAMP1 + LAG_INTERVAL_SECONDS;

    let quantized_timestamp1 = (TIMESTAMP1 - LAG_INTERVAL_SECONDS) / LAG_INTERVAL_SECONDS;
    let adjusted_timestamp1 = quantized_timestamp1 * LAG_INTERVAL_SECONDS;
    let quantized_timestamp2 = (TIMESTAMP2 - LAG_INTERVAL_SECONDS) / LAG_INTERVAL_SECONDS;
    let adjusted_timestamp2 = quantized_timestamp2 * LAG_INTERVAL_SECONDS;

    let mut server = mockito::Server::new_async().await;

    // Define a mock response for a GET request with a specific adjusted_timestamp in the path
    let _mock_response1 = server
        .mock("GET", "/") // Match the base path only.
        .match_query(mockito::Matcher::UrlEncoded("timestamp".into(), adjusted_timestamp1.to_string()))
        .with_header("Content-Type", "application/json")
        .with_body(
            json!({
                "price": expected_rate_hex,
                "decimals": 18
            })
            .to_string(),
        )
        .create();
    // Second response (same matcher) returns a different value on the next call.
    let _mock_response2 = server
        .mock("GET", "/")
        .match_query(mockito::Matcher::UrlEncoded(
            "timestamp".into(),
            adjusted_timestamp2.to_string(),
        ))
        .with_header("Content-Type", "application/json")
        .with_body(
            json!({
                "price": different_rate_hex,
                "decimals": 18
            })
            .to_string(),
        )
        .create();

    let url_and_headers = UrlAndHeaders {
        url: Url::parse(&server.url()).unwrap(),
        headers: BTreeMap::new(), // No additional headers needed for this test.
    };
    let url_header_list = Some(vec![url_and_headers.into()]);
    let config = EthToStrkOracleConfig {
        url_header_list,
        lag_interval_seconds: LAG_INTERVAL_SECONDS,
        ..Default::default()
    };
    let client = EthToStrkOracleClient::new(config.clone());

    // First request should fail because the cache is empty.
    assert!(client.eth_to_fri_rate(TIMESTAMP1).await.is_err());
    // Wait for the query to resolve.
    while client.eth_to_fri_rate(TIMESTAMP1).await.is_err() {
        tokio::task::yield_now().await; // Don't block the executor.
    }
    let rate1 = client.eth_to_fri_rate(TIMESTAMP1).await.unwrap();
    assert_eq!(rate1, EXPECTED_RATE);
    // Second request should resolve immediately due to the cache.
    let rate2 = client.eth_to_fri_rate(TIMESTAMP2).await.unwrap();
    assert_eq!(rate2, EXPECTED_RATE);

    // Wait for the query to resolve, and the price to be updated.
    for _ in 0..100 {
        let current_rate = client.eth_to_fri_rate(TIMESTAMP2).await.unwrap();
        if current_rate > EXPECTED_RATE {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    // Third request should already successfully get the query from the server.
    let rate3 = client.eth_to_fri_rate(TIMESTAMP2).await.unwrap();
    assert_eq!(rate3, different_rate);
}

#[tokio::test]
async fn eth_to_fri_rate_clears_cache_when_config_changes() {
    const EXPECTED_RATE: u128 = 123456;
    let expected_rate_hex = format!("0x{EXPECTED_RATE:x}");
    const LAG_INTERVAL_SECONDS: u64 = 60;
    const MAX_CACHE_SIZE: usize = 100;
    const QUERY_TIMEOUT_SEC: u64 = 10;

    const TIMESTAMP: u64 = 1234567890;

    let mut server = mockito::Server::new_async().await;

    // Define a mock response for a GET request with the expected rate.
    let _mock_response1 = server
        .mock("GET", "/") // Match the base path only.
        .match_query(mockito::Matcher::Any)
        .with_header("Content-Type", "application/json")
        .with_body(
            json!({
                "price": expected_rate_hex,
                "decimals": 18
            })
            .to_string(),
        )
        .create();

    let url_and_headers = UrlAndHeaders {
        url: Url::parse(&server.url()).unwrap(),
        headers: BTreeMap::new(), // No additional headers needed for this test.
    };
    let url_header_list = Some(vec![url_and_headers.into()]);
    let config = EthToStrkOracleConfig {
        url_header_list: url_header_list.clone(),
        lag_interval_seconds: LAG_INTERVAL_SECONDS,
        max_cache_size: MAX_CACHE_SIZE,
        query_timeout_sec: QUERY_TIMEOUT_SEC,
    };
    let mut client = EthToStrkOracleClient::new(config.clone());

    // First request should fail because the cache is empty.
    assert!(client.eth_to_fri_rate(TIMESTAMP).await.is_err());
    // Wait for the query to resolve.
    while client.eth_to_fri_rate(TIMESTAMP).await.is_err() {
        tokio::task::yield_now().await; // Don't block the executor.
    }
    let rate1 = client.eth_to_fri_rate(TIMESTAMP).await.unwrap();
    assert_eq!(rate1, EXPECTED_RATE);

    // Update the config to a new lag interval.
    let new_config = EthToStrkOracleConfig {
        url_header_list: url_header_list.clone(),
        lag_interval_seconds: LAG_INTERVAL_SECONDS * 2,
        max_cache_size: MAX_CACHE_SIZE,
        query_timeout_sec: QUERY_TIMEOUT_SEC,
    };
    client.update_dynamic_config(new_config);

    // Again the first request should fail because the cache has been cleared.
    assert!(client.eth_to_fri_rate(TIMESTAMP).await.is_err());
    // Wait for the query to resolve.
    while client.eth_to_fri_rate(TIMESTAMP).await.is_err() {
        tokio::task::yield_now().await; // Don't block the executor.
    }
    let rate2 = client.eth_to_fri_rate(TIMESTAMP).await.unwrap();
    assert_eq!(rate2, EXPECTED_RATE);

    // Update the config to a new max cache size.
    let new_config = EthToStrkOracleConfig {
        url_header_list: url_header_list.clone(),
        lag_interval_seconds: LAG_INTERVAL_SECONDS * 2,
        max_cache_size: MAX_CACHE_SIZE * 2,
        query_timeout_sec: QUERY_TIMEOUT_SEC,
    };
    client.update_dynamic_config(new_config);

    // Again the first request should fail because the cache has been cleared.
    assert!(client.eth_to_fri_rate(TIMESTAMP).await.is_err());
    // Wait for the query to resolve.
    while client.eth_to_fri_rate(TIMESTAMP).await.is_err() {
        tokio::task::yield_now().await; // Don't block the executor.
    }
    let rate2 = client.eth_to_fri_rate(TIMESTAMP).await.unwrap();
    assert_eq!(rate2, EXPECTED_RATE);

    // Update the config to a new query timeout.
    let new_config = EthToStrkOracleConfig {
        url_header_list,
        lag_interval_seconds: LAG_INTERVAL_SECONDS * 2,
        max_cache_size: MAX_CACHE_SIZE * 2,
        query_timeout_sec: QUERY_TIMEOUT_SEC * 2,
    };
    client.update_dynamic_config(new_config);

    // Again the first request should fail because the cache has been cleared.
    assert!(client.eth_to_fri_rate(TIMESTAMP).await.is_err());
    // Wait for the query to resolve.
    while client.eth_to_fri_rate(TIMESTAMP).await.is_err() {
        tokio::task::yield_now().await; // Don't block the executor.
    }
    let rate2 = client.eth_to_fri_rate(TIMESTAMP).await.unwrap();
    assert_eq!(rate2, EXPECTED_RATE);
}

#[tokio::test]
async fn eth_to_fri_rate_two_urls() {
    const EXPECTED_RATE: u128 = 123456;
    let expected_rate_hex = format!("0x{EXPECTED_RATE:x}");
    const LAG_INTERVAL_SECONDS: u64 = 60;
    const TIMESTAMP1: u64 = 1234567890;
    const TIMESTAMP2: u64 = TIMESTAMP1 + LAG_INTERVAL_SECONDS * 2; // New quantized bucket
    const TIMESTAMP3: u64 = TIMESTAMP2 + LAG_INTERVAL_SECONDS * 2; // New quantized bucket
    let mut server1 = mockito::Server::new_async().await;
    let mut server2 = mockito::Server::new_async().await;

    // Define a mock response with badly formatted JSON for server1
    let _mock_response1 = make_server(&mut server1, json!({"foo": "0x0", "bar": 18})).await;
    // For server2 we get the expected response.
    let _mock_response2 =
        make_server(&mut server2, json!({"price": &expected_rate_hex, "decimals": 18})).await;

    let url_header_list = Some(vec![
        UrlAndHeaders {
            url: Url::parse(&server1.url()).unwrap(),
            headers: BTreeMap::new(), // No additional headers needed for this test.
        }
        .into(),
        UrlAndHeaders {
            url: Url::parse(&server2.url()).unwrap(),
            headers: BTreeMap::new(), // No additional headers needed for this test.
        }
        .into(),
    ]);
    let config = EthToStrkOracleConfig {
        url_header_list,
        lag_interval_seconds: LAG_INTERVAL_SECONDS,
        ..Default::default()
    };
    let mut client = EthToStrkOracleClient::new(config.clone());
    // First request should fail because the cache is empty.
    assert!(client.eth_to_fri_rate(TIMESTAMP1).await.is_err());
    // Wait for the query to resolve.
    while client.eth_to_fri_rate(TIMESTAMP1).await.is_err() {
        tokio::task::yield_now().await; // Don't block the executor.
    }
    let rate1 = client.eth_to_fri_rate(TIMESTAMP1).await.unwrap();
    assert_eq!(rate1, EXPECTED_RATE);

    // Note this server fails on missing "decimals", not "price".
    let _mock_response3 =
        make_server(&mut server2, json!({"price": &expected_rate_hex, "bar": 18})).await;
    // First request should fail because the cache is empty.
    assert!(client.eth_to_fri_rate(TIMESTAMP2).await.is_err());
    // Wait for the query to resolve.
    loop {
        match client.eth_to_fri_rate(TIMESTAMP2).await {
            Ok(_) => panic!("Both servers should be returning bad JSON!"),
            Err(EthToStrkOracleClientError::QueryNotReadyError(_)) => {}
            Err(EthToStrkOracleClientError::AllUrlsFailedError(_, index)) => {
                assert!(index == 1, "Last error should be index 1 (server2).");
                break; // This is the expected error, since server1 and 2 returned bad JSON.
            }
            Err(e) => panic!("Unexpected error: {e:?}"),
        }
        tokio::task::yield_now().await; // Don't block the executor.
    }

    // Update the config to only use one URL (for server2).
    let new_config = EthToStrkOracleConfig {
        url_header_list: Some(vec![
            UrlAndHeaders {
                url: Url::parse(&server2.url()).unwrap(),
                headers: BTreeMap::new(), // No additional headers needed for this test.
            }
            .into(),
        ]),
        lag_interval_seconds: LAG_INTERVAL_SECONDS,
        ..Default::default()
    };
    client.update_dynamic_config(new_config);

    // First request should fail because the cache is empty.
    assert!(client.eth_to_fri_rate(TIMESTAMP3).await.is_err());
    // Wait for the query to resolve.
    loop {
        match client.eth_to_fri_rate(TIMESTAMP3).await {
            Ok(_) => panic!("Both servers should be returning bad JSON!"),
            Err(EthToStrkOracleClientError::QueryNotReadyError(_)) => {}
            Err(EthToStrkOracleClientError::AllUrlsFailedError(_, index)) => {
                assert!(index == 0, "Last error should be index 0 (only server2 in the config).");
                break; // This is the expected error, since only server2 is in the config.
            }
            Err(e) => panic!("Unexpected error: {e:?}"),
        }
        tokio::task::yield_now().await; // Don't block the executor.
    }
}
