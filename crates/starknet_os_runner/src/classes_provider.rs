use std::collections::{BTreeMap, HashSet};

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass;

use crate::errors::ClassesProviderError;

/// The classes required for a Starknet OS run.
/// Matches the fields in `StarknetOsInput` and `OsBlockInput`.
pub struct ClassesInput {
    /// Deprecated (Cairo 0) contract classes.
    /// Maps ClassHash to the contract class definition.
    pub deprecated_compiled_classes: BTreeMap<ClassHash, ContractClass>,
    /// Cairo 1+ contract classes (CASM).
    /// Maps CompiledClassHash to the CASM contract class definition.
    pub compiled_classes: BTreeMap<CompiledClassHash, CasmContractClass>,
}

pub trait ClassesProvider {
    /// Fetches all classes required for the OS run based on the executed class hashes.
    fn get_classes(
        &self,
        executed_class_hashes: &HashSet<ClassHash>,
    ) -> Result<ClassesInput, ClassesProviderError>;
}
