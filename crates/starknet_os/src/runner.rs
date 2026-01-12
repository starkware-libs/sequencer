use apollo_starknet_os_program::{AGGREGATOR_PROGRAM, OS_PROGRAM, VIRTUAL_OS_PROGRAM};
use blockifier::execution::contract_class::TrackedResource;
use blockifier::state::state_api::StateReader;
use cairo_vm::cairo_run::CairoRunConfig;
use cairo_vm::hint_processor::hint_processor_definition::HintProcessor;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::program::Program;
use cairo_vm::vm::runners::cairo_pie::{
    BuiltinAdditionalData, CairoPie, OutputBuiltinAdditionalData,
};
use cairo_vm::vm::runners::cairo_runner::CairoRunner;
use starknet_types_core::felt::Felt;

use crate::errors::StarknetOsError;
use crate::hint_processor::aggregator_hint_processor::{AggregatorHintProcessor, AggregatorInput};
use crate::hint_processor::common_hint_processor::CommonHintProcessor;
use crate::hint_processor::panicking_state_reader::PanickingStateReader;
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::hint_implementation::output::OUTPUT_ATTRIBUTE_FACT_TOPOLOGY;
use crate::io::os_input::{OsBlockInput, OsHints, StarknetOsInput};
use crate::io::os_output::{StarknetAggregatorRunnerOutput, StarknetOsRunnerOutput};
use crate::io::virtual_os_output::VirtualOsRunnerOutput;
use crate::metrics::{AggregatorMetrics, OsMetrics};
use crate::vm_utils::vm_error_with_code_snippet;

pub const DEFAULT_OS_LAYOUT: LayoutName = LayoutName::all_cairo;

pub struct RunnerReturnObject {
    pub raw_output: Vec<Felt>,
    pub cairo_pie: CairoPie,
    pub cairo_runner: CairoRunner,
}

// TODO(Aner): replace the return type with Result<StarknetRunnerOutput,...>
// TODO(Aner): Make generic (CommonHintProcessor trait) depend on testing flag.
pub(crate) fn run_program<HP: HintProcessor + CommonHintProcessor>(
    layout: LayoutName,
    program: &Program,
    hint_processor: &mut HP,
) -> Result<RunnerReturnObject, StarknetOsError> {
    // Init CairoRunConfig.
    // TODO(Einat): Set trace_enabled to false once blake opcodes are counted in the VM.
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
        // TODO(Meshi): add a fill holes flag.
        false,
    )?;

    if cairo_run_config.proof_mode {
        cairo_runner.finalize_segments()?;
    }

    let raw_output = crate::io::os_output::get_run_output(&cairo_runner.vm)?;

    cairo_runner.vm.verify_auto_deductions().map_err(StarknetOsError::VirtualMachineError)?;
    cairo_runner
        .read_return_values(allow_missing_builtins)
        .map_err(StarknetOsError::RunnerError)?;

    // MEMORY OPTIMIZATION: Skip full relocation for CairoPie generation.
    // The relocate() call creates relocated_memory and relocated_trace which are NOT used by
    // get_cairo_pie(). CairoPie uses the original vm.segments.memory directly.
    // Only compute_effective_sizes() is needed for segment size metadata.
    // This saves ~2.7GB peak memory by avoiding trace duplication for large blocks.
    cairo_runner.vm.segments.compute_effective_sizes();

    // MEMORY OPTIMIZATION: Clear the trace before creating CairoPie.
    // The trace is only needed for proof generation (via relocate_trace), not for CairoPie.
    // For large blocks, the trace can be ~1.5GB. Clearing it before get_cairo_pie()
    // prevents peak memory from including both trace AND CairoPieMemory simultaneously.
    cairo_runner.vm.clear_trace();

    #[cfg(any(test, feature = "testing"))]
    crate::test_utils::validations::validate_builtins(&mut cairo_runner);

    // Parse the Cairo VM output.
    let cairo_pie = cairo_runner.get_cairo_pie().map_err(StarknetOsError::RunnerError)?;

    // MEMORY OPTIMIZATION: Clear VM memory after CairoPie creation.
    // The CairoPie now has its own copy of the memory, so we can release the original.
    // This prevents holding ~1.2GB of duplicate memory during metrics collection.
    cairo_runner.vm.segments.memory.clear_data();
    Ok(RunnerReturnObject { raw_output, cairo_pie, cairo_runner })
}

pub fn run_os<S: StateReader>(
    layout: LayoutName,
    OsHints {
        os_hints_config,
        os_input: StarknetOsInput { os_block_inputs, deprecated_compiled_classes, compiled_classes },
    }: OsHints,
    state_readers: Vec<S>,
) -> Result<StarknetOsRunnerOutput, StarknetOsError> {
    // Create the hint processor.
    let mut snos_hint_processor = SnosHintProcessor::new(
        &OS_PROGRAM,
        os_hints_config,
        os_block_inputs.iter().collect(),
        deprecated_compiled_classes,
        compiled_classes,
        state_readers,
    )?;

    // Run the OS program.
    let runner_output = run_program(layout, &OS_PROGRAM, &mut snos_hint_processor)?;
    generate_os_output(runner_output, snos_hint_processor)
}

fn generate_os_output(
    mut runner_output: RunnerReturnObject,
    mut snos_hint_processor: SnosHintProcessor<'_, impl StateReader>,
) -> Result<StarknetOsRunnerOutput, StarknetOsError> {
    let BuiltinAdditionalData::Output(OutputBuiltinAdditionalData {
        attributes: output_attributes,
        ..
    }) = runner_output
        .cairo_pie
        .additional_data
        .0
        .get(&BuiltinName::output)
        .expect("Output builtin should be present in the CairoPie.")
    else {
        panic!("Output builtin additional data should be of type OutputBuiltinAdditionalData.")
    };

    let is_onchain_kzg_da = !snos_hint_processor.os_hints_config.full_output
        && snos_hint_processor.os_hints_config.use_kzg_da;
    if is_onchain_kzg_da {
        assert!(output_attributes.is_empty(), "No attributes should be added in KZG mode.");
    } else {
        assert!(
            output_attributes.contains_key(OUTPUT_ATTRIBUTE_FACT_TOPOLOGY),
            "{OUTPUT_ATTRIBUTE_FACT_TOPOLOGY:?} is missing.",
        );
    }

    Ok(StarknetOsRunnerOutput {
        raw_os_output: runner_output.raw_output,
        cairo_pie: runner_output.cairo_pie,
        da_segment: snos_hint_processor.get_da_segment().take(),
        metrics: OsMetrics::new(&mut runner_output.cairo_runner, &snos_hint_processor)?,
        #[cfg(any(test, feature = "testing"))]
        txs_trace: snos_hint_processor
            .get_current_execution_helper()
            .unwrap()
            .os_logger
            .get_txs()
            .clone(),
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
pub fn run_aggregator(
    layout: LayoutName,
    aggregator_input: AggregatorInput,
) -> Result<StarknetAggregatorRunnerOutput, StarknetOsError> {
    // Create the aggregator hint processor.
    let mut aggregator_hint_processor =
        AggregatorHintProcessor::new(&AGGREGATOR_PROGRAM, aggregator_input);

    let mut runner_output =
        run_program(layout, &AGGREGATOR_PROGRAM, &mut aggregator_hint_processor)?;

    Ok(StarknetAggregatorRunnerOutput {
        aggregator_output: runner_output.raw_output,
        cairo_pie: runner_output.cairo_pie,
        metrics: AggregatorMetrics::new(&mut runner_output.cairo_runner)?,
        #[cfg(any(test, feature = "testing"))]
        unused_hints: aggregator_hint_processor.get_unused_hints(),
    })
}

/// Validates that all tracked resources in all execution infos are SierraGas.
/// Called before execution to provide informative error message.
fn validate_tracked_resources(os_block_inputs: &[OsBlockInput]) -> Result<(), StarknetOsError> {
    for block_input in os_block_inputs.iter() {
        for (tx, tx_execution_info) in
            block_input.transactions.iter().zip(&block_input.tx_execution_infos)
        {
            for call_info in tx_execution_info.call_info_iter(tx.tx_type()) {
                if call_info.tracked_resource != TrackedResource::SierraGas {
                    return Err(StarknetOsError::InvalidTrackedResource {
                        expected: TrackedResource::SierraGas,
                        actual: call_info.tracked_resource,
                    });
                }
            }
        }
    }
    Ok(())
}

/// Runs the virtual OS.
pub fn run_virtual_os(
    OsHints {
        os_hints_config,
        os_input: StarknetOsInput { os_block_inputs, deprecated_compiled_classes, compiled_classes },
    }: OsHints,
) -> Result<VirtualOsRunnerOutput, StarknetOsError> {
    // Validate that all tracked resources are SierraGas.
    validate_tracked_resources(&os_block_inputs)?;

    // Create the hint processor - reuse the SNOS hint processor with the virtual OS program.
    let mut snos_hint_processor = SnosHintProcessor::new(
        &VIRTUAL_OS_PROGRAM,
        os_hints_config,
        os_block_inputs.iter().collect(),
        deprecated_compiled_classes,
        compiled_classes,
        vec![PanickingStateReader; os_block_inputs.len()],
    )?;

    // Run the virtual OS program.
    let runner_output =
        run_program(DEFAULT_OS_LAYOUT, &VIRTUAL_OS_PROGRAM, &mut snos_hint_processor)?;

    Ok(VirtualOsRunnerOutput {
        raw_output: runner_output.raw_output,
        cairo_pie: runner_output.cairo_pie,
    })
}
