use std::collections::HashMap;

use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_types_core::felt::Felt;

use super::CompiledClassHashMigrationUpdater;
use crate::state::cached_state::CachedState;
use crate::state::state_api::StateReader;
use crate::test_utils::dict_state_reader::DictStateReader;

/// Migration must record the pre-block (v1) hash as an initial read.
#[test]
fn migration_records_pre_block_compiled_class_hash_as_initial_read() {
    let class_hash = ClassHash(Felt::from(1234_u32));
    let compiled_class_hash_v1 = CompiledClassHash(Felt::from(1_u32));
    let compiled_class_hash_v2 = CompiledClassHash(Felt::from(2_u32));

    // Pre-block state holds v1; the cache is empty, as if the hash was never force-read.
    let dict_state_reader = DictStateReader {
        class_hash_to_compiled_class_hash: HashMap::from([(class_hash, compiled_class_hash_v1)]),
        ..Default::default()
    };
    let mut state = CachedState::new(dict_state_reader);

    state
        .set_compiled_class_hash_migration(&HashMap::from([(
            class_hash,
            (compiled_class_hash_v2, compiled_class_hash_v1),
        )]))
        .unwrap();

    // Migration wrote v2.
    assert_eq!(state.get_compiled_class_hash(class_hash).unwrap(), compiled_class_hash_v2);

    // v1 was captured as an initial read, and the diff is a clean v1 -> v2 migration.
    let state_diff = state.to_state_diff().unwrap().state_maps;
    assert_eq!(
        state_diff.compiled_class_hashes,
        HashMap::from([(class_hash, compiled_class_hash_v2)])
    );

    #[cfg(feature = "reexecution")]
    assert_eq!(
        state.get_initial_reads().unwrap().compiled_class_hashes,
        HashMap::from([(class_hash, compiled_class_hash_v1)]),
    );
}
