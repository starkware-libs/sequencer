use crate::hint_processor::os_state_reader::OsStateReader;

pub struct StarknetOsInput {}

impl OsStateReader for StarknetOsInput {}

pub struct OsExecutionHelper<T: OsStateReader> {
    _os_initial_state_reader: T,
}

impl<T: OsStateReader> OsExecutionHelper<T> {
    pub fn new(os_initial_state_reader: T) -> Self {
        Self { _os_initial_state_reader: os_initial_state_reader }
    }
}
