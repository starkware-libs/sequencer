use pretty_assertions::assert_eq;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::felt;

use crate::ethereum_base_layer_contract::{EthereumBaseLayerConfig, EthereumBaseLayerContract};
use crate::test_utils::get_test_ethereum_node;
use crate::BaseLayerContract;

// TODO: move to global test_utils crate and use everywhere instead of relying on the
// confusing `#[ignore]` api to mark slow tests.
fn in_ci() -> bool {
    std::env::var("CI").is_ok()
}

#[tokio::test]
// Note: the test requires ganache-cli installed, otherwise it is ignored.
async fn latest_proved_block_ethereum() {
    if !in_ci() {
        return;
    }

    let (node_handle, starknet_contract_address) = get_test_ethereum_node();
    let config = EthereumBaseLayerConfig {
        node_url: node_handle.0.endpoint().parse().unwrap(),
        starknet_contract_address,
    };
    let contract = EthereumBaseLayerContract::new(config).unwrap();

    let first_sn_state_update = (BlockNumber(100), BlockHash(felt!("0x100")));
    let second_sn_state_update = (BlockNumber(200), BlockHash(felt!("0x200")));
    let third_sn_state_update = (BlockNumber(300), BlockHash(felt!("0x300")));

    type Scenario = (u64, Option<(BlockNumber, BlockHash)>);
    let scenarios: Vec<Scenario> = vec![
        (0, Some(third_sn_state_update)),
        (5, Some(third_sn_state_update)),
        (15, Some(second_sn_state_update)),
        (25, Some(first_sn_state_update)),
        (1000, None),
    ];
    for (scenario, expected) in scenarios {
        let latest_block = contract.latest_proved_block(scenario).await.unwrap();
        assert_eq!(latest_block, expected);
    }
}
