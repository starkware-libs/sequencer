use blockifier::state::cached_state::CachedState;
use blockifier::state::state_api::StateReader;
#[cfg(any(feature = "testing", test))]
use blockifier::test_utils::dict_state_reader::DictStateReader;
use cairo_vm::types::program::Program;

use crate::io::os_input::StarknetOsInput;

/// A helper struct that provides access to the OS state and commitments.
pub struct OsExecutionHelper<S: StateReader> {
    pub(crate) cached_state: CachedState<S>,
    pub(crate) os_input: StarknetOsInput,
    pub(crate) os_program: Program,
}

impl<S: StateReader> OsExecutionHelper<S> {
    pub fn new(os_input: StarknetOsInput, os_program: Program) -> Self {
        Self { cached_state: Self::initialize_cached_state(&os_input), os_input, os_program }
    }

    // TODO(Dori): Create a cached state with all initial read values from the OS input.
    fn initialize_cached_state(_os_input: &StarknetOsInput) -> CachedState<S> {
        todo!()
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
