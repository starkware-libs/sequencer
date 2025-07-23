use std::time::Duration;

use mockito::{Matcher, Server, ServerGuard};
use url::Url;

use crate::monitor::{
    L1EndpointMonitor,
    L1EndpointMonitorConfig,
    L1EndpointMonitorError,
    HEALTH_CHECK_RPC_METHOD,
};

// Unreachable localhost endpoints for simulating failures.
// Using localhost to prevent IO (so don't switch to example.com in order to avoid port issues).
// Using these ports since they are not well-used ports in unix and privileged (<1024),
// so unless the user runs as root and binds them explicitly, they should be closed.
const BAD_ENDPOINT_1: &str = "http://localhost:1";
const BAD_ENDPOINT_2: &str = "http://localhost:2";

// Helper to assert the active URL and current index in one call.
async fn check_get_active_l1_endpoint_success(
    monitor: &mut L1EndpointMonitor,
    expected_returned_url: &Url,
    expected_index_of_returned_url: usize,
) {
    let active = monitor.get_active_l1_endpoint().await.unwrap();
    assert_eq!(&active, expected_returned_url);
    assert_eq!(monitor.current_l1_endpoint_index, expected_index_of_returned_url);
}

fn url(url: &str) -> Url {
    Url::parse(url).unwrap()
}

fn l1_endpoint_monitor_config(ordered_l1_endpoint_urls: Vec<Url>) -> L1EndpointMonitorConfig {
    L1EndpointMonitorConfig { ordered_l1_endpoint_urls, timeout_millis: Duration::from_millis(100) }
}

/// Used to mock an L1 endpoint, like infura.
/// This can be replaced by Anvil, but for unit tests it isn't worth the large overhead Anvil
/// entails, given that we only need a valid HTTP response from the given url to test the API.
pub struct MockL1Endpoint {
    pub url: Url,
    pub endpoint: ServerGuard,
}

async fn mock_working_l1_endpoint() -> MockL1Endpoint {
    // Very simple mock is all we need _for now_: create a thin http server that expect a single
    // call to the given API and return a valid response. Note that the validity of the response
    // is coupled with the RPC method used. Server is dropped when the guard drops.
    let mut server_guard = Server::new_async().await;
    server_guard
        .mock("POST", "/")
        // Catch this specific RPC method.
        .match_body(Matcher::PartialJsonString(format!(
            r#"{{ "method": "{}"}}"#,
            HEALTH_CHECK_RPC_METHOD
        )))
        .with_status(200)
        // Return 2_u64 as a valid response for the method.
        .with_body(r#"{"jsonrpc":"2.0","id":1,"result":"0x2"}"#)
        .create_async()
        .await;

    let url = Url::parse(&server_guard.url()).unwrap();
    MockL1Endpoint { url, endpoint: server_guard }
}

#[tokio::test]
async fn non_responsive_skips_to_next() {
    // Setup.
    let endpoint = mock_working_l1_endpoint().await;
    let good_endpoint = endpoint.url.clone();

    let mut monitor = L1EndpointMonitor {
        current_l1_endpoint_index: 0,
        config: l1_endpoint_monitor_config(vec![url(BAD_ENDPOINT_1), good_endpoint.clone()]),
    };

    // Test.
    check_get_active_l1_endpoint_success(&mut monitor, &good_endpoint, 1).await;
}

#[tokio::test]
async fn current_endpoint_still_works() {
    // Setup.
    let endpoint = mock_working_l1_endpoint().await;
    let good_endpoint = endpoint.url.clone();

    let mut monitor = L1EndpointMonitor {
        current_l1_endpoint_index: 1,
        config: l1_endpoint_monitor_config(vec![
            url(BAD_ENDPOINT_1),
            good_endpoint.clone(),
            url(BAD_ENDPOINT_2),
        ]),
    };

    // Test.
    check_get_active_l1_endpoint_success(&mut monitor, &good_endpoint, 1).await;
}

#[tokio::test]
async fn wrap_around_success() {
    // Setup.
    let endpoint = mock_working_l1_endpoint().await;
    let good_url = endpoint.url.clone();

    let mut monitor = L1EndpointMonitor {
        current_l1_endpoint_index: 2,
        config: l1_endpoint_monitor_config(vec![
            url(BAD_ENDPOINT_1),
            good_url.clone(),
            url(BAD_ENDPOINT_2),
        ]),
    };

    // Test.
    check_get_active_l1_endpoint_success(&mut monitor, &good_url, 1).await;
}

#[tokio::test]
async fn all_down_fails() {
    // Setup.
    let mut monitor = L1EndpointMonitor {
        current_l1_endpoint_index: 0,
        config: l1_endpoint_monitor_config(vec![url(BAD_ENDPOINT_1), url(BAD_ENDPOINT_2)]),
    };

    // Test.
    let result = monitor.get_active_l1_endpoint().await;
    assert_eq!(result, Err(L1EndpointMonitorError::NoActiveL1Endpoint));
    assert_eq!(monitor.current_l1_endpoint_index, 0);
}

#[tokio::test]
async fn initialized_with_unknown_url_returns_error() {
    let some_valid_endpoint = mock_working_l1_endpoint().await;
    let config = l1_endpoint_monitor_config(vec![some_valid_endpoint.url]);
    let unknown_url = url(BAD_ENDPOINT_1);
    let result = L1EndpointMonitor::new(config.clone(), &unknown_url);
    assert_eq!(result, Err(L1EndpointMonitorError::InitializationError { unknown_url }));
}
