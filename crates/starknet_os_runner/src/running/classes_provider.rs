use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use blockifier::execution::contract_class::{
    program_hints_to_casm_hints,
    CompiledClassV1,
    RunnableCompiledClass,
};
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
use tracing::error;

use crate::errors::ClassesProviderError;

/// Converts a `CompiledClassV1` to a `CasmContractClass` for OS execution.
/// Note: Some fields are not preserved in `CompiledClassV1` and are set to default values:
/// - `compiler_version`: Set to empty string
/// - `pythonic_hints`: Set to None
#[allow(dead_code)]
pub(crate) fn compiled_class_v1_to_casm(
    class: &CompiledClassV1,
) -> Result<CasmContractClass, ClassesProviderError> {
    // TODO(Aviv): Consider using dummy prime since it is not used in the OS.
    let prime = Felt::prime();

    let bytecode: Vec<BigUintAsHex> = class
        .program
        .iter_data()
        .map(|maybe_relocatable| match maybe_relocatable {
            MaybeRelocatable::Int(felt) => Ok(BigUintAsHex { value: felt.to_biguint() }),
            MaybeRelocatable::RelocatableValue(relocatable) => {
                error!(
                    "Unexpected error: bytecode of a class contained a relocatable value: {:?}",
                    relocatable
                );
                Err(ClassesProviderError::InvalidBytecodeElement)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(CasmContractClass {
        prime,
        compiler_version: String::new(),
        bytecode,
        bytecode_segment_lengths: Some(class.bytecode_segment_felt_sizes().into()),
        hints: program_hints_to_casm_hints(&class.program.shared_program_data.hints_collection)?,
        pythonic_hints: None,
        entry_points_by_type: (&class.entry_points_by_type).into(),
    })
}

/// Fetch class from the state reader and contract manager.
/// Returns error if the class is deprecated.
#[allow(dead_code)]
fn fetch_class<S>(
    state_reader_and_contract_manager: Arc<StateReaderAndContractManager<S>>,
    class_hash: ClassHash,
) -> Result<(CompiledClassHash, CasmContractClass), ClassesProviderError>
where
    S: FetchCompiledClasses + Send + Sync + 'static,
{
    let compiled_class = state_reader_and_contract_manager.get_compiled_class(class_hash)?;

    let compiled_class_hash = state_reader_and_contract_manager
        .get_compiled_class_hash_v2(class_hash, &compiled_class)?;

    match compiled_class {
        RunnableCompiledClass::V0(_v0) => {
            Err(ClassesProviderError::DeprecatedContractError(class_hash))
        }
        RunnableCompiledClass::V1(compiled_class_v1) => {
            let casm = compiled_class_v1_to_casm(&compiled_class_v1)?;
            Ok((compiled_class_hash, casm))
        }
        #[cfg(feature = "cairo_native")]
        RunnableCompiledClass::V1Native(compiled_class_v1_native) => {
            let compiled_class_v1 = compiled_class_v1_native.casm();
            let casm = compiled_class_v1_to_casm(&compiled_class_v1)?;
            Ok((compiled_class_hash, casm))
        }
        // Required when blockifier's `cairo_native` feature is enabled via feature unification
        // but this crate's `cairo_native` feature is not (e.g. clippy with --all-features).
        #[allow(unreachable_patterns)]
        _ => unreachable!(),
    }
}

/// The classes required for a Starknet OS run.
#[allow(dead_code)]
pub(crate) struct ClassesInput {
    /// Cairo 1+ contract classes (CASM).
    /// Maps CompiledClassHash to the CASM contract class definition.
    pub(crate) compiled_classes: BTreeMap<CompiledClassHash, CasmContractClass>,
    /// Maps ClassHash to CompiledClassHash.
    pub(crate) class_hash_to_compiled_class_hash: HashMap<ClassHash, CompiledClassHash>,
}

#[async_trait]
#[allow(dead_code)]
pub(crate) trait ClassesProvider {
    /// Fetches all classes required for the OS run based on the executed class hashes.
    async fn get_classes(
        &self,
        executed_class_hashes: &HashSet<ClassHash>,
    ) -> Result<ClassesInput, ClassesProviderError>;
}

#[async_trait]
impl<S> ClassesProvider for Arc<StateReaderAndContractManager<S>>
where
    S: FetchCompiledClasses + Send + Sync + 'static,
{
    async fn get_classes(
        &self,
        executed_class_hashes: &HashSet<ClassHash>,
    ) -> Result<ClassesInput, ClassesProviderError> {
        // Clone the Arc so we can move an owned value into `spawn_blocking`.
        let shared_contract_class_manager = self.clone();

        // Creating tasks to fetch classes in parallel.
        let tasks = executed_class_hashes.iter().map(|&class_hash| {
            let manager = shared_contract_class_manager.clone();

            tokio::task::spawn_blocking(move || fetch_class(manager, class_hash))
        });

        // Fetching classes in parallel.
        let results = try_join_all(tasks)
            .await
            .map_err(|e| ClassesProviderError::GetClassesError(format!("Task join error: {e}")))?;

        // Collecting results into maps.
        let mut compiled_classes = BTreeMap::new();
        let mut class_hash_to_compiled_class_hash = HashMap::new();

        for (class_hash, result) in executed_class_hashes.iter().zip(results) {
            let (compiled_class_hash, casm) = result?;
            compiled_classes.insert(compiled_class_hash, casm);
            class_hash_to_compiled_class_hash.insert(*class_hash, compiled_class_hash);
        }

        Ok(ClassesInput { compiled_classes, class_hash_to_compiled_class_hash })
    }
}
