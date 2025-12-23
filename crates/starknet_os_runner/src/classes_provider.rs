use std::collections::{BTreeMap, HashSet};

use blockifier::execution::contract_class::{CompiledClassV1, RunnableCompiledClass};
use blockifier::state::contract_class_manager::ContractClassManager;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_utils::bigint::BigUintAsHex;
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_api::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_types_core::felt::Felt;

use crate::errors::ClassesProviderError;

/// The classes required for a Starknet OS run.
/// Matches the fields in `StarknetOsInput`.
pub struct ClassesInput {
    /// Deprecated (Cairo 0) contract classes.
    /// Maps ClassHash to the contract class definition.
    pub deprecated_compiled_classes: BTreeMap<ClassHash, DeprecatedContractClass>,
    /// Cairo 1+ contract classes (CASM).
    /// Maps CompiledClassHash to the CASM contract class definition.
    pub compiled_classes: BTreeMap<CompiledClassHash, CasmContractClass>,
}

pub trait ClassesProvider {
    /// Fetches a class from an external source (e.g., storage, network).
    /// Called when the class is not found in the `ContractClassManager`.
    fn fetch_class(&self, class_hash: ClassHash) -> Result<ContractClass, ClassesProviderError>;

    /// Gets all classes required for the OS run.
    ///
    /// For each class hash:
    /// 1. Tries to get the class from `ContractClassManager` first
    /// 2. Falls back to `fetch_class` if not found in manager
    /// 3. Converts to the appropriate format for OS execution
    fn get_classes(
        &self,
        executed_class_hashes: &HashSet<ClassHash>,
        contract_class_manager: &ContractClassManager,
    ) -> Result<ClassesInput, ClassesProviderError> {
        let mut deprecated_compiled_classes = BTreeMap::new();
        let mut compiled_classes = BTreeMap::new();

        for &class_hash in executed_class_hashes {
            // Try manager first.
            if let Some(runnable) = contract_class_manager.get_runnable(&class_hash) {
                match runnable {
                    RunnableCompiledClass::V0(_) => {
                        unimplemented!("V0 class conversion from manager not yet supported")
                    }
                    RunnableCompiledClass::V1(v1) => {
                        let casm = Self::compiled_class_v1_to_casm(&v1);
                        // TODO(Aviv): The state reader in the contract class manager does not
                        // support get compiled class hash v2. We need to
                        // implement this in the state reader.
                        let compiled_class_hash = contract_class_manager
                            .get_compiled_class_hash_v2(&class_hash)
                            .unwrap_or_else(|| casm.hash(&HashVersion::V2));
                        compiled_classes.insert(compiled_class_hash, casm);
                    }
                }
                continue;
            }

            // Fallback to fetch.
            match self.fetch_class(class_hash)? {
                ContractClass::V0(deprecated_class) => {
                    deprecated_compiled_classes.insert(class_hash, deprecated_class);
                }
                ContractClass::V1((casm, _sierra_version)) => {
                    let compiled_hash = casm.hash(&HashVersion::V2);
                    compiled_classes.insert(compiled_hash, casm);
                }
            }
        }

        Ok(ClassesInput { deprecated_compiled_classes, compiled_classes })
    }

    /// Converts a `CompiledClassV1` to a `CasmContractClass` for OS execution.
    /// Note: Some fields are not preserved in `CompiledClassV1` and are set to default values:
    /// - `compiler_version`: Set to empty string
    /// - `hints`: Set to empty (OS doesn't use them from this struct)
    /// - `pythonic_hints`: Set to None
    fn compiled_class_v1_to_casm(class: &CompiledClassV1) -> CasmContractClass {
        // TODO(Aviv): Consider using dummy prime since it is not used in the OS.
        let prime = Felt::prime();

        let bytecode: Vec<BigUintAsHex> = class
            .program
            .iter_data()
            .map(|maybe_relocatable| match maybe_relocatable {
                MaybeRelocatable::Int(felt) => BigUintAsHex { value: felt.to_biguint() },
                _ => panic!("Expected all bytecode elements to be MaybeRelocatable::Int"),
            })
            .collect();

        CasmContractClass {
            prime,
            compiler_version: String::new(),
            bytecode,
            bytecode_segment_lengths: Some(class.bytecode_segment_felt_sizes().into()),
            hints: Vec::new(),
            pythonic_hints: None,
            entry_points_by_type: (&class.entry_points_by_type).into(),
        }
    }
}
