use crate::core::EventCommitment;
use crate::crypto::HashChain;
use crate::hash::StarkFelt;
use crate::transaction::{Event, TransactionHash};

#[cfg(test)]
#[path = "event_hash_test.rs"]
mod event_hash_test;

/// Poseidon(
///    from_address, transaction_hash,
///    num_keys, key0, key1, ...,
///    num_contents, content0, content1, ...
/// ).
pub fn calculate_event_hash(event: &Event, transaction_hash: &TransactionHash) -> EventCommitment {
    let keys = &event.content.keys.iter().map(|k| k.0).collect::<Vec<StarkFelt>>();
    let data = &event.content.data.0;
    EventCommitment(
        HashChain::new()
            .chain(event.from_address.0.key())
            .chain(&transaction_hash.0)
            .chain_size_and_elements(keys)
            .chain_size_and_elements(data)
            .get_poseidon_hash(),
    )
}
