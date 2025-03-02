use std::fs::File;
use std::process::Command;

use alloy::node_bindings::{Anvil, AnvilInstance, NodeError as AnvilError};
pub(crate) use alloy::primitives::Address as EthereumContractAddress;
use colored::*;
use ethers::utils::{Ganache, GanacheInstance};
use starknet_api::hash::StarkHash;
use tar::Archive;
use tempfile::{tempdir, TempDir};

use crate::ethereum_base_layer_contract::EthereumBaseLayerConfig;

type TestEthereumNodeHandle = (GanacheInstance, TempDir);

const MINIMAL_GANACHE_VERSION: u8 = 7;

// Default funded account, there are more fixed funded accounts,
// see https://github.com/foundry-rs/foundry/tree/master/crates/anvil.
// This address is the sender address of messages sent to L2 by Anvil.
pub const DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS: StarkHash =
    StarkHash::from_hex_unchecked("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
// This address is commonly used as the L1 address of the Starknet core contract.
pub const DEFAULT_ANVIL_L1_DEPLOYED_ADDRESS: &str = "0x5fbdb2315678afecb367f032d93f642f64180aa3";

// Returns a Ganache instance, preset with a Starknet core contract and some state updates:
// Starknet contract address: 0xe2aF2c1AE11fE13aFDb7598D0836398108a4db0A
//     Ethereum block number   starknet block number   starknet block hash
//      10                      100                     0x100
//      20                      200                     0x200
//      30                      300                     0x300
// The blockchain is at Ethereum block number 31.
// Note: Requires Ganache@7.4.3 installed.
// TODO(Gilad): `Ganache` and `ethers` have both been deprecated. Also, `ethers`s' replacement,
// `alloy`, no longer supports `Ganache`. Once we decide on a Ganache replacement, fix this test and
// fully remove `ethers`.
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
    // If the port is not set explicitly, a random value will be used.
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

pub fn ethereum_base_layer_config(anvil: &AnvilInstance) -> EthereumBaseLayerConfig {
    EthereumBaseLayerConfig {
        node_url: anvil.endpoint_url(),
        starknet_contract_address: DEFAULT_ANVIL_L1_DEPLOYED_ADDRESS.parse().unwrap(),
    }
}
