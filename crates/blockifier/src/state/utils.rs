use starknet_api::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use starknet_api::core::{ClassHash, CompiledClassHash};

use crate::execution::contract_class::RunnableCompiledClass;
use crate::state::errors::StateError;
use crate::state::state_api::{StateReader, StateResult};

/// Default implementation of `get_compiled_class_hash_v2`, of state reader trait.
/// Returns the compiled class hash (v2) of the given class hash.
/// Returns `CompiledClassHash::default()` if no v1_class is found for the given class hash.
#[allow(dead_code)]
pub fn get_compiled_class_hash_v2(
    state_reader: &impl StateReader,
    class_hash: ClassHash,
) -> StateResult<CompiledClassHash> {
    match state_reader.get_compiled_class(class_hash) {
        Ok(RunnableCompiledClass::V0(_)) | Err(StateError::UndeclaredClassHash(_)) => {
            Ok(CompiledClassHash::default())
        }
        Ok(RunnableCompiledClass::V1(class)) => Ok(class.hash(&HashVersion::V2)),
        #[cfg(feature = "cairo_native")]
        Ok(RunnableCompiledClass::V1Native(class)) => Ok(class.hash(&HashVersion::V2)),
        Err(e) => Err(e),
    }
}
