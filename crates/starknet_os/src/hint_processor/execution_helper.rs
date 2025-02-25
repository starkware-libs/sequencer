use blockifier::context::ChainInfo;
use blockifier::state::cached_state::CachedState;
use blockifier::state::state_api::StateReader;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::test_utils::initial_test_state::test_state;
use starknet_api::transaction::fields::Fee;

use crate::io::os_input::StarknetOsInput;

/// A helper struct that provides access to the OS state and commitments.
pub struct OsExecutionHelper<S: StateReader> {
    pub cached_state: CachedState<S>,
    _os_input: StarknetOsInput,
}

impl<S: StateReader> OsExecutionHelper<S> {
    pub fn new(os_input: StarknetOsInput) -> Self {
        Self { cached_state: Self::initialize_cached_state(&os_input), _os_input: os_input }
    }

    // TODO(Dori): Create a cached state with all initial read values from the OS input.
    fn initialize_cached_state(_os_input: &StarknetOsInput) -> CachedState<S> {
        todo!()
    }
}

#[cfg(any(feature = "testing", test))]
impl OsExecutionHelper<DictStateReader> {
    pub fn new_for_testing(os_input: StarknetOsInput) -> Self {
        Self {
            cached_state: test_state(&ChainInfo::create_for_testing(), Fee(0), &[]),
            _os_input: os_input,
        }
    }
}
