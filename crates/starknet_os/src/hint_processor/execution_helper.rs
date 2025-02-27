use blockifier::state::cached_state::CachedState;
use blockifier::state::state_api::StateReader;
use cairo_vm::types::program::Program;

use crate::io::os_input::StarknetOsInput;

/// A helper struct that provides access to the OS state and commitments.
pub struct OsExecutionHelper<S: StateReader> {
    pub(crate) cached_state: CachedState<S>,
    pub(crate) os_input: StarknetOsInput,
    pub(crate) _os_program: Program,
}

impl<S: StateReader> OsExecutionHelper<S> {
    pub fn new(os_input: StarknetOsInput, os_program: Program) -> Self {
        Self {
            cached_state: Self::initialize_cached_state(&os_input),
            os_input,
            _os_program: os_program,
        }
    }

    // TODO(Dori): Create a cached state with all initial read values from the OS input.
    fn initialize_cached_state(_os_input: &StarknetOsInput) -> CachedState<S> {
        todo!()
    }
}
