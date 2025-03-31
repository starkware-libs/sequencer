use std::collections::HashMap;

use blockifier::state::cached_state::{CachedState, StateMaps};
use blockifier::state::state_api::StateReader;
#[cfg(any(feature = "testing", test))]
use blockifier::test_utils::dict_state_reader::DictStateReader;

use crate::errors::StarknetOsError;
use crate::hint_processor::os_logger::OsLogger;
use crate::io::os_input::{CachedStateInput, OsBlockInput};

/// A helper struct that provides access to the OS state and commitments.
pub struct OsExecutionHelper<S: StateReader> {
    pub(crate) cached_state: CachedState<S>,
    pub(crate) os_block_input: OsBlockInput,
    pub(crate) os_logger: OsLogger,
}

impl<S: StateReader> OsExecutionHelper<S> {
    pub fn new(
        os_block_input: OsBlockInput,
        state_reader: S,
        state_input: CachedStateInput,
        debug_mode: bool,
    ) -> Result<Self, StarknetOsError> {
        Ok(Self {
            cached_state: Self::initialize_cached_state(state_reader, state_input)?,
            os_block_input,
            os_logger: OsLogger::new(debug_mode),
        })
    }

    fn initialize_cached_state(
        state_reader: S,
        state_input: CachedStateInput,
    ) -> Result<CachedState<S>, StarknetOsError> {
        let mut empty_cached_state = CachedState::new(state_reader);
        let mut state_maps = StateMaps::default();

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
        empty_cached_state.update_cache(&state_maps, HashMap::new());

        Ok(empty_cached_state)
    }
}

#[cfg(any(feature = "testing", test))]
impl OsExecutionHelper<DictStateReader> {
    pub fn new_for_testing(state_reader: DictStateReader, os_input: OsBlockInput) -> Self {
        Self {
            cached_state: CachedState::from(state_reader),
            os_block_input: os_input,
            os_logger: OsLogger::new(true),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutionHelperError {
    #[error("Called a block execution-helper before it was initialized.")]
    NoCurrentExecutionHelper,
    #[error("Execution helper index {0} is out of bounds.")]
    ExecutionHelpersManagerOutOfBounds(usize),
}
