//! In-memory proving using the stwo_run_and_prove library.
//!
//! This module provides types and functions for proving CairoPie instances
//! in-memory without writing to temporary files.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use cairo_program_runner_lib::ProgramInput;
use cairo_program_runner_lib::hints::types::{HashFunc, SimpleBootloaderInput, Task, TaskSpec};
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::runners::cairo_pie::{CairoPie, SegmentInfo};
use stwo_run_and_prove_lib::{ProveConfig, RunConfig, StwoProverEntryPoint, stwo_run_and_prove};

/// Merges extra memory segments in a CairoPie into a single segment, replicating the behavior of
/// `CairoPie::write_zip_file(_, merge_extra_segments=true)`.
///
/// The prover expects extra segments to be merged. Without this, CairoPies from `run_virtual_os`
/// may have multiple extra segments, producing a different trace layout.
fn merge_extra_segments(cairo_pie: &mut CairoPie) {
    if cairo_pie.metadata.extra_segments.is_empty() {
        return;
    }

    let new_index = cairo_pie.metadata.extra_segments[0].index;
    let mut accumulated_size = 0usize;
    let mut segment_offsets: HashMap<usize, usize> = HashMap::new();

    for segment in &cairo_pie.metadata.extra_segments {
        segment_offsets.insert(segment.index as usize, accumulated_size);
        accumulated_size += segment.size;
    }

    cairo_pie.metadata.extra_segments =
        vec![SegmentInfo { index: new_index, size: accumulated_size }];

    for ((segment, offset), value) in &mut cairo_pie.memory.0 {
        if let Some(&base_offset) = segment_offsets.get(segment) {
            *offset += base_offset;
            *segment = new_index as usize;
        }
        if let MaybeRelocatable::RelocatableValue(relocatable) = value {
            if let Some(&base_offset) = segment_offsets.get(&(relocatable.segment_index as usize)) {
                relocatable.offset += base_offset;
                relocatable.segment_index = new_index;
            }
        }
    }
}

fn create_bootloader_input_from_pie(cairo_pie: CairoPie) -> SimpleBootloaderInput {
    let task = Task::Pie(cairo_pie);
    let task_spec = TaskSpec { task: Rc::new(task), program_hash_function: HashFunc::Blake };
    SimpleBootloaderInput { fact_topologies_path: None, single_page: true, tasks: vec![task_spec] }
}

/// Runs the stwo prover on a CairoPie in-memory (synchronous).
///
/// # Arguments
///
/// * `bootloader_program_path` - Path to the compiled simple bootloader program.
/// * `cairo_pie` - The CairoPie to prove.
/// * `program_output_path` - Optional path for program output (proof facts).
/// * `prove_config` - Configuration for the prover.
pub(crate) fn prove_pie_in_memory(
    bootloader_program_path: PathBuf,
    mut cairo_pie: CairoPie,
    program_output_path: Option<PathBuf>,
    prove_config: ProveConfig,
) -> Result<(), stwo_run_and_prove_lib::StwoRunAndProveError> {
    merge_extra_segments(&mut cairo_pie);
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
