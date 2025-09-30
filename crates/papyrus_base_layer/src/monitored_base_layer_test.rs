use std::sync::Arc;

use apollo_l1_endpoint_monitor_types::MockL1EndpointMonitorClient;
use url::Url;

use crate::ethereum_base_layer_contract::EthereumBaseLayerContract;
use crate::monitored_base_layer::MonitoredEthereumBaseLayer;

#[tokio::test]
async fn switch_between_endpoints() {
    // Setup.
    let url1 = Url::parse("http://first_endpoint").unwrap();
    let url2 = Url::parse("http://second_endpoint").unwrap();
    let urls = [url1.clone(), url2.clone()];
    let base_layer = EthereumBaseLayerContract::new(Default::default(), url1.clone());
    let mut l1_endpoint_monitor = MockL1EndpointMonitorClient::new();
    l1_endpoint_monitor
        .expect_get_active_l1_endpoint()
        .times(1)
        .returning(move || Ok(url1.clone()));
    l1_endpoint_monitor
        .expect_get_active_l1_endpoint()
        .times(1)
        .returning(move || Ok(url2.clone()));
    let l1_endpoint_monitor_client = Arc::new(l1_endpoint_monitor);
    let monitored_base_layer =
        MonitoredEthereumBaseLayer::new(base_layer, l1_endpoint_monitor_client).await;

    // let get_url = monitored_base_layer.get().await.unwrap().inner.get_url().await.unwrap();
    monitored_base_layer.ensure_operational().await.unwrap();
    let get_url = monitored_base_layer.current_node_url.read().await.clone();
    assert_eq!(get_url, urls[0]);

    // Trying a second time should switch the URL to the second one.
    monitored_base_layer.ensure_operational().await.unwrap();
    let get_url = monitored_base_layer.current_node_url.read().await.clone();
    assert_eq!(get_url, urls[1]);
}
