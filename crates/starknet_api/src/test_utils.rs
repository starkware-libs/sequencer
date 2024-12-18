use std::collections::HashMap;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};

use infra_utils::path::current_dir;
use serde::{Deserialize, Serialize};
use serde_json::to_string_pretty;
use starknet_types_core::felt::Felt;

use crate::block::{BlockNumber, GasPrice, GasPriceVector, GasPrices, NonzeroGasPrice};
use crate::core::{ChainId, ContractAddress, Nonce};
use crate::execution_resources::GasAmount;
use crate::rpc_transaction::RpcTransaction;
use crate::transaction::fields::Fee;
use crate::transaction::{Transaction, TransactionHash};

pub mod declare;
pub mod deploy_account;
pub mod invoke;
pub mod l1_handler;

/// Returns the path to a file in the resources directory. This assumes the current working
/// directory has a `resources` folder. The value for file_path should be the path to the required
/// file in the folder "resources".
pub fn path_in_resources<P: AsRef<Path>>(file_path: P) -> PathBuf {
    current_dir().unwrap().join("resources").join(file_path)
}

/// Reads from the directory containing the manifest at run time, same as current working directory.
pub fn read_json_file<P: AsRef<Path>>(path_in_resource_dir: P) -> serde_json::Value {
    let path = path_in_resources(path_in_resource_dir);
    let json_str = read_to_string(path.to_str().unwrap())
        .unwrap_or_else(|_| panic!("Failed to read file at path: {}", path.display()));
    serde_json::from_str(&json_str).unwrap()
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

#[derive(Debug, Default)]
pub struct NonceManager {
    next_nonce: HashMap<ContractAddress, Felt>,
}

impl NonceManager {
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

/// Converts a [`RpcTransaction`] to a JSON string.
pub fn rpc_tx_to_json(tx: &RpcTransaction) -> String {
    let mut tx_json = serde_json::to_value(tx)
        .unwrap_or_else(|tx| panic!("Failed to serialize transaction: {tx:?}"));

    // Add type and version manually
    let type_string = match tx {
        RpcTransaction::Declare(_) => "DECLARE",
        RpcTransaction::DeployAccount(_) => "DEPLOY_ACCOUNT",
        RpcTransaction::Invoke(_) => "INVOKE",
    };

    tx_json
        .as_object_mut()
        .unwrap()
        .extend([("type".to_string(), type_string.into()), ("version".to_string(), "0x3".into())]);

    // Serialize back to pretty JSON string
    to_string_pretty(&tx_json).expect("Failed to serialize transaction")
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
