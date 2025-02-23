use std::collections::HashMap;

use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::cached_state::{CachedState, StateMaps};
use blockifier::state::state_api::StateReader;
#[cfg(any(feature = "testing", test))]
use blockifier::test_utils::dict_state_reader::DictStateReader;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::Relocatable;
use starknet_api::contract_class::SierraVersion;

use crate::errors::StarknetOsError;
use crate::io::os_input::{CachedStateInput, StarknetOsInput};

/// A helper struct that provides access to the OS state and commitments.
pub struct OsExecutionHelper<S: StateReader> {
    pub(crate) cached_state: CachedState<S>,
    pub(crate) os_input: StarknetOsInput,
    // TODO(Meshi): change it to pub(crate) when it is used in future hints.
    pub(crate) os_program: Program,
}

impl<S: StateReader> OsExecutionHelper<S> {
    pub fn new(
        os_input: StarknetOsInput,
        os_program: Program,
        state_reader: S,
        state_input: CachedStateInput,
    ) -> Result<Self, StarknetOsError> {
        Ok(Self {
            cached_state: Self::initialize_cached_state(state_reader, state_input)?,
            os_input,
            os_program,
        })
    }

    fn initialize_cached_state(
        state_reader: S,
        state_input: CachedStateInput,
    ) -> Result<CachedState<S>, StarknetOsError> {
        let mut empty_cached_state = CachedState::new(state_reader);
        let mut state_maps = StateMaps::default();
        let mut contract_classes = HashMap::new();

        // Insert contracts.
        for (class_hash, deprecated_class) in state_input.deprecated_compiled_classes.into_iter() {
            contract_classes
                .insert(class_hash, RunnableCompiledClass::V0(deprecated_class.try_into()?));
        }
        for (class_hash, class) in state_input.compiled_classes.into_iter() {
            // It doesn't matter which version is used.
            let sierra_version = SierraVersion::LATEST;
            contract_classes
                .insert(class_hash, RunnableCompiledClass::V1((class, sierra_version).try_into()?));
        }

        // Insert storage.
        for (contract_address, storage) in state_input.storage.into_iter() {
            for (key, value) in storage.into_iter() {
                state_maps.storage.insert((contract_address, key), value);
            }
        }
        // Insert nonces.
        state_maps.nonces = state_input.address_to_nonce;

        // Insert class hashes.
        state_maps.class_hashes = state_input.address_to_class_hash;

        // Insert compiled class hashes.
        state_maps.compiled_class_hashes = state_input.class_hash_to_compiled_class_hash;

        // Update the cached state.
        empty_cached_state.update_cache(&state_maps, contract_classes);

        Ok(empty_cached_state)
    }
}

#[cfg(any(feature = "testing", test))]
impl OsExecutionHelper<DictStateReader> {
    pub fn new_for_testing(
        state_reader: DictStateReader,
        os_input: StarknetOsInput,
        os_program: Program,
    ) -> Self {
        Self { cached_state: CachedState::from(state_reader), os_input, os_program }
    }
}

#[derive(Clone)]
pub(crate) struct StateUpdatePointers {
    pub(crate) _state_tree_pointer: Relocatable,
    pub(crate) _class_tree_pointer: Relocatable,
}
