use std::env;
use std::fs::File;
use std::future::Future;
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use assert_matches::assert_matches;
use axum::body::Body;
use blockifier::context::{BlockContext, ChainInfo};
use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::{
    CairoVersion,
    BALANCE,
    CURRENT_BLOCK_TIMESTAMP,
    DEFAULT_ETH_L1_GAS_PRICE,
    DEFAULT_STRK_L1_GAS_PRICE,
    TEST_SEQUENCER_ADDRESS,
};
use blockifier::transaction::objects::FeeType;
use blockifier::versioned_constants::VersionedConstants;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use indexmap::IndexMap;
use mempool_test_utils::starknet_api_test_utils::{
    rpc_tx_to_json,
    AccountId,
    Contract,
    MultiAccountTransactionGenerator,
};
use papyrus_common::pending_classes::PendingClasses;
use papyrus_consensus::config::ConsensusConfig;
use papyrus_execution::execution_utils::get_nonce_at;
use papyrus_network::network_manager::test_utils::create_network_config_connected_to_broadcast_channels;
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::consensus::ProposalPart;
use papyrus_rpc::{run_server, RpcConfig};
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::class::ClassStorageWriter;
use papyrus_storage::compiled_class::CasmStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::state::{StateStorageReader, StateStorageWriter};
use papyrus_storage::test_utils::{get_test_storage, get_test_storage_with_config_by_scope};
use papyrus_storage::{StorageConfig, StorageReader, StorageWriter};
use reqwest::{Client, Response};
use rstest::{fixture, rstest};
use serde_json::{json, Value};
use starknet_api::abi::abi_utils::get_fee_token_var_address;
use starknet_api::block::{
    BlockBody,
    BlockHeader,
    BlockHeaderWithoutHash,
    BlockNumber,
    BlockTimestamp,
    GasPricePerToken,
};
use starknet_api::core::{ChainId, ClassHash, ContractAddress, Nonce, SequencerContractAddress};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::state::{StateNumber, StorageKey, ThinStateDiff};
use starknet_api::transaction::fields::Fee;
use starknet_api::transaction::TransactionHash;
use starknet_api::{contract_address, felt};
use starknet_batcher::block_builder::BlockBuilderConfig;
use starknet_batcher::config::BatcherConfig;
use starknet_client::reader::PendingData;
use starknet_consensus_manager::config::ConsensusManagerConfig;
use starknet_gateway::config::{
    GatewayConfig,
    RpcStateReaderConfig,
    StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_http_server::config::HttpServerConfig;
use starknet_monitoring_endpoint::config::MonitoringEndpointConfig;
use starknet_monitoring_endpoint::test_utils::IsAliveClient;
use starknet_sequencer_infra::trace_util::configure_tracing;
use starknet_sequencer_node::config::node_config::SequencerNodeConfig;
use starknet_sequencer_node::config::test_utils::RequiredParams;
use starknet_types_core::felt::Felt;
use strum::IntoEnumIterator;
use tempfile::{tempdir, TempDir};
use tokio::net::TcpListener;
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use tokio::task::{self, JoinHandle};
use tokio::time::interval;
use tracing::{error, info};

const NODE_CONFIG_CHANGES_FILE_PATH: &str = "node_integration_test_config_changes.json";

type ContractClassesMap =
    (Vec<(ClassHash, DeprecatedContractClass)>, Vec<(ClassHash, CasmContractClass)>);

pub struct StorageTestSetup {
    pub chain_id: ChainId,
    pub rpc_storage_reader: StorageReader,
    pub rpc_storage_handle: TempDir,
    pub batcher_storage_config: StorageConfig,
    pub batcher_storage_handle: TempDir,
}

impl StorageTestSetup {
    pub fn new(test_defined_accounts: Vec<Contract>) -> Self {
        let ((rpc_storage_reader, mut rpc_storage_writer), rpc_storage_file_handle) =
            get_test_storage();
        create_test_state(&mut rpc_storage_writer, test_defined_accounts.clone());
        let ((_, mut batcher_storage_writer), batcher_storage_config, batcher_storage_file_handle) =
            get_test_storage_with_config_by_scope(papyrus_storage::StorageScope::StateOnly);
        create_test_state(&mut batcher_storage_writer, test_defined_accounts);
        Self {
            chain_id: batcher_storage_config.db_config.chain_id.clone(),
            rpc_storage_reader,
            rpc_storage_handle: rpc_storage_file_handle,
            batcher_storage_config,
            batcher_storage_handle: batcher_storage_file_handle,
        }
    }
}

/// A variable number of identical accounts and test contracts are initialized and funded.
fn create_test_state(storage_writer: &mut StorageWriter, test_defined_accounts: Vec<Contract>) {
    let block_context = BlockContext::create_for_testing();

    let into_contract = |contract: FeatureContract| Contract {
        contract,
        sender_address: contract.get_instance_address(0),
    };
    let default_test_contracts = [
        FeatureContract::TestContract(CairoVersion::Cairo0),
        FeatureContract::TestContract(CairoVersion::Cairo1),
    ]
    .into_iter()
    .map(into_contract)
    .collect();

    let erc20_contract = FeatureContract::ERC20(CairoVersion::Cairo0);
    let erc20_contract = into_contract(erc20_contract);

    initialize_papyrus_test_state(
        storage_writer,
        block_context.chain_info(),
        test_defined_accounts,
        default_test_contracts,
        erc20_contract,
    );
}

fn initialize_papyrus_test_state(
    storage_writer: &mut StorageWriter,
    chain_info: &ChainInfo,
    test_defined_accounts: Vec<Contract>,
    default_test_contracts: Vec<Contract>,
    erc20_contract: Contract,
) {
    let state_diff = prepare_state_diff(
        chain_info,
        &test_defined_accounts,
        &default_test_contracts,
        &erc20_contract,
    );

    let contract_classes_to_retrieve =
        test_defined_accounts.into_iter().chain(default_test_contracts).chain([erc20_contract]);
    let (cairo0_contract_classes, cairo1_contract_classes) =
        prepare_compiled_contract_classes(contract_classes_to_retrieve);

    write_state_to_papyrus_storage(
        storage_writer,
        state_diff,
        &cairo0_contract_classes,
        &cairo1_contract_classes,
    )
}

fn prepare_state_diff(
    chain_info: &ChainInfo,
    test_defined_accounts: &[Contract],
    default_test_contracts: &[Contract],
    erc20_contract: &Contract,
) -> ThinStateDiff {
    let mut state_diff_builder = ThinStateDiffBuilder::new(chain_info);

    // Setup the common test contracts that are used by default in all test invokes.
    // TODO(batcher): this does nothing until we actually start excuting stuff in the batcher.
    state_diff_builder.set_contracts(default_test_contracts).declare().deploy();

    // Declare and deploy and the ERC20 contract, so that transfers from it can be made.
    state_diff_builder.set_contracts(std::slice::from_ref(erc20_contract)).declare().deploy();

    // TODO(deploy_account_support): once we have batcher with execution, replace with:
    // ```
    // state_diff_builder.set_contracts(accounts_defined_in_the_test).declare().fund();
    // ```
    // or use declare txs and transfers for both.
    state_diff_builder.inject_accounts_into_state(test_defined_accounts);

    state_diff_builder.build()
}

fn prepare_compiled_contract_classes(
    contract_classes_to_retrieve: impl Iterator<Item = Contract>,
) -> ContractClassesMap {
    let mut cairo0_contract_classes = Vec::new();
    let mut cairo1_contract_classes = Vec::new();
    for contract in contract_classes_to_retrieve {
        match contract.cairo_version() {
            CairoVersion::Cairo0 => {
                cairo0_contract_classes.push((
                    contract.class_hash(),
                    serde_json::from_str(&contract.raw_class()).unwrap(),
                ));
            }
            // todo(rdr): including both Cairo1 and Native versions for now. Temporal solution to
            // avoid compilation errors when using the "cairo_native" feature
            _ => {
                cairo1_contract_classes.push((
                    contract.class_hash(),
                    serde_json::from_str(&contract.raw_class()).unwrap(),
                ));
            }
        }
    }

    (cairo0_contract_classes, cairo1_contract_classes)
}

fn write_state_to_papyrus_storage(
    storage_writer: &mut StorageWriter,
    state_diff: ThinStateDiff,
    cairo0_contract_classes: &[(ClassHash, DeprecatedContractClass)],
    cairo1_contract_classes: &[(ClassHash, CasmContractClass)],
) {
    let block_number = BlockNumber(0);
    let block_header = test_block_header(block_number);
    let cairo0_contract_classes: Vec<_> =
        cairo0_contract_classes.iter().map(|(hash, contract)| (*hash, contract)).collect();

    let mut write_txn = storage_writer.begin_rw_txn().unwrap();

    for (class_hash, casm) in cairo1_contract_classes {
        write_txn = write_txn.append_casm(class_hash, casm).unwrap();
    }
    write_txn
        .append_header(block_number, &block_header)
        .unwrap()
        .append_body(block_number, BlockBody::default())
        .unwrap()
        .append_state_diff(block_number, state_diff)
        .unwrap()
        .append_classes(block_number, &[], &cairo0_contract_classes)
        .unwrap()
        .commit()
        .unwrap();
}

fn test_block_header(block_number: BlockNumber) -> BlockHeader {
    BlockHeader {
        block_header_without_hash: BlockHeaderWithoutHash {
            block_number,
            sequencer: SequencerContractAddress(contract_address!(TEST_SEQUENCER_ADDRESS)),
            l1_gas_price: GasPricePerToken {
                price_in_wei: DEFAULT_ETH_L1_GAS_PRICE.into(),
                price_in_fri: DEFAULT_STRK_L1_GAS_PRICE.into(),
            },
            l1_data_gas_price: GasPricePerToken {
                price_in_wei: DEFAULT_ETH_L1_GAS_PRICE.into(),
                price_in_fri: DEFAULT_STRK_L1_GAS_PRICE.into(),
            },
            l2_gas_price: GasPricePerToken {
                price_in_wei: VersionedConstants::latest_constants()
                    .convert_l1_to_l2_gas_price_round_up(DEFAULT_ETH_L1_GAS_PRICE.into()),
                price_in_fri: VersionedConstants::latest_constants()
                    .convert_l1_to_l2_gas_price_round_up(DEFAULT_STRK_L1_GAS_PRICE.into()),
            },
            timestamp: BlockTimestamp(CURRENT_BLOCK_TIMESTAMP),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Spawns a papyrus rpc server for given state reader.
/// Returns the address of the rpc server.
pub async fn spawn_test_rpc_state_reader(
    storage_reader: StorageReader,
    chain_id: ChainId,
) -> SocketAddr {
    let rpc_config = RpcConfig {
        chain_id,
        server_address: get_available_socket().await.to_string(),
        ..Default::default()
    };
    let (addr, handle) = run_server(
        &rpc_config,
        Arc::new(RwLock::new(None)),
        Arc::new(RwLock::new(PendingData::default())),
        Arc::new(RwLock::new(PendingClasses::default())),
        storage_reader,
        "NODE VERSION",
    )
    .await
    .unwrap();
    // Spawn the server handle to keep the server running, otherwise the server will stop once the
    // handler is out of scope.
    tokio::spawn(handle.stopped());
    addr
}

/// Constructs a thin state diff from lists of contracts, where each contract can be declared,
/// deployed, and in case it is an account, funded.
#[derive(Default)]
struct ThinStateDiffBuilder<'a> {
    contracts: &'a [Contract],
    deprecated_declared_classes: Vec<ClassHash>,
    declared_classes: IndexMap<ClassHash, starknet_api::core::CompiledClassHash>,
    deployed_contracts: IndexMap<ContractAddress, ClassHash>,
    storage_diffs: IndexMap<ContractAddress, IndexMap<StorageKey, Felt>>,
    // TODO(deploy_account_support): delete field once we have batcher with execution.
    nonces: IndexMap<ContractAddress, Nonce>,
    chain_info: ChainInfo,
    initial_account_balance: Felt,
}

impl<'a> ThinStateDiffBuilder<'a> {
    fn new(chain_info: &ChainInfo) -> Self {
        const TEST_INITIAL_ACCOUNT_BALANCE: Fee = BALANCE;
        let erc20 = FeatureContract::ERC20(CairoVersion::Cairo0);
        let erc20_class_hash = erc20.get_class_hash();

        let deployed_contracts: IndexMap<ContractAddress, ClassHash> = FeeType::iter()
            .map(|fee_type| (chain_info.fee_token_address(&fee_type), erc20_class_hash))
            .collect();

        Self {
            chain_info: chain_info.clone(),
            initial_account_balance: felt!(TEST_INITIAL_ACCOUNT_BALANCE.0),
            deployed_contracts,
            ..Default::default()
        }
    }

    fn set_contracts(&mut self, contracts: &'a [Contract]) -> &mut Self {
        self.contracts = contracts;
        self
    }

    fn declare(&mut self) -> &mut Self {
        for contract in self.contracts {
            match contract.cairo_version() {
                CairoVersion::Cairo0 => {
                    self.deprecated_declared_classes.push(contract.class_hash())
                }
                // todo(rdr): including both Cairo1 and Native versions for now. Temporal solution
                // to avoid compilation errors when using the "cairo_native" feature
                _ => {
                    self.declared_classes.insert(contract.class_hash(), Default::default());
                }
            }
        }
        self
    }

    fn deploy(&mut self) -> &mut Self {
        for contract in self.contracts {
            self.deployed_contracts.insert(contract.sender_address, contract.class_hash());
        }
        self
    }

    /// Only applies for contracts that are accounts, for non-accounts only declare and deploy work.
    fn fund(&mut self) -> &mut Self {
        for account in self.contracts {
            assert_matches!(
                account.contract,
                FeatureContract::AccountWithLongValidate(_)
                    | FeatureContract::AccountWithoutValidations(_)
                    | FeatureContract::FaultyAccount(_),
                "Only Accounts can be funded, {account:?} is not an account",
            );

            let fee_token_address = get_fee_token_var_address(account.sender_address);
            for fee_type in FeeType::iter() {
                self.storage_diffs
                    .entry(self.chain_info.fee_token_address(&fee_type))
                    .or_default()
                    .insert(fee_token_address, self.initial_account_balance);
            }
        }

        self
    }

    // TODO(deploy_account_support): delete method once we have batcher with execution.
    fn inject_accounts_into_state(&mut self, accounts_defined_in_the_test: &'a [Contract]) {
        self.set_contracts(accounts_defined_in_the_test).declare().deploy().fund();

        // Set nonces as 1 in the state so that subsequent invokes can pass validation.
        self.nonces = self
            .deployed_contracts
            .iter()
            .map(|(&address, _)| (address, Nonce(Felt::ONE)))
            .collect();
    }

    fn build(self) -> ThinStateDiff {
        ThinStateDiff {
            storage_diffs: self.storage_diffs,
            deployed_contracts: self.deployed_contracts,
            declared_classes: self.declared_classes,
            deprecated_declared_classes: self.deprecated_declared_classes,
            nonces: self.nonces,
            ..Default::default()
        }
    }
}

/// A utility macro that takes a list of config fields and returns a json dictionary with "field
/// name : field value" entries, where prefixed "config." name is removed from the entry key.
///
/// # Example (not running, to avoid function visibility modifications):
///
/// use serde_json::json;
/// struct ConfigStruct {
///    field_1: u32,
///    field_2: String,
///    field_3: u32,
/// }
/// let config = ConfigStruct { field_1: 1, field_2: "2".to_string() , field_3: 3};
/// let json_data = config_fields_to_json!(config.field_1, config.field_2);
/// assert_eq!(json_data, json!({"field_1": 1, "field_2": "2"}));
macro_rules! config_fields_to_json {
    ( $( $expr:expr ),+ , ) => {
        json!({
            $(
                strip_config_prefix(stringify!($expr)): $expr
            ),+
        })
    };
}

/// Creates a config file for the sequencer node for the end to end integration test.
pub(crate) fn dump_config_file_changes(
    config: &SequencerNodeConfig,
    required_params: RequiredParams,
    dir: PathBuf,
) -> PathBuf {
    // Dump config changes file for the sequencer node.
    // TODO(Tsabary): auto dump the entirety of RequiredParams fields.
    let json_data = config_fields_to_json!(
        required_params.chain_id,
        required_params.eth_fee_token_address,
        required_params.strk_fee_token_address,
        required_params.sequencer_address,
        config.rpc_state_reader_config.json_rpc_version,
        config.rpc_state_reader_config.url,
        config.batcher_config.storage.db_config.path_prefix,
        config.http_server_config.ip,
        config.http_server_config.port,
        config.consensus_manager_config.consensus_config.start_height,
    );
    let node_config_path = dump_json_data(json_data, NODE_CONFIG_CHANGES_FILE_PATH, dir);
    assert!(node_config_path.exists(), "File does not exist: {:?}", node_config_path);

    node_config_path
}

/// Dumps the input JSON data to a file at the specified path.
fn dump_json_data(json_data: Value, path: &str, dir: PathBuf) -> PathBuf {
    let temp_dir_path = dir.join(path);
    // Serialize the JSON data to a pretty-printed string
    let json_string = serde_json::to_string_pretty(&json_data).unwrap();

    // Write the JSON string to a file
    let mut file = File::create(&temp_dir_path).unwrap();
    file.write_all(json_string.as_bytes()).unwrap();

    info!("Writing required config changes to: {:?}", &temp_dir_path);
    temp_dir_path
}

/// Strips the "config." and "required_params." prefixes from the input string.
fn strip_config_prefix(input: &str) -> &str {
    input
        .strip_prefix("config.")
        .or_else(|| input.strip_prefix("required_params."))
        .unwrap_or(input)
}

pub async fn create_config(
    rpc_server_addr: SocketAddr,
    batcher_storage_config: StorageConfig,
) -> (SequencerNodeConfig, RequiredParams, BroadcastTopicChannels<ProposalPart>) {
    let chain_id = batcher_storage_config.db_config.chain_id.clone();
    // TODO(Tsabary): create chain_info in setup, and pass relevant values throughout.
    let mut chain_info = ChainInfo::create_for_testing();
    chain_info.chain_id = chain_id.clone();
    let fee_token_addresses = chain_info.fee_token_addresses.clone();
    let batcher_config = create_batcher_config(batcher_storage_config, chain_info.clone());
    let gateway_config = create_gateway_config(chain_info).await;
    let http_server_config = create_http_server_config().await;
    let rpc_state_reader_config = test_rpc_state_reader_config(rpc_server_addr);
    let (consensus_manager_config, consensus_proposals_channels) =
        create_consensus_manager_config_and_channels();
    (
        SequencerNodeConfig {
            batcher_config,
            consensus_manager_config,
            gateway_config,
            http_server_config,
            rpc_state_reader_config,
            ..SequencerNodeConfig::default()
        },
        RequiredParams {
            chain_id,
            eth_fee_token_address: fee_token_addresses.eth_fee_token_address,
            strk_fee_token_address: fee_token_addresses.strk_fee_token_address,
            sequencer_address: ContractAddress::from(1312_u128), // Arbitrary non-zero value.
        },
        consensus_proposals_channels,
    )
}

fn create_consensus_manager_config_and_channels()
-> (ConsensusManagerConfig, BroadcastTopicChannels<ProposalPart>) {
    let (network_config, broadcast_channels) =
        create_network_config_connected_to_broadcast_channels(
            papyrus_network::gossipsub_impl::Topic::new(
                starknet_consensus_manager::consensus_manager::NETWORK_TOPIC,
            ),
        );
    let consensus_manager_config = ConsensusManagerConfig {
        consensus_config: ConsensusConfig {
            start_height: BlockNumber(1),
            consensus_delay: Duration::from_secs(1),
            network_config,
            ..Default::default()
        },
    };
    (consensus_manager_config, broadcast_channels)
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

fn create_txs_for_integration_test(
    mut tx_generator: MultiAccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    const ACCOUNT_ID_0: AccountId = 0;
    const ACCOUNT_ID_1: AccountId = 1;

    // Create RPC transactions.
    let account0_invoke_nonce1 =
        tx_generator.account_with_id(ACCOUNT_ID_0).generate_invoke_with_tip(2);
    let account0_invoke_nonce2 =
        tx_generator.account_with_id(ACCOUNT_ID_0).generate_invoke_with_tip(3);
    let account1_invoke_nonce1 =
        tx_generator.account_with_id(ACCOUNT_ID_1).generate_invoke_with_tip(4);

    vec![account0_invoke_nonce1, account0_invoke_nonce2, account1_invoke_nonce1]
}

fn create_account_txs(
    mut tx_generator: MultiAccountTransactionGenerator,
    account_id: AccountId,
    n_txs: usize,
) -> Vec<RpcTransaction> {
    (0..n_txs)
        .map(|_| tx_generator.account_with_id(account_id).generate_invoke_with_tip(1))
        .collect()
}

async fn send_rpc_txs<'a, Fut>(
    rpc_txs: Vec<RpcTransaction>,
    send_rpc_tx_fn: &'a mut dyn FnMut(RpcTransaction) -> Fut,
) -> Vec<TransactionHash>
where
    Fut: Future<Output = TransactionHash> + 'a,
{
    let mut tx_hashes = vec![];
    for rpc_tx in rpc_txs {
        tx_hashes.push(send_rpc_tx_fn(rpc_tx).await);
    }
    tx_hashes
}

/// Creates and runs the integration test scenario for the sequencer integration test. Returns a
/// list of transaction hashes, in the order they are expected to be in the mempool.
pub async fn run_integration_test_scenario<'a, Fut>(
    tx_generator: MultiAccountTransactionGenerator,
    send_rpc_tx_fn: &'a mut dyn FnMut(RpcTransaction) -> Fut,
) -> Vec<TransactionHash>
where
    Fut: Future<Output = TransactionHash> + 'a,
{
    let rpc_txs = create_txs_for_integration_test(tx_generator);
    let tx_hashes = send_rpc_txs(rpc_txs, send_rpc_tx_fn).await;

    // Return the transaction hashes in the order they should be given by the mempool:
    // Transactions from the same account are ordered by nonce; otherwise, higher tips are given
    // priority.
    assert!(
        tx_hashes.len() == 3,
        "Unexpected number of transactions sent in the integration test scenario. Found {} \
         transactions",
        tx_hashes.len()
    );
    vec![tx_hashes[2], tx_hashes[0], tx_hashes[1]]
}

/// Returns a list of the transaction hashes, in the order they are expected to be in the mempool.
pub async fn send_account_txs<'a, Fut>(
    tx_generator: MultiAccountTransactionGenerator,
    account_id: AccountId,
    n_txs: usize,
    send_rpc_tx_fn: &'a mut dyn FnMut(RpcTransaction) -> Fut,
) -> Vec<TransactionHash>
where
    Fut: Future<Output = TransactionHash> + 'a,
{
    let rpc_txs = create_account_txs(tx_generator, n_txs, account_id);
    send_rpc_txs(rpc_txs, send_rpc_tx_fn).await
}

pub async fn create_gateway_config(chain_info: ChainInfo) -> GatewayConfig {
    let stateless_tx_validator_config = StatelessTransactionValidatorConfig {
        validate_non_zero_l1_gas_fee: true,
        max_calldata_length: 10,
        max_signature_length: 2,
        ..Default::default()
    };
    let stateful_tx_validator_config = StatefulTransactionValidatorConfig::default();

    GatewayConfig { stateless_tx_validator_config, stateful_tx_validator_config, chain_info }
}

pub async fn create_http_server_config() -> HttpServerConfig {
    // TODO(Tsabary): use ser_generated_param.
    let socket = get_available_socket().await;
    HttpServerConfig { ip: socket.ip(), port: socket.port() }
}

pub fn create_batcher_config(
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

pub struct IntegrationTestSetup {
    // Client for adding transactions to the sequencer node.
    pub add_tx_http_client: HttpTestClient,
    // Client for checking liveness of the sequencer node.
    pub is_alive_test_client: IsAliveClient,
    // Path to the node configuration file.
    pub node_config_path: PathBuf,
    // Storage reader for the batcher.
    pub batcher_storage_config: StorageConfig,
    // Handlers for the storage and config files, maintained so the files are not deleted. Since
    // these are only maintained to avoid dropping the handlers, private visibility suffices, and
    // as such, the '#[allow(dead_code)]' attributes are used to suppress the warning.
    #[allow(dead_code)]
    batcher_storage_handle: TempDir,
    #[allow(dead_code)]
    rpc_storage_handle: TempDir,
    #[allow(dead_code)]
    node_config_dir_handle: TempDir,
}

impl IntegrationTestSetup {
    pub async fn new_from_tx_generator(tx_generator: &MultiAccountTransactionGenerator) -> Self {
        // Creating the storage for the test.
        let storage_for_test = StorageTestSetup::new(tx_generator.accounts());

        // Spawn a papyrus rpc server for a papyrus storage reader.
        let rpc_server_addr = spawn_test_rpc_state_reader(
            storage_for_test.rpc_storage_reader,
            storage_for_test.chain_id,
        )
        .await;

        // Derive the configuration for the sequencer node.
        let (config, required_params, _) =
            create_config(rpc_server_addr, storage_for_test.batcher_storage_config).await;

        let node_config_dir_handle = tempdir().unwrap();
        let node_config_path = dump_config_file_changes(
            &config,
            required_params,
            node_config_dir_handle.path().to_path_buf(),
        );

        // Wait for the node to start.
        let MonitoringEndpointConfig { ip, port } = config.monitoring_endpoint_config;
        let is_alive_test_client = IsAliveClient::new(SocketAddr::from((ip, port)));

        let HttpServerConfig { ip, port } = config.http_server_config;
        let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

        IntegrationTestSetup {
            add_tx_http_client,
            is_alive_test_client,
            batcher_storage_handle: storage_for_test.batcher_storage_handle,
            batcher_storage_config: config.batcher_config.storage,
            rpc_storage_handle: storage_for_test.rpc_storage_handle,
            node_config_dir_handle,
            node_config_path,
        }
    }
}

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    create_integration_test_tx_generator()
}

// TODO(Tsabary): Move to a suitable util location.
async fn spawn_node_child_task(node_config_path: PathBuf) -> Child {
    // Get the current working directory for the project
    let project_path = env::current_dir().expect("Failed to get current directory").join("../..");

    let filtered_vars: Vec<String> = env::vars()
        .filter(|(key, _value)| key.starts_with("CARGO_"))
        .map(|(key, _value)| key) // Extract only the key
        .collect();

    // Command::new(env!("CARGO_BIN_EXE_my_crate"))
    let mut command = Command::new(env!("CARGO_BIN_EXE_starknet_sequencer_node"));

    command
        .current_dir(&project_path)
        .arg("--config_file")
        .arg(node_config_path.to_str().unwrap())
        .stderr(Stdio::inherit())
        .stdout(Stdio::inherit())
        .kill_on_drop(true); // Required for stopping the node when the handle is dropped.

    for cargo_env_var in filtered_vars {
        command.env_remove(cargo_env_var);
    }

    command.spawn().expect("Failed to spawn the sequencer node.")

    // TODO(Tsabary): Capture output to a log file, and present it in case of a failure.
    // TODO(Tsabary): Change invocation from "cargo run" to separate compilation and invocation
    // (build, and then invoke the binary).
    // Command::new("cargo")
    //     .arg("run")
    //     .arg("--bin")
    //     .arg("starknet_sequencer_node")
    //     .arg("--quiet")
    //     .current_dir(&project_path)
    //     .arg("--")
    //     .arg("--config_file")
    //     .arg(node_config_path.to_str().unwrap())
    //     .stderr(Stdio::inherit())
    //     .stdout(Stdio::null())
    //     .kill_on_drop(true) // Required for stopping the node when the handle is dropped.
    //     .spawn()
    //     .expect("Failed to spawn the sequencer node.")
}

async fn spawn_run_node(node_config_path: PathBuf) -> JoinHandle<()> {
    task::spawn(async move {
        info!("Running the node from its spawned task.");
        let _node_run_result = spawn_node_child_task(node_config_path).
            await. // awaits the completion of spawn_node_child_task.
            wait(). // runs the node until completion -- should be running indefinitely.
            await; // awaits the completion of the node.
        panic!("Node stopped unexpectedly.");
    })
}

/// Reads the latest block number from the storage.
fn get_latest_block_number(storage_reader: &StorageReader) -> BlockNumber {
    let txn = storage_reader.begin_ro_txn().unwrap();
    txn.get_state_marker()
        .expect("There should always be a state marker")
        .prev()
        .expect("There should be a previous block in the storage, set by the test setup")
}

/// Reads an account nonce after a block number from storage.
fn get_account_nonce(
    storage_reader: &StorageReader,
    block_number: BlockNumber,
    contract_address: ContractAddress,
) -> Nonce {
    let txn = storage_reader.begin_ro_txn().unwrap();
    let state_number = StateNumber::unchecked_right_after_block(block_number);
    get_nonce_at(&txn, state_number, None, contract_address)
        .expect("Should always be Ok(Some(Nonce))")
        .expect("Should always be Some(Nonce)")
}

/// Sample a storage until sufficiently many blocks have been stored. Returns an error if after
/// the given number of attempts the target block number has not been reached.
async fn await_block(
    interval_duration: Duration,
    target_block_number: BlockNumber,
    max_attempts: usize,
    storage_reader: &StorageReader,
) -> Result<(), ()> {
    let mut interval = interval(interval_duration);
    let mut count = 0;
    loop {
        // Read the latest block number.
        let latest_block_number = get_latest_block_number(storage_reader);
        count += 1;

        // Check if reached the target block number.
        if latest_block_number >= target_block_number {
            info!("Found block {} after {} queries.", target_block_number, count);
            return Ok(());
        }

        // Check if reached the maximum attempts.
        if count > max_attempts {
            error!(
                "Latest block is {}, expected {}, stopping sampling.",
                latest_block_number, target_block_number
            );
            return Err(());
        }

        // Wait for the next interval.
        interval.tick().await;
    }
}

#[rstest]
#[tokio::test]
async fn test_end_to_end_integration(mut tx_generator: MultiAccountTransactionGenerator) {
    const EXPECTED_BLOCK_NUMBER: BlockNumber = BlockNumber(15);

    configure_tracing();
    info!("Running integration test setup.");

    // Creating the storage for the test.

    let integration_test_setup = IntegrationTestSetup::new_from_tx_generator(&tx_generator).await;

    info!("Running sequencer node.");
    let node_run_handle = spawn_run_node(integration_test_setup.node_config_path).await;

    // Wait for the node to start.
    match integration_test_setup.is_alive_test_client.await_alive(Duration::from_secs(5), 30).await
    {
        Ok(_) => {}
        Err(_) => panic!("Node is not alive."),
    }

    info!("Running integration test simulator.");

    let send_rpc_tx_fn =
        &mut |rpc_tx| integration_test_setup.add_tx_http_client.assert_add_tx_success(rpc_tx);

    const ACCOUNT_ID_0: AccountId = 0;
    let n_txs = 50;
    let sender_address = tx_generator.account_with_id(ACCOUNT_ID_0).sender_address();
    info!("Sending {n_txs} txs.");
    let tx_hashes = send_account_txs(tx_generator, ACCOUNT_ID_0, n_txs, send_rpc_tx_fn).await;

    info!("Awaiting until {EXPECTED_BLOCK_NUMBER} blocks have been created.");

    let (batcher_storage_reader, _) =
        papyrus_storage::open_storage(integration_test_setup.batcher_storage_config)
            .expect("Failed to open batcher's storage");

    match await_block(Duration::from_secs(5), EXPECTED_BLOCK_NUMBER, 15, &batcher_storage_reader)
        .await
    {
        Ok(_) => {}
        Err(_) => panic!("Did not reach expected block number."),
    }

    info!("Shutting the node down.");
    node_run_handle.abort();
    let res = node_run_handle.await;
    assert!(
        res.expect_err("Node should have been stopped.").is_cancelled(),
        "Node should have been stopped."
    );

    info!("Verifying tx sender account nonce.");
    let expected_nonce_value = tx_hashes.len() + 1;
    let expected_nonce =
        Nonce(Felt::from_hex_unchecked(format!("0x{:X}", expected_nonce_value).as_str()));
    let nonce = get_account_nonce(&batcher_storage_reader, EXPECTED_BLOCK_NUMBER, sender_address);
    assert_eq!(nonce, expected_nonce);
}
