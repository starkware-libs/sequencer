#![allow(dead_code)] // Test utilities - used in test modules that aren't visible during library compilation

use std::fs::File;
use std::process::Command;
use std::sync::Arc;

use alloy::network::TransactionBuilder;
use alloy::node_bindings::NodeError as AnvilError;
use alloy::primitives::{address as ethereum_address, I256, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use alloy::sol;
use alloy::sol_types::SolValue;
use ethers::utils::{Ganache, GanacheInstance};
use starknet_api::hash::StarkHash;
use tar::Archive;
use tempfile::{tempdir, TempDir};
use tracing::debug;
use url::Url;

use crate::ethereum_base_layer_contract::{
    EthereumBaseLayerConfig,
    EthereumBaseLayerContract,
    EthereumContractAddress,
    Starknet,
};

type TestEthereumNodeHandle = (GanacheInstance, TempDir);

/// Wrapper for Anvil provider that keeps the Anvil instance alive
/// This allows us to use connect_anvil_with_wallet_and_config while still
/// being able to return something that provides the endpoint URL
pub struct AnvilNodeHandle {
    url: Url,
    // Hold the provider to keep the Anvil instance alive
    // Using Arc to share the provider
    _provider: Arc<dyn std::any::Any + Send + Sync>,
}

impl AnvilNodeHandle {
    fn new(url: Url, provider: Arc<dyn std::any::Any + Send + Sync>) -> Self {
        Self { url, _provider: provider }
    }

    /// Returns the endpoint URL for the Anvil instance
    pub fn endpoint_url(&self) -> Url {
        self.url.clone()
    }
}

const MINIMAL_GANACHE_VERSION: u8 = 7;

// This address is commonly used as the L1 address of the Starknet core contract.
// TODO(Arni): Replace with constant with use of `AnvilInstance::address(&self)`.
pub const DEFAULT_ANVIL_L1_DEPLOYED_ADDRESS: &str = "0x5fbdb2315678afecb367f032d93f642f64180aa3";

// Default funded accounts.
// This address is the sender address of messages sent to L2 by Anvil.
// Given an `AnvilInstance`, this address can be retrieved by calling `anvil.addresses()[0]`.
pub const DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS: StarkHash =
    StarkHash::from_hex_unchecked("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
/// One of the 10 pre-funded Anvil preloaded accounts. Retrieved by calling `anvil.addresses()[3]`.
// TODO(Gilad): consider moving into anvil base layer.
pub const ARBITRARY_ANVIL_L1_ACCOUNT_ADDRESS: EthereumContractAddress =
    ethereum_address!("0x90F79bf6EB2c4f870365E785982E1f101E93b906");
/// One of the 10 pre-funded Anvil preloaded accounts. Retrieved by calling `anvil.addresses()[4]`.
// TODO(Gilad): consider moving into anvil base layer.
pub const OTHER_ARBITRARY_ANVIL_L1_ACCOUNT_ADDRESS: EthereumContractAddress =
    ethereum_address!("0x15d34AAf54267DB7D7c367839AAf71A00a2C6A65");

// TODO(dan): remove this once we fully support anvil
// ***
// DEPRECATED: Use the anvil constructor, this constructor is deprecated as it uses ganache.
// ***
//
// Returns a Ganache instance, preset with a Starknet core contract and some state updates:
// Starknet contract address: 0xe2aF2c1AE11fE13aFDb7598D0836398108a4db0A
//     Ethereum block number   starknet block number   starknet block hash
//      10                      100                     0x100
//      20                      200                     0x200
//      30                      300                     0x300
// The blockchain is at Ethereum block number 31.
// Note: Requires Ganache@7.4.3 installed.
// TODO(Gilad): `Ganache` and `ethers` have both been deprecated. Also, `ethers`s' replacement,
// `alloy`, no longer supports `Ganache`. Once we fully support anvil, remove this util, `ganache`
// and `ethers``.
#[deprecated(note = "Ganache is dead and will be removed soon, use Anvil instead, unless you \
                     need something we don't support on anvil yet.")]
pub fn get_test_ethereum_node() -> (TestEthereumNodeHandle, EthereumContractAddress) {
    const SN_CONTRACT_ADDR: &str = "0xe2aF2c1AE11fE13aFDb7598D0836398108a4db0A";
    // Verify correct Ganache version.
    let ganache_version = String::from_utf8_lossy(
        &Command::new("ganache")
            .arg("--version")
            .output()
            .expect("Failed to get Ganache version, check if it is installed.")
            .stdout,
    )
    .to_string();
    const GANACHE_VERSION_PREFIX: &str = "ganache v";
    let ganache_version = ganache_version
        .strip_prefix(GANACHE_VERSION_PREFIX)
        .expect("Failed to parse Ganache version.");
    let major_version = ganache_version
        .split('.')
        .next()
        .expect("Failed to parse Ganache major version.")
        .parse::<u8>()
        .expect("Failed to parse Ganache major version.");
    assert!(
        major_version >= MINIMAL_GANACHE_VERSION,
        "Wrong Ganache version, expecting at least version 7. To install, run `npm install -g \
         ganache`."
    );
    const DB_NAME: &str = "ganache-db";
    let db_archive_path = format!("resources/{DB_NAME}.tar");

    // Unpack the Ganache db tar file into a temporary dir.
    let mut archive = Archive::new(File::open(db_archive_path).expect("Ganache db not found."));
    let ganache_db = tempdir().unwrap();
    archive.unpack(ganache_db.path()).unwrap();

    // Start Ganache instance. This will panic if Ganache is not installed.
    let db_path = ganache_db.path().join(DB_NAME);
    let ganache = Ganache::new().args(["--db", db_path.to_str().unwrap()]).spawn();

    ((ganache, ganache_db), SN_CONTRACT_ADDR.to_string().parse().unwrap())
}

// FIXME: This should be part of AnvilBaseLayer, however the usage in the simulator doesn't allow
// that, since it is coupled with a manual invocation of an anvil instance that is managed inside
// the github workflow.
pub async fn make_block_history_on_anvil(
    sender_address: EthereumContractAddress,
    receiver_address: EthereumContractAddress,
    base_layer_config: EthereumBaseLayerConfig,
    url: &Url,
    num_blocks: usize,
) {
    let base_layer = EthereumBaseLayerContract::new(base_layer_config.clone(), url.clone());
    let provider = base_layer.contract.provider();
    let mut prev_block_number =
        usize::try_from(provider.get_block_number().await.unwrap()).unwrap();
    for _ in 0..num_blocks {
        let tx = TransactionRequest::default()
            .with_from(sender_address)
            .with_to(receiver_address)
            .with_value(U256::from(100));
        let pending =
            provider.send_transaction(tx).await.expect("Could not post transaction to base layer");
        let receipt: alloy::rpc::types::TransactionReceipt = pending
            .get_receipt()
            .await
            .expect("Could not get receipt for transaction to base layer");
        debug!(
            "Added L1 transaction to L1 block: {} with gas price: {}, blob price: {}",
            receipt.block_number.unwrap(),
            receipt.effective_gas_price,
            receipt.blob_gas_price.unwrap()
        );
        // Make sure the transactions trigger creation of new blocks.
        let new_block_number = usize::try_from(receipt.block_number.unwrap()).unwrap();
        assert!(new_block_number > prev_block_number);
        prev_block_number = new_block_number;
    }
}

/// Mine multiple blocks instantly on Anvil using the `anvil_mine` RPC method.
///
/// Note: This creates empty blocks. For blocks with transactions, use the
/// `make_block_history_on_anvil` function instead.
pub async fn anvil_mine_blocks(
    base_layer_config: EthereumBaseLayerConfig,
    num_blocks: u64,
    url: &Url,
) {
    let base_layer = EthereumBaseLayerContract::new(base_layer_config.clone(), url.clone());
    let provider = base_layer.contract.provider();

    let block_before = provider.get_block_number().await.expect("Failed to get block number");
    debug!("Block number before mining: {}", block_before);

    let _result: Option<String> = provider
        .raw_request("anvil_mine".into(), [num_blocks])
        .await
        .expect("Failed to mine blocks on Anvil");

    let block_after = provider.get_block_number().await.expect("Failed to get block number");
    debug!("Block number after mining: {}", block_after);
}

/// Represents a state update to be applied at a specific Ethereum block.
#[derive(Clone, Debug)]
pub struct StateUpdateConfig {
    /// The Ethereum block number at which this state update should be applied.
    pub ethereum_block: u64,
    /// The Starknet block number for this state update.
    pub starknet_block_number: u64,
    /// The Starknet block hash for this state update.
    pub starknet_block_hash: u64,
}

/// Creates an Anvil instance with the specified state updates.
///
/// - Deploys the Starknet contract
/// - Initializes the contract
/// - Applies state updates at the specified Ethereum blocks
/// - Mines to the final block
///
/// # Arguments
///
/// * `state_updates` - A list of state updates to apply, in order. Each update will be applied at
///   the specified Ethereum block number.
/// * `final_block` - Final block number to mine to after all state updates are applied.
///
/// Returns the Anvil instance and the deployed contract address.
pub(crate) async fn get_test_anvil_node(
    state_updates: &[StateUpdateConfig],
    final_block: u64,
) -> (AnvilNodeHandle, EthereumContractAddress) {
    const DEFAULT_ANVIL_PORT: u16 = 8545;
    let anvil_client = ProviderBuilder::new()
        .connect_anvil_with_wallet_and_config(|anvil_config| anvil_config.port(DEFAULT_ANVIL_PORT))
        .unwrap_or_else(|error| match error {
            AnvilError::SpawnError(e) if e.to_string().contains("No such file or directory") => {
                panic!(
                    "\nAnvil binary not found!\nInstall instructions:\n\
                     cargo install --git https://github.com/foundry-rs/foundry \
                     anvil --locked --tag=v0.3.0\n"
                )
            }
            _ => panic!("Failed to spawn Anvil: {}", error),
        });

    let deployed_contract =
        Starknet::deploy(anvil_client.clone()).await.expect("Failed to deploy Starknet contract");
    let contract_address = *deployed_contract.address();

    let provider = anvil_client.root().clone();
    let url: Url = format!("http://127.0.0.1:{}", DEFAULT_ANVIL_PORT).parse().unwrap();
    let node_handle = AnvilNodeHandle::new(url.clone(), Arc::new(anvil_client.clone().erased()));

    let base_layer_config = EthereumBaseLayerConfig {
        starknet_contract_address: contract_address,
        ..Default::default()
    };
    let contract = Starknet::new(contract_address, provider.clone());
    let base_layer =
        EthereumBaseLayerContract { contract, config: base_layer_config.clone(), url: url.clone() };

    initialize_mocked_starknet_contract(&base_layer).await;

    let mut current_block = provider.get_block_number().await.expect("Failed to get block number");
    let mut prev_starknet_block_number = 1u64; // After initialization, contract has block 1
    let mut prev_starknet_block_hash = 0u64;

    // Apply each state update
    for state_update in state_updates.iter() {
        // Mine to the block before the target Ethereum block
        // Note: When we send a transaction, Anvil automatically mines a new block to include it.
        // So we need to be at (target_block - 1) before sending the transaction.
        if current_block < state_update.ethereum_block - 1 {
            let blocks_to_mine = state_update.ethereum_block - 1 - current_block;
            anvil_mine_blocks(base_layer_config.clone(), blocks_to_mine, &url).await;
        }

        update_starknet_state(
            &base_layer,
            MockedStateUpdate {
                new_block_number: state_update.starknet_block_number,
                new_block_hash: state_update.starknet_block_hash,
                prev_block_number: prev_starknet_block_number,
                prev_block_hash: prev_starknet_block_hash,
            },
        )
        .await;

        current_block = state_update.ethereum_block;
        prev_starknet_block_number = state_update.starknet_block_number;
        prev_starknet_block_hash = state_update.starknet_block_hash;
    }

    // Mine to the final block
    if current_block < final_block {
        let blocks_to_mine = final_block - current_block;
        anvil_mine_blocks(base_layer_config.clone(), blocks_to_mine, &url).await;
    }

    (node_handle, contract_address)
}

async fn initialize_mocked_starknet_contract(base_layer: &EthereumBaseLayerContract) {
    let init_data = InitializeData {
        programHash: U256::from(1),
        configHash: U256::from(1),
        initialState: StateUpdate {
            blockNumber: I256::from_dec_str("1").unwrap(),
            ..Default::default()
        },
        ..Default::default()
    };

    let encoded_data = init_data.abi_encode();
    base_layer
        .contract
        .initializeMock(encoded_data.into())
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
}

async fn update_starknet_state(base_layer: &EthereumBaseLayerContract, update: MockedStateUpdate) {
    let mut output = vec![U256::from(0); STARKNET_OUTPUT_HEADER_SIZE + 2];
    output[STARKNET_OUTPUT_PREV_BLOCK_NUMBER_OFFSET] = U256::from(update.prev_block_number);
    output[STARKNET_OUTPUT_NEW_BLOCK_NUMBER_OFFSET] = U256::from(update.new_block_number);
    output[STARKNET_OUTPUT_PREV_BLOCK_HASH_OFFSET] = U256::from(update.prev_block_hash);
    output[STARKNET_OUTPUT_NEW_BLOCK_HASH_OFFSET] = U256::from(update.new_block_hash);

    base_layer
        .contract
        .updateState(output, Default::default())
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
}

struct MockedStateUpdate {
    new_block_number: u64,
    new_block_hash: u64,
    prev_block_number: u64,
    prev_block_hash: u64,
}

const STARKNET_OUTPUT_PREV_BLOCK_NUMBER_OFFSET: usize = 2;
const STARKNET_OUTPUT_NEW_BLOCK_NUMBER_OFFSET: usize = 3;
const STARKNET_OUTPUT_PREV_BLOCK_HASH_OFFSET: usize = 4;
const STARKNET_OUTPUT_NEW_BLOCK_HASH_OFFSET: usize = 5;
const STARKNET_OUTPUT_HEADER_SIZE: usize = 10;

sol! {
    #[derive(Debug, Default)]
    struct StateUpdate {
        uint256 globalRoot;
        int256 blockNumber;
        uint256 blockHash;
    }

    #[derive(Debug, Default)]
    struct InitializeData {
        uint256 programHash;
        uint256 aggregatorProgramHash;
        address verifier;
        uint256 configHash;
        StateUpdate initialState;
    }
}
