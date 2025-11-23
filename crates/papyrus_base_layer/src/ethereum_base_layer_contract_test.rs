use std::time::Duration;

use url::Url;

use crate::ethereum_base_layer_contract::{EthereumBaseLayerConfig, EthereumBaseLayerContract};
use crate::BaseLayerContract;

#[tokio::test]
#[ignore = "This test uses external dependencies, like Infura. But still it is a good \
            reference/sanity check."]
async fn fusaka_blob_fee_sanity_check() {
    let config = EthereumBaseLayerConfig {
        fusaka_no_bpo_start_block_number: 0,
        bpo1_start_block_number: 0,
        bpo2_start_block_number: 0,
        timeout_millis: Duration::from_millis(5000),
        ..Default::default()
    };

    // Timeline: Sepolia went on Fusaka on epoch 272640 (slot 8724480) which is about block 9408577
    // It went on BPO1 on epoch 274176 (slot 8773632) which is about block 9456501
    // It went on BPO2 on epoch 275712 (slot 8822784) which is about block 9504747
    let infura_api_key = std::env::var("INFURA_API_KEY")
        .expect("expected infura api key to be set in INFURA_API_KEY environment variable");
    let url = Url::parse(&format!("https://sepolia.infura.io/v3/{}", infura_api_key))
        .expect("expected infura url to be valid");
    let mut base_layer = EthereumBaseLayerContract::new(config.clone());

    // This is a known time when the data gas price was relatively high:
    // https://sepolia.blobscan.com/block/9716185
    // The blob fee here is 0.010629722 wei.
    let block_number = 9716185;
    let base_fee_from_blobscan = 10629722;
    let block_header = base_layer
        .get_block_header(block_number)
        .await
        .expect("expected call to get block header to succeed")
        .expect("expected block header to be found");

    assert_eq!(block_header.blob_fee, base_fee_from_blobscan);

    // Now try to unset the fusaka configuration, to see if we get a massively bigger blob fee.
    base_layer.config.fusaka_no_bpo_start_block_number = 100000000;
    base_layer.config.bpo1_start_block_number = 1000000000;
    base_layer.config.bpo2_start_block_number = 1000000000;
    let block_header = base_layer
        .get_block_header(block_number)
        .await
        .expect("expected call to get block header to succeed")
        .expect("expected block header to be found");

    assert!(block_header.blob_fee > 1000 * base_fee_from_blobscan);

    // Choose a mainnet block number that is not yet on Fusaka (but has non-zero blob fee).
    // https://blobscan.com/block/23824000
    // The blob fee here is 31.042082881 Gwei.
    let url = Url::parse(&format!("https://mainnet.infura.io/v3/{}", infura_api_key))
        .expect("expected infura url to be valid");
    let mut base_layer = EthereumBaseLayerContract::new(config, url);
    base_layer.config.fusaka_no_bpo_start_block_number = 100000000;
    base_layer.config.bpo1_start_block_number = 1000000000;
    base_layer.config.bpo2_start_block_number = 1000000000;
    let block_number = 23824000;
    let base_fee_from_blobscan = 31042082881;
    let block_header = base_layer
        .get_block_header(block_number)
        .await
        .expect("expected call to get block header to succeed")
        .expect("expected block header to be found");
    assert_eq!(block_header.blob_fee, base_fee_from_blobscan);

    // But if we set the fusaka update to have already happened, we should get a much lower fee.
    base_layer.config.fusaka_no_bpo_start_block_number = 0;
    base_layer.config.bpo1_start_block_number = 0;
    base_layer.config.bpo2_start_block_number = 0;
    let block_header = base_layer
        .get_block_header(block_number)
        .await
        .expect("expected call to get block header to succeed")
        .expect("expected block header to be found");
    assert!(block_header.blob_fee * 1000 < base_fee_from_blobscan);
}
