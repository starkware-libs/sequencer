use std::env;

use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use blockifier_reexecution::state_reader::rpc_state_reader::RpcStateReader;
use blockifier_reexecution::utils::get_chain_info;
use rstest::fixture;
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::core::{ChainId, ContractAddress};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::{RpcInvokeTransaction, RpcInvokeTransactionV3, RpcTransaction};
use starknet_api::transaction::fields::{AllResourceBounds, ResourceBounds};
use starknet_types_core::felt::Felt;
use url::Url;

use crate::running::rpc_records::{
    record_path,
    records_exist,
    MockRpcServer,
    RecordingProxy,
    RpcRecords,
};
use crate::running::runner::{RpcRunnerFactory, RunnerConfig};
use crate::running::storage_proofs::{RpcStorageProofsProvider, StorageProofConfig};
use crate::running::virtual_block_executor::{
    RpcVirtualBlockExecutor,
    RpcVirtualBlockExecutorConfig,
};

/// Block number to use for testing (mainnet block with known state).
pub const TEST_BLOCK_NUMBER: u64 = 800000;

/// STRK token contract address on mainnet.
pub const STRK_TOKEN_ADDRESS: Felt =
    Felt::from_hex_unchecked("0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d");

/// A known account address on mainnet (Starknet Foundation).
pub const SENDER_ADDRESS: Felt =
    Felt::from_hex_unchecked("0x01176a1bd84444c89232ec27754698e5d2e7e1a7f1539f12027f28b23ec9f3d8");

// --- Sepolia ---

/// STRK token contract address on Sepolia.
pub const STRK_TOKEN_ADDRESS_SEPOLIA: Felt =
    Felt::from_hex_unchecked("0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d");

/// Dummy account on Sepolia (no signature validation required).
/// This account uses the `account_with_dummy_validate` contract which always returns VALIDATED.
pub const DUMMY_ACCOUNT_ADDRESS: Felt =
    Felt::from_hex_unchecked("0x0786ed7d8dcbf1489241d65a4dd55f18b984c078558ce12def69802526fa918e");

/// Gets the RPC URL from the `NODE_URL` env var (defaults to `http://localhost:9545`).
pub fn get_rpc_url() -> String {
    env::var("NODE_URL").unwrap_or_else(|_| "http://localhost:9545/rpc/v0_10".to_string())
}

/// Gets the chain ID from the `CHAIN_ID` env var (defaults to `ChainId::Sepolia`).
pub fn get_chain_id() -> ChainId {
    env::var("CHAIN_ID").map_or(ChainId::Sepolia, |s| s.into())
}

/// Gets an optional STRK fee token address override from the `STRK_FEE_TOKEN_ADDRESS` env var.
/// Returns `None` if the env var is not set.
pub fn get_strk_fee_token_override() -> Option<ContractAddress> {
    env::var("STRK_FEE_TOKEN_ADDRESS").ok().map(|s| {
        ContractAddress::try_from(Felt::from_hex_unchecked(&s))
            .expect("Invalid STRK_FEE_TOKEN_ADDRESS")
    })
}

/// Fixture that creates an RpcStateReader for mainnet testing.
#[fixture]
pub fn rpc_state_reader() -> RpcStateReader {
    let node_url = get_rpc_url();
    RpcStateReader::new_with_config_from_url(
        node_url,
        get_chain_info(&ChainId::Mainnet, None),
        BlockId::Number(BlockNumber(TEST_BLOCK_NUMBER)),
    )
}

/// Fixture that creates an RpcVirtualBlockExecutor for mainnet testing.
#[fixture]
pub fn rpc_virtual_block_executor(rpc_state_reader: RpcStateReader) -> RpcVirtualBlockExecutor {
    RpcVirtualBlockExecutor {
        rpc_state_reader,
        // Skip transaction validation for testing.
        validate_txs: false,
        // TODO(Aviv): enable the config once there is a v0.10+ node that supports the
        // `RETURN_INITIAL_READS` flag on testnet.
        config: RpcVirtualBlockExecutorConfig { prefetch_state: false, ..Default::default() },
    }
}

/// Fixture that creates an RpcStorageProofsProvider for mainnet testing.
#[fixture]
pub fn rpc_provider() -> RpcStorageProofsProvider {
    let rpc_url = Url::parse(&get_rpc_url()).expect("Invalid RPC URL");
    RpcStorageProofsProvider::new(rpc_url)
}

/// Creates an [`RpcRunnerFactory`] pointed at the given RPC URL.
///
/// Chain ID and STRK fee token address are read from the environment via [`get_chain_id`] and
/// [`get_strk_fee_token_override`]. The factory runs the committer, meaning it will:
/// - Build a FactsDb from RPC proofs and execution data.
/// - Execute the committer to compute new state roots.
/// - Generate commitment infos with actual root changes.
pub(crate) fn runner_factory(rpc_url: &str) -> RpcRunnerFactory {
    let rpc_url = Url::parse(rpc_url).expect("Invalid RPC URL");
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());

    let runner_config = RunnerConfig {
        storage_proof_config: StorageProofConfig { include_state_changes: true },
        // TODO(Aviv): enable the config once there is a v0.10+ node that supports the
        // `RETURN_INITIAL_READS` flag on testnet.
        virtual_block_executor_config: RpcVirtualBlockExecutorConfig {
            prefetch_state: false,
            ..Default::default()
        },
    };

    let chain_info = get_chain_info(&get_chain_id(), get_strk_fee_token_override());
    RpcRunnerFactory::new(rpc_url, chain_info, contract_class_manager, runner_config)
}

/// Holds the test RPC infrastructure (proxy or mock server) alive for the duration of a test.
///
/// In recording mode, call [`TestRpcSetup::finalize`] after the test to save the recorded
/// interactions to disk.
pub(crate) enum TestRpcSetup {
    /// Live mode: direct RPC access to a real node.
    Live { rpc_url: String },
    /// Recording mode: proxy that forwards to a real node while recording interactions.
    Recording { proxy: RecordingProxy, test_name: String },
    /// Offline mode: mock server that replays pre-recorded interactions.
    Offline { server: MockRpcServer },
}

impl TestRpcSetup {
    /// Returns the RPC URL to use for this test mode.
    pub(crate) fn rpc_url(&self) -> String {
        match self {
            TestRpcSetup::Live { rpc_url } => rpc_url.clone(),
            TestRpcSetup::Recording { proxy, .. } => proxy.url.clone(),
            TestRpcSetup::Offline { server } => server.url(),
        }
    }

    /// Finalizes the test setup. In recording mode, saves the recorded interactions to disk.
    /// No-op in live and offline modes.
    pub(crate) fn finalize(self) {
        if let TestRpcSetup::Recording { proxy, test_name } = self {
            let records = proxy.into_records();
            records.save(&record_path(&test_name));
        }
    }
}

/// Resolves the test mode and sets up the appropriate RPC infrastructure.
///
/// Mode priority:
/// 1. `RECORD_RPC_RECORDS=1` env var set -> **Recording mode**: starts a recording proxy that
///    forwards to the real RPC node (via `NODE_URL`) while capturing all request/response pairs.
/// 2. Records file exists for `test_name` -> **Offline mode**: starts a mock HTTP server that
///    replays pre-recorded interactions (no network access required).
/// 3. Otherwise -> **Live mode**: uses the real RPC URL directly (requires `NODE_URL`).
pub(crate) async fn resolve_test_mode(test_name: &str) -> TestRpcSetup {
    if env::var("RECORD_RPC_RECORDS").is_ok() {
        let real_url = get_rpc_url();
        let proxy = RecordingProxy::new(&real_url).await;
        TestRpcSetup::Recording { proxy, test_name: test_name.to_string() }
    } else if records_exist(test_name) {
        let records = RpcRecords::load(&record_path(test_name));
        let server = MockRpcServer::new(&records).await;
        TestRpcSetup::Offline { server }
    } else {
        let rpc_url = get_rpc_url();
        TestRpcSetup::Live { rpc_url }
    }
}

/// Builds a client-side `RpcTransaction::Invoke` (v3) for the given sender and calldata.
///
/// Resource bounds for client-side transactions in tests.
/// Provides enough L2 gas for execution without fee enforcement.
pub fn resource_bounds_for_client_side_tx() -> AllResourceBounds {
    AllResourceBounds {
        l2_gas: ResourceBounds {
            max_amount: GasAmount(10_000_000),
            max_price_per_unit: GasPrice(0),
        },
        ..Default::default()
    }
}

/// Uses minimal resource bounds suitable for client-side (virtual) execution — the transaction
/// is never broadcast on-chain so fees are irrelevant.
pub(crate) fn build_client_side_rpc_invoke(
    sender_address: ContractAddress,
    calldata: starknet_api::transaction::fields::Calldata,
) -> RpcTransaction {
    let rpc_invoke_v3 = RpcInvokeTransactionV3 {
        sender_address,
        calldata,
        resource_bounds: resource_bounds_for_client_side_tx(),
        signature: Default::default(),
        nonce: Default::default(),
        tip: Default::default(),
        paymaster_data: Default::default(),
        account_deployment_data: Default::default(),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
        proof_facts: Default::default(),
        proof: Default::default(),
    };
    RpcTransaction::Invoke(RpcInvokeTransaction::V3(rpc_invoke_v3))
}
