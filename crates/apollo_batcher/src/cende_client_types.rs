//! Local copies of types from apollo_starknet_client for use in batcher, prefixed with
//! StarknetClient.
use std::collections::HashMap;

// TODO(noamsp): find a way to share the TransactionReceipt from apollo_starknet_client and
// remove this module.
use blockifier::transaction::objects::TransactionExecutionInfo;
use cairo_vm::types::builtin_name::BuiltinName;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ContractAddress, EntryPointSelector, EthAddress};
use starknet_api::execution_resources::GasVector;
use starknet_api::hash::StarkHash;
use starknet_api::transaction::fields::Fee;
use starknet_api::transaction::{
    Event,
    L1ToL2Payload,
    L2ToL1Payload,
    TransactionHash,
    TransactionOffsetInBlock,
};

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
struct L1ToL2Nonce(pub StarkHash);

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
struct L1ToL2Message {
    pub from_address: EthAddress,
    pub to_address: ContractAddress,
    pub selector: EntryPointSelector,
    pub payload: L1ToL2Payload,
    #[serde(default)]
    pub nonce: L1ToL2Nonce,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
struct L2ToL1Message {
    pub from_address: ContractAddress,
    pub to_address: EthAddress,
    pub payload: L2ToL1Payload,
}

// Note: the serialization is different from the one in starknet_api.
#[derive(Hash, Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
enum Builtin {
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

/// The execution resources used by a transaction.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct ExecutionResources {
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
enum TransactionExecutionStatus {
    #[serde(rename = "SUCCEEDED")]
    #[default]
    Succeeded,
    #[serde(rename = "REVERTED")]
    Reverted,
}

// TODO(Arni): Consider deleting derive default for this type. Same for members of this struct.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct StarknetClientTransactionReceipt {
    transaction_index: TransactionOffsetInBlock,
    transaction_hash: TransactionHash,
    #[serde(default)]
    l1_to_l2_consumed_message: Option<L1ToL2Message>,
    l2_to_l1_messages: Vec<L2ToL1Message>,
    events: Vec<Event>,
    #[serde(default)]
    execution_resources: ExecutionResources,
    actual_fee: Fee,
    // TODO(Yair): Check if we can remove the serde(default).
    #[serde(default)]
    execution_status: TransactionExecutionStatus,
    // Note that in starknet_api this field is named `revert_reason`.
    // Assumption: if the transaction execution status is Succeeded, then revert_error is None, and
    // if the transaction execution status is Reverted, then revert_error is Some.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    revert_error: Option<String>,
}

// Conversion logic from blockifier types to StarknetClient types.
impl
    From<(
        TransactionHash,
        // TODO(Arni): change the type of this parameter to TransactionOffsetInBlock
        usize,
        &TransactionExecutionInfo,
    )> for StarknetClientTransactionReceipt
{
    fn from(
        (tx_hash, tx_index, tx_execution_info): (TransactionHash, usize, &TransactionExecutionInfo),
    ) -> Self {
        let l2_to_l1_messages = get_l2_to_l1_messages(tx_execution_info);
        let events = get_events_from_execution_info(tx_execution_info);

        // TODO(Arni): I assume this is not the correct way to fill this field.
        let revert_error =
            tx_execution_info.revert_error.as_ref().map(|revert_error| revert_error.to_string());

        let execution_resources = get_execution_resources(tx_execution_info);
        Self {
            transaction_index: TransactionOffsetInBlock(tx_index),
            transaction_hash: tx_hash,
            // TODO(Arni): Fill this up. This is relevant only for L1 handler transactions.
            l1_to_l2_consumed_message: None,
            l2_to_l1_messages,
            events,
            revert_error,
            execution_resources,
            ..Default::default()
        }
    }
}

fn get_l2_to_l1_messages(execution_info: &TransactionExecutionInfo) -> Vec<L2ToL1Message> {
    let call_info = if let Some(ref call_info) = execution_info.execute_call_info {
        call_info
    } else {
        return vec![];
    };

    let mut l2_to_l1_messages = vec![];
    for call in call_info.iter() {
        let messages =
            call.execution.l2_to_l1_messages.iter().map(|l2_to_l1_message| L2ToL1Message {
                from_address: call.call.caller_address,
                to_address: l2_to_l1_message.message.to_address,
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

    let mut accumulated_events = vec![];
    for call_info in call_info.iter() {
        let events = call_info.execution.events.iter().map(|event| Event {
            from_address: call_info.call.caller_address,
            content: event.event.clone(),
        });
        accumulated_events.extend(events);
    }

    accumulated_events
}

fn convert_builtin_name_to_builtin(builtin_name: &BuiltinName) -> Builtin {
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

fn get_execution_resources(execution_info: &TransactionExecutionInfo) -> ExecutionResources {
    let receipt = &execution_info.receipt;
    let resources = &receipt.resources.computation.vm_resources;

    let builtin_instance_counter = resources
        .builtin_instance_counter
        .iter()
        .map(|(builtin_name, &count)| {
            (
                convert_builtin_name_to_builtin(builtin_name),
                u64::try_from(count).expect("Failed to convert usize to u64"),
            )
        })
        .collect();

    ExecutionResources {
        n_steps: u64::try_from(resources.n_steps).expect("Failed to convert usize to u64"),
        builtin_instance_counter,
        n_memory_holes: u64::try_from(resources.n_memory_holes)
            .expect("Failed to convert usize to u64"),
        data_availability: Some(receipt.da_gas),
        total_gas_consumed: Some(receipt.gas),
    }
}
