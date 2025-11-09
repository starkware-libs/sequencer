use alloy::consensus::Header;
use alloy::primitives::B256;
use alloy::providers::mock::Asserter;
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::{Block, BlockTransactions, Header as AlloyRpcHeader};
use assert_matches::assert_matches;
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_api::hash::StarkHash;
use url::Url;

use crate::ethereum_base_layer_contract::{
    EthereumBaseLayerConfig,
    EthereumBaseLayerContract,
    EthereumBaseLayerError,
    Starknet,
};
use crate::BaseLayerContract;

// TODO(Gilad): Use everywhere instead of relying on the confusing `#[ignore]` api to mark slow
// tests.
pub fn in_ci() -> bool {
    std::env::var("CI").is_ok()
}

fn base_layer_with_mocked_provider() -> (EthereumBaseLayerContract, Asserter) {
    // See alloy docs, functions as a queue of mocked responses, success or failure.
    let asserter = Asserter::new();

    let provider = ProviderBuilder::new().connect_mocked_client(asserter.clone()).root().clone();
    let contract = Starknet::new(Default::default(), provider);
    let base_layer = EthereumBaseLayerContract {
        contract,
        config: Default::default(),
        url: Url::parse("http://dummy_url").unwrap(),
    };

    (base_layer, asserter)
}

#[tokio::test]
async fn latest_proved_block_ethereum() {
    if !in_ci() {
        return;
    }

    // State update constants.
    const FIRST_ETHEREUM_BLOCK: u64 = 6;
    const FIRST_STARKNET_BLOCK_NUMBER: u64 = 100;
    const FIRST_STARKNET_BLOCK_HASH: u64 = 0x100;

    const SECOND_ETHEREUM_BLOCK: u64 = 16;
    const SECOND_STARKNET_BLOCK_NUMBER: u64 = 200;
    const SECOND_STARKNET_BLOCK_HASH: u64 = 0x200;

    const THIRD_ETHEREUM_BLOCK: u64 = 26;
    const THIRD_STARKNET_BLOCK_NUMBER: u64 = 300;
    const THIRD_STARKNET_BLOCK_HASH: u64 = 0x300;

    const FINAL_ETHEREUM_BLOCK: u64 = 31;
    const INITIAL_STARKNET_BLOCK_NUMBER: u64 = 1;

    // Define the state updates: Ethereum block → (Starknet block number, Starknet block hash)
    let state_updates = vec![
        crate::test_utils::StateUpdateConfig {
            ethereum_block: FIRST_ETHEREUM_BLOCK,
            starknet_block_number: FIRST_STARKNET_BLOCK_NUMBER,
            starknet_block_hash: FIRST_STARKNET_BLOCK_HASH,
        },
        crate::test_utils::StateUpdateConfig {
            ethereum_block: SECOND_ETHEREUM_BLOCK,
            starknet_block_number: SECOND_STARKNET_BLOCK_NUMBER,
            starknet_block_hash: SECOND_STARKNET_BLOCK_HASH,
        },
        crate::test_utils::StateUpdateConfig {
            ethereum_block: THIRD_ETHEREUM_BLOCK,
            starknet_block_number: THIRD_STARKNET_BLOCK_NUMBER,
            starknet_block_hash: THIRD_STARKNET_BLOCK_HASH,
        },
    ];

    let initial_state = BlockHashAndNumber {
        number: BlockNumber(INITIAL_STARKNET_BLOCK_NUMBER),
        hash: BlockHash(StarkHash::from_hex_unchecked("0x0")),
    };
    let first_sn_state_update = BlockHashAndNumber {
        number: BlockNumber(FIRST_STARKNET_BLOCK_NUMBER),
        hash: BlockHash(StarkHash::from_hex_unchecked(&format!(
            "0x{:x}",
            FIRST_STARKNET_BLOCK_HASH
        ))),
    };
    let second_sn_state_update = BlockHashAndNumber {
        number: BlockNumber(SECOND_STARKNET_BLOCK_NUMBER),
        hash: BlockHash(StarkHash::from_hex_unchecked(&format!(
            "0x{:x}",
            SECOND_STARKNET_BLOCK_HASH
        ))),
    };
    let third_sn_state_update = BlockHashAndNumber {
        number: BlockNumber(THIRD_STARKNET_BLOCK_NUMBER),
        hash: BlockHash(StarkHash::from_hex_unchecked(&format!(
            "0x{:x}",
            THIRD_STARKNET_BLOCK_HASH
        ))),
    };

    let (node_handle, starknet_contract_address) =
        crate::test_utils::get_test_anvil_node(&state_updates, FINAL_ETHEREUM_BLOCK).await;
    let contract = EthereumBaseLayerContract::new(
        EthereumBaseLayerConfig { starknet_contract_address, ..Default::default() },
        node_handle.endpoint_url(),
    );

    type Scenario = (u64, Option<BlockHashAndNumber>);
    let scenarios: Vec<Scenario> = vec![
        // Latest block
        (0, Some(third_sn_state_update)), // finality 0 → block 31 → block 300
        // At third state update (block 26)
        (5, Some(third_sn_state_update)), // finality 5 → block 26 → block 300
        // At second state update (block 16)
        (15, Some(second_sn_state_update)), // finality 15 → block 16 → block 200
        // At first state update (block 6)
        (25, Some(first_sn_state_update)), // finality 25 → block 6 → block 100
        // Before first state update (block 5, before update at block 6)
        (26, Some(initial_state)), // finality 26 → block 5 → initial state (block 1)
        // Error case: finality too high
        (1000, None), // finality 1000 → error (block 31 - 1000 would be negative)
    ];

    for (scenario, expected) in scenarios {
        let latest_block = contract.latest_proved_block(scenario).await;
        match latest_block {
            Ok(latest_block) => {
                assert_eq!(latest_block, expected, "Failed at finality {}", scenario)
            }
            Err(e) => {
                assert_matches!(
                    e,
                    EthereumBaseLayerError::LatestBlockNumberReturnedTooLow(_, _),
                    "Expected error at finality {} but got: {:?}",
                    scenario,
                    e
                );
            }
        }
    }
}

#[tokio::test]
async fn get_gas_price_and_timestamps() {
    if !in_ci() {
        return;
    }
    // Setup.
    let (mut base_layer, asserter) = base_layer_with_mocked_provider();

    // Selected in order to make the blob calc below non trivial.
    const BLOB_GAS: u128 = 10000000;

    let header = Header {
        base_fee_per_gas: Some(5),
        excess_blob_gas: Some(BLOB_GAS.try_into().unwrap()),
        ..Default::default()
    };

    // Test pectra blob.

    let mocked_block_response =
        &Some(Block::new(AlloyRpcHeader::new(header), BlockTransactions::<B256>::default()));
    asserter.push_success(mocked_block_response);
    let header = base_layer.get_block_header(0).await.unwrap().unwrap();

    assert_eq!(header.base_fee_per_gas, 5);

    // See eip4844::fake_exponential().
    // Roughly e ** (BLOB_GAS / eip7691::BLOB_GASPRICE_UPDATE_FRACTION_PECTRA)
    let expected_pectra_blob_calc = 7;
    assert_eq!(header.blob_fee, expected_pectra_blob_calc);

    // Test legacy blob

    asserter.push_success(mocked_block_response);
    base_layer.config.prague_blob_gas_calc = false;
    let header = base_layer.get_block_header(0).await.unwrap().unwrap();
    // Roughly e ** (BLOB_GAS / eip4844::BLOB_GASPRICE_UPDATE_FRACTION)
    let expected_original_blob_calc = 19;
    assert_eq!(header.blob_fee, expected_original_blob_calc);
}
