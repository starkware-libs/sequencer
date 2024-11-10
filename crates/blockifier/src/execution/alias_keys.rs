use crate::state::cached_state::{CachedState, StateChanges};
use crate::state::state_api::{StateReader, StateResult};

/// Returns the number of charged new allocated aliases.
pub fn n_charged_invoke_aliases<S: StateReader>(
    _state: &CachedState<S>,
    _state_changes: &StateChanges,
) -> StateResult<usize> {
    // TODO: Implement this function
    Ok(0)
}
