use std::collections::HashSet;
use std::future::ready;
use std::net::SocketAddr;

use futures::StreamExt;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use papyrus_network::gossipsub_impl::Topic;
use papyrus_network::network_manager::test_utils::create_network_configs_connected_to_broadcast_channels;
use papyrus_network::network_manager::{BroadcastTopicChannels, BroadcastTopicClientTrait};
use papyrus_protobuf::mempool::RpcTransactionWrapper;
use rstest::{fixture, rstest};
use starknet_api::executable_transaction::AccountTransaction;
use starknet_api::rpc_transaction::{
    RpcDeployAccountTransaction,
    RpcInvokeTransaction,
    RpcTransaction,
};
use starknet_api::transaction::TransactionHash;
use starknet_http_server::config::HttpServerConfig;
use starknet_http_server::test_utils::{create_http_server_config, HttpTestClient};
use starknet_integration_tests::state_reader::StorageTestSetup;
use starknet_integration_tests::test_identifiers::TestIdentifier;
use starknet_integration_tests::utils::{
    create_batcher_config,
    create_chain_info,
    create_gateway_config,
    create_integration_test_tx_generator,
    create_state_sync_config,
    create_txs_for_integration_test,
    run_integration_test_scenario,
    test_tx_hashes_for_integration_test,
};
use starknet_mempool_p2p::config::MempoolP2pConfig;
use starknet_mempool_p2p::MEMPOOL_TOPIC;
use starknet_monitoring_endpoint::config::MonitoringEndpointConfig;
use starknet_monitoring_endpoint::test_utils::IsAliveClient;
use starknet_sequencer_infra::test_utils::AvailablePorts;
use starknet_sequencer_infra::trace_util::configure_tracing;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::config::component_execution_config::{
    ActiveComponentExecutionConfig,
    ReactiveComponentExecutionConfig,
    ReactiveComponentExecutionMode,
};
use starknet_sequencer_node::config::node_config::SequencerNodeConfig;
use starknet_sequencer_node::servers::run_component_servers;
use starknet_sequencer_node::utils::create_node_modules;
use tempfile::TempDir;

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    create_integration_test_tx_generator()
}

// TODO(Shahak/AlonLukatch): remove code duplication with FlowTestSetup.
async fn setup(
    tx_generator: &MultiAccountTransactionGenerator,
    test_identifier: TestIdentifier,
) -> (SequencerNodeConfig, BroadcastTopicChannels<RpcTransactionWrapper>, Vec<TempDir>) {
    configure_tracing().await;
    let accounts = tx_generator.accounts();
    let chain_info = create_chain_info();
    let storage_for_test = StorageTestSetup::new(accounts.to_vec(), &chain_info);
    let mut available_ports = AvailablePorts::new(test_identifier.into(), 0);

    // Derive the configuration for the mempool node.
    let components = ComponentConfig {
        consensus_manager: ActiveComponentExecutionConfig::disabled(),
        batcher: ReactiveComponentExecutionConfig {
            execution_mode: ReactiveComponentExecutionMode::Disabled,
            local_server_config: None,
            ..Default::default()
        },
        ..Default::default()
    };

    let batcher_config =
        create_batcher_config(storage_for_test.batcher_storage_config, chain_info.clone());
    let gateway_config = create_gateway_config(chain_info).await;
    let http_server_config =
        create_http_server_config(available_ports.get_next_local_host_socket());
    let state_sync_config = create_state_sync_config(
        storage_for_test.state_sync_storage_config,
        available_ports.get_next_port(),
    );
    let ports = available_ports.get_next_ports(2);
    let (mut network_configs, broadcast_channels) =
        create_network_configs_connected_to_broadcast_channels::<RpcTransactionWrapper>(
            Topic::new(MEMPOOL_TOPIC),
            ports,
        );
    let network_config = network_configs.pop().unwrap();
    let mempool_p2p_config = MempoolP2pConfig { network_config, ..Default::default() };
    let monitoring_endpoint_config =
        MonitoringEndpointConfig { port: available_ports.get_next_port(), ..Default::default() };
    let config = SequencerNodeConfig {
        components,
        batcher_config,
        gateway_config,
        http_server_config,
        mempool_p2p_config,
        monitoring_endpoint_config,
        state_sync_config,
        ..SequencerNodeConfig::default()
    };
    (
        config,
        broadcast_channels,
        vec![storage_for_test.batcher_storage_handle, storage_for_test.state_sync_storage_handle],
    )
}

async fn wait_for_sequencer_node(config: &SequencerNodeConfig) {
    let MonitoringEndpointConfig { ip, port, .. } = config.monitoring_endpoint_config;
    let is_alive_test_client = IsAliveClient::new(SocketAddr::from((ip, port)));

    is_alive_test_client.await_alive(5000, 50).await.expect("Node should be alive.");
}

#[rstest]
#[tokio::test]
async fn test_mempool_sends_tx_to_other_peer(mut tx_generator: MultiAccountTransactionGenerator) {
    let (config, mut broadcast_channels, _temp_dir_handles) =
        setup(&tx_generator, TestIdentifier::MempoolSendsTxToOtherPeerTest).await;
    let (_clients, servers) = create_node_modules(&config);

    let HttpServerConfig { ip, port } = config.http_server_config;
    let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

    // Build and run the sequencer node.
    let sequencer_node_future = run_component_servers(servers);
    let _sequencer_node_handle = tokio::spawn(sequencer_node_future);

    // Wait for server to spin up and for p2p to discover other peer.
    wait_for_sequencer_node(&config).await;

    let mut expected_txs = HashSet::new();

    // Create and send transactions.
    let _tx_hashes = run_integration_test_scenario(
        &mut tx_generator,
        create_txs_for_integration_test,
        &mut |tx: RpcTransaction| {
            expected_txs.insert(tx.clone()); // push the sent tx to the expected_txs list
            add_tx_http_client.assert_add_tx_success(tx)
        },
        test_tx_hashes_for_integration_test,
    )
    .await;

    while !expected_txs.is_empty() {
        let tx =
            broadcast_channels.broadcasted_messages_receiver.next().await.unwrap().0.unwrap().0;
        assert!(expected_txs.contains(&tx));
        expected_txs.remove(&tx);
    }
}

#[rstest]
#[tokio::test]
async fn test_mempool_receives_tx_from_other_peer(
    mut tx_generator: MultiAccountTransactionGenerator,
) {
    const RECEIVED_TX_POLL_INTERVAL: u64 = 100; // milliseconds between calls to read received txs from the broadcast channel
    const TXS_RETRIVAL_TIMEOUT: u64 = 2000; // max milliseconds spent polling the received txs before timing out

    let (config, mut broadcast_channels, _temp_dir_handles) =
        setup(&tx_generator, TestIdentifier::MempoolReceivesTxFromOtherPeerTest).await;
    let (clients, servers) = create_node_modules(&config);
    let mempool_client = clients.get_mempool_shared_client().unwrap();
    // Build and run the sequencer node.
    let sequencer_node_future = run_component_servers(servers);
    let _sequencer_node_handle = tokio::spawn(sequencer_node_future);
    // Wait for server to spin up and for p2p to discover other peer.
    wait_for_sequencer_node(&config).await;

    let mut expected_txs = HashSet::new();

    let _tx_hashes = run_integration_test_scenario(
        &mut tx_generator,
        create_txs_for_integration_test,
        &mut |tx: RpcTransaction| {
            expected_txs.insert(tx.clone());
            ready(TransactionHash::default()) // using the default value because we don't use the hash anyways.
        },
        test_tx_hashes_for_integration_test,
    )
    .await;
    for tx in &expected_txs {
        broadcast_channels
            .broadcast_topic_client
            .broadcast_message(RpcTransactionWrapper(tx.clone()))
            .await
            .unwrap();
    }

    let mut received_txs: Vec<AccountTransaction> = vec![];
    // Polling for as many rounds as needed up to the set constant
    for _ in 0..(TXS_RETRIVAL_TIMEOUT / RECEIVED_TX_POLL_INTERVAL) {
        if received_txs.len() == expected_txs.len() {
            break;
        }
        received_txs.append(
            // Querying for more txs than we sent verifies there are no extra txs
            &mut mempool_client.get_txs(expected_txs.len() - received_txs.len() + 1).await.unwrap(),
        );
        tokio::time::sleep(std::time::Duration::from_millis(RECEIVED_TX_POLL_INTERVAL)).await;
    }
    assert_eq!(received_txs.len(), expected_txs.len());

    for tx in received_txs {
        // TODO: change mempool to store RpcTransaction
        let converted_tx: RpcTransaction = match tx {
            AccountTransaction::Declare(_declare_tx) => {
                panic!("No implementation for converting DeclareTransaction to an RpcTransaction")
            }
            AccountTransaction::DeployAccount(deploy_account_transaction) => {
                RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(
                    deploy_account_transaction.clone().into(),
                ))
            }
            AccountTransaction::Invoke(invoke_transaction) => {
                RpcTransaction::Invoke(RpcInvokeTransaction::V3(invoke_transaction.clone().into()))
            }
        };
        assert!(expected_txs.contains(&converted_tx));
    }
}
