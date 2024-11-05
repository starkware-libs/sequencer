use std::future::Future;
use std::net::SocketAddr;

use axum::body::Body;
use blockifier::context::ChainInfo;
use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::CairoVersion;
use mempool_test_utils::starknet_api_test_utils::{
    rpc_tx_to_json,
    AccountId,
    MultiAccountTransactionGenerator,
};
use papyrus_consensus::config::ConsensusConfig;
use papyrus_storage::StorageConfig;
use pretty_assertions::assert_eq;
use reqwest::{Client, Response};
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::contract_address;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_batcher::block_builder::BlockBuilderConfig;
use starknet_batcher::config::BatcherConfig;
use starknet_batcher_types::batcher_types::{
    BuildProposalInput,
    DecisionReachedInput,
    GetProposalContent,
    GetProposalContentInput,
    ProposalId,
    StartHeightInput,
};
use starknet_batcher_types::communication::SharedBatcherClient;
use starknet_consensus_manager::config::ConsensusManagerConfig;
use starknet_gateway::config::{
    GatewayConfig,
    RpcStateReaderConfig,
    StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_http_server::config::HttpServerConfig;
use starknet_integration_tests::integration_test_setup::IntegrationTestSetup;
use starknet_integration_tests::integration_test_utils::{
    create_batcher_config,
    create_config,
    create_gateway_config,
    create_http_server_config,
    create_integration_test_tx_generator,
    run_integration_test_scenario,
    test_rpc_state_reader_config,
    HttpTestClient,
};
use starknet_integration_tests::state_reader::{spawn_test_rpc_state_reader, StorageTestSetup};
use starknet_sequencer_infra::trace_util::configure_tracing;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::config::test_utils::RequiredParams;
use starknet_sequencer_node::config::{
    ComponentExecutionConfig,
    ComponentExecutionMode,
    SequencerNodeConfig,
};
use starknet_sequencer_node::servers::run_component_servers;
use starknet_sequencer_node::utils::create_node_modules;
use starknet_task_executor::tokio_executor::TokioExecutor;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    create_integration_test_tx_generator()
}

#[rstest]
#[tokio::test]
async fn test_end_to_end(tx_generator: MultiAccountTransactionGenerator) {
    // Setup.
    let mock_running_system = IntegrationTestSetup::new_from_tx_generator(&tx_generator).await;

    // Create and send transactions.
    let expected_batched_tx_hashes = run_integration_test_scenario(tx_generator, &|tx| {
        mock_running_system.assert_add_tx_success(tx)
    })
    .await;

    // Test.
    run_consensus_for_end_to_end_test(
        &mock_running_system.batcher_client,
        &expected_batched_tx_hashes,
    )
    .await;
}

/// This function should mirror
/// [`run_consensus`](papyrus_consensus::manager::run_consensus). It makes requests
/// from the batcher client and asserts the expected responses were received.
pub async fn run_consensus_for_end_to_end_test(
    batcher_client: &SharedBatcherClient,
    expected_batched_tx_hashes: &[TransactionHash],
) {
    // Start height.
    // TODO(Arni): Get the current height and retrospective_block_hash from the rpc storage or use
    // consensus directly.
    let current_height = BlockNumber(1);
    batcher_client.start_height(StartHeightInput { height: current_height }).await.unwrap();

    // Build proposal.
    let proposal_id = ProposalId(0);
    let retrospective_block_hash = None;
    let build_proposal_duaration = chrono::TimeDelta::new(1, 0).unwrap();
    batcher_client
        .build_proposal(BuildProposalInput {
            proposal_id,
            deadline: chrono::Utc::now() + build_proposal_duaration,
            retrospective_block_hash,
        })
        .await
        .unwrap();

    // Get proposal content.
    let mut executed_tx_hashes: Vec<TransactionHash> = vec![];
    let _proposal_commitment = loop {
        let response = batcher_client
            .get_proposal_content(GetProposalContentInput { proposal_id })
            .await
            .unwrap();
        match response.content {
            GetProposalContent::Txs(batched_txs) => {
                executed_tx_hashes.append(&mut batched_txs.iter().map(|tx| tx.tx_hash()).collect());
            }
            GetProposalContent::Finished(proposal_commitment) => {
                break proposal_commitment;
            }
        }
    };

    // Decision reached.
    batcher_client.decision_reached(DecisionReachedInput { proposal_id }).await.unwrap();

    assert_eq!(expected_batched_tx_hashes, executed_tx_hashes);
}

#[rstest]
#[tokio::test]
async fn test_mempool_sends_tx_to_other_peer(tx_generator: MultiAccountTransactionGenerator) {
    let handle = Handle::current();
    let task_executor = TokioExecutor::new(handle);

    // Configure and start tracing.
    configure_tracing();

    let accounts = tx_generator.accounts();
    let storage_for_test = StorageTestSetup::new(accounts);

    // Spawn a papyrus rpc server for a papyrus storage reader.
    let rpc_server_addr =
        spawn_test_rpc_state_reader(storage_for_test.rpc_storage_reader, storage_for_test.chain_id)
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

    let chain_id = storage_for_test.batcher_storage_config.db_config.chain_id.clone();
    // TODO(Tsabary): create chain_info in setup, and pass relevant values throughout.
    let mut chain_info = ChainInfo::create_for_testing();
    chain_info.chain_id = chain_id.clone();
    let fee_token_addresses = chain_info.fee_token_addresses.clone();
    let batcher_config =
        create_batcher_config(storage_for_test.batcher_storage_config, chain_info.clone());
    let gateway_config = create_gateway_config(chain_info).await;
    let http_server_config = create_http_server_config().await;
    let rpc_state_reader_config = test_rpc_state_reader_config(rpc_server_addr);
    let consensus_manager_config = ConsensusManagerConfig {
        consensus_config: ConsensusConfig { start_height: BlockNumber(1), ..Default::default() },
    };
    let mempool_p2p_config = MempoolP2pConfig { network_config: 
    let config = SequencerNodeConfig {
        components,
        batcher_config,
        consensus_manager_config,
        gateway_config,
        http_server_config,
        rpc_state_reader_config,
        ..SequencerNodeConfig::default()
    };

    let (clients, servers) = create_node_modules(&config);

    let HttpServerConfig { ip, port } = config.http_server_config;
    let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

    // Build and run the sequencer node.
    let sequencer_node_future = run_component_servers(servers);
    let sequencer_node_handle = task_executor.spawn_with_handle(sequencer_node_future);

    // Wait for server to spin up.
    // TODO(Gilad): Replace with a persistent Client with a built-in retry to protect against CI
    // flakiness.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Create and send transactions.
    let expected_batched_tx_hashes = run_integration_test_scenario(tx_generator, &|tx| {
        add_tx_http_client.assert_add_tx_success(tx)
    })
    .await;

    
}
