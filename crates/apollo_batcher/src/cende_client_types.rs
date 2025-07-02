//! Local copies of types from apollo_starknet_client for use in batcher, prefixed with
//! StarknetClient.
use std::collections::HashMap;

use apollo_starknet_client::reader::objects::state::StateDiff;
use apollo_starknet_client::reader::objects::transaction::ReservedDataAvailabilityMode;
use apollo_starknet_client::reader::{DeclaredClassHashEntry, DeployedContract, StorageEntry};
use blockifier::execution::call_info::OrderedEvent;
use blockifier::state::cached_state::{StateMaps, StorageView};
// TODO(noamsp): find a way to share the TransactionReceipt from apollo_starknet_client and
// remove this module.
use blockifier::transaction::objects::TransactionExecutionInfo;
use cairo_vm::types::builtin_name::BuiltinName;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use starknet_api::block::{
    BlockInfo,
    BlockTimestamp,
    GasPricePerToken,
    GasPrices,
    StarknetVersion,
};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    EntryPointSelector,
    EthAddress,
    Nonce,
};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::executable_transaction::L1HandlerTransaction as ExecutableL1HandlerTransaction;
use starknet_api::execution_resources::GasVector;
use starknet_api::hash::StarkHash;
use starknet_api::rpc_transaction::{
    InternalRpcDeployAccountTransaction,
    InternalRpcTransaction,
    RpcDeployAccountTransaction,
    RpcInvokeTransaction,
};
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    AllResourceBounds,
    Calldata,
    ContractAddressSalt,
    Fee,
    PaymasterData,
    ResourceBounds,
    Tip,
    TransactionSignature,
};
use starknet_api::transaction::{
    Event,
    L1ToL2Payload,
    L2ToL1Payload,
    TransactionHash,
    TransactionOffsetInBlock,
    TransactionVersion,
};

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L1ToL2Nonce(pub StarkHash);

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct L1ToL2Message {
    pub from_address: EthAddress,
    pub to_address: ContractAddress,
    pub selector: EntryPointSelector,
    pub payload: L1ToL2Payload,
    #[serde(default)]
    pub nonce: L1ToL2Nonce,
}

// TODO(Arni): This code already appears somewhere else in the codebase, consider sharing it.
impl From<starknet_api::transaction::L1HandlerTransaction> for L1ToL2Message {
    fn from(l1_handler_transaction: starknet_api::transaction::L1HandlerTransaction) -> Self {
        let calldata = l1_handler_transaction.calldata;
        let from_address = calldata.0[0].try_into().expect("Failed to convert EthAddress");
        let payload = L1ToL2Payload(calldata.0[1..].to_vec());
        Self {
            from_address,
            to_address: l1_handler_transaction.contract_address,
            selector: l1_handler_transaction.entry_point_selector,
            payload,
            nonce: L1ToL2Nonce(l1_handler_transaction.nonce.0),
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct L2ToL1Message {
    pub from_address: ContractAddress,
    pub to_address: EthAddress,
    pub payload: L2ToL1Payload,
}

// Note: the serialization is different from the one in starknet_api.
#[derive(Hash, Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub enum Builtin {
    #[serde(rename = "range_check_builtin")]
    RangeCheck,
    #[serde(rename = "pedersen_builtin")]
    Pedersen,
    #[serde(rename = "poseidon_builtin")]
    Poseidon,
    #[serde(rename = "ec_op_builtin")]
    EcOp,
    #[serde(rename = "ecdsa_builtin")]
    Ecdsa,
    #[serde(rename = "bitwise_builtin")]
    Bitwise,
    #[serde(rename = "keccak_builtin")]
    Keccak,
    // Note: in starknet_api this variant doesn't exist.
    #[serde(rename = "output_builtin")]
    Output,
    #[serde(rename = "segment_arena_builtin")]
    SegmentArena,
    #[serde(rename = "add_mod_builtin")]
    AddMod,
    #[serde(rename = "mul_mod_builtin")]
    MulMod,
    #[serde(rename = "range_check96_builtin")]
    RangeCheck96,
}

impl From<BuiltinName> for Builtin {
    fn from(builtin_name: BuiltinName) -> Self {
        match builtin_name {
            BuiltinName::range_check => Builtin::RangeCheck,
            BuiltinName::pedersen => Builtin::Pedersen,
            BuiltinName::poseidon => Builtin::Poseidon,
            BuiltinName::ec_op => Builtin::EcOp,
            BuiltinName::ecdsa => Builtin::Ecdsa,
            BuiltinName::bitwise => Builtin::Bitwise,
            BuiltinName::keccak => Builtin::Keccak,
            BuiltinName::output => Builtin::Output,
            BuiltinName::segment_arena => Builtin::SegmentArena,
            BuiltinName::add_mod => Builtin::AddMod,
            BuiltinName::mul_mod => Builtin::MulMod,
            BuiltinName::range_check96 => Builtin::RangeCheck96,
        }
    }
}

/// The execution resources used by a transaction.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExecutionResources {
    // Note: in starknet_api this field is named `steps`
    pub n_steps: u64,
    pub builtin_instance_counter: HashMap<Builtin, u64>,
    // Note: in starknet_api this field is named `memory_holes`
    pub n_memory_holes: u64,
    // This field is missing in blocks created before v0.13.1, even if the feeder gateway is of
    // that version
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_availability: Option<GasVector>,
    // This field is missing in blocks created before v0.13.2, even if the feeder gateway is of
    // that version
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_gas_consumed: Option<GasVector>,
}

/// Transaction execution status.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, Default)]
pub enum TransactionExecutionStatus {
    #[serde(rename = "SUCCEEDED")]
    #[default]
    Succeeded,
    #[serde(rename = "REVERTED")]
    Reverted,
}

// TODO(Arni): Consider deleting derive default for this type. Same for members of this struct.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StarknetClientTransactionReceipt {
    pub transaction_index: TransactionOffsetInBlock,
    pub transaction_hash: TransactionHash,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub l1_to_l2_consumed_message: Option<L1ToL2Message>,
    pub l2_to_l1_messages: Vec<L2ToL1Message>,
    pub events: Vec<Event>,
    #[serde(default)]
    pub execution_resources: ExecutionResources,
    pub actual_fee: Fee,
    // TODO(Yair): Check if we can remove the serde(default).
    #[serde(default)]
    pub execution_status: TransactionExecutionStatus,
    // Note that in starknet_api this field is named `revert_reason`.
    // Assumption: if the transaction execution status is Succeeded, then revert_error is None, and
    // if the transaction execution status is Reverted, then revert_error is Some.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revert_error: Option<String>,
}

// Conversion logic from blockifier types to StarknetClient types.
impl
    From<(
        TransactionHash,
        // TODO(Arni): change the type of this parameter to TransactionOffsetInBlock
        usize,
        &TransactionExecutionInfo,
        Option<starknet_api::transaction::L1HandlerTransaction>,
    )> for StarknetClientTransactionReceipt
{
    fn from(
        (tx_hash, tx_index, tx_execution_info, l1_handler): (
            TransactionHash,
            usize,
            &TransactionExecutionInfo,
            Option<starknet_api::transaction::L1HandlerTransaction>,
        ),
    ) -> Self {
        let l2_to_l1_messages = get_l2_to_l1_messages(tx_execution_info);
        let events = get_events_from_execution_info(tx_execution_info);
        let execution_resources = get_execution_resources(tx_execution_info);
        let execution_status = if tx_execution_info.is_reverted() {
            TransactionExecutionStatus::Reverted
        } else {
            TransactionExecutionStatus::Succeeded
        };

        // TODO(Arni): I assume this is not the correct way to fill this field.
        let revert_error =
            tx_execution_info.revert_error.as_ref().map(|revert_error| revert_error.to_string());

        Self {
            transaction_index: TransactionOffsetInBlock(tx_index),
            transaction_hash: tx_hash,
            // TODO(Arni): Fill this up. This is relevant only for L1 handler transactions.
            l1_to_l2_consumed_message: l1_handler.map(L1ToL2Message::from),
            l2_to_l1_messages,
            events,
            execution_resources,
            actual_fee: tx_execution_info.receipt.fee,
            execution_status,
            revert_error,
        }
    }
}

fn get_l2_to_l1_messages(execution_info: &TransactionExecutionInfo) -> Vec<L2ToL1Message> {
    // TODO(Arni): Fix this call. The iterator returns all the call infos in the order: `validate`,
    // `execute`, `fee_transfer`. For `deploy_account` transactions, the order is `execute`,
    // `validate`, `fee_transfer`.
    let call_info_iterator = execution_info.non_optional_call_infos();

    let mut l2_to_l1_messages = vec![];
    for call in call_info_iterator {
        let messages =
            call.execution.l2_to_l1_messages.iter().map(|l2_to_l1_message| L2ToL1Message {
                from_address: call.call.caller_address,
                to_address: EthAddress::try_from(l2_to_l1_message.message.to_address)
                    .expect("Failed to convert L1Address to EthAddress"),
                payload: l2_to_l1_message.message.payload.clone(),
            });
        l2_to_l1_messages.extend(messages);
    }

    l2_to_l1_messages
}

fn get_events_from_execution_info(execution_info: &TransactionExecutionInfo) -> Vec<Event> {
    let call_info = if let Some(ref call_info) = execution_info.execute_call_info {
        call_info
    } else {
        return vec![];
    };

    // Collect all the events from the call infos, along with their order.
    let mut accumulated_sortable_events = vec![];
    for call_info in call_info.iter() {
        let sortable_events = call_info
            .execution
            .events
            .iter()
            .map(|orderable_event| (call_info.call.caller_address, orderable_event));
        accumulated_sortable_events.extend(sortable_events);
    }
    // Sort the events by their order.
    accumulated_sortable_events.sort_by_key(|(_, OrderedEvent { order, .. })| *order);

    // Convert the sorted events into the StarknetClient Event type.
    accumulated_sortable_events
        .iter()
        .map(|(from_address, OrderedEvent { event, .. })| Event {
            from_address: *from_address,
            content: event.clone(),
        })
        .collect()
}

fn get_execution_resources(execution_info: &TransactionExecutionInfo) -> ExecutionResources {
    let receipt = &execution_info.receipt;
    let resources = &receipt.resources.computation.total_vm_resources();
    let builtin_instance_counter = resources
        .builtin_instance_counter
        .iter()
        .map(|(&builtin_name, &count)| {
            (builtin_name.into(), count.try_into().expect("Failed to convert usize to u64"))
        })
        .collect();

    ExecutionResources {
        n_steps: resources.n_steps.try_into().expect("Failed to convert usize to u64"),
        builtin_instance_counter,
        n_memory_holes: resources
            .n_memory_holes
            .try_into()
            .expect("Failed to convert usize to u64"),
        data_availability: Some(receipt.da_gas),
        total_gas_consumed: Some(receipt.gas),
    }
}

// TODO(shahak): consider extracting common fields out (version, hash, type).
// This is a modified version of the enum
// `apollo_starknet_client::reader::objects::transaction::Transaction`.
// The main difference is that the `Deploy` variant is not present in this enum.
// Also a few modifications were made to the serialization format.
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "type")]
pub enum CendePreconfirmedTransaction {
    #[serde(rename = "DECLARE")]
    Declare(IntermediateDeclareTransaction),
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(IntermediateDeployAccountTransaction),
    #[serde(rename = "INVOKE_FUNCTION")]
    Invoke(IntermediateInvokeTransaction),
    #[serde(rename = "L1_HANDLER")]
    L1Handler(L1HandlerTransaction),
}

impl CendePreconfirmedTransaction {
    pub fn transaction_hash(&self) -> TransactionHash {
        match self {
            CendePreconfirmedTransaction::Declare(tx) => tx.transaction_hash,
            CendePreconfirmedTransaction::DeployAccount(tx) => tx.transaction_hash,
            CendePreconfirmedTransaction::Invoke(tx) => tx.transaction_hash,
            CendePreconfirmedTransaction::L1Handler(tx) => tx.transaction_hash,
        }
    }
}

impl From<InternalConsensusTransaction> for CendePreconfirmedTransaction {
    fn from(transaction: InternalConsensusTransaction) -> Self {
        match transaction {
            InternalConsensusTransaction::RpcTransaction(internal_rpc_transaction) => {
                internal_rpc_transaction.into()
            }
            InternalConsensusTransaction::L1Handler(l1_handler_transaction) => {
                l1_handler_transaction.into()
            }
        }
    }
}

// TODO(Arni): Share code with `crates/apollo_consensus_orchestrator/src/cende/central_objects.rs`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Eq)]
pub struct CentralResourceBounds {
    #[serde(rename = "L1_GAS")]
    l1_gas: ResourceBounds,
    #[serde(rename = "L2_GAS")]
    l2_gas: ResourceBounds,
    #[serde(rename = "L1_DATA_GAS")]
    l1_data_gas: ResourceBounds,
}

impl From<AllResourceBounds> for CentralResourceBounds {
    fn from(resource_bounds: AllResourceBounds) -> CentralResourceBounds {
        CentralResourceBounds {
            l1_gas: resource_bounds.l1_gas,
            l2_gas: resource_bounds.l2_gas,
            l1_data_gas: resource_bounds.l1_data_gas,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IntermediateDeclareTransaction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_bounds: Option<CentralResourceBounds>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tip: Option<Tip>,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compiled_class_hash: Option<CompiledClassHash>,
    pub sender_address: ContractAddress,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce_data_availability_mode: Option<ReservedDataAvailabilityMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_data_availability_mode: Option<ReservedDataAvailabilityMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paymaster_data: Option<PaymasterData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_deployment_data: Option<AccountDeploymentData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_fee: Option<Fee>,
    pub version: TransactionVersion,
    pub transaction_hash: TransactionHash,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IntermediateDeployAccountTransaction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_bounds: Option<CentralResourceBounds>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tip: Option<Tip>,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: Calldata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce_data_availability_mode: Option<ReservedDataAvailabilityMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_data_availability_mode: Option<ReservedDataAvailabilityMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paymaster_data: Option<PaymasterData>,
    // In early versions of starknet, the `sender_address` field was originally named
    // `contract_address`.
    #[serde(alias = "contract_address")]
    pub sender_address: ContractAddress,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_fee: Option<Fee>,
    pub transaction_hash: TransactionHash,
    pub version: TransactionVersion,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IntermediateInvokeTransaction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_bounds: Option<CentralResourceBounds>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tip: Option<Tip>,
    pub calldata: Calldata,
    // In early versions of starknet, the `sender_address` field was originally named
    // `contract_address`.
    #[serde(alias = "contract_address")]
    pub sender_address: ContractAddress,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_point_selector: Option<EntryPointSelector>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<Nonce>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_fee: Option<Fee>,
    pub signature: TransactionSignature,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce_data_availability_mode: Option<ReservedDataAvailabilityMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_data_availability_mode: Option<ReservedDataAvailabilityMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paymaster_data: Option<PaymasterData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_deployment_data: Option<AccountDeploymentData>,
    pub transaction_hash: TransactionHash,
    pub version: TransactionVersion,
}

impl From<InternalRpcTransaction> for CendePreconfirmedTransaction {
    fn from(internal_rpc_transaction: InternalRpcTransaction) -> Self {
        let tx_hash = internal_rpc_transaction.tx_hash;
        match internal_rpc_transaction.tx {
            starknet_api::rpc_transaction::InternalRpcTransactionWithoutTxHash::Declare(
                declare_transaction,
            ) => {
                let version = declare_transaction.version();
                CendePreconfirmedTransaction::Declare(IntermediateDeclareTransaction {
                    resource_bounds: Some(declare_transaction.resource_bounds.into()),
                    tip: Some(declare_transaction.tip),
                    signature: declare_transaction.signature,
                    nonce: declare_transaction.nonce,
                    class_hash: declare_transaction.class_hash,
                    compiled_class_hash: Some(declare_transaction.compiled_class_hash),
                    sender_address: declare_transaction.sender_address,
                    nonce_data_availability_mode: Some(
                        declare_transaction.nonce_data_availability_mode.into(),
                    ),
                    fee_data_availability_mode: Some(
                        declare_transaction.fee_data_availability_mode.into(),
                    ),
                    paymaster_data: Some(declare_transaction.paymaster_data),
                    account_deployment_data: Some(declare_transaction.account_deployment_data),
                    version,
                    transaction_hash: tx_hash,
                    // Irrelevant for V3 declare transactions.
                    max_fee: None,
                })
            }
            starknet_api::rpc_transaction::InternalRpcTransactionWithoutTxHash::DeployAccount(
                deploy_account_transaction,
            ) => {
                let version = deploy_account_transaction.version();
                let InternalRpcDeployAccountTransaction {
                    tx: RpcDeployAccountTransaction::V3(tx),
                    contract_address,
                } = deploy_account_transaction;
                CendePreconfirmedTransaction::DeployAccount(IntermediateDeployAccountTransaction {
                    resource_bounds: Some(tx.resource_bounds.into()),
                    tip: Some(tx.tip),
                    signature: tx.signature,
                    nonce: tx.nonce,
                    class_hash: tx.class_hash,
                    contract_address_salt: tx.contract_address_salt,
                    constructor_calldata: tx.constructor_calldata,
                    nonce_data_availability_mode: Some(tx.nonce_data_availability_mode.into()),
                    fee_data_availability_mode: Some(tx.fee_data_availability_mode.into()),
                    paymaster_data: Some(tx.paymaster_data),
                    sender_address: contract_address,
                    transaction_hash: tx_hash,
                    version,
                    // Irrelevant for V3 deploy account transactions.
                    max_fee: None,
                })
            }
            starknet_api::rpc_transaction::InternalRpcTransactionWithoutTxHash::Invoke(
                invoke_transaction,
            ) => {
                let version = invoke_transaction.version();
                let RpcInvokeTransaction::V3(tx) = invoke_transaction;
                CendePreconfirmedTransaction::Invoke(IntermediateInvokeTransaction {
                    resource_bounds: Some(tx.resource_bounds.into()),
                    tip: Some(tx.tip),
                    calldata: tx.calldata,
                    sender_address: tx.sender_address,
                    nonce: Some(tx.nonce),
                    signature: tx.signature,
                    nonce_data_availability_mode: Some(tx.nonce_data_availability_mode.into()),
                    fee_data_availability_mode: Some(tx.fee_data_availability_mode.into()),
                    paymaster_data: Some(tx.paymaster_data),
                    account_deployment_data: Some(tx.account_deployment_data),
                    version,
                    transaction_hash: tx_hash,
                    // Irrelevant for V3 invoke transactions.
                    entry_point_selector: None,
                    max_fee: None,
                })
            }
        }
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct L1HandlerTransaction {
    pub transaction_hash: TransactionHash,
    pub version: TransactionVersion,
    #[serde(default)]
    pub nonce: Nonce,
    pub contract_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub calldata: Calldata,
}

impl From<ExecutableL1HandlerTransaction> for CendePreconfirmedTransaction {
    fn from(l1_handler_transaction: ExecutableL1HandlerTransaction) -> Self {
        let ExecutableL1HandlerTransaction { tx, tx_hash, .. } = l1_handler_transaction;
        CendePreconfirmedTransaction::L1Handler(L1HandlerTransaction {
            transaction_hash: tx_hash,
            version: tx.version,
            nonce: tx.nonce,
            contract_address: tx.contract_address,
            entry_point_selector: tx.entry_point_selector,
            calldata: tx.calldata,
        })
    }
}

const PRE_CONFIRMED_STATUS: &str = "PRE_CONFIRMED";

#[derive(Serialize, Clone)]
pub struct CendeBlockMetadata {
    pub status: &'static str,
    pub starknet_version: StarknetVersion,
    pub l1_da_mode: L1DataAvailabilityMode,
    pub l1_gas_price: GasPricePerToken,
    pub l1_data_gas_price: GasPricePerToken,
    pub l2_gas_price: GasPricePerToken,
    pub timestamp: BlockTimestamp,
    pub sequencer_address: ContractAddress,
}

impl CendeBlockMetadata {
    pub fn new(block_info: BlockInfo) -> Self {
        let l1_da_mode = match block_info.use_kzg_da {
            true => L1DataAvailabilityMode::Blob,
            false => L1DataAvailabilityMode::Calldata,
        };

        let (l1_gas_price, l1_data_gas_price, l2_gas_price) =
            get_gas_prices(&block_info.gas_prices);

        // TODO(noamsp): use correct version.
        let starknet_version = StarknetVersion::default();

        Self {
            status: PRE_CONFIRMED_STATUS,
            starknet_version,
            l1_da_mode,
            l1_gas_price,
            l1_data_gas_price,
            l2_gas_price,
            timestamp: block_info.block_timestamp,
            sequencer_address: block_info.sequencer_address,
        }
    }
}

fn get_gas_prices(
    gas_prices: &GasPrices,
) -> (GasPricePerToken, GasPricePerToken, GasPricePerToken) {
    (
        GasPricePerToken {
            price_in_fri: gas_prices.strk_gas_prices.l1_gas_price.into(),
            price_in_wei: gas_prices.eth_gas_prices.l1_gas_price.into(),
        },
        GasPricePerToken {
            price_in_fri: gas_prices.strk_gas_prices.l1_data_gas_price.into(),
            price_in_wei: gas_prices.eth_gas_prices.l1_data_gas_price.into(),
        },
        GasPricePerToken {
            price_in_fri: gas_prices.strk_gas_prices.l2_gas_price.into(),
            price_in_wei: gas_prices.eth_gas_prices.l2_gas_price.into(),
        },
    )
}

#[derive(Serialize)]
pub struct CendePreconfirmedBlock {
    #[serde(flatten)]
    pub metadata: CendeBlockMetadata,
    pub transactions: Vec<CendePreconfirmedTransaction>,
    pub transaction_receipts: Vec<Option<StarknetClientTransactionReceipt>>,
    pub transaction_state_diffs: Vec<Option<StateDiff>>,
}

pub struct StarknetClientStateDiff(pub StateDiff);

impl From<StateMaps> for StarknetClientStateDiff {
    fn from(state_maps: StateMaps) -> Self {
        StarknetClientStateDiff(StateDiff {
            storage_diffs: IndexMap::from(StorageView(state_maps.storage))
                .into_iter()
                .map(|(address, entries)| {
                    (
                        address,
                        entries
                            .into_iter()
                            .map(|(key, value)| StorageEntry { key, value })
                            .collect(),
                    )
                })
                .collect(),
            deployed_contracts: state_maps
                .class_hashes
                .into_iter()
                .map(|(address, class_hash)| DeployedContract { address, class_hash })
                .collect(),
            declared_classes: state_maps
                .compiled_class_hashes
                .into_iter()
                .map(|(class_hash, compiled_class_hash)| DeclaredClassHashEntry {
                    class_hash,
                    compiled_class_hash,
                })
                .collect(),
            old_declared_contracts: Default::default(),
            nonces: state_maps.nonces.into_iter().collect(),
            replaced_classes: Default::default(),
        })
    }
}
