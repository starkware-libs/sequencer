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
use reqwest::{Client, Response};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_api::{contract_address, patricia_key};
use starknet_batcher::block_builder::BlockBuilderConfig;
use starknet_batcher::config::BatcherConfig;
use starknet_consensus_manager::config::ConsensusManagerConfig;
use starknet_gateway::config::{
    GatewayConfig,
    RpcStateReaderConfig,
    StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_http_server::config::HttpServerConfig;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::config::test_utils::RequiredParams;
use starknet_sequencer_node::config::{
    ComponentExecutionConfig,
    ComponentExecutionMode,
    SequencerNodeConfig,
};
use tokio::net::TcpListener;

pub async fn create_config(
    rpc_server_addr: SocketAddr,
    batcher_storage_config: StorageConfig,
) -> (SequencerNodeConfig, RequiredParams) {
    // TODO(Arni/ Matan): Enable the consensus in the end to end test.
    let components = ComponentConfig {
        consensus_manager: ComponentExecutionConfig {
            execution_mode: ComponentExecutionMode::Disabled,
            ..Default::default()
        },
        ..Default::default()
    };

    let chain_id = batcher_storage_config.db_config.chain_id.clone();
    let mut chain_info = ChainInfo::create_for_testing();
    chain_info.chain_id = chain_id.clone();
    let batcher_config = create_batcher_config(batcher_storage_config, chain_info.clone());
    let gateway_config = create_gateway_config(chain_info).await;
    let http_server_config = create_http_server_config().await;
    let rpc_state_reader_config = test_rpc_state_reader_config(rpc_server_addr);
    let consensus_manager_config = ConsensusManagerConfig {
        consensus_config: ConsensusConfig { start_height: BlockNumber(1), ..Default::default() },
    };
    (
        SequencerNodeConfig {
            components,
            batcher_config,
            consensus_manager_config,
            gateway_config,
            http_server_config,
            rpc_state_reader_config,
            ..SequencerNodeConfig::default()
        },
        RequiredParams {
            chain_id,
            eth_fee_token_address: chain_info.fee_token_addresses.eth_fee_token_address,
            strk_fee_token_address: chain_info.fee_token_addresses.strk_fee_token_address,
        },
    )
}

pub fn test_rpc_state_reader_config(rpc_server_addr: SocketAddr) -> RpcStateReaderConfig {
    // TODO(Tsabary): get the latest version from the RPC crate.
    const RPC_SPEC_VERSION: &str = "V0_8";
    const JSON_RPC_VERSION: &str = "2.0";
    RpcStateReaderConfig {
        url: format!("http://{rpc_server_addr:?}/rpc/{RPC_SPEC_VERSION}"),
        json_rpc_version: JSON_RPC_VERSION.to_string(),
    }
}

/// Returns a unique IP address and port for testing purposes.
///
/// Tests run in parallel, so servers (like RPC or web) running on separate tests must have
/// different ports, otherwise the server will fail with "address already in use".
pub async fn get_available_socket() -> SocketAddr {
    // Dynamically select port.
    // First, set the port to 0 (dynamic port).
    TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind to address")
        // Then, resolve to the actual selected port.
        .local_addr()
        .expect("Failed to get local address")
}

/// A test utility client for interacting with an http server.
pub struct HttpTestClient {
    socket: SocketAddr,
    client: Client,
}

impl HttpTestClient {
    pub fn new(socket: SocketAddr) -> Self {
        let client = Client::new();
        Self { socket, client }
    }

    pub async fn assert_add_tx_success(&self, rpc_tx: RpcTransaction) -> TransactionHash {
        let response = self.add_tx(rpc_tx).await;
        assert!(response.status().is_success());

        response.json().await.unwrap()
    }

    // TODO: implement when usage eventually arises.
    pub async fn assert_add_tx_error(&self, _tx: RpcTransaction) -> GatewaySpecError {
        todo!()
    }

    // Prefer using assert_add_tx_success or other higher level methods of this client, to ensure
    // tests are boilerplate and implementation-detail free.
    pub async fn add_tx(&self, rpc_tx: RpcTransaction) -> Response {
        let tx_json = rpc_tx_to_json(&rpc_tx);
        self.client
            .post(format!("http://{}/add_tx", self.socket))
            .header("content-type", "application/json")
            .body(Body::from(tx_json))
            .send()
            .await
            .unwrap()
    }
}

/// Creates a multi-account transaction generator for integration tests.
pub fn create_integration_test_tx_generator() -> MultiAccountTransactionGenerator {
    let mut tx_generator: MultiAccountTransactionGenerator =
        MultiAccountTransactionGenerator::new();

    for account in [
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1),
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo0),
    ] {
        tx_generator.register_account_for_flow_test(account);
    }
    tx_generator
}

/// Creates and runs a scenario for the sequencer integration test. Returns a list of transaction
/// hashes, in the order they are expected to be in the mempool.
pub async fn run_integration_test_scenario<'a, Fut>(
    mut tx_generator: MultiAccountTransactionGenerator,
    send_rpc_tx_fn: &'a dyn Fn(RpcTransaction) -> Fut,
) -> Vec<TransactionHash>
where
    Fut: Future<Output = TransactionHash> + 'a,
{
    const ACCOUNT_ID_0: AccountId = 0;
    const ACCOUNT_ID_1: AccountId = 1;

    // Create RPC transactions.
    let account0_invoke_nonce1 =
        tx_generator.account_with_id(ACCOUNT_ID_0).generate_invoke_with_tip(2);
    let account0_invoke_nonce2 =
        tx_generator.account_with_id(ACCOUNT_ID_0).generate_invoke_with_tip(3);
    let account1_invoke_nonce1 =
        tx_generator.account_with_id(ACCOUNT_ID_1).generate_invoke_with_tip(4);

    let rpc_txs = vec![account0_invoke_nonce1, account0_invoke_nonce2, account1_invoke_nonce1];

    // Send RPC transactions.
    let mut tx_hashes = vec![];
    for rpc_tx in rpc_txs {
        tx_hashes.push(send_rpc_tx(rpc_tx, send_rpc_tx_fn).await);
    }

    // Return the transaction hashes in the order they should be given by the mempool:
    // Transactions from the same account are ordered by nonce; otherwise, higher tips are given
    // priority.
    vec![tx_hashes[2], tx_hashes[0], tx_hashes[1]]
}

/// Sends an RPC transaction using the supplied sending function, and returns its hash.
fn send_rpc_tx<'a, Fut>(
    rpc_tx: RpcTransaction,
    send_rpc_tx_fn: &'a dyn Fn(RpcTransaction) -> Fut,
) -> Fut
where
    Fut: Future<Output = TransactionHash> + 'a,
{
    send_rpc_tx_fn(rpc_tx)
}

async fn create_gateway_config(chain_info: ChainInfo) -> GatewayConfig {
    let stateless_tx_validator_config = StatelessTransactionValidatorConfig {
        validate_non_zero_l1_gas_fee: true,
        max_calldata_length: 10,
        max_signature_length: 2,
        ..Default::default()
    };
    let stateful_tx_validator_config = StatefulTransactionValidatorConfig::default();

    GatewayConfig { stateless_tx_validator_config, stateful_tx_validator_config, chain_info }
}

async fn create_http_server_config() -> HttpServerConfig {
    // TODO(Tsabary): use ser_generated_param.
    let socket = get_available_socket().await;
    HttpServerConfig { ip: socket.ip(), port: socket.port() }
}

fn create_batcher_config(
    batcher_storage_config: StorageConfig,
    chain_info: ChainInfo,
) -> BatcherConfig {
    // TODO(Arni): Create BlockBuilderConfig create for testing method and use here.
    const SEQUENCER_ADDRESS_FOR_TESTING: u128 = 1991;

    BatcherConfig {
        storage: batcher_storage_config,
        block_builder_config: BlockBuilderConfig {
            chain_info,
            sequencer_address: contract_address!(SEQUENCER_ADDRESS_FOR_TESTING),
            ..Default::default()
        },
        ..Default::default()
    }
}
