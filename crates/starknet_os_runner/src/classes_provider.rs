use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use blockifier::execution::contract_class::{CompiledClassV1, RunnableCompiledClass};
use blockifier::state::state_api::StateReader;
use blockifier::state::state_reader_and_contract_manager::{
    FetchCompiledClasses,
    StateReaderAndContractManager,
};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_utils::bigint::BigUintAsHex;
use cairo_vm::types::relocatable::MaybeRelocatable;
use futures::future::try_join_all;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_types_core::felt::Felt;

use crate::errors::ClassesProviderError;

/// Converts a `CompiledClassV1` to a `CasmContractClass` for OS execution.
/// Note: Some fields are not preserved in `CompiledClassV1` and are set to default values:
/// - `compiler_version`: Set to empty string
/// - `hints`: Set to empty (OS doesn't use them from this struct for Cairo 1 contracts)
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

/// The classes required for a Starknet OS run.
/// Matches the fields in `StarknetOsInput`.
pub struct ClassesInput {
    /// Cairo 1+ contract classes (CASM).
    /// Maps CompiledClassHash to the CASM contract class definition.
    pub compiled_classes: BTreeMap<CompiledClassHash, CasmContractClass>,
}

#[async_trait]
pub trait ClassesProvider: Sized + Clone + Send + Sync + 'static {
    /// Fetches all classes required for the OS run based on the executed class hashes.
    /// This default implementation parallelizes fetching by spawning blocking tasks.
    async fn get_classes(
        &self,
        executed_class_hashes: &HashSet<ClassHash>,
    ) -> Result<ClassesInput, ClassesProviderError> {
        // Creating tasks to fetch classes in parallel.
        let tasks = executed_class_hashes.iter().map(|&class_hash| {
            let provider = self.clone();
            tokio::task::spawn_blocking(move || provider.fetch_class(class_hash))
        });

        // Fetching classes in parallel.
        // If any task fails, the entire operation fails.
        let results = try_join_all(tasks)
            .await
            .map_err(|e| ClassesProviderError::GetClassesError(format!("Task join error: {e}")))?;

        // Collecting results into a BTreeMap.
        let compiled_classes = results
            .into_iter()
            .collect::<Result<BTreeMap<CompiledClassHash, CasmContractClass>, ClassesProviderError>>()?;

        Ok(ClassesInput { compiled_classes })
    }

    /// Fetches class by class hash.
    fn fetch_class(
        &self,
        class_hash: ClassHash,
    ) -> Result<(CompiledClassHash, CasmContractClass), ClassesProviderError>;
}

#[async_trait]
impl<S: FetchCompiledClasses + Send + Sync + 'static> ClassesProvider
    for Arc<StateReaderAndContractManager<S>>
{
    /// Fetch class from the state reader and contract manager.
    /// Returns error if the class is deprecated.
    fn fetch_class(
        &self,
        class_hash: ClassHash,
    ) -> Result<(CompiledClassHash, CasmContractClass), ClassesProviderError> {
        let compiled_class = self.get_compiled_class(class_hash)?;
        // TODO(Aviv): Make sure that the state reader is not returning dummy compiled class hash.
        let compiled_class_hash = self.get_compiled_class_hash_v2(class_hash, &compiled_class)?;
        match compiled_class {
            RunnableCompiledClass::V0(_v0) => {
                Err(ClassesProviderError::DeprecatedContractError(class_hash))
            }
            RunnableCompiledClass::V1(compiled_class_v1) => {
                let casm = compiled_class_v1_to_casm(&compiled_class_v1);
                Ok((compiled_class_hash, casm))
            }
            #[cfg(feature = "cairo_native")]
            RunnableCompiledClass::V1Native(compiled_class_v1_native) => {
                let compiled_class_v1 = compiled_class_v1_native.casm();
                let casm = compiled_class_v1_to_casm(&compiled_class_v1);
                Ok((compiled_class_hash, casm))
            }
        }
    }
}
