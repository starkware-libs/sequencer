use alloy::primitives::{I256, U256};
use alloy::providers::Provider;
use alloy::rpc::types::eth::Filter as EthEventFilter;
use alloy::sol_types::SolEventInterface;
use apollo_integration_tests::anvil_base_layer::{AnvilBaseLayer, MockedStateUpdate};
use assert_matches::assert_matches;
use papyrus_base_layer::ethereum_base_layer_contract::Starknet;
use papyrus_base_layer::BaseLayerContract;
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};

#[tokio::test]
async fn test_mocked_starknet_state_update() {
    let base_layer = AnvilBaseLayer::new().await;

    // Check that the contract was initialized (during the construction above).
    let no_finality = 0;
    let genesis_block_number = 1;
    let genesis_block_hash = 0;
    let initial_state = base_layer.latest_proved_block(no_finality).await.unwrap().unwrap();
    assert_eq!(
        initial_state.number,
        BlockNumber(genesis_block_number),
        "Starknet contract was not initiailized."
    );
    assert_eq!(
        initial_state.hash,
        BlockHash(genesis_block_hash.into()),
        "Starknet contract was not initiailized."
    );

    // Negative flow: update state should always have sequential block numbers.
    let wrong_next_block_number = genesis_block_number + 2;
    let incorrect_new_block_number_result = base_layer
        .update_mocked_starknet_contract_state(MockedStateUpdate {
            new_block_number: wrong_next_block_number,
            new_block_hash: 2,
            prev_block_hash: genesis_block_hash,
        })
        .await;
    assert_matches!(
        &incorrect_new_block_number_result.unwrap_err(),
        e if e.to_string().contains("INVALID_PREV_BLOCK_NUMBER")
    );

    // Happy flow.
    let next_block_number = genesis_block_number + 1;
    let new_block_hash = 2;
    base_layer
        .update_mocked_starknet_contract_state(MockedStateUpdate {
            new_block_number: next_block_number,
            new_block_hash,
            prev_block_hash: genesis_block_hash,
        })
        .await
        .unwrap();

    let updated_block_number_and_hash =
        base_layer.latest_proved_block(no_finality).await.unwrap().unwrap();
    assert_eq!(
        updated_block_number_and_hash,
        BlockHashAndNumber {
            number: BlockNumber(next_block_number),
            hash: BlockHash(new_block_hash.into())
        }
    );

    // Check that LogStateUpdate event was emitted (we don't use this event in the sequencer at the
    // time this was written).
    let event = base_layer
        .ethereum_base_layer
        .contract
        .provider()
        .get_logs(&EthEventFilter::new().from_block(1))
        .await
        .unwrap();
    let event = event.first().unwrap();

    match Starknet::StarknetEvents::decode_log(&event.inner, true).unwrap().data {
        Starknet::StarknetEvents::LogStateUpdate(state_update) => {
            assert_eq!(
                state_update.blockNumber,
                I256::from_dec_str(&next_block_number.to_string()).unwrap(),
            );
            assert_eq!(state_update.blockHash, U256::from(new_block_hash));
        }
        _ => panic!("Expected LogStateUpdate event"),
    }
}
