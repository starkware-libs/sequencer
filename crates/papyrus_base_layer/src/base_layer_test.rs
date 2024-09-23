use std::fs::File;
use std::process::Command;

use ethers::utils::{Ganache, GanacheInstance};
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::felt;
use tar::Archive;
use tempfile::{tempdir, TempDir};

use crate::ethereum_base_layer_contract::{
    EthereumBaseLayerConfig,
    EthereumBaseLayerContract,
    EthereumContractAddress,
};
use crate::BaseLayerContract;

type TestEthereumNodeHandle = (GanacheInstance, TempDir);

const MINIMAL_GANACHE_VERSION: u8 = 7;

// Returns a Ganache instance, preset with a Starknet core contract and some state updates:
// Starknet contract address: 0xe2aF2c1AE11fE13aFDb7598D0836398108a4db0A
//     Ethereum block number   starknet block number   starknet block hash
//      10                      100                     0x100
//      20                      200                     0x200
//      30                      300                     0x300
// The blockchain is at Ethereum block number 31.
// Note: Requires Ganache@7.4.3 installed.
// TODO: `Ganache` and `ethers` have both been deprecated. Also, `ethers`s' replacement, `alloy`,
// no longer supports `Ganache`. Once we decide on a Ganache replacement, fix this test and fully
// remove `ethers`.
fn get_test_ethereum_node() -> (TestEthereumNodeHandle, EthereumContractAddress) {
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

#[test_with::executable(ganache)]
#[tokio::test]
// Note: the test requires ganache-cli installed, otherwise it is ignored.
async fn latest_proved_block_ethereum() {
    let (node_handle, starknet_contract_address) = get_test_ethereum_node();
    let config = EthereumBaseLayerConfig {
        node_url: node_handle.0.endpoint().parse().unwrap(),
        starknet_contract_address,
    };
    let contract = EthereumBaseLayerContract::new(config).unwrap();

    let first_sn_state_update = (BlockNumber(100), BlockHash(felt!("0x100")));
    let second_sn_state_update = (BlockNumber(200), BlockHash(felt!("0x200")));
    let third_sn_state_update = (BlockNumber(300), BlockHash(felt!("0x300")));

    type Scenario = (u64, Option<(BlockNumber, BlockHash)>);
    let scenarios: Vec<Scenario> = vec![
        (0, Some(third_sn_state_update)),
        (5, Some(third_sn_state_update)),
        (15, Some(second_sn_state_update)),
        (25, Some(first_sn_state_update)),
        (1000, None),
    ];
    for (scenario, expected) in scenarios {
        let latest_block = contract.latest_proved_block(scenario).await.unwrap();
        assert_eq!(latest_block, expected);
    }
}
