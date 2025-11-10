use alloy::providers::Provider;
use apollo_base_layer_tests::anvil_base_layer::{AnvilBaseLayer, MockedStateUpdate};
use assert_matches::assert_matches;
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerError;
use papyrus_base_layer::BaseLayerContract;
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_api::hash::StarkHash;

#[tokio::test]
async fn latest_proved_block_ethereum() {
    // State update constants.
    const FIRST_ETHEREUM_BLOCK: u64 = 6;
    const FIRST_STARKNET_BLOCK_NUMBER: u64 = 2;
    const FIRST_STARKNET_BLOCK_HASH: u64 = 0x2;

    const SECOND_ETHEREUM_BLOCK: u64 = 16;
    const SECOND_STARKNET_BLOCK_NUMBER: u64 = 3;
    const SECOND_STARKNET_BLOCK_HASH: u64 = 0x3;

    const THIRD_ETHEREUM_BLOCK: u64 = 26;
    const THIRD_STARKNET_BLOCK_NUMBER: u64 = 4;
    const THIRD_STARKNET_BLOCK_HASH: u64 = 0x4;

    const FINAL_ETHEREUM_BLOCK: u64 = 31;
    const INITIAL_STARKNET_BLOCK_NUMBER: u64 = 1;

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

    let base_layer = AnvilBaseLayer::new(None).await;
    let provider = &base_layer.anvil_provider;

    let mut current_block = provider.get_block_number().await.expect("Failed to get block number");
    let mut prev_starknet_block_hash = 0u64;

    // Apply state updates.
    if current_block < FIRST_ETHEREUM_BLOCK - 1 {
        let blocks_to_mine = FIRST_ETHEREUM_BLOCK - 1 - current_block;
        let _result: Option<String> = provider
            .raw_request("anvil_mine".into(), [blocks_to_mine])
            .await
            .expect("Failed to mine blocks on Anvil");
    }
    base_layer
        .update_mocked_starknet_contract_state(MockedStateUpdate {
            new_block_number: FIRST_STARKNET_BLOCK_NUMBER,
            new_block_hash: FIRST_STARKNET_BLOCK_HASH,
            prev_block_hash: prev_starknet_block_hash,
        })
        .await
        .expect("Failed to update state");
    current_block = FIRST_ETHEREUM_BLOCK;
    prev_starknet_block_hash = FIRST_STARKNET_BLOCK_HASH;

    if current_block < SECOND_ETHEREUM_BLOCK - 1 {
        let blocks_to_mine = SECOND_ETHEREUM_BLOCK - 1 - current_block;
        let _result: Option<String> = provider
            .raw_request("anvil_mine".into(), [blocks_to_mine])
            .await
            .expect("Failed to mine blocks on Anvil");
    }
    base_layer
        .update_mocked_starknet_contract_state(MockedStateUpdate {
            new_block_number: SECOND_STARKNET_BLOCK_NUMBER,
            new_block_hash: SECOND_STARKNET_BLOCK_HASH,
            prev_block_hash: prev_starknet_block_hash,
        })
        .await
        .expect("Failed to update state");
    current_block = SECOND_ETHEREUM_BLOCK;
    prev_starknet_block_hash = SECOND_STARKNET_BLOCK_HASH;

    if current_block < THIRD_ETHEREUM_BLOCK - 1 {
        let blocks_to_mine = THIRD_ETHEREUM_BLOCK - 1 - current_block;
        let _result: Option<String> = provider
            .raw_request("anvil_mine".into(), [blocks_to_mine])
            .await
            .expect("Failed to mine blocks on Anvil");
    }
    base_layer
        .update_mocked_starknet_contract_state(MockedStateUpdate {
            new_block_number: THIRD_STARKNET_BLOCK_NUMBER,
            new_block_hash: THIRD_STARKNET_BLOCK_HASH,
            prev_block_hash: prev_starknet_block_hash,
        })
        .await
        .expect("Failed to update state");
    current_block = THIRD_ETHEREUM_BLOCK;

    // Mine to the final Ethereum block.
    if current_block < FINAL_ETHEREUM_BLOCK {
        let blocks_to_mine = FINAL_ETHEREUM_BLOCK - current_block;
        let _result: Option<String> = provider
            .raw_request("anvil_mine".into(), [blocks_to_mine])
            .await
            .expect("Failed to mine blocks on Anvil");
    }

    // Finality constants: finality = FINAL_ETHEREUM_BLOCK - target_block
    const FINALITY_LATEST_ETHEREUM_BLOCK: u64 = 0;
    const FINALITY_AT_THIRD_UPDATE: u64 = FINAL_ETHEREUM_BLOCK - THIRD_ETHEREUM_BLOCK;
    const FINALITY_AFTER_SECOND_BEFORE_THIRD: u64 = FINAL_ETHEREUM_BLOCK - THIRD_ETHEREUM_BLOCK + 1;
    const FINALITY_AT_SECOND_UPDATE: u64 = FINAL_ETHEREUM_BLOCK - SECOND_ETHEREUM_BLOCK;
    const FINALITY_AFTER_FIRST_BEFORE_SECOND: u64 =
        FINAL_ETHEREUM_BLOCK - SECOND_ETHEREUM_BLOCK + 1;
    const FINALITY_AT_FIRST_UPDATE: u64 = FINAL_ETHEREUM_BLOCK - FIRST_ETHEREUM_BLOCK;
    const FINALITY_BEFORE_FIRST_UPDATE: u64 = FINAL_ETHEREUM_BLOCK - FIRST_ETHEREUM_BLOCK + 1;
    const FINALITY_ERROR_CASE: u64 = FINAL_ETHEREUM_BLOCK + 10; // Error: finality too high

    type Scenario = (u64, Option<BlockHashAndNumber>);
    let scenarios: Vec<Scenario> = vec![
        (FINALITY_LATEST_ETHEREUM_BLOCK, Some(third_sn_state_update)),
        (FINALITY_AT_THIRD_UPDATE, Some(third_sn_state_update)),
        (FINALITY_AFTER_SECOND_BEFORE_THIRD, Some(second_sn_state_update)),
        (FINALITY_AT_SECOND_UPDATE, Some(second_sn_state_update)),
        (FINALITY_AFTER_FIRST_BEFORE_SECOND, Some(first_sn_state_update)),
        (FINALITY_AT_FIRST_UPDATE, Some(first_sn_state_update)),
        (FINALITY_BEFORE_FIRST_UPDATE, Some(initial_state)),
        (FINALITY_ERROR_CASE, None),
    ];

    for (scenario, expected) in scenarios {
        let latest_block = base_layer.latest_proved_block(scenario).await;
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
