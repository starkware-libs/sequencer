use std::fs::File;
use std::process::Command;

use alloy::network::TransactionBuilder;
use alloy::node_bindings::{Anvil, AnvilInstance, NodeError as AnvilError};
use alloy::primitives::{address as ethereum_address, U256};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use colored::*;
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
    StarknetL1Contract,
};

type TestEthereumNodeHandle = (GanacheInstance, TempDir);

const MINIMAL_GANACHE_VERSION: u8 = 7;

// See Anvil documentation:
// https://docs.rs/ethers-core/latest/ethers_core/utils/struct.Anvil.html#method.new.
const DEFAULT_ANVIL_PORT: u16 = 8545;
// This address is commonly used as the L1 address of the Starknet core contract.
// TODO(Arni): Replace with constant with use of `AnvilInstance::address(&self)`.
pub const DEFAULT_ANVIL_L1_DEPLOYED_ADDRESS: &str = "0x5fbdb2315678afecb367f032d93f642f64180aa3";

// Default funded accounts.
// This address is the sender address of messages sent to L2 by Anvil.
// Given an `AnvilInstance`, this address can be retrieved by calling `anvil.addresses()[0]`.
pub const DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS: StarkHash =
    StarkHash::from_hex_unchecked("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
/// One of the 10 pre-funded Anvil preloaded accounts.
pub const ARBITRARY_ANVIL_L1_ACCOUNT_ADDRESS: EthereumContractAddress =
    ethereum_address!("0x90F79bf6EB2c4f870365E785982E1f101E93b906");
/// One of the 10 pre-funded Anvil preloaded accounts.
pub const OTHER_ARBITRARY_ANVIL_L1_ACCOUNT_ADDRESS: EthereumContractAddress =
    ethereum_address!("0x15d34AAf54267DB7D7c367839AAf71A00a2C6A65");

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

// TODO(Arni): Make port non-optional.
// Spin up Anvil instance, a local Ethereum node, dies when dropped.
pub fn anvil(port: Option<u16>) -> AnvilInstance {
    let mut anvil = Anvil::new();
    // If the port is not set explicitly, a random ephemeral port is bound and used.
    if let Some(port) = port {
        anvil = anvil.port(port);
    }

    anvil.try_spawn().unwrap_or_else(|error| match error {
        AnvilError::SpawnError(e) if e.to_string().contains("No such file or directory") => {
            panic!(
                "\n{}\n{}\n",
                "Anvil binary not found!".bold().red(),
                "Install instructions (for local development):\n
                 cargo install --git \
                 https://github.com/foundry-rs/foundry anvil --locked --tag=v0.3.0"
                    .yellow()
            )
        }
        _ => panic!("Failed to spawn Anvil: {}", error.to_string().red()),
    })
}

pub fn ethereum_base_layer_config_for_anvil(port: Option<u16>) -> EthereumBaseLayerConfig {
    // Use the specified port if provided; otherwise, default to Anvil's default port.
    let non_optional_port = port.unwrap_or(DEFAULT_ANVIL_PORT);
    let endpoint = format!("http://localhost:{non_optional_port}");
    EthereumBaseLayerConfig {
        node_url: Url::parse(&endpoint).unwrap(),
        starknet_contract_address: DEFAULT_ANVIL_L1_DEPLOYED_ADDRESS.parse().unwrap(),
        ..Default::default()
    }
}

pub fn anvil_instance_from_config(config: &EthereumBaseLayerConfig) -> AnvilInstance {
    let port = config.node_url.port();
    let anvil = anvil(port);
    assert_eq!(config.node_url, anvil.endpoint_url(), "Unexpected config for Anvil instance.");
    anvil
}

pub async fn spawn_anvil_and_deploy_starknet_l1_contract(
    config: &EthereumBaseLayerConfig,
) -> (AnvilInstance, StarknetL1Contract) {
    let anvil = anvil_instance_from_config(config);
    let starknet_l1_contract = deploy_starknet_l1_contract(config.clone()).await;
    (anvil, starknet_l1_contract)
}

pub async fn deploy_starknet_l1_contract(config: EthereumBaseLayerConfig) -> StarknetL1Contract {
    let ethereum_base_layer_contract = EthereumBaseLayerContract::new(config);
    Starknet::deploy(ethereum_base_layer_contract.contract.provider().clone()).await.unwrap()
}

pub async fn make_block_history_on_anvil(
    sender_address: EthereumContractAddress,
    receiver_address: EthereumContractAddress,
    base_layer_config: EthereumBaseLayerConfig,
    num_blocks: usize,
) {
    let base_layer = EthereumBaseLayerContract::new(base_layer_config.clone());
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
