use apollo_starknet_os_program::{AGGREGATOR_PROGRAM, OS_PROGRAM};
use blockifier::state::state_api::StateReader;
use cairo_vm::cairo_run::CairoRunConfig;
use cairo_vm::hint_processor::hint_processor_definition::HintProcessor;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::program::Program;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use cairo_vm::vm::runners::cairo_runner::CairoRunner;
#[cfg(feature = "include_program_output")]
use starknet_types_core::felt::Felt;

use crate::errors::StarknetOsError;
use crate::hint_processor::aggregator_hint_processor::{AggregatorHintProcessor, AggregatorInput};
use crate::hint_processor::common_hint_processor::CommonHintProcessor;
use crate::hint_processor::panicking_state_reader::PanickingStateReader;
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::io::os_input::{OsHints, StarknetOsInput};
use crate::io::os_output::{StarknetAggregatorRunnerOutput, StarknetOsRunnerOutput};
use crate::metrics::OsMetrics;
use crate::vm_utils::vm_error_with_code_snippet;

pub struct RunnerReturnObject {
    #[cfg(feature = "include_program_output")]
    pub raw_output: Vec<Felt>,
    pub cairo_pie: CairoPie,
    pub cairo_runner: CairoRunner,
}

// TODO(Aner): replace the return type with Result<StarknetRunnerOutput,...>
// TODO(Aner): Make generic (CommonHintProcessor trait) depend on testing flag.
fn run_program<'a, HP: HintProcessor + CommonHintProcessor<'a>>(
    layout: LayoutName,
    program: &Program,
    hint_processor: &mut HP,
) -> Result<RunnerReturnObject, StarknetOsError> {
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

    // Run the Cairo VM.
    cairo_runner
        .run_until_pc(end, hint_processor)
        .map_err(|err| Box::new(vm_error_with_code_snippet(&cairo_runner, err)))?;

    // End the Cairo VM run.
    let disable_finalize_all = false;
    cairo_runner.end_run(
        cairo_run_config.disable_trace_padding,
        disable_finalize_all,
        hint_processor,
    )?;

    if cairo_run_config.proof_mode {
        cairo_runner.finalize_segments()?;
    }

    #[cfg(feature = "include_program_output")]
    let raw_output = crate::io::os_output::get_run_output(&cairo_runner.vm)?;

    cairo_runner.vm.verify_auto_deductions().map_err(StarknetOsError::VirtualMachineError)?;
    cairo_runner
        .read_return_values(allow_missing_builtins)
        .map_err(StarknetOsError::RunnerError)?;
    cairo_runner
        .relocate(cairo_run_config.relocate_mem)
        .map_err(|e| StarknetOsError::VirtualMachineError(e.into()))?;

    // Parse the Cairo VM output.
    let cairo_pie = cairo_runner.get_cairo_pie().map_err(StarknetOsError::RunnerError)?;
    Ok(RunnerReturnObject {
        #[cfg(feature = "include_program_output")]
        raw_output,
        cairo_pie,
        cairo_runner,
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
) -> Result<StarknetOsRunnerOutput, StarknetOsError> {
    // Create the hint processor.
    let mut snos_hint_processor = SnosHintProcessor::new(
        &OS_PROGRAM,
        os_hints_config,
        os_block_inputs.iter().collect(),
        cached_state_inputs,
        deprecated_compiled_classes,
        compiled_classes,
        state_readers,
    )?;

    let mut runner_output = run_program(layout, &OS_PROGRAM, &mut snos_hint_processor)?;

    Ok(StarknetOsRunnerOutput {
        #[cfg(feature = "include_program_output")]
        os_output: {
            use crate::io::os_output_types::TryFromOutputIter;
            // Prepare and check expected output.
            let os_raw_output = runner_output.raw_output;
            let os_output = crate::io::os_output::OsOutput::try_from_output_iter(
                &mut os_raw_output.into_iter(),
            )?;
            log::debug!(
                "OsOutput for block number={}: {os_output:?}",
                os_output.common_os_output.new_block_number
            );
            os_output
        },
        cairo_pie: runner_output.cairo_pie,
        da_segment: snos_hint_processor.get_da_segment().take(),
        metrics: OsMetrics::new(&mut runner_output.cairo_runner, &snos_hint_processor)?,
        #[cfg(any(test, feature = "testing"))]
        unused_hints: snos_hint_processor.get_unused_hints(),
    })
}

/// Run the OS with a "stateless" state reader - panics if the state is accessed for data that was
/// not pre-loaded as part of the input.
pub fn run_os_stateless(
    layout: LayoutName,
    os_hints: OsHints,
) -> Result<StarknetOsRunnerOutput, StarknetOsError> {
    let n_blocks = os_hints.os_input.os_block_inputs.len();
    run_os(layout, os_hints, vec![PanickingStateReader; n_blocks])
}

/// Run the Aggregator.
#[allow(clippy::result_large_err)]
pub fn run_aggregator(
    layout: LayoutName,
    aggregator_input: AggregatorInput,
) -> Result<StarknetAggregatorRunnerOutput, StarknetOsError> {
    // Create the aggregator hint processor.
    let mut aggregator_hint_processor =
        AggregatorHintProcessor::new(&AGGREGATOR_PROGRAM, aggregator_input);

    let runner_output = run_program(layout, &AGGREGATOR_PROGRAM, &mut aggregator_hint_processor)?;

    Ok(StarknetAggregatorRunnerOutput {
        #[cfg(feature = "include_program_output")]
        aggregator_output: runner_output.raw_output,
        cairo_pie: runner_output.cairo_pie,
        #[cfg(any(test, feature = "testing"))]
        unused_hints: aggregator_hint_processor.get_unused_hints(),
    })
}
