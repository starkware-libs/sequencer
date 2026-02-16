//! In-memory proving using the stwo_run_and_prove library.
//!
//! This module provides types and functions for proving CairoPie instances
//! in-memory without writing to temporary files.

use std::path::PathBuf;
use std::rc::Rc;

use cairo_program_runner_lib::hints::types::{HashFunc, SimpleBootloaderInput, Task, TaskSpec};
use cairo_program_runner_lib::ProgramInput;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use stwo_run_and_prove_lib::{
    ProveConfig,
    RunConfig,
    StwoProverEntryPoint,
    stwo_run_and_prove,
};

use super::error::StwoRunAndProveError;

/// Creates a `SimpleBootloaderInput` from a `CairoPie` for in-memory proving.
///
/// This wraps the CairoPie in the appropriate task structures expected by the
/// simple bootloader, avoiding the need to serialize to disk.
fn create_bootloader_input_from_pie(cairo_pie: CairoPie) -> SimpleBootloaderInput {
    let task = Task::Pie(cairo_pie);
    let task_spec = TaskSpec { task: Rc::new(task), program_hash_function: HashFunc::Blake };
    SimpleBootloaderInput { fact_topologies_path: None, single_page: true, tasks: vec![task_spec] }
}

/// Runs the stwo prover on a CairoPie in-memory.
///
/// The synchronous proving work is offloaded to a blocking thread via
/// `tokio::task::spawn_blocking`.
///
/// # Arguments
///
/// * `bootloader_program_path` - Path to the compiled simple bootloader program.
/// * `cairo_pie` - The CairoPie to prove.
/// * `program_output_path` - Optional path for program output (proof facts).
/// * `prove_config` - Configuration for the prover.
pub(crate) async fn prove_pie_in_memory(
    bootloader_program_path: PathBuf,
    cairo_pie: CairoPie,
    program_output_path: Option<PathBuf>,
    prove_config: ProveConfig,
) -> Result<(), StwoRunAndProveError> {
    Ok(tokio::task::spawn_blocking(move || {
        prove_pie_in_memory_sync(
            bootloader_program_path,
            cairo_pie,
            program_output_path,
            prove_config,
        )
    })
    .await??)
}

/// Synchronous inner implementation of in-memory proving.
fn prove_pie_in_memory_sync(
    bootloader_program_path: PathBuf,
    cairo_pie: CairoPie,
    program_output_path: Option<PathBuf>,
    prove_config: ProveConfig,
) -> Result<(), stwo_run_and_prove_lib::StwoRunAndProveError> {
    let bootloader_input = create_bootloader_input_from_pie(cairo_pie);
    let program_input = ProgramInput::from_value(bootloader_input);

    let run_config = RunConfig {
        program_path: bootloader_program_path,
        program_input: Some(program_input),
        program_output: program_output_path,
        debug_data_dir: None,
        save_debug_data: false,
        extra_hint_processor: None,
    };

    stwo_run_and_prove(run_config, prove_config, Box::new(StwoProverEntryPoint))
}
