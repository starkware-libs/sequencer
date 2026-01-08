use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use blockifier::execution::contract_class::{CompiledClassV1, RunnableCompiledClass};
use blockifier::state::state_api::StateReader;
use blockifier::state::state_reader_and_contract_manager::{
    FetchCompiledClasses,
    StateReaderAndContractManager,
};
use cairo_lang_casm::hints::Hint;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_utils::bigint::BigUintAsHex;
use cairo_vm::serde::deserialize_program::HintParams;
use cairo_vm::types::relocatable::MaybeRelocatable;
use futures::future::try_join_all;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_types_core::felt::Felt;

use crate::errors::ClassesProviderError;

/// Converts a `CompiledClassV1` to a `CasmContractClass` for OS execution.
/// Note: Some fields are not preserved in `CompiledClassV1` and are set to default values:
/// - `compiler_version`: Set to empty string (not used by OS)
/// - `pythonic_hints`: Set to None (not used by OS)
/// - `hints`: Extracted from `Program` using PC positions and hint code lookup
fn compiled_class_v1_to_casm(
    class: &CompiledClassV1,
    class_hash: ClassHash,
    compiled_class_hash: CompiledClassHash,
) -> CasmContractClass {
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

    // Extract hints from Program with PC positions
    // The hints_collection contains HintParams with code strings, which we convert back to Hint
    // by looking them up in class.hints (which maps serialized hint strings to Hint objects)
    let hints: Vec<(usize, Vec<Hint>)> = {
        let hints_with_pc: BTreeMap<usize, Vec<HintParams>> =
            (&class.program.shared_program_data.hints_collection).into();
        hints_with_pc
            .into_iter()
            .map(|(pc, hint_params_vec)| {
                let hints: Vec<Hint> = hint_params_vec
                    .iter()
                    .filter_map(|hint_params| {
                        // Look up the hint by its serialized code string
                        class.hints.get(&hint_params.code).cloned()
                    })
                    .collect();
                (pc, hints)
            })
            .filter(|(_, hints)| !hints.is_empty()) // Only include PCs with hints
            .collect()
    };

    let new_casm = CasmContractClass {
        prime,
        compiler_version: String::new(),
        bytecode,
        bytecode_segment_lengths: <Option<_> as From<_>>::from(class.bytecode_segment_felt_sizes()),
        hints,
        pythonic_hints: None,
        entry_points_by_type: (&class.entry_points_by_type).into(),
    };

    // DEBUG: Compare with original CASM
    {
        use blockifier_reexecution::debug_casm_storage;

        if let Some(original_casm_json) = debug_casm_storage::get_original_casm(compiled_class_hash)
        {
            eprintln!(
                "=== DEBUG: Comparing CASMs for class_hash {:#x} (compiled_class_hash {:#x}) ===",
                class_hash.0, compiled_class_hash.0
            );

            // Parse original CASM from JSON
            if let Ok(original_casm) =
                serde_json::from_str::<CasmContractClass>(&original_casm_json)
            {
                // Compare field by field, ignoring defaults
                if original_casm.prime != new_casm.prime {
                    eprintln!(
                        "  DIFFERENCE: prime - original: {:?}, new: {:?}",
                        original_casm.prime, new_casm.prime
                    );
                }

                if !original_casm.compiler_version.is_empty()
                    && original_casm.compiler_version != new_casm.compiler_version
                {
                    eprintln!(
                        "  DIFFERENCE: compiler_version - original: {:?}, new: {:?}",
                        original_casm.compiler_version, new_casm.compiler_version
                    );
                }

                // Compare bytecode element by element
                if original_casm.bytecode.len() != new_casm.bytecode.len() {
                    eprintln!(
                        "  DIFFERENCE: bytecode length - original: {}, new: {}",
                        original_casm.bytecode.len(),
                        new_casm.bytecode.len()
                    );
                } else {
                    let mut diff_count = 0;
                    for (i, (orig, new)) in
                        original_casm.bytecode.iter().zip(new_casm.bytecode.iter()).enumerate()
                    {
                        if orig.value != new.value {
                            eprintln!(
                                "  DIFFERENCE: bytecode[{}] - original: {:?}, new: {:?}",
                                i, orig.value, new.value
                            );
                            diff_count += 1;
                        }
                    }
                    if diff_count == 0 {
                        eprintln!(
                            "  OK: bytecode matches ({} elements)",
                            original_casm.bytecode.len()
                        );
                    } else {
                        eprintln!("  Total bytecode differences: {}", diff_count);
                    }
                }

                if original_casm.bytecode_segment_lengths != new_casm.bytecode_segment_lengths {
                    eprintln!(
                        "  DIFFERENCE: bytecode_segment_lengths - original: {:?}, new: {:?}",
                        original_casm.bytecode_segment_lengths, new_casm.bytecode_segment_lengths
                    );
                } else {
                    eprintln!("  OK: bytecode_segment_lengths matches");
                }

                // Compare hints element by element
                if original_casm.hints.len() != new_casm.hints.len() {
                    eprintln!(
                        "  DIFFERENCE: hints length - original: {}, new: {}",
                        original_casm.hints.len(),
                        new_casm.hints.len()
                    );
                } else if !original_casm.hints.is_empty() {
                    let mut diff_count = 0;
                    for (i, (orig, new)) in
                        original_casm.hints.iter().zip(new_casm.hints.iter()).enumerate()
                    {
                        if orig != new {
                            eprintln!(
                                "  DIFFERENCE: hints[{}] - original: {:?}, new: {:?}",
                                i, orig, new
                            );
                            diff_count += 1;
                        }
                    }
                    if diff_count == 0 {
                        eprintln!("  OK: hints match ({} elements)", original_casm.hints.len());
                    } else {
                        eprintln!("  Total hints differences: {}", diff_count);
                    }
                } else {
                    eprintln!("  OK: hints match (both empty)");
                }

                // Compare pythonic_hints element by element
                match (&original_casm.pythonic_hints, &new_casm.pythonic_hints) {
                    (Some(orig_hints), Some(new_hints)) => {
                        if orig_hints.len() != new_hints.len() {
                            eprintln!(
                                "  DIFFERENCE: pythonic_hints length - original: {}, new: {}",
                                orig_hints.len(),
                                new_hints.len()
                            );
                        } else {
                            let mut diff_count = 0;
                            for (i, (orig, new)) in
                                orig_hints.iter().zip(new_hints.iter()).enumerate()
                            {
                                if orig != new {
                                    eprintln!(
                                        "  DIFFERENCE: pythonic_hints[{}] - original: {:?}, new: \
                                         {:?}",
                                        i, orig, new
                                    );
                                    diff_count += 1;
                                }
                            }
                            if diff_count == 0 {
                                eprintln!(
                                    "  OK: pythonic_hints match ({} elements)",
                                    orig_hints.len()
                                );
                            } else {
                                eprintln!("  Total pythonic_hints differences: {}", diff_count);
                            }
                        }
                    }
                    (Some(_), None) => {
                        eprintln!("  DIFFERENCE: pythonic_hints - original: Some(...), new: None");
                    }
                    (None, Some(_)) => {
                        eprintln!("  DIFFERENCE: pythonic_hints - original: None, new: Some(...)");
                    }
                    (None, None) => {
                        eprintln!("  OK: pythonic_hints match (both None)");
                    }
                }

                // Compare entry_points_by_type element by element
                // This is a complex nested structure, so we'll compare the serialized JSON
                // to see exact differences
                let orig_ep_json =
                    serde_json::to_string_pretty(&original_casm.entry_points_by_type)
                        .unwrap_or_else(|_| "Failed to serialize".to_string());
                let new_ep_json = serde_json::to_string_pretty(&new_casm.entry_points_by_type)
                    .unwrap_or_else(|_| "Failed to serialize".to_string());

                if orig_ep_json != new_ep_json {
                    eprintln!("  DIFFERENCE: entry_points_by_type");
                    eprintln!("    Original JSON:\n{}", orig_ep_json);
                    eprintln!("    New JSON:\n{}", new_ep_json);
                } else {
                    eprintln!("  OK: entry_points_by_type matches");
                }
            } else {
                eprintln!("  ERROR: Failed to parse original CASM from JSON");
            }

            eprintln!("=== END DEBUG: CASM comparison ===");
        } else {
            eprintln!(
                "WARNING: No original CASM found for class_hash {:#x} (compiled_class_hash {:#x})",
                class_hash.0, compiled_class_hash.0
            );
        }
    }

    new_casm
}

/// Fetch class from the state reader and contract manager.
/// Returns error if the class is deprecated.
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
            let casm =
                compiled_class_v1_to_casm(&compiled_class_v1, class_hash, compiled_class_hash);
            Ok((compiled_class_hash, casm))
        }
        #[cfg(feature = "cairo_native")]
        RunnableCompiledClass::V1Native(compiled_class_v1_native) => {
            let compiled_class_v1 = compiled_class_v1_native.casm();
            let casm =
                compiled_class_v1_to_casm(&compiled_class_v1, class_hash, compiled_class_hash);
            Ok((compiled_class_hash, casm))
        }
    }
}

/// The classes required for a Starknet OS run.
pub struct ClassesInput {
    /// Cairo 1+ contract classes (CASM).
    /// Maps CompiledClassHash to the CASM contract class definition.
    pub compiled_classes: BTreeMap<CompiledClassHash, CasmContractClass>,
    /// Mapping from ClassHash to CompiledClassHash for all executed classes.
    pub class_hash_to_compiled_class_hash: HashMap<ClassHash, CompiledClassHash>,
}

#[async_trait]
pub trait ClassesProvider {
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
            let class_hash_for_result = class_hash;

            tokio::task::spawn_blocking(move || {
                fetch_class(manager, class_hash_for_result).map(|(compiled_class_hash, casm)| {
                    (class_hash_for_result, compiled_class_hash, casm)
                })
            })
        });

        // Fetching classes in parallel.
        let results = try_join_all(tasks)
            .await
            .map_err(|e| ClassesProviderError::GetClassesError(format!("Task join error: {e}")))?;

        // Build both mappings
        let mut compiled_classes = BTreeMap::new();
        let mut class_hash_to_compiled_class_hash = HashMap::new();

        for result in results {
            let (class_hash, compiled_class_hash, casm) = result?;
            compiled_classes.insert(compiled_class_hash, casm);
            class_hash_to_compiled_class_hash.insert(class_hash, compiled_class_hash);
        }

        Ok(ClassesInput { compiled_classes, class_hash_to_compiled_class_hash })
    }
}
