use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use apollo_infra_utils::path::current_dir;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_utils::bigint::BigUintAsHex;
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;

use crate::block::{
    BlockInfo,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    GasPriceVector,
    GasPrices,
    NonzeroGasPrice,
};
use crate::contract_address;
use crate::contract_class::{ContractClass, SierraVersion};
use crate::core::{ChainId, ContractAddress, Nonce};
use crate::executable_transaction::AccountTransaction;
use crate::execution_resources::GasAmount;
use crate::rpc_transaction::{InternalRpcTransaction, RpcTransaction};
use crate::transaction::fields::{AllResourceBounds, Fee, ResourceBounds};
use crate::transaction::{Transaction, TransactionHash};

pub mod declare;
pub mod deploy_account;
pub mod invoke;
pub mod l1_handler;

// TODO(Dori, 1/2/2024): Remove these constants once all tests use the `contracts` and
//   `initial_test_state` modules for testing.
// Addresses.
pub const TEST_SEQUENCER_ADDRESS: &str = "0x1000";
pub const TEST_ERC20_CONTRACT_ADDRESS: &str = "0x1001";
pub const TEST_ERC20_CONTRACT_ADDRESS2: &str = "0x1002";

// The block number of the BlockContext being used for testing.
pub const CURRENT_BLOCK_NUMBER: u64 = 2001;
pub const CURRENT_BLOCK_NUMBER_FOR_VALIDATE: u64 = 2000;

// The block timestamp of the BlockContext being used for testing.
pub const CURRENT_BLOCK_TIMESTAMP: u64 = 1072023;
pub const CURRENT_BLOCK_TIMESTAMP_FOR_VALIDATE: u64 = 1069200;

pub static CHAIN_ID_FOR_TESTS: LazyLock<ChainId> =
    LazyLock::new(|| ChainId::Other("CHAIN_ID_SUBDIR".to_owned()));

/// Returns the path to a file in the resources directory. This assumes the current working
/// directory has a `resources` folder. The value for file_path should be the path to the required
/// file in the folder "resources".
pub fn path_in_resources<P: AsRef<Path>>(file_path: P) -> PathBuf {
    current_dir().unwrap().join("resources").join(file_path)
}

/// Reads from the directory containing the manifest at run time, same as current working directory.
pub fn read_json_file<P: AsRef<Path>, T>(path_in_resource_dir: P) -> T
where
    T: for<'a> serde::de::Deserialize<'a>,
{
    let path = path_in_resources(path_in_resource_dir);
    let file =
        File::open(&path).unwrap_or_else(|_| panic!("Failed to open file at path: {path:?}"));
    serde_json::from_reader(file)
        .unwrap_or_else(|_| panic!("Failed to parse JSON from file at path: {path:?}"))
}

#[derive(Deserialize, Serialize, Debug)]
/// A struct used for reading the transaction test data (e.g., for transaction hash tests).
pub struct TransactionTestData {
    /// The actual transaction.
    pub transaction: Transaction,
    /// The expected transaction hash.
    pub transaction_hash: TransactionHash,
    /// An optional transaction hash to query.
    pub only_query_transaction_hash: Option<TransactionHash>,
    pub chain_id: ChainId,
    pub block_number: BlockNumber,
}

#[derive(Debug, Default, Clone)]
pub struct NonceManager {
    next_nonce: HashMap<ContractAddress, Felt>,
}

impl NonceManager {
    pub fn get(&self, account_address: ContractAddress) -> Nonce {
        Nonce(*self.next_nonce.get(&account_address).unwrap_or(&Felt::default()))
    }

    pub fn next(&mut self, account_address: ContractAddress) -> Nonce {
        let next = self.next_nonce.remove(&account_address).unwrap_or_default();
        self.next_nonce.insert(account_address, next + 1);
        Nonce(next)
    }

    /// Decrements the nonce of the account, unless it is zero.
    pub fn rollback(&mut self, account_address: ContractAddress) {
        let current = *self.next_nonce.get(&account_address).unwrap_or(&Felt::default());
        if current != Felt::ZERO {
            self.next_nonce.insert(account_address, current - 1);
        }
    }
}

/// A utility macro to create a [`Nonce`] from a hex string / unsigned integer
/// representation.
#[macro_export]
macro_rules! nonce {
    ($s:expr) => {
        $crate::core::Nonce(starknet_types_core::felt::Felt::from($s))
    };
}

/// A utility macro to create a [`StorageKey`](crate::state::StorageKey) from a hex string /
/// unsigned integer representation.
#[macro_export]
macro_rules! storage_key {
    ($s:expr) => {
        $crate::state::StorageKey(starknet_api::patricia_key!($s))
    };
}

/// A utility macro to create a [`CompiledClassHash`](crate::core::CompiledClassHash) from a hex
/// string / unsigned integer representation.
#[macro_export]
macro_rules! compiled_class_hash {
    ($s:expr) => {
        $crate::core::CompiledClassHash(starknet_types_core::felt::Felt::from($s))
    };
}

// V3 transactions:
pub const DEFAULT_L1_GAS_AMOUNT: GasAmount = GasAmount(u64::pow(10, 6));
pub const DEFAULT_L1_DATA_GAS_MAX_AMOUNT: GasAmount = GasAmount(u64::pow(10, 6));
pub const DEFAULT_L2_GAS_MAX_AMOUNT: GasAmount = GasAmount(u64::pow(10, 9));
pub const MAX_L1_GAS_PRICE: NonzeroGasPrice = DEFAULT_STRK_L1_GAS_PRICE;
pub const MAX_L2_GAS_PRICE: NonzeroGasPrice = DEFAULT_STRK_L2_GAS_PRICE;
pub const MAX_L1_DATA_GAS_PRICE: NonzeroGasPrice = DEFAULT_STRK_L1_DATA_GAS_PRICE;

pub const DEFAULT_ETH_L1_GAS_PRICE: NonzeroGasPrice =
    NonzeroGasPrice::new_unchecked(GasPrice(100 * u128::pow(10, 9))); // Given in units of Wei.
pub const DEFAULT_STRK_L1_GAS_PRICE: NonzeroGasPrice =
    NonzeroGasPrice::new_unchecked(GasPrice(100 * u128::pow(10, 9))); // Given in units of Fri.
pub const DEFAULT_ETH_L1_DATA_GAS_PRICE: NonzeroGasPrice =
    NonzeroGasPrice::new_unchecked(GasPrice(u128::pow(10, 6))); // Given in units of Wei.
pub const DEFAULT_STRK_L1_DATA_GAS_PRICE: NonzeroGasPrice =
    NonzeroGasPrice::new_unchecked(GasPrice(u128::pow(10, 9))); // Given in units of Fri.
pub const DEFAULT_ETH_L2_GAS_PRICE: NonzeroGasPrice =
    NonzeroGasPrice::new_unchecked(GasPrice(u128::pow(10, 6)));
pub const DEFAULT_STRK_L2_GAS_PRICE: NonzeroGasPrice =
    NonzeroGasPrice::new_unchecked(GasPrice(u128::pow(10, 9)));

pub const DEFAULT_GAS_PRICES: GasPrices = GasPrices {
    eth_gas_prices: GasPriceVector {
        l1_gas_price: DEFAULT_ETH_L1_GAS_PRICE,
        l2_gas_price: DEFAULT_ETH_L2_GAS_PRICE,
        l1_data_gas_price: DEFAULT_ETH_L1_DATA_GAS_PRICE,
    },
    strk_gas_prices: GasPriceVector {
        l1_gas_price: DEFAULT_STRK_L1_GAS_PRICE,
        l2_gas_price: DEFAULT_STRK_L2_GAS_PRICE,
        l1_data_gas_price: DEFAULT_STRK_L1_DATA_GAS_PRICE,
    },
};

// Deprecated transactions:
pub const MAX_FEE: Fee = DEFAULT_L1_GAS_AMOUNT.nonzero_saturating_mul(DEFAULT_ETH_L1_GAS_PRICE);

impl BlockInfo {
    pub fn create_for_testing() -> Self {
        Self {
            block_number: BlockNumber(CURRENT_BLOCK_NUMBER),
            block_timestamp: BlockTimestamp(CURRENT_BLOCK_TIMESTAMP),
            sequencer_address: contract_address!(TEST_SEQUENCER_ADDRESS),
            gas_prices: DEFAULT_GAS_PRICES,
            // TODO(Yoni): change to true.
            use_kzg_da: false,
        }
    }

    pub fn create_for_testing_with_kzg(use_kzg_da: bool) -> Self {
        Self { use_kzg_da, ..Self::create_for_testing() }
    }
}

/// A trait for producing test transactions.
pub trait TestingTxArgs {
    fn get_rpc_tx(&self) -> RpcTransaction;
    fn get_internal_tx(&self) -> InternalRpcTransaction;
    /// Returns the executable transaction for the transaction.
    /// Note: In the declare transaction, `class_info` is constructed using a default compiled
    /// contract class, so if the test requires a specific contract class this function
    /// shouldn't be used.
    fn get_executable_tx(&self) -> AccountTransaction;
}

impl ContractClass {
    pub fn test_casm_contract_class() -> Self {
        let default_casm = CasmContractClass {
            prime: Default::default(),
            compiler_version: Default::default(),
            bytecode: vec![
                BigUintAsHex { value: BigUint::from(1_u8) },
                BigUintAsHex { value: BigUint::from(1_u8) },
                BigUintAsHex { value: BigUint::from(1_u8) },
            ],
            bytecode_segment_lengths: Default::default(),
            hints: Default::default(),
            pythonic_hints: Default::default(),
            entry_points_by_type: Default::default(),
        };
        ContractClass::V1((default_casm, SierraVersion::default()))
    }
}

pub const VALID_L1_GAS_MAX_AMOUNT: u64 = 203484;
pub const VALID_L1_GAS_MAX_PRICE_PER_UNIT: u128 = 100000000000000;
// Enough to declare the test class, but under the OS's upper limit.
pub const VALID_L2_GAS_MAX_AMOUNT: u64 = 1_100_000_000;
pub const VALID_L2_GAS_MAX_PRICE_PER_UNIT: u128 = 100000000000000;
pub const VALID_L1_DATA_GAS_MAX_AMOUNT: u64 = 203484;
pub const VALID_L1_DATA_GAS_MAX_PRICE_PER_UNIT: u128 = 100000000000000;

pub fn resource_bounds_for_testing() -> AllResourceBounds {
    AllResourceBounds {
        l1_gas: ResourceBounds {
            max_amount: GasAmount(VALID_L1_GAS_MAX_AMOUNT),
            max_price_per_unit: GasPrice(VALID_L1_GAS_MAX_PRICE_PER_UNIT),
        },
        l2_gas: ResourceBounds {
            max_amount: GasAmount(VALID_L2_GAS_MAX_AMOUNT),
            max_price_per_unit: GasPrice(VALID_L2_GAS_MAX_PRICE_PER_UNIT),
        },
        l1_data_gas: ResourceBounds {
            max_amount: GasAmount(VALID_L1_DATA_GAS_MAX_AMOUNT),
            max_price_per_unit: GasPrice(VALID_L1_DATA_GAS_MAX_PRICE_PER_UNIT),
        },
    }
}
