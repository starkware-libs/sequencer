use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::{create_trivial_calldata, CairoVersion, RunnableCairo1};
use mempool_test_utils::starknet_api_test_utils::{
    test_valid_resource_bounds,
    MultiAccountTransactionGenerator,
};
use papyrus_network::gossipsub_impl::Topic;
use papyrus_network::network_manager::test_utils::{
    create_connected_network_configs,
    network_config_into_broadcast_channels,
};
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::mempool::RpcTransactionWrapper;
use starknet_api::test_utils::invoke::rpc_invoke_tx;
use starknet_api::{invoke_tx_args, nonce};
use starknet_gateway_types::gateway_types::GatewayInput;
use starknet_integration_tests::state_reader::StorageTestSetup;
use starknet_integration_tests::test_identifiers::TestIdentifier;
use starknet_integration_tests::utils::{
    create_chain_info,
    create_gateway_config,
    create_integration_test_tx_generator,
};
use starknet_sequencer_infra::test_utils::AvailablePorts;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::config::component_execution_config::{
    ActiveComponentExecutionConfig,
    ActiveComponentExecutionMode,
    ReactiveComponentExecutionConfig,
    ReactiveComponentExecutionMode,
};
use starknet_sequencer_node::config::node_config::SequencerNodeConfig;
use starknet_sequencer_node::utils::create_node_modules;
use tempfile::TempDir;

pub const MEMPOOL_TOPIC: &str = "starknet_mempool_transaction_propagation/0.1.0";

async fn node_setup(
    tx_generator: &MultiAccountTransactionGenerator,
    test_identifier: TestIdentifier,
) -> (SequencerNodeConfig, BroadcastTopicChannels<RpcTransactionWrapper>, Vec<TempDir>) {
    let components = ComponentConfig {
        consensus_manager: ActiveComponentExecutionConfig {
            execution_mode: ActiveComponentExecutionMode::Disabled,
        },
        batcher: ReactiveComponentExecutionConfig {
            execution_mode: ReactiveComponentExecutionMode::Disabled,
            ..Default::default()
        },
        http_server: ActiveComponentExecutionConfig {
            execution_mode: ActiveComponentExecutionMode::Disabled,
        },
        monitoring_endpoint: ActiveComponentExecutionConfig {
            execution_mode: ActiveComponentExecutionMode::Disabled,
        },

        ..Default::default()
    };

    let accounts = tx_generator.accounts();
    let chain_info = create_chain_info();
    let storage_for_test = StorageTestSetup::new(accounts.to_vec(), &chain_info);

    let gateway_config = create_gateway_config(chain_info);

    let config =
        SequencerNodeConfig { components, gateway_config, ..SequencerNodeConfig::default() };

    let mut available_ports = AvailablePorts::new(test_identifier.into(), 0);
    let ports = available_ports.get_next_ports(2);
    let mut network_configs = create_connected_network_configs(ports);
    let channels_network_config = network_configs.pop().unwrap();
    let broadcast_channels =
        network_config_into_broadcast_channels(channels_network_config, Topic::new(MEMPOOL_TOPIC));

    (
        config,
        broadcast_channels,
        vec![storage_for_test.batcher_storage_handle, storage_for_test.state_sync_storage_handle],
    )
}

#[tokio::test]
#[ignore = "Not yet implemented: Simulate mempool non-responsiveness without crash (simulate \
            latency issue)"]
async fn test_mempool_non_responsive() {}

#[tokio::test]
#[ignore = "Not yet implemented: On crash, mempool resets and starts empty"]
async fn test_mempool_crash() {}

#[tokio::test]
#[ignore = "Not yet implemented: Simulate gateway state non-responsiveness (latency issue)"]
async fn test_gateway_state_non_responsive() {}

#[tokio::test]
#[ignore = "Not yet implemented: Add high-priority transaction to a full mempool"]
async fn test_add_tx_high_priority_full_mempool() {}

#[tokio::test]
#[ignore = "Not yet implemented: Add low-priority transaction to a full mempool (should not enter)"]
async fn test_add_tx_low_priority_full_mempool() {}

#[tokio::test]
#[ignore = "Not yet implemented: Simulate a single account sending many transactions (e.g., an \
            exchange)"]
async fn test_single_account_stress() {}

#[tokio::test]
#[ignore = "Not yet implemented"]
async fn test_duplicate_tx_error_handling() {
    // Setup.
    let tx_generator = create_integration_test_tx_generator();
    let (config, _broadcast_channels, _temp_dir_handles) =
        node_setup(&tx_generator, TestIdentifier::MempoolHandlesDuplicateTxTest).await;
    let (clients, _servers) = create_node_modules(&config);

    let gateway_client =
        clients.get_gateway_shared_client().expect("Gateway Client should be available");

    assert_eq!(1,2);

    let invoke_args = invoke_tx_args!(
        sender_address: tx_generator.accounts()[0].sender_address(),
        nonce: nonce!(1),
    );
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let rpc_tx = rpc_invoke_tx(invoke_tx_args!(
        resource_bounds: test_valid_resource_bounds(),
        calldata: create_trivial_calldata(test_contract.get_instance_address(0)),
        ..invoke_args
    ));

    let gateway_input = GatewayInput { rpc_tx, message_metadata: None };

    // Test.
    let res = gateway_client.add_tx(gateway_input.clone()).await;
    assert!(res.is_ok());

    // // Resend the same transaction and expect duplicate nonce error.
    // let res = gateway_client.add_tx(gateway_input).await;

    // // TODO: Check for MempoolError once it is properly mapped to a GatewayError.
    // // Currently, the code maps all errors to a general GatewaySpecError::UnexpectedError.
    // // Assert.
    // assert!(res.is_err());
}

#[tokio::test]
#[ignore = "Not yet implemented"]
async fn test_duplicate_nonce_error_handling() {}

#[tokio::test]
#[ignore = "Not yet implemented: go over edge cases that occur when commit_block arrived at the
            mempool before it arrived at the gateway, and vice versa. For example, account nonces
            in the GW during add_tx will be different from what the mempool knows about.
            NOTE: this is for after the first POC, in the first POC the mempool tracks account
            nonces internally, indefinitely (which is of course not scalable and is only for POC)"]
async fn test_commit_block_races() {}
