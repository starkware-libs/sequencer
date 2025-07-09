use std::collections::HashMap;

use blockifier::state::cached_state::CommitmentStateDiff;
use starknet_committer::block_committer::input::{
    ConfigImpl,
    Input,
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia_storage::storage_trait::{DbKey, DbValue};

#[allow(dead_code)]
pub(crate) type CommitterInput = Input<ConfigImpl>;

#[allow(dead_code)]
pub(crate) fn create_committer_input(
    state_diff: CommitmentStateDiff,
    fact_storage: HashMap<DbKey, DbValue>,
    contracts_trie_root_hash: HashOutput,
    classes_trie_root_hash: HashOutput,
) -> CommitterInput {
    let state_diff = StateDiff {
        address_to_class_hash: state_diff.address_to_class_hash.into_iter().collect(),
        address_to_nonce: state_diff.address_to_nonce.into_iter().collect(),
        class_hash_to_compiled_class_hash: state_diff
            .class_hash_to_compiled_class_hash
            .into_iter()
            .map(|(k, v)| (k, v.into()))
            .collect(),
        storage_updates: state_diff
            .storage_updates
            .into_iter()
            .map(|(address, updates)| {
                (
                    address,
                    updates
                        .into_iter()
                        .map(|(k, v)| (StarknetStorageKey(k), StarknetStorageValue(v)))
                        .collect(),
                )
            })
            .collect(),
    };
    let config = ConfigImpl::default();

    CommitterInput {
        state_diff,
        storage: fact_storage,
        contracts_trie_root_hash,
        classes_trie_root_hash,
        config,
    }
}
