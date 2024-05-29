use starknet_types_core::felt::Felt;
use starknet_types_core::hash::StarkHash;

use crate::core::EventCommitment;
use crate::crypto::patricia_hash::calculate_root;
use crate::crypto::utils::HashChain;
use crate::transaction::{Event, TransactionHash};

#[cfg(test)]
#[path = "event_commitment_test.rs"]
mod event_commitment_test;

/// The elements used to calculate a leaf in the transactions Patricia tree.
#[derive(Clone)]
pub struct EventLeafElement {
    pub(crate) event: Event,
    pub(crate) transaction_hash: TransactionHash,
}

/// Returns the root of a Patricia tree where each leaf is an event hash.
pub fn calculate_events_commitment<H: StarkHash>(
    event_leaf_elements: &[EventLeafElement],
) -> EventCommitment {
    let event_leaves = event_leaf_elements.iter().map(calculate_event_hash).collect();
    EventCommitment(calculate_root::<H>(event_leaves))
}

// Poseidon(
//    from_address, transaction_hash,
//    num_keys, key0, key1, ...,
//    num_contents, content0, content1, ...
// ).
fn calculate_event_hash(event_leaf_element: &EventLeafElement) -> Felt {
    let keys = &event_leaf_element.event.content.keys.iter().map(|k| k.0).collect::<Vec<Felt>>();
    let data = &event_leaf_element.event.content.data.0;
    HashChain::new()
        .chain(event_leaf_element.event.from_address.0.key())
        .chain(&event_leaf_element.transaction_hash.0)
        .chain_size_and_elements(keys)
        .chain_size_and_elements(data)
        .get_poseidon_hash()
}
