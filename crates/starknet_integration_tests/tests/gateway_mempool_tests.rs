use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::{create_trivial_calldata, CairoVersion};
use mempool_test_utils::starknet_api_test_utils::{test_valid_resource_bounds, Contract};
use rstest::{fixture, rstest};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::test_utils::invoke::{rpc_invoke_tx, InvokeTxArgs};
use starknet_api::{invoke_tx_args, nonce};
use starknet_gateway_types::gateway_types::GatewayInput;
use starknet_integration_tests::state_reader::{spawn_test_rpc_state_reader, StorageTestSetup};
use starknet_integration_tests::utils::{
    create_chain_info,
    create_gateway_config,
    create_integration_test_tx_generator,
    test_rpc_state_reader_config,
};
use starknet_sequencer_node::clients::SequencerNodeClients;
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

// Fixtures

#[fixture]
fn accounts() -> Vec<Contract> {
    let tx_generator = create_integration_test_tx_generator();
    tx_generator.accounts()
}

// Functions

async fn node_setup(accounts: Vec<Contract>) -> SequencerNodeClients {
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
        http_server: ComponentExecutionConfig {
            execution_mode: ComponentExecutionMode::Disabled,
            local_server_config: None,
            ..Default::default()
        },
        monitoring_endpoint: ComponentExecutionConfig {
            execution_mode: ComponentExecutionMode::Disabled,
            local_server_config: None,
            ..Default::default()
        },

        ..Default::default()
    };

    let chain_info = create_chain_info();
    let storage_for_test = StorageTestSetup::new(accounts, chain_info.chain_id.clone());
    let gateway_config = create_gateway_config(chain_info.clone()).await;

    // Spawn a papyrus rpc server for a papyrus storage reader.
    let rpc_server_addr = spawn_test_rpc_state_reader(
        storage_for_test.rpc_storage_reader,
        chain_info.chain_id.clone(),
    )
    .await;
    let rpc_state_reader_config = test_rpc_state_reader_config(rpc_server_addr);

    let config = SequencerNodeConfig {
        components,
        gateway_config,
        rpc_state_reader_config,
        ..SequencerNodeConfig::default()
    };
    let (clients, servers) = create_node_modules(&config);
    let sequencer_node_future = run_component_servers(servers);

    let handle = Handle::current();
    let task_executor = TokioExecutor::new(handle);
    task_executor.spawn_with_handle(sequencer_node_future);

    clients
}

fn invoke_rpc_tx_for_testing(invoke_args: InvokeTxArgs) -> RpcTransaction {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1);

    rpc_invoke_tx(invoke_tx_args!(
        resource_bounds: test_valid_resource_bounds(),
        calldata: create_trivial_calldata(test_contract.get_instance_address(0)),
        ..invoke_args
    ))
}

// Tests

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
async fn test_duplicate_tx_error_handling() {}

#[rstest]
#[tokio::test]
async fn test_duplicate_nonce_error_handling(accounts: Vec<Contract>) {
    // Setup.
    let sender_address = accounts[0].sender_address;

    let clients = node_setup(accounts).await;
    let gateway_client =
        clients.get_gateway_shared_client().expect("Gateway Client should be available");

    let rpc_tx = invoke_rpc_tx_for_testing(invoke_tx_args!(
        sender_address: sender_address,
        nonce: nonce!(1),
    ));
    let gateway_input = GatewayInput { rpc_tx, message_metadata: None };

    // Test.
    let res = gateway_client.add_tx(gateway_input.clone()).await;
    assert!(res.is_ok());

    // Resend the same transaction and expect duplicate nonce error.
    let res = gateway_client.add_tx(gateway_input).await;

    // TODO: Check for MempoolError once it is properly mapped to a GatewayError.
    // Currently, the code maps all errors to a general GatewaySpecError::UnexpectedError.
    // Assert.
    assert!(res.is_err());
}

#[tokio::test]
#[ignore = "Not yet implemented: go over edge cases that occur when commit_block arrived at the
            mempool before it arrived at the gateway, and vice versa. For example, account nonces
            in the GW during add_tx will be different from what the mempool knows about.
            NOTE: this is for after the first POC, in the first POC the mempool tracks account
            nonces internally, indefinitely (which is of course not scalable and is only for POC)"]
async fn test_commit_block_races() {}
