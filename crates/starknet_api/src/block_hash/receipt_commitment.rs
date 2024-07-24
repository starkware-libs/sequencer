use starknet_types_core::felt::Felt;
use starknet_types_core::hash::StarkHash;

use super::block_hash_calculator::{TransactionHashingData, TransactionOutputForHash};
use crate::core::ReceiptCommitment;
use crate::crypto::patricia_hash::calculate_root;
use crate::crypto::utils::HashChain;
use crate::hash::starknet_keccak_hash;
use crate::transaction::{GasVector, MessageToL1, TransactionExecutionStatus, TransactionHash};

#[cfg(test)]
#[path = "receipt_commitment_test.rs"]
mod receipt_commitment_test;

// The elements used to calculate a leaf in the transactions Patricia tree.
#[derive(Clone)]
pub struct ReceiptElement {
    pub transaction_hash: TransactionHash,
    pub transaction_output: TransactionOutputForHash,
}

impl From<&TransactionHashingData> for ReceiptElement {
    fn from(transaction_data: &TransactionHashingData) -> Self {
        Self {
            transaction_hash: transaction_data.transaction_hash,
            transaction_output: transaction_data.transaction_output.clone(),
        }
    }
}

/// Returns the root of a Patricia tree where each leaf is a receipt hash.
pub fn calculate_receipt_commitment<H: StarkHash>(
    receipt_elements: &[ReceiptElement],
) -> ReceiptCommitment {
    ReceiptCommitment(calculate_root::<H>(
        receipt_elements.iter().map(calculate_receipt_hash).collect(),
    ))
}

// Poseidon(
//    transaction hash, amount of fee paid, hash of messages sent, revert reason,
//    execution resources
// ).
fn calculate_receipt_hash(receipt_element: &ReceiptElement) -> Felt {
    let hash_chain = HashChain::new()
        .chain(&receipt_element.transaction_hash)
        .chain(&receipt_element.transaction_output.actual_fee.0.into())
        .chain(&calculate_messages_sent_hash(&receipt_element.transaction_output.messages_sent))
        .chain(&get_revert_reason_hash(&receipt_element.transaction_output.execution_status));
    chain_gas_consumed(hash_chain, &receipt_element.transaction_output.gas_consumed)
        .get_poseidon_hash()
}

// Poseidon(
//      num_messages_sent,
//      from_address_0, to_address_0, payload_length_0, payload_0,
//      from_address_1, to_address_1, payload_length_1, payload_1, ...
// ).
fn calculate_messages_sent_hash(messages_sent: &Vec<MessageToL1>) -> Felt {
    let mut messages_hash_chain = HashChain::new().chain(&messages_sent.len().into());
    for message_sent in messages_sent {
        messages_hash_chain = messages_hash_chain
            .chain(&message_sent.from_address)
            .chain(&message_sent.to_address.into())
            .chain_size_and_elements(&message_sent.payload.0);
    }
    messages_hash_chain.get_poseidon_hash()
}

// Returns starknet-keccak of the revert reason ASCII string, or 0 if the transaction succeeded.
fn get_revert_reason_hash(execution_status: &TransactionExecutionStatus) -> Felt {
    match execution_status {
        TransactionExecutionStatus::Succeeded => Felt::ZERO,
        TransactionExecutionStatus::Reverted(reason) => {
            starknet_keccak_hash(reason.revert_reason.as_bytes())
        }
    }
}

// Chains:
// L2 gas consumed (In the current RPC: always 0),
// L1 gas consumed (In the current RPC:
//      L1 gas consumed for calldata + L1 gas consumed for steps and builtins.
// L1 data gas consumed (In the current RPC: L1 data gas consumed for blob).
fn chain_gas_consumed(hash_chain: HashChain, gas_consumed: &GasVector) -> HashChain {
    hash_chain
        .chain(&Felt::ZERO) // L2 gas consumed
        .chain(&gas_consumed.l1_gas.into())
        .chain(&gas_consumed.l1_data_gas.into())
}
