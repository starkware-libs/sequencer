#![allow(dead_code)]
use blockifier::state::state_api::UpdatableState;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use starknet_committer::block_committer::input::StateDiff;

pub(crate) trait FlowTestState: Clone + UpdatableState + Sync + Send + 'static {
    fn create_empty_state() -> Self;

    /// Given a state diff with possible trivial entries (e.g., storage updates that set a value to
    /// it's previous value), return a state diff with only non-trivial entries.
    fn nontrivial_diff(&self, mut diff: StateDiff) -> StateDiff {
        // Remove trivial storage updates.
        diff.storage_updates.retain(|address, updates| {
            updates.retain(|key, value| {
                let current_value = self.get_storage_at(*address, key.0).unwrap_or_default();
                current_value != value.0
            });
            !updates.is_empty()
        });

        // Remove trivial nonce updates.
        diff.address_to_nonce.retain(|address, new_nonce| {
            let current_nonce = self.get_nonce_at(*address).unwrap_or_default();
            &current_nonce != new_nonce
        });

        // Remove trivial class hash updates.
        diff.address_to_class_hash.retain(|address, new_class_hash| {
            let current_class_hash = self.get_class_hash_at(*address).unwrap_or_default();
            &current_class_hash != new_class_hash
        });

        diff.class_hash_to_compiled_class_hash.retain(|class_hash, compiled_hash| {
            // Assume V2 hashes are stored in the state.
            let current_compiled_class_hash =
                self.get_compiled_class_hash(*class_hash).unwrap_or_default();
            current_compiled_class_hash.0 != compiled_hash.0
        });

        diff
    }
}

impl FlowTestState for DictStateReader {
    fn create_empty_state() -> Self {
        DictStateReader::default()
    }
}
