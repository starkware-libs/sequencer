//! Local copies of types from apollo_starknet_client for use in batcher, prefixed with
//! StarknetClient.
use std::collections::HashMap;

// TODO(noamsp): find a way to share the TransactionReceipt from apollo_starknet_client and
// remove this module.
use blockifier::transaction::objects::TransactionExecutionInfo;
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
pub struct StarknetClientL1ToL2Nonce(pub StarkHash);

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct StarknetClientL1ToL2Message {
    pub from_address: EthAddress,
    pub to_address: ContractAddress,
    pub selector: EntryPointSelector,
    pub payload: L1ToL2Payload,
    #[serde(default)]
    pub nonce: StarknetClientL1ToL2Nonce,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct StarknetClientL2ToL1Message {
    pub from_address: ContractAddress,
    pub to_address: EthAddress,
    pub payload: L2ToL1Payload,
}

// Note: the serialization is different from the one in starknet_api.
#[derive(Hash, Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub enum StarknetClientBuiltin {
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
pub struct StarknetClientExecutionResources {
    // Note: in starknet_api this field is named `steps`
    pub n_steps: u64,
    pub builtin_instance_counter: HashMap<StarknetClientBuiltin, u64>,
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
pub enum StarknetClientTransactionExecutionStatus {
    #[serde(rename = "SUCCEEDED")]
    #[default]
    Succeeded,
    #[serde(rename = "REVERTED")]
    Reverted,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StarknetClientTransactionReceipt {
    pub transaction_index: TransactionOffsetInBlock,
    pub transaction_hash: TransactionHash,
    #[serde(default)]
    pub l1_to_l2_consumed_message: StarknetClientL1ToL2Message,
    pub l2_to_l1_messages: Vec<StarknetClientL2ToL1Message>,
    pub events: Vec<Event>,
    #[serde(default)]
    pub execution_resources: StarknetClientExecutionResources,
    pub actual_fee: Fee,
    // TODO(Yair): Check if we can remove the serde(default).
    #[serde(default)]
    pub execution_status: StarknetClientTransactionExecutionStatus,
    // Note that in starknet_api this field is named `revert_reason`.
    // Assumption: if the transaction execution status is Succeeded, then revert_error is None, and
    // if the transaction execution status is Reverted, then revert_error is Some.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revert_error: Option<String>,
}

// Conversion logic from blockifier types to StarknetClient types.
impl From<(TransactionHash, usize, TransactionExecutionInfo)> for StarknetClientTransactionReceipt {
    fn from(
        (tx_hash, tx_index, tx_execution_info): (TransactionHash, usize, TransactionExecutionInfo),
    ) -> Self {
        // Events - convert from OrderedEvent to Event
        let events = tx_execution_info.execute_call_info.as_ref().map_or(Vec::new(), |call_info| {
            call_info
                .execution
                .events
                .iter()
                .map(|ordered_event| Event {
                    from_address: call_info.call.storage_address,
                    content: ordered_event.event.clone(),
                })
                .collect()
        });

        // L2 to L1 messages - get from_address from call_info and construct messages
        let l2_to_l1_messages =
            tx_execution_info.execute_call_info.as_ref().map_or(Vec::new(), |call_info| {
                call_info
                    .execution
                    .l2_to_l1_messages
                    .iter()
                    .map(|ordered_msg| StarknetClientL2ToL1Message {
                        from_address: call_info.call.storage_address,
                        to_address: ordered_msg.message.to_address,
                        payload: ordered_msg.message.payload.clone(),
                    })
                    .collect()
            });

        // L1 to L2 message (not available, set default)
        let l1_to_l2_consumed_message = StarknetClientL1ToL2Message::default();

        // Execution resources
        let execution_resources = tx_execution_info
            .execute_call_info
            .as_ref()
            .map(|call_info| {
                let resources = &call_info.resources;
                // TODO(noamsp): fill the builtin_instance_counter from tx_execution_info.
                let builtin_instance_counter = HashMap::new();

                StarknetClientExecutionResources {
                    n_steps: resources
                        .n_steps
                        .try_into()
                        .expect("Failed to convert n_steps to u64"),
                    n_memory_holes: resources
                        .n_memory_holes
                        .try_into()
                        .expect("Failed to convert n_memory_holes to u64"),
                    builtin_instance_counter,
                    data_availability: Some(tx_execution_info.receipt.da_gas),
                    total_gas_consumed: Some(tx_execution_info.receipt.gas),
                }
            })
            .unwrap_or_else(|| StarknetClientExecutionResources {
                data_availability: Some(tx_execution_info.receipt.da_gas),
                total_gas_consumed: Some(tx_execution_info.receipt.gas),
                ..Default::default()
            });

        // Status - determine from revert_error presence
        let execution_status = if tx_execution_info.revert_error.is_some() {
            StarknetClientTransactionExecutionStatus::Reverted
        } else {
            StarknetClientTransactionExecutionStatus::Succeeded
        };

        // Revert error
        let revert_error = tx_execution_info.revert_error.map(|e| e.to_string());

        Self {
            transaction_index: TransactionOffsetInBlock(tx_index),
            transaction_hash: tx_hash,
            l1_to_l2_consumed_message,
            l2_to_l1_messages,
            events,
            execution_resources,
            actual_fee: tx_execution_info.receipt.fee,
            execution_status,
            revert_error,
        }
    }
}
