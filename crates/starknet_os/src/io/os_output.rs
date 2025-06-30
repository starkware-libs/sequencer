use cairo_vm::hint_processor::hint_processor_utils::felt_to_usize;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::errors::memory_errors::MemoryError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;
use cairo_vm::vm::runners::builtin_runner::BuiltinRunner;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use cairo_vm::vm::vm_core::VirtualMachine;
use num_traits::ToPrimitive;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, EntryPointSelector, EthAddress, Nonce};
use starknet_api::hash::StarkHash;
use starknet_api::transaction::MessageToL1;
use starknet_types_core::felt::Felt;

use crate::errors::StarknetOsError;
use crate::metrics::OsMetrics;

#[derive(Debug, thiserror::Error)]
pub enum OsOutputError {
    #[error("Missing expected field: {0}.")]
    MissingFieldInOutput(String),
    #[error("Invalid output in field: {0}. Error: {1}")]
    InvalidOsOutputField(String, String),
}

fn wrap_missing(val: Option<Felt>, val_name: &str) -> Result<Felt, OsOutputError> {
    val.ok_or_else(|| OsOutputError::MissingFieldInOutput(val_name.to_string()))
}

fn wrap_to_usize(val: Felt, val_name: &str) -> Result<usize, OsOutputError> {
    felt_to_usize(&val)
        .map_err(|e| OsOutputError::InvalidOsOutputField(val_name.to_string(), e.to_string()))
}

fn wrap_to_usize_missing(val: Option<Felt>, val_name: &str) -> Result<usize, OsOutputError> {
    wrap_to_usize(wrap_missing(val, val_name)?, val_name)
}

fn wrap_to_bool_missing(val: Option<Felt>, val_name: &str) -> Result<bool, OsOutputError> {
    let felt_val = wrap_missing(val, val_name)?;
    if felt_val == Felt::ZERO || felt_val == Felt::ONE {
        return Ok(felt_val == Felt::ONE);
    }
    Err(OsOutputError::InvalidOsOutputField(
        val_name.to_string(),
        format!("Expected a bool felt, got {felt_val}"),
    ))
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
// An L1 to L2 message header, the message payload is concatenated to the end of the header.
pub struct MessageToL2 {
    // The L1 address of the contract sending the message.
    from_address: EthAddress,
    // The L2 address of the contract receiving the message.
    to_address: ContractAddress,
    nonce: Nonce,
    selector: EntryPointSelector,
    payload: Vec<Felt>,
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
// TODO(tzahi): Complete the struct.
pub struct OsStateDiff {}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
pub struct OsOutput {
    // The root before.
    pub initial_root: StarkHash,
    // The root after.
    pub final_root: StarkHash,
    // The previous block number.
    pub prev_block_number: BlockNumber,
    // The new block number.
    pub new_block_number: BlockNumber,
    // The previous block hash.
    pub prev_block_hash: StarkHash,
    // The new block hash.
    pub new_block_hash: StarkHash,
    // The hash of the OS program, if the aggregator was used. Zero if the OS was used directly.
    pub os_program_hash: StarkHash,
    // The hash of the OS config.
    pub starknet_os_config_hash: StarkHash,
    // Indicates whether KZG data availability was used.
    pub use_kzg_da: bool,
    // Indicates whether previous state values are included in the state update information.
    pub full_output: bool,
    // Messages from L2 to L1.
    pub messages_to_l1: Vec<MessageToL1>,
    // Messages from L1 to L2.
    pub messages_to_l2: Vec<MessageToL2>,
    // The state diff.
    pub state_diff: Option<OsStateDiff>,
}

impl OsOutput {
    pub fn from_raw_output_iter<It: Iterator<Item = Felt>>(
        mut output_iter: It,
    ) -> Result<Self, OsOutputError> {
        let initial_root = wrap_missing(output_iter.next(), "initial_root")?;
        let final_root = wrap_missing(output_iter.next(), "final_root")?;
        let prev_block_number = BlockNumber(
            wrap_missing(output_iter.next(), "prev_block_number")?.try_into().map_err(
                |e: <u64 as TryFrom<Felt>>::Error| {
                    OsOutputError::InvalidOsOutputField(
                        "prev_block_number".to_string(),
                        e.to_string(),
                    )
                },
            )?,
        );
        let new_block_number =
            BlockNumber(wrap_missing(output_iter.next(), "new_block_number")?.try_into().map_err(
                |e: <u64 as TryFrom<Felt>>::Error| {
                    OsOutputError::InvalidOsOutputField(
                        "new_block_number".to_string(),
                        e.to_string(),
                    )
                },
            )?);
        let prev_block_hash = wrap_missing(output_iter.next(), "prev_block_hash")?;
        let new_block_hash = wrap_missing(output_iter.next(), "new_block_hash")?;
        let os_program_hash = wrap_missing(output_iter.next(), "os_program_hash")?;
        let starknet_os_config_hash = wrap_missing(output_iter.next(), "starknet_os_config_hash")?;
        let use_kzg_da = wrap_to_bool_missing(output_iter.next(), "use_kzg_da")?;
        let full_output = wrap_to_bool_missing(output_iter.next(), "full_output")?;

        if use_kzg_da {
            // Skip KZG data.

            let _kzg_z = wrap_missing(output_iter.next(), "kzg_z")?;
            let n_blobs = wrap_to_usize_missing(output_iter.next(), "n_blobs")?;
            // Skip 'n_blobs' commitments and evaluations.
            output_iter.nth((2 * 2 * n_blobs) - 1);
        }

        // Messages to L1 and L2.
        let messages_to_l1_segment_size =
            wrap_to_usize_missing(output_iter.next(), "messages_to_l1_segment_size")?;
        let mut messages_to_l1_iter =
            output_iter.by_ref().take(messages_to_l1_segment_size).peekable();
        let messages_to_l1 = Vec::<MessageToL1>::new();

        while messages_to_l1_iter.peek().is_some() {
            todo!("Handle L1 messages");
        }

        let messages_to_l2_segment_size =
            wrap_to_usize_missing(output_iter.next(), "messages_to_l2_segment_size")?;
        let mut messages_to_l2_iter =
            output_iter.by_ref().take(messages_to_l2_segment_size).peekable();
        let messages_to_l2 = Vec::<MessageToL2>::new();

        while messages_to_l2_iter.peek().is_some() {
            todo!("Handle L2 messages");
        }

        // State diff.
        let state_diff = if use_kzg_da {
            None
        } else {
            todo!("Handle StateDiff");
        };

        Ok(Self {
            initial_root,
            final_root,
            prev_block_number,
            new_block_number,
            prev_block_hash,
            new_block_hash,
            os_program_hash,
            starknet_os_config_hash,
            use_kzg_da,
            full_output,
            messages_to_l1,
            messages_to_l2,
            state_diff,
        })
    }
}

pub struct StarknetOsRunnerOutput {
    // TODO(Tzahi): Use OsOutput struct once fully supported..
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
