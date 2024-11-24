use std::fs::File;
use std::process::Command;

pub(crate) use alloy_primitives::Address as EthereumContractAddress;
use ethers::utils::{Ganache, GanacheInstance};
use tar::Archive;
use tempfile::{tempdir, TempDir};

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
