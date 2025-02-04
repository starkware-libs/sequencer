use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::cached_state::CachedState;
use blockifier::state::state_api::{StateReader, StateResult};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::io::os_input::{OsCommitments, StarknetOsInput};

/// A helper struct that provides access to the OS state and commitments.
pub struct OsExecutionHelper<S: StateReader> {
    cached_state: CachedState<S>,
    _commitments: OsCommitments,
}

impl<S: StateReader> OsExecutionHelper<S> {
    pub fn new(os_input: &StarknetOsInput) -> Self {
        Self {
            cached_state: Self::initialise_cached_state(os_input),
            _commitments: OsCommitments::new(os_input),
        }
    }

    // TODO(Dori): Create a cached state with all initial read values from the OS input.
    fn initialise_cached_state(_os_input: &StarknetOsInput) -> CachedState<S> {
        todo!()
    }
}
