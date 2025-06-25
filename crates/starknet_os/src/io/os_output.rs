use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::errors::memory_errors::MemoryError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;
use cairo_vm::vm::runners::builtin_runner::BuiltinRunner;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use cairo_vm::vm::vm_core::VirtualMachine;
use num_traits::ToPrimitive;
use starknet_types_core::felt::Felt;

use crate::errors::StarknetOsError;
use crate::metrics::OsMetrics;

pub struct StarknetOsRunnerOutput {
    // TODO(Tzahi): Define a struct for the output.
    pub os_output: Vec<Felt>,
    pub cairo_pie: CairoPie,
    pub da_segment: Option<Vec<Felt>>,
    pub metrics: OsMetrics,
    #[cfg(any(test, feature = "testing"))]
    pub unused_hints: std::collections::HashSet<crate::hints::enum_definition::AllHints>,
}

pub struct StarknetAggregatorRunnerOutput {
    // TODO(Aner): Define a struct for the output.
    pub aggregator_output: Vec<Felt>,
    pub cairo_pie: CairoPie,
    #[cfg(any(test, feature = "testing"))]
    pub unused_hints: std::collections::HashSet<crate::hints::enum_definition::AllHints>,
}

// Retrieve the output ptr data of a finalized run as a vec of felts.
pub fn get_run_output(vm: &VirtualMachine) -> Result<Vec<Felt>, StarknetOsError> {
    let (output_base, output_size) = get_output_info(vm)?;
    get_raw_output(vm, output_base, output_size)
}

/// Gets the output base segment and the output size from the VM return values and the VM
/// output builtin.
fn get_output_info(vm: &VirtualMachine) -> Result<(usize, usize), StarknetOsError> {
    let output_base = vm
        .get_builtin_runners()
        .iter()
        .find(|&runner| matches!(runner, BuiltinRunner::Output(_)))
        .ok_or_else(|| StarknetOsError::VirtualMachineError(VirtualMachineError::NoOutputBuiltin))?
        .base();
    let n_builtins = vm.get_builtin_runners().len();
    let builtin_end_ptrs = vm
        .get_return_values(n_builtins)
        .map_err(|err| StarknetOsError::VirtualMachineError(err.into()))?;

    // Find the output_builtin returned offset.
    let output_size = builtin_end_ptrs
        .iter()
        .find_map(|ptr| {
            if let MaybeRelocatable::RelocatableValue(Relocatable { segment_index, offset }) = ptr {
                // Negative index is reserved for temporary memory segments.
                // See https://github.com/lambdaclass/cairo-vm/blob/ed3117098dd33c96056880af6fa67f9
                //      b2caebfb4/vm/src/vm/vm_memory/memory_segments.rs#L57.
                if segment_index.to_usize().expect("segment_index is unexpectedly negative")
                    == output_base
                {
                    Some(offset)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .ok_or_else(|| {
            StarknetOsError::VirtualMachineError(
                MemoryError::MissingMemoryCells(BuiltinName::output.into()).into(),
            )
        })?;

    Ok((output_base, *output_size))
}

/// Gets the OS output as an array of felts based on the output base and size.
fn get_raw_output(
    vm: &VirtualMachine,
    output_base: usize,
    output_size: usize,
) -> Result<Vec<Felt>, StarknetOsError> {
    // Get output and check that everything is an integer.
    let output_address = Relocatable::from((
        output_base.to_isize().expect("Output segment index unexpectedly exceeds isize::MAX"),
        0,
    ));
    let range_of_output = vm.get_range(output_address, output_size);
    range_of_output
        .iter()
        .map(|x| match x {
            Some(cow) => match **cow {
                MaybeRelocatable::Int(val) => Ok(val),
                MaybeRelocatable::RelocatableValue(val) => {
                    Err(StarknetOsError::VirtualMachineError(
                        VirtualMachineError::ExpectedIntAtRange(Box::new(Some(val.into()))),
                    ))
                }
            },
            None => Err(StarknetOsError::VirtualMachineError(
                MemoryError::MissingMemoryCells(BuiltinName::output.into()).into(),
            )),
        })
        .collect()
}
