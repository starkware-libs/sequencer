use apollo_starknet_os_program::{AGGREGATOR_PROGRAM, OS_PROGRAM};
use blockifier::state::state_api::StateReader;
use cairo_vm::cairo_run::CairoRunConfig;
use cairo_vm::hint_processor::hint_processor_definition::HintProcessor;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::errors::vm_exception::VmException;
use cairo_vm::vm::runners::cairo_runner::CairoRunner;

use crate::errors::StarknetOsError;
use crate::hint_processor::aggregator_hint_processor::{AggregatorHintProcessor, AggregatorInput};
use crate::hint_processor::common_hint_processor::CommonHintProcessor;
use crate::hint_processor::panicking_state_reader::PanickingStateReader;
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::io::os_input::{OsHints, StarknetOsInput};
use crate::io::os_output::{get_run_output, StarknetRunnerOutput};

struct RunnerInitilizationData {
    proof_mode: bool,
    disable_trace_padding: bool,
    relocate_mem: bool,
    allow_missing_builtins: bool,
    cairo_runner: CairoRunner,
    end: Relocatable,
}

fn initialize_run(
    layout: LayoutName,
    program: &Program,
) -> Result<RunnerInitilizationData, StarknetOsError> {
    // Init CairoRunConfig.
    let cairo_run_config =
        CairoRunConfig { layout, relocate_mem: true, trace_enabled: true, ..Default::default() };
    let allow_missing_builtins = cairo_run_config.allow_missing_builtins.unwrap_or(false);

    // Init cairo runner.
    let mut cairo_runner = CairoRunner::new(
        program,
        cairo_run_config.layout,
        cairo_run_config.dynamic_layout_params,
        cairo_run_config.proof_mode,
        cairo_run_config.trace_enabled,
        cairo_run_config.disable_trace_padding,
    )?;

    // Init the Cairo VM.
    let end = cairo_runner.initialize(allow_missing_builtins)?;

    // Return the initialization data.
    Ok(RunnerInitilizationData {
        proof_mode: cairo_run_config.proof_mode,
        disable_trace_padding: cairo_run_config.disable_trace_padding,
        relocate_mem: cairo_run_config.relocate_mem,
        allow_missing_builtins,
        cairo_runner,
        end,
    })
}

// TODO(Aner): Make generic (CommonHintProcessor trait) depend on testing flag.
fn run_runner<'a, HP: HintProcessor + CommonHintProcessor<'a>>(
    cairo_runner: &mut CairoRunner,
    end: Relocatable,
    mut hint_processor: HP,
    config_proof_mode: bool,
    config_disable_trace_padding: bool,
    config_relocate_mem: bool,
    allow_missing_builtins: bool,
) -> Result<StarknetRunnerOutput, StarknetOsError> {
    // Run the Cairo VM.
    cairo_runner
        .run_until_pc(end, &mut hint_processor)
        .map_err(|err| Box::new(VmException::from_vm_error(cairo_runner, err)))?;

    // End the Cairo VM run.
    let disable_finalize_all = false;
    cairo_runner.end_run(
        config_disable_trace_padding,
        disable_finalize_all,
        &mut hint_processor,
    )?;

    if config_proof_mode {
        cairo_runner.finalize_segments()?;
    }

    // Prepare and check expected output.
    let output = get_run_output(&cairo_runner.vm)?;
    // TODO(Tzahi): log the output once it will have a proper struct.
    cairo_runner.vm.verify_auto_deductions().map_err(StarknetOsError::VirtualMachineError)?;
    cairo_runner
        .read_return_values(allow_missing_builtins)
        .map_err(StarknetOsError::RunnerError)?;
    cairo_runner
        .relocate(config_relocate_mem)
        .map_err(|e| StarknetOsError::VirtualMachineError(e.into()))?;

    // Parse the Cairo VM output.
    let cairo_pie = cairo_runner.get_cairo_pie().map_err(StarknetOsError::RunnerError)?;
    Ok(StarknetRunnerOutput {
        output,
        cairo_pie,
        #[cfg(any(test, feature = "testing"))]
        unused_hints: hint_processor.get_unused_hints(),
    })
}

pub fn run_os<S: StateReader>(
    layout: LayoutName,
    OsHints {
        os_hints_config,
        os_input:
            StarknetOsInput {
                os_block_inputs,
                cached_state_inputs,
                deprecated_compiled_classes,
                compiled_classes,
            },
    }: OsHints,
    state_readers: Vec<S>,
) -> Result<StarknetRunnerOutput, StarknetOsError> {
    let RunnerInitilizationData {
        proof_mode: config_proof_mode,
        disable_trace_padding: config_disable_trace_padding,
        relocate_mem: config_relocate_mem,
        allow_missing_builtins,
        mut cairo_runner,
        end,
    } = initialize_run(layout, &OS_PROGRAM)?;

    // Create the hint processor.
    let snos_hint_processor = SnosHintProcessor::new(
        &OS_PROGRAM,
        os_hints_config,
        os_block_inputs.iter().collect(),
        cached_state_inputs,
        deprecated_compiled_classes,
        compiled_classes,
        state_readers,
    )?;

    run_runner(
        &mut cairo_runner,
        end,
        snos_hint_processor,
        config_proof_mode,
        config_disable_trace_padding,
        config_relocate_mem,
        allow_missing_builtins,
    )
}

/// Run the OS with a "stateless" state reader - panics if the state is accessed for data that was
/// not pre-loaded as part of the input.
pub fn run_os_stateless(
    layout: LayoutName,
    os_hints: OsHints,
) -> Result<StarknetRunnerOutput, StarknetOsError> {
    let n_blocks = os_hints.os_input.os_block_inputs.len();
    run_os(layout, os_hints, vec![PanickingStateReader; n_blocks])
}

/// Run the Aggregator.
#[allow(clippy::result_large_err)]
pub fn run_aggregator(
    layout: LayoutName,
    aggregator_input: AggregatorInput,
) -> Result<StarknetRunnerOutput, StarknetOsError> {
    let RunnerInitilizationData {
        proof_mode,
        disable_trace_padding,
        relocate_mem,
        allow_missing_builtins,
        mut cairo_runner,
        end,
    } = initialize_run(layout, &AGGREGATOR_PROGRAM)?;

    // Create the aggregator hint processor.
    let aggregator_hint_processor =
        AggregatorHintProcessor::new(&AGGREGATOR_PROGRAM, aggregator_input);

    run_runner(
        &mut cairo_runner,
        end,
        aggregator_hint_processor,
        proof_mode,
        disable_trace_padding,
        relocate_mem,
        allow_missing_builtins,
    )
}
