#![allow(unused_mut, unused_imports, unused_variables)]
use starknet_class_manager_types::MockClassManagerClient;
use starknet_gateway_types::communication::MockGatewayClient;

use crate::config::MempoolP2pConfig;

#[tokio::test]
async fn transaction_queue_is_broadcasted_after_enough_time_has_passed() {
    let mut mempool_p2p_config =
        MempoolP2pConfig { max_transaction_batch_size: 2, ..Default::default() };
    let gateway_client = MockGatewayClient::new();
    let class_manager_client = MockClassManagerClient::new();
    // creating a MempoolP2pPropagatorClient is really annoying. should ask Nadin about it tomorrow
}
