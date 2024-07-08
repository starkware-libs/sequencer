use starknet_types_core::felt::Felt;
use starknet_types_core::hash::StarkHash as CoreStarkHash;

use super::block_hash_calculator::TransactionHashingData;
use crate::core::TransactionCommitment;
use crate::crypto::patricia_hash::calculate_root;
use crate::crypto::utils::HashChain;
use crate::transaction::{TransactionHash, TransactionSignature};

#[cfg(test)]
#[path = "transaction_commitment_test.rs"]
mod transaction_commitment_test;

/// The elements used to calculate a leaf in the transactions Patricia tree.
#[derive(Clone)]
pub struct TransactionLeafElement {
    pub(crate) transaction_hash: TransactionHash,
    pub(crate) transaction_signature: Option<TransactionSignature>,
}

impl From<&TransactionHashingData> for TransactionLeafElement {
    fn from(transaction_data: &TransactionHashingData) -> Self {
        Self {
            transaction_hash: transaction_data.transaction_hash,
            transaction_signature: transaction_data.transaction_signature.clone(),
        }
    }
}

/// Returns the root of a Patricia tree where each leaf is
/// H(transaction_hash, transaction_signature).
/// The leaf of a transaction types without a signature field is: H(transaction_hash, 0).
pub fn calculate_transaction_commitment<H: CoreStarkHash>(
    transaction_leaf_elements: &[TransactionLeafElement],
) -> TransactionCommitment {
    let transaction_leaves =
        transaction_leaf_elements.iter().map(calculate_transaction_leaf).collect();
    TransactionCommitment(calculate_root::<H>(transaction_leaves))
}

fn calculate_transaction_leaf(transaction_leaf_elements: &TransactionLeafElement) -> Felt {
    HashChain::new()
        .chain(&transaction_leaf_elements.transaction_hash.0)
        .chain_iter(
            transaction_leaf_elements
                .transaction_signature
                .as_ref()
                .unwrap_or(&TransactionSignature(vec![Felt::ZERO]))
                .0
                .iter(),
        )
        .get_poseidon_hash()
}
