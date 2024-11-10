use std::collections::HashMap;

use starknet_types_core::felt::Felt;

use crate::state::cached_state::{CachedState, StorageEntry};
use crate::state::state_api::{StateReader, StateResult};

/// Returns the number of storage keys charged for new allocated aliases.
pub fn n_charged_storage_keys<S: StateReader>(
    _state: &CachedState<S>,
    _storage_changes: &HashMap<StorageEntry, Felt>,
) -> StateResult<usize> {
    // TODO: Implement this function
    Ok(0)
}
