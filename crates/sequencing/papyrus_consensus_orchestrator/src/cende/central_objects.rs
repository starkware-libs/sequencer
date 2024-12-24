use assert_matches::assert_matches;
use blockifier::state::cached_state::CommitmentStateDiff;
use indexmap::{indexmap, IndexMap};
use serde::{Deserialize, Serialize};
use serde_repr::*;
use starknet_api::block::{
    BlockInfo,
    BlockNumber,
    BlockTimestamp,
    NonzeroGasPrice,
    StarknetVersion,
};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::executable_transaction::{AccountTransaction, InvokeTransaction, Transaction};
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    Calldata,
    PaymasterData,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_api::transaction::TransactionHash;
use starknet_types_core::felt::Felt;

/// Central objects are required in order to continue processing the block by the centralized
/// Python pipline. These objects are written to the Aerospike database and are used by python
/// services. In the future, all services will be decentralized and this module will be removed.
#[cfg(test)]
#[path = "central_objects_test.rs"]
mod central_objects_test;

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct CentralResourcePrice {
    pub price_in_wei: NonzeroGasPrice,
    pub price_in_fri: NonzeroGasPrice,
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct CentralBlockInfo {
    pub block_number: BlockNumber,
    pub block_timestamp: BlockTimestamp,
    pub sequencer_address: ContractAddress,
    pub l1_gas_price: CentralResourcePrice,
    pub l1_data_gas_price: CentralResourcePrice,
    pub l2_gas_price: CentralResourcePrice,
    pub use_kzg_da: bool,
    pub starknet_version: Option<StarknetVersion>,
}

impl From<(BlockInfo, StarknetVersion)> for CentralBlockInfo {
    fn from((block_info, starknet_version): (BlockInfo, StarknetVersion)) -> CentralBlockInfo {
        CentralBlockInfo {
            block_number: block_info.block_number,
            block_timestamp: block_info.block_timestamp,
            sequencer_address: block_info.sequencer_address,
            l1_gas_price: CentralResourcePrice {
                price_in_wei: block_info.gas_prices.eth_gas_prices.l1_gas_price,
                price_in_fri: block_info.gas_prices.strk_gas_prices.l1_gas_price,
            },
            l1_data_gas_price: CentralResourcePrice {
                price_in_wei: block_info.gas_prices.eth_gas_prices.l1_data_gas_price,
                price_in_fri: block_info.gas_prices.strk_gas_prices.l1_data_gas_price,
            },
            l2_gas_price: CentralResourcePrice {
                price_in_wei: block_info.gas_prices.eth_gas_prices.l2_gas_price,
                price_in_fri: block_info.gas_prices.strk_gas_prices.l2_gas_price,
            },
            use_kzg_da: block_info.use_kzg_da,
            starknet_version: Some(starknet_version),
        }
    }
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct CentralStateDiff {
    pub address_to_class_hash: IndexMap<ContractAddress, ClassHash>,
    pub nonces: IndexMap<DataAvailabilityMode, IndexMap<ContractAddress, Nonce>>,
    pub storage_updates:
        IndexMap<DataAvailabilityMode, IndexMap<ContractAddress, IndexMap<StorageKey, Felt>>>,
    pub declared_classes: IndexMap<ClassHash, CompiledClassHash>,
    pub block_info: CentralBlockInfo,
}

impl From<(CommitmentStateDiff, BlockInfo, StarknetVersion)> for CentralStateDiff {
    fn from(
        (state_diff, block_info, starknet_version): (
            CommitmentStateDiff,
            BlockInfo,
            StarknetVersion,
        ),
    ) -> CentralStateDiff {
        CentralStateDiff {
            address_to_class_hash: state_diff.address_to_class_hash,
            nonces: indexmap!(DataAvailabilityMode::L1=> state_diff.address_to_nonce),
            storage_updates: indexmap!(DataAvailabilityMode::L1=> state_diff.storage_updates),
            declared_classes: state_diff.class_hash_to_compiled_class_hash,
            block_info: (block_info, starknet_version).into(),
        }
    }
}

// This object is serialized as a string by starknet_api, but the Python implementation requires it
// to be serialized as an integer. To address this, a separate object is provided that serializes
// into an integer specifically for cende.
#[derive(Clone, Debug, Default, PartialEq, Deserialize_repr, Serialize_repr)]
#[serde(into = "u8", try_from = "u8")]
#[repr(u8)]
pub enum CentralDataAvailabilityMode {
    #[default]
    L1 = 0,
    L2 = 1,
}

impl From<DataAvailabilityMode> for CentralDataAvailabilityMode {
    fn from(mode: DataAvailabilityMode) -> CentralDataAvailabilityMode {
        match mode {
            DataAvailabilityMode::L1 => CentralDataAvailabilityMode::L1,
            DataAvailabilityMode::L2 => CentralDataAvailabilityMode::L2,
        }
    }
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct CentralInvokeTransactionV3 {
    pub sender_address: ContractAddress,
    pub calldata: Calldata,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub resource_bounds: ValidResourceBounds,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce_data_availability_mode: CentralDataAvailabilityMode,
    pub fee_data_availability_mode: CentralDataAvailabilityMode,
    pub hash_value: TransactionHash,
}

impl From<InvokeTransaction> for CentralInvokeTransactionV3 {
    fn from(tx: InvokeTransaction) -> CentralInvokeTransactionV3 {
        assert_matches!(tx.tx, starknet_api::transaction::InvokeTransaction::V3(_));
        CentralInvokeTransactionV3 {
            sender_address: tx.sender_address(),
            calldata: tx.calldata(),
            signature: tx.signature(),
            nonce: tx.nonce(),
            resource_bounds: tx.resource_bounds(),
            tip: tx.tip(),
            paymaster_data: tx.paymaster_data(),
            account_deployment_data: tx.account_deployment_data(),
            nonce_data_availability_mode: tx.nonce_data_availability_mode().into(),
            fee_data_availability_mode: tx.fee_data_availability_mode().into(),
            hash_value: tx.tx_hash(),
        }
    }
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "version")]
pub enum CentralInvokeTransaction {
    #[serde(rename = "0x3")]
    V3(CentralInvokeTransactionV3),
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum CentralTransaction {
    #[serde(rename = "INVOKE_FUNCTION")]
    Invoke(CentralInvokeTransaction),
}

impl From<Transaction> for CentralTransaction {
    fn from(tx: Transaction) -> CentralTransaction {
        match tx {
            Transaction::Account(AccountTransaction::Invoke(invoke_tx)) => {
                CentralTransaction::Invoke(CentralInvokeTransaction::V3(
                    CentralInvokeTransactionV3::from(invoke_tx),
                ))
            }
            Transaction::Account(_) => unimplemented!(),
            Transaction::L1Handler(_) => unimplemented!(),
        }
    }
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct CentralTransactionTimestamp(pub u64);

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct CentralTransactionWritten {
    pub tx: CentralTransaction,
    pub time_created: CentralTransactionTimestamp,
}

impl From<(Transaction, u64)> for CentralTransactionWritten {
    fn from((tx, timestamp): (Transaction, u64)) -> CentralTransactionWritten {
        CentralTransactionWritten {
            tx: CentralTransaction::from(tx),
            // This timestamp is required for metrics data. Yoni and Noa approved that it is
            // sufficient to take the time during the batcher run.
            time_created: CentralTransactionTimestamp(timestamp),
        }
    }
}
