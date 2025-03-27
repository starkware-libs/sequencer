use blockifier::state::state_api::StateReader;
use cairo_vm::cairo_run::CairoRunConfig;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::program::Program;
use cairo_vm::vm::errors::vm_exception::VmException;
use cairo_vm::vm::runners::cairo_runner::CairoRunner;

use crate::errors::StarknetOsError;
use crate::hint_processor::execution_helper::OsExecutionHelper;
use crate::hint_processor::panicking_state_reader::PanickingStateReader;
use crate::hint_processor::snos_hint_processor::{
    DeprecatedSyscallHintProcessor,
    SnosHintProcessor,
    SyscallHintProcessor,
};
use crate::io::os_input::{CachedStateInput, OsHints};
use crate::io::os_output::{get_run_output, StarknetOsRunnerOutput};

pub fn run_os<S: StateReader>(
    compiled_os: &[u8],
    layout: LayoutName,
    os_hints: OsHints,
    state_reader: S,
    cached_state_input: CachedStateInput,
) -> Result<StarknetOsRunnerOutput, StarknetOsError> {
    // Init CairoRunConfig.
    let cairo_run_config =
        CairoRunConfig { layout, relocate_mem: true, trace_enabled: true, ..Default::default() };
    let allow_missing_builtins = cairo_run_config.allow_missing_builtins.unwrap_or(false);

    // Load the Starknet OS Program.
    let os_program = Program::from_bytes(compiled_os, Some(cairo_run_config.entrypoint))?;

    // Init cairo runner.
    let mut cairo_runner = CairoRunner::new(
        &os_program,
        cairo_run_config.layout,
        cairo_run_config.proof_mode,
        cairo_run_config.trace_enabled,
    )?;

    // Init the Cairo VM.
    let end = cairo_runner.initialize(allow_missing_builtins)?;

    // Create execution helper.
    let execution_helper = OsExecutionHelper::new(
        os_hints.os_input,
        state_reader,
        cached_state_input,
        os_hints.os_hints_config.debug_mode,
    )?;

    // Create syscall handlers.
    let syscall_handler = SyscallHintProcessor::new();
    let deprecated_syscall_handler = DeprecatedSyscallHintProcessor {};

    // Create the hint processor.
    let mut snos_hint_processor = SnosHintProcessor::new(
        os_program,
        execution_helper,
        os_hints.os_hints_config,
        syscall_handler,
        deprecated_syscall_handler,
    );

    // Run the Cairo VM.
    cairo_runner
        .run_until_pc(end, &mut snos_hint_processor)
        .map_err(|err| VmException::from_vm_error(&cairo_runner, err))?;

    // End the Cairo VM run.
    let disable_finalize_all = false;
    cairo_runner.end_run(
        cairo_run_config.disable_trace_padding,
        disable_finalize_all,
        &mut snos_hint_processor,
    )?;

    if cairo_run_config.proof_mode {
        cairo_runner.finalize_segments()?;
    }

    // Prepare and check expected output.
    let os_output = get_run_output(&cairo_runner.vm)?;
    // TODO(Tzahi): log the output once it will have a proper struct.
    cairo_runner.vm.verify_auto_deductions().map_err(StarknetOsError::VirtualMachineError)?;
    cairo_runner
        .read_return_values(allow_missing_builtins)
        .map_err(StarknetOsError::RunnerError)?;
    cairo_runner
        .relocate(cairo_run_config.relocate_mem)
        .map_err(|e| StarknetOsError::VirtualMachineError(e.into()))?;

    // Parse the Cairo VM output.
    let cairo_pie = cairo_runner.get_cairo_pie().map_err(StarknetOsError::RunnerError)?;

    Ok(StarknetOsRunnerOutput { os_output, cairo_pie })
}

/// Run the OS with a "stateless" state reader - panics if the state is accessed for data that was
/// not pre-loaded as part of the input.
pub fn run_os_stateless(
    compiled_os: &[u8],
    layout: LayoutName,
    os_hints: OsHints,
    cached_state_input: CachedStateInput,
) -> Result<StarknetOsRunnerOutput, StarknetOsError> {
    run_os(compiled_os, layout, os_hints, PanickingStateReader, cached_state_input)
}
