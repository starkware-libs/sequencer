use blockifier::state::cached_state::CachedState;
use blockifier::state::state_api::StateReader;

use crate::io::os_input::StarknetOsInput;

/// A helper struct that provides access to the OS state and commitments.
pub struct OsExecutionHelper<S: StateReader> {
    _cached_state: CachedState<S>,
    _os_input: StarknetOsInput,
}

impl<S: StateReader> OsExecutionHelper<S> {
    pub fn new(os_input: StarknetOsInput) -> Self {
        Self { _cached_state: Self::initialize_cached_state(&os_input), _os_input: os_input }
    }

    // TODO(Dori): Create a cached state with all initial read values from the OS input.
    fn initialize_cached_state(_os_input: &StarknetOsInput) -> CachedState<S> {
        todo!()
    }
}
