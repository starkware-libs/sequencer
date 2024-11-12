use std::collections::HashSet;
use std::future::ready;
use std::net::SocketAddr;

use futures::StreamExt;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use papyrus_network::gossipsub_impl::Topic;
use papyrus_network::network_manager::test_utils::create_network_configs_connected_to_broadcast_channels;
use papyrus_network::network_manager::BroadcastTopicClientTrait;
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
use starknet_http_server::test_utils::HttpTestClient;
use starknet_integration_tests::state_reader::{spawn_test_rpc_state_reader, StorageTestSetup};
use starknet_integration_tests::utils::{
    create_batcher_config,
    create_chain_info,
    create_gateway_config,
    create_http_server_config,
    create_integration_test_tx_generator,
    run_integration_test_scenario,
    test_rpc_state_reader_config,
};
use blockifier::context::ChainInfo;
use starknet_mempool_p2p::config::MempoolP2pConfig;
use starknet_mempool_p2p::MEMPOOL_TOPIC;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::config::component_execution_config::{
    ComponentExecutionConfig,
    ComponentExecutionMode,
};
use starknet_sequencer_node::config::node_config::SequencerNodeConfig;
use starknet_sequencer_node::servers::run_component_servers;
use starknet_sequencer_node::utils::create_node_modules;
use starknet_task_executor::tokio_executor::TokioExecutor;
use tokio::runtime::Handle;

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    create_integration_test_tx_generator()
}

// TODO: remove code duplication with FlowTestSetup
#[rstest]
#[tokio::test]
async fn test_mempool_sends_tx_to_other_peer(mut tx_generator: MultiAccountTransactionGenerator) {
    let handle = Handle::current();
    let task_executor = TokioExecutor::new(handle);

    let chain_info = create_chain_info();
    let accounts = tx_generator.accounts();
    let storage_for_test = StorageTestSetup::new(accounts, &chain_info);

    // Spawn a papyrus rpc server for a papyrus storage reader.
    let rpc_server_addr = spawn_test_rpc_state_reader(
        storage_for_test.rpc_storage_reader,
        chain_info.chain_id.clone(),
    )
    .await;

    // Derive the configuration for the mempool node.
    let components = ComponentConfig {
        consensus_manager: ComponentExecutionConfig {
            execution_mode: ComponentExecutionMode::Disabled,
            local_server_config: None,
            ..Default::default()
        },
        batcher: ComponentExecutionConfig {
            execution_mode: ComponentExecutionMode::Disabled,
            local_server_config: None,
            ..Default::default()
        },
        ..Default::default()
    };

    let batcher_config =
        create_batcher_config(storage_for_test.batcher_storage_config, chain_info.clone());
    let gateway_config = create_gateway_config(chain_info).await;
    let http_server_config = create_http_server_config().await;
    let rpc_state_reader_config = test_rpc_state_reader_config(rpc_server_addr);
    let (mut network_configs, mut broadcast_channels) =
        create_network_configs_connected_to_broadcast_channels::<RpcTransactionWrapper>(
            1,
            Topic::new(MEMPOOL_TOPIC),
        );
    let network_config = network_configs.pop().unwrap();
    let mempool_p2p_config = MempoolP2pConfig { network_config, ..Default::default() };
    let config = SequencerNodeConfig {
        components,
        batcher_config,
        gateway_config,
        http_server_config,
        rpc_state_reader_config,
        mempool_p2p_config,
        ..SequencerNodeConfig::default()
    };

    let (_clients, servers) = create_node_modules(&config);

    let HttpServerConfig { ip, port } = config.http_server_config;
    let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

    // Build and run the sequencer node.
    let sequencer_node_future = run_component_servers(servers);
    let _sequencer_node_handle = task_executor.spawn_with_handle(sequencer_node_future);

    // Wait for server to spin up and for p2p to discover other peer.
    // TODO(Gilad): Replace with a persistent Client with a built-in retry to protect against CI
    // flakiness.
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    let mut expected_txs = HashSet::new();

    // Create and send transactions.
    let _tx_hashes = run_integration_test_scenario(&mut tx_generator, &mut |tx: RpcTransaction| {
        expected_txs.insert(tx.clone()); // push the sent tx to the expected_txs list
        add_tx_http_client.assert_add_tx_success(tx)
    })
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
async fn test_mempool_receives_tx_from_other_peer(tx_generator: MultiAccountTransactionGenerator) {
    let handle = Handle::current();
    let task_executor = TokioExecutor::new(handle);
    let accounts = tx_generator.accounts();
    let mut chain_info = ChainInfo::create_for_testing();
    let chain_id = chain_info.chain_id.clone();
    let storage_for_test = StorageTestSetup::new(accounts, chain_id.clone());
    // Spawn a papyrus rpc server for a papyrus storage reader.
    let rpc_server_addr =
        spawn_test_rpc_state_reader(storage_for_test.rpc_storage_reader, chain_id)
            .await;
    // Derive the configuration for the mempool node.
    let components = ComponentConfig {
        consensus_manager: ComponentExecutionConfig {
            execution_mode: ComponentExecutionMode::Disabled,
            local_server_config: None,
            ..Default::default()
        },
        batcher: ComponentExecutionConfig {
            execution_mode: ComponentExecutionMode::Disabled,
            local_server_config: None,
            ..Default::default()
        },
        ..Default::default()
    };
    let batcher_config =
        create_batcher_config(storage_for_test.batcher_storage_config, chain_info.clone());
    let gateway_config = create_gateway_config(chain_info).await;
    let http_server_config = create_http_server_config().await;
    let rpc_state_reader_config = test_rpc_state_reader_config(rpc_server_addr);
    let (mut network_config, mut broadcast_channels) =
       create_network_configs_connected_to_broadcast_channels::<RpcTransactionWrapper>(1, Topic::new(
            MEMPOOL_TOPIC,
        ));
    let mempool_p2p_config = MempoolP2pConfig { network_config: network_config.pop().unwrap(), ..Default::default() };
    let config = SequencerNodeConfig {
        components,
        batcher_config,
        gateway_config,
        http_server_config,
        rpc_state_reader_config,
        mempool_p2p_config,
        ..SequencerNodeConfig::default()
    };
    let (clients, servers) = create_node_modules(&config);
    let mempool_client = clients.get_mempool_shared_client().unwrap();
    // Build and run the sequencer node.
    let sequencer_node_future = run_component_servers(servers);
    let _sequencer_node_handle = task_executor.spawn_with_handle(sequencer_node_future);
    // Wait for server to spin up and for p2p to discover other peer.
    // TODO(Gilad): Replace with a persistent Client with a built-in retry to protect against CI
    // flakiness.
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    let mut expected_txs = HashSet::new();

    let _tx_hashes = run_integration_test_scenario(tx_generator, &mut |tx: RpcTransaction| {
        expected_txs.insert(tx.clone());
        ready(TransactionHash::default()) // using the default value because we don't use the hash anyways.
    })
    .await;
    for tx in expected_txs.iter() {
        broadcast_channels
            .broadcast_topic_client
            .broadcast_message(RpcTransactionWrapper(tx.clone()))
            .await
            .unwrap();
    }

    // waiting for the tx to be received (TODO: figure out a better solution)
    tokio::time::sleep(std::time::Duration::from_millis(2000)).await;

    let received_tx = mempool_client.get_txs(expected_txs.len()).await.unwrap();

    // make sure we received all the transactions
    assert_eq!(received_tx.len(), expected_txs.len());

    for tx in received_tx.iter() {
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
