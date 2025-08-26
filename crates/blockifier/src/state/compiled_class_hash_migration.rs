use std::collections::HashMap;

use starknet_api::core::ClassHash;

use crate::blockifier::transaction_executor::CompiledClassHashV2ToV1;
use crate::state::cached_state::CachedState;
use crate::state::state_api::{State, StateReader, StateResult};

pub trait CompiledClassHashMigrationUpdater {
    fn set_compiled_class_hash_migration(
        &mut self,
        class_hashes_to_migrate: &HashMap<ClassHash, CompiledClassHashV2ToV1>,
    ) -> StateResult<()>;
}

impl<S: StateReader> CompiledClassHashMigrationUpdater for CachedState<S> {
    // Sets the new compiled class hashes for the class hashes that need to be migrated.
    fn set_compiled_class_hash_migration(
        &mut self,
        class_hashes_to_migrate: &HashMap<ClassHash, CompiledClassHashV2ToV1>,
    ) -> StateResult<()> {
        for (class_hash, (compiled_class_hash_v2, compiled_class_hash_v1)) in
            class_hashes_to_migrate
        {
            // Sanity check: the compiled class hashes should not be equal.
            assert_ne!(
                compiled_class_hash_v1, compiled_class_hash_v2,
                "Classes for migration should hold v1 (Poseidon) hash in the state."
            );

            // TODO(Meshi): Consider panic here instead of returning an error.
            self.set_compiled_class_hash(*class_hash, *compiled_class_hash_v2)?;
        }

        Ok(())
    }
}
