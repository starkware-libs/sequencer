use apollo_l1_endpoint_monitor::monitor::{L1EndpointMonitor, L1EndpointMonitorConfig};
use apollo_l1_endpoint_monitor_types::L1EndpointMonitorError;
use papyrus_base_layer::test_utils::anvil;
use url::Url;

/// Integration test: two Anvil nodes plus a bogus endpoint to exercise cycling and failure.
#[tokio::test]
async fn end_to_end_cycle_and_recovery() {
    // Spin up two ephemeral Anvil nodes.
    // IMPORTANT: This is one of the only cases where two anvil nodes are needed simultaneously,
    // since we are flow testing two separate L1 nodes. Other tests should never use more than one
    // at a time!
    let good_node_1 = anvil(None);
    let good_url_1 = good_node_1.endpoint_url();
    let good_node_2 = anvil(None);
    let good_url_2 = good_node_2.endpoint_url();

    // Bogus endpoint on port 1 that is likely to be unbound, see the unit tests for more details.
    let bad_node_url = Url::parse("http://localhost:1").unwrap();

    // Initialize monitor starting at the bad index.
    let mut monitor = L1EndpointMonitor {
        current_l1_endpoint_index: 0,
        config: L1EndpointMonitorConfig {
            ordered_l1_endpoint_urls: vec![
                bad_node_url.clone(),
                good_url_1.clone(),
                good_url_2.clone(),
            ],
        },
    };

    // 1) First call: skip bad and take the first good one.
    let active1 = monitor.get_active_l1_endpoint().await.unwrap();
    assert_eq!(active1, good_url_1);
    assert_eq!(monitor.current_l1_endpoint_index, 1);

    // 2) Anvil 1 is going down.
    drop(good_node_1);

    // Next call: now the first good node is down, switch to second good node.
    let active2 = monitor.get_active_l1_endpoint().await.unwrap();
    assert_eq!(active2, good_url_2);
    assert_eq!(monitor.current_l1_endpoint_index, 2);

    // 3) Anvil 2 is now also down!
    drop(good_node_2);

    // All endpoints are now down --> error. Do this twice for idempotency.
    for _ in 0..2 {
        let result = monitor.get_active_l1_endpoint().await;
        assert_eq!(result, Err(L1EndpointMonitorError::NoActiveL1Endpoint));
        assert_eq!(monitor.current_l1_endpoint_index, 2);
    }

    // ANVIL node 1 has risen!
    let good_node_1 = anvil(None);
    // Anvil is configured to use an ephemeral port, so this new node will be bound to a fresh port.
    // We cannot reuse the previous URL since the old port may no longer be available.
    let good_url_1 = good_node_1.endpoint_url();
    monitor.config.ordered_l1_endpoint_urls[1] = good_url_1.clone();
    // Index wraps around from 2 to 0, 0 is still down so 1 is picked, which is operational now.
    let active3 = monitor.get_active_l1_endpoint().await.unwrap();
    assert_eq!(active3, good_url_1);
    assert_eq!(monitor.current_l1_endpoint_index, 1);
}
