use std::collections::HashMap;

use assert_matches::assert_matches;
use blockifier::abi::constants as abi_constants;
use blockifier::execution::call_info::CallInfo;
use blockifier::fee::receipt::TransactionReceipt;
use blockifier::transaction::objects::{ExecutionResourcesTraits, TransactionExecutionInfo};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::NestedIntList;
use indexmap::{indexmap, IndexMap};
use serde::Serialize;
use starknet_api::block::{
    BlockInfo,
    BlockNumber,
    BlockTimestamp,
    NonzeroGasPrice,
    StarknetVersion,
};
use starknet_api::contract_class::SierraVersion;
use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    EntryPointSelector,
    Nonce,
};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::executable_transaction::{
    AccountTransaction,
    DeclareTransaction,
    DeployAccountTransaction,
    InvokeTransaction,
    L1HandlerTransaction,
    Transaction,
};
use starknet_api::execution_resources::GasVector;
use starknet_api::state::{StorageKey, ThinStateDiff};
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    Calldata,
    ContractAddressSalt,
    Fee,
    PaymasterData,
    ResourceBounds,
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

#[derive(Debug, PartialEq, Serialize)]
pub struct CentralResourcePrice {
    pub price_in_wei: NonzeroGasPrice,
    pub price_in_fri: NonzeroGasPrice,
}

#[derive(Debug, PartialEq, Serialize)]
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

#[derive(Debug, PartialEq, Serialize)]
pub struct CentralStateDiff {
    pub address_to_class_hash: IndexMap<ContractAddress, ClassHash>,
    pub nonces: IndexMap<DataAvailabilityMode, IndexMap<ContractAddress, Nonce>>,
    pub storage_updates:
        IndexMap<DataAvailabilityMode, IndexMap<ContractAddress, IndexMap<StorageKey, Felt>>>,
    pub declared_classes: IndexMap<ClassHash, CompiledClassHash>,
    pub block_info: CentralBlockInfo,
}

// We convert to CentralStateDiff from ThinStateDiff since this object is already sent to consensus
// for the Sync service, otherwise we could have used the CommitmentStateDiff as well.
impl From<(ThinStateDiff, BlockInfo, StarknetVersion)> for CentralStateDiff {
    fn from(
        (state_diff, block_info, starknet_version): (ThinStateDiff, BlockInfo, StarknetVersion),
    ) -> CentralStateDiff {
        assert!(
            state_diff.deprecated_declared_classes.is_empty(),
            "Deprecated classes are not supported"
        );

        let mut address_to_class_hash = state_diff.deployed_contracts;
        address_to_class_hash.extend(state_diff.replaced_classes);

        CentralStateDiff {
            address_to_class_hash,
            nonces: indexmap!(DataAvailabilityMode::L1=> state_diff.nonces),
            storage_updates: indexmap!(DataAvailabilityMode::L1=> state_diff.storage_diffs),
            declared_classes: state_diff.declared_classes,
            block_info: (block_info, starknet_version).into(),
        }
    }
}

#[derive(Debug, PartialEq, Serialize)]
pub struct CentralResourceBounds {
    #[serde(rename = "L1_GAS")]
    pub l1_gas: ResourceBounds,
    #[serde(rename = "L2_GAS")]
    pub l2_gas: ResourceBounds,
    #[serde(rename = "L1_DATA_GAS")]
    pub l1_data_gas: ResourceBounds,
}

impl From<ValidResourceBounds> for CentralResourceBounds {
    fn from(resource_bounds: ValidResourceBounds) -> CentralResourceBounds {
        match resource_bounds {
            ValidResourceBounds::AllResources(resource_bounds) => CentralResourceBounds {
                l1_gas: resource_bounds.l1_gas,
                l2_gas: resource_bounds.l2_gas,
                l1_data_gas: resource_bounds.l1_data_gas,
            },
            _ => panic!("Transaction should be V3"),
        }
    }
}

#[derive(Debug, PartialEq, Serialize)]
pub struct CentralInvokeTransactionV3 {
    pub resource_bounds: CentralResourceBounds,
    pub tip: Tip,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub sender_address: ContractAddress,
    pub calldata: Calldata,
    pub nonce_data_availability_mode: u32,
    pub fee_data_availability_mode: u32,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
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
            resource_bounds: tx.resource_bounds().into(),
            tip: tx.tip(),
            paymaster_data: tx.paymaster_data(),
            account_deployment_data: tx.account_deployment_data(),
            nonce_data_availability_mode: tx.nonce_data_availability_mode().into(),
            fee_data_availability_mode: tx.fee_data_availability_mode().into(),
            hash_value: tx.tx_hash(),
        }
    }
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "version")]
pub enum CentralInvokeTransaction {
    #[serde(rename = "0x3")]
    V3(CentralInvokeTransactionV3),
}

#[derive(Debug, PartialEq, Serialize)]
pub struct CentralDeployAccountTransactionV3 {
    pub resource_bounds: CentralResourceBounds,
    pub tip: Tip,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub contract_address_salt: ContractAddressSalt,
    pub sender_address: ContractAddress,
    pub constructor_calldata: Calldata,
    pub nonce_data_availability_mode: u32,
    pub fee_data_availability_mode: u32,
    pub paymaster_data: PaymasterData,
    pub hash_value: TransactionHash,
}

impl From<DeployAccountTransaction> for CentralDeployAccountTransactionV3 {
    fn from(tx: DeployAccountTransaction) -> CentralDeployAccountTransactionV3 {
        CentralDeployAccountTransactionV3 {
            resource_bounds: tx.resource_bounds().into(),
            tip: tx.tip(),
            signature: tx.signature(),
            nonce: tx.nonce(),
            class_hash: tx.class_hash(),
            contract_address_salt: tx.contract_address_salt(),
            constructor_calldata: tx.constructor_calldata(),
            nonce_data_availability_mode: tx.nonce_data_availability_mode().into(),
            fee_data_availability_mode: tx.fee_data_availability_mode().into(),
            paymaster_data: tx.paymaster_data(),
            hash_value: tx.tx_hash(),
            sender_address: tx.contract_address,
        }
    }
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "version")]
pub enum CentralDeployAccountTransaction {
    #[serde(rename = "0x3")]
    V3(CentralDeployAccountTransactionV3),
}

fn into_string_tuple(val: SierraVersion) -> (String, String, String) {
    (format!("0x{:x}", val.major), format!("0x{:x}", val.minor), format!("0x{:x}", val.patch))
}

#[derive(Debug, PartialEq, Serialize)]
pub struct CentralDeclareTransactionV3 {
    pub resource_bounds: CentralResourceBounds,
    pub tip: Tip,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub compiled_class_hash: CompiledClassHash,
    pub sender_address: ContractAddress,
    pub nonce_data_availability_mode: u32,
    pub fee_data_availability_mode: u32,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub sierra_program_size: usize,
    pub abi_size: usize,
    pub sierra_version: (String, String, String),
    pub hash_value: TransactionHash,
}

impl From<DeclareTransaction> for CentralDeclareTransactionV3 {
    fn from(tx: DeclareTransaction) -> CentralDeclareTransactionV3 {
        CentralDeclareTransactionV3 {
            resource_bounds: tx.resource_bounds().into(),
            tip: tx.tip(),
            signature: tx.signature(),
            nonce: tx.nonce(),
            class_hash: tx.class_hash(),
            compiled_class_hash: tx.compiled_class_hash(),
            sender_address: tx.sender_address(),
            nonce_data_availability_mode: tx.nonce_data_availability_mode().into(),
            fee_data_availability_mode: tx.fee_data_availability_mode().into(),
            paymaster_data: tx.paymaster_data(),
            account_deployment_data: tx.account_deployment_data(),
            sierra_program_size: tx.class_info.sierra_program_length,
            abi_size: tx.class_info.abi_length,
            sierra_version: into_string_tuple(tx.class_info.sierra_version),
            hash_value: tx.tx_hash,
        }
    }
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "version")]
pub enum CentralDeclareTransaction {
    #[serde(rename = "0x3")]
    V3(CentralDeclareTransactionV3),
}

#[derive(Debug, PartialEq, Serialize)]
pub struct CentralL1HandlerTransaction {
    pub contract_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub calldata: Calldata,
    pub nonce: Nonce,
    pub paid_fee_on_l1: Fee,
    pub hash_value: TransactionHash,
}

impl From<L1HandlerTransaction> for CentralL1HandlerTransaction {
    fn from(tx: L1HandlerTransaction) -> CentralL1HandlerTransaction {
        CentralL1HandlerTransaction {
            hash_value: tx.tx_hash,
            contract_address: tx.tx.contract_address,
            entry_point_selector: tx.tx.entry_point_selector,
            calldata: tx.tx.calldata,
            nonce: tx.tx.nonce,
            paid_fee_on_l1: tx.paid_fee_on_l1,
        }
    }
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum CentralTransaction {
    #[serde(rename = "INVOKE_FUNCTION")]
    Invoke(CentralInvokeTransaction),
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(CentralDeployAccountTransaction),
    #[serde(rename = "DECLARE")]
    Declare(CentralDeclareTransaction),
    #[serde(rename = "L1_HANDLER")]
    L1Handler(CentralL1HandlerTransaction),
}

impl From<Transaction> for CentralTransaction {
    fn from(tx: Transaction) -> CentralTransaction {
        match tx {
            Transaction::Account(AccountTransaction::Invoke(invoke_tx)) => {
                CentralTransaction::Invoke(CentralInvokeTransaction::V3(invoke_tx.into()))
            }
            Transaction::Account(AccountTransaction::DeployAccount(deploy_tx)) => {
                CentralTransaction::DeployAccount(CentralDeployAccountTransaction::V3(
                    deploy_tx.into(),
                ))
            }
            Transaction::Account(AccountTransaction::Declare(declare_tx)) => {
                CentralTransaction::Declare(CentralDeclareTransaction::V3(declare_tx.into()))
            }
            Transaction::L1Handler(l1_handler) => CentralTransaction::L1Handler(l1_handler.into()),
        }
    }
}

#[derive(Debug, PartialEq, Serialize)]
pub struct CentralTransactionWritten {
    pub tx: CentralTransaction,
    pub time_created: u64,
}

impl From<(Transaction, u64)> for CentralTransactionWritten {
    fn from((tx, timestamp): (Transaction, u64)) -> CentralTransactionWritten {
        CentralTransactionWritten {
            tx: CentralTransaction::from(tx),
            // This timestamp is required for metrics data. Yoni and Noa approved that it is
            // sufficient to take the time during the batcher run.
            time_created: timestamp,
        }
    }
}

// Converts the CasmContractClass into a format that serializes into the python object.
// TODO(Yael): remove allow dead code once used
#[allow(dead_code)]
pub fn casm_contract_class_central_format(
    compiled_class_hash: CasmContractClass,
) -> CasmContractClass {
    CasmContractClass {
        // The rust object allows these fields to be none, while in python they are mandatory.
        bytecode_segment_lengths: Some(
            compiled_class_hash.bytecode_segment_lengths.unwrap_or(NestedIntList::Node(vec![])),
        ),
        pythonic_hints: Some(compiled_class_hash.pythonic_hints.unwrap_or_default()),
        ..compiled_class_hash
    }
}

/// A mapping from a transaction execution resource to its actual usage.
#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct ResourcesMapping(pub HashMap<String, usize>);

impl From<TransactionReceipt> for ResourcesMapping {
    fn from(receipt: TransactionReceipt) -> ResourcesMapping {
        let vm_resources = &receipt.resources.computation.vm_resources;
        let mut resources = HashMap::from([(
            abi_constants::N_STEPS_RESOURCE.to_string(),
            vm_resources.total_n_steps() + receipt.resources.computation.n_reverted_steps,
        )]);
        resources.extend(
            vm_resources
                .prover_builtins()
                .iter()
                .map(|(builtin, value)| (builtin.to_str_with_suffix().to_string(), *value)),
        );

        ResourcesMapping(resources)
    }
}

#[derive(Debug, Serialize)]
pub struct CentralTransactionExecutionInfo {
    pub validate_call_info: Option<CallInfo>,
    pub execute_call_info: Option<CallInfo>,
    pub fee_transfer_call_info: Option<CallInfo>,
    pub actual_fee: Fee,
    pub da_gas: GasVector,
    pub actual_resources: ResourcesMapping,
    pub revert_error: Option<String>,
    pub total_gas: GasVector,
}

impl From<TransactionExecutionInfo> for CentralTransactionExecutionInfo {
    fn from(tx_execution_info: TransactionExecutionInfo) -> CentralTransactionExecutionInfo {
        CentralTransactionExecutionInfo {
            validate_call_info: tx_execution_info.validate_call_info,
            execute_call_info: tx_execution_info.execute_call_info,
            fee_transfer_call_info: tx_execution_info.fee_transfer_call_info,
            actual_fee: tx_execution_info.receipt.fee,
            da_gas: tx_execution_info.receipt.da_gas,
            revert_error: tx_execution_info.revert_error.map(|error| error.to_string()),
            total_gas: tx_execution_info.receipt.gas,
            actual_resources: tx_execution_info.receipt.into(),
        }
    }
}
