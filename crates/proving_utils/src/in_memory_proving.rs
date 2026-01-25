//! In-memory proving using the stwo_run_and_prove library.
//!
//! This module provides types and functions for proving CairoPie instances
//! in-memory without writing to temporary files.

use std::path::PathBuf;
use std::rc::Rc;

// Re-export cairo-air for ProofFormat access.
pub use cairo_air;
// Re-export types from cairo-program-runner-lib.
pub use cairo_program_runner_lib::hints::types::{HashFunc, SimpleBootloaderInput, Task, TaskSpec};
pub use cairo_program_runner_lib::ProgramInput;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
// Re-export types from stwo_run_and_prove library.
pub use stwo_run_and_prove_lib::{
    ProveConfig,
    StwoProverEntryPoint,
    StwoRunAndProveError,
    stwo_run_and_prove,
};

/// Creates a `SimpleBootloaderInput` from a `CairoPie` for in-memory proving.
///
/// This wraps the CairoPie in the appropriate task structures expected by the
/// simple bootloader, avoiding the need to serialize to disk.
pub fn create_bootloader_input_from_pie(cairo_pie: CairoPie) -> SimpleBootloaderInput {
    let task = Task::Pie(cairo_pie);
    let task_spec = TaskSpec { task: Rc::new(task), program_hash_function: HashFunc::Blake };
    SimpleBootloaderInput { fact_topologies_path: None, single_page: true, tasks: vec![task_spec] }
}

/// Runs the stwo prover on a CairoPie in-memory.
///
/// # Arguments
///
/// * `bootloader_program_path` - Path to the compiled simple bootloader program.
/// * `cairo_pie` - The CairoPie to prove.
/// * `program_output_path` - Optional path for program output (proof facts).
/// * `prove_config` - Configuration for the prover.
///
/// # Returns
///
/// Ok(()) on success, or a `StwoRunAndProveError` on failure.
pub fn prove_pie_in_memory(
    bootloader_program_path: PathBuf,
    cairo_pie: CairoPie,
    program_output_path: Option<PathBuf>,
    prove_config: ProveConfig,
) -> Result<(), StwoRunAndProveError> {
    let bootloader_input = create_bootloader_input_from_pie(cairo_pie);
    let program_input = ProgramInput::from_value(bootloader_input);

    stwo_run_and_prove(
        bootloader_program_path,
        Some(program_input),
        program_output_path,
        prove_config,
        Box::new(StwoProverEntryPoint),
        None,  // debug_data_dir
        false, // save_debug_data
    )
}
