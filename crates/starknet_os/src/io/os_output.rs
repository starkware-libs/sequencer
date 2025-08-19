use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::errors::memory_errors::MemoryError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;
use cairo_vm::vm::runners::builtin_runner::BuiltinRunner;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use cairo_vm::vm::vm_core::VirtualMachine;
use num_traits::ToPrimitive;
use starknet_api::block::{BlockNumber, PreviousBlockNumber};
use starknet_api::core::{ContractAddress, EntryPointSelector, EthAddress, Nonce};
use starknet_api::hash::StarkHash;
use starknet_api::transaction::{L1ToL2Payload, L2ToL1Payload, MessageToL1};
use starknet_types_core::felt::Felt;

use crate::errors::StarknetOsError;
use crate::io::os_output_types::{
    FullCommitmentOsStateDiff,
    FullOsStateDiff,
    PartialCommitmentOsStateDiff,
    PartialOsStateDiff,
    TryFromOutputIter,
};
use crate::metrics::OsMetrics;

// from_address, to_address, payload_size.
pub(crate) const MESSAGE_TO_L1_CONST_FIELD_SIZE: usize = 3;
// from_address, to_address, nonce, selector, payload_size.
pub(crate) const MESSAGE_TO_L2_CONST_FIELD_SIZE: usize = 5;

#[derive(Debug, thiserror::Error)]
pub enum OsOutputError {
    #[error("Missing expected field: {0}.")]
    MissingFieldInOutput(String),
    #[error("Invalid output in field: {value_name}. Val: {val}. Error: {message}")]
    InvalidOsOutputField { value_name: String, val: Felt, message: String },
    #[error("Failed to convert to FullOsOutput. State diff variant is of a different type")]
    ConvertToFullOutput,
}

pub(crate) fn wrap_missing<T>(val: Option<T>, val_name: &str) -> Result<T, OsOutputError> {
    val.ok_or_else(|| OsOutputError::MissingFieldInOutput(val_name.to_string()))
}

pub(crate) fn try_into_custom_error<T: TryFrom<Felt>>(
    val: Felt,
    val_name: &str,
) -> Result<T, OsOutputError>
where
    <T as TryFrom<Felt>>::Error: std::fmt::Display,
{
    val.try_into().map_err(|e: <T as TryFrom<Felt>>::Error| OsOutputError::InvalidOsOutputField {
        value_name: val_name.to_string(),
        val,
        message: e.to_string(),
    })
}

pub(crate) fn wrap_missing_as<T: TryFrom<Felt>>(
    val: Option<Felt>,
    val_name: &str,
) -> Result<T, OsOutputError>
where
    <T as TryFrom<Felt>>::Error: std::fmt::Display,
{
    try_into_custom_error(wrap_missing(val, val_name)?, val_name)
}

pub(crate) fn felt_as_bool(felt_val: Felt, val_name: &str) -> Result<bool, OsOutputError> {
    if felt_val == Felt::ZERO || felt_val == Felt::ONE {
        return Ok(felt_val == Felt::ONE);
    }
    Err(OsOutputError::InvalidOsOutputField {
        value_name: val_name.to_string(),
        val: felt_val,
        message: "Expected a bool felt".to_string(),
    })
}

pub(crate) fn wrap_missing_as_bool(
    val: Option<Felt>,
    val_name: &str,
) -> Result<bool, OsOutputError> {
    let felt_val = wrap_missing(val, val_name)?;
    felt_as_bool(felt_val, val_name)
}

pub fn message_l1_from_output_iter<It: Iterator<Item = Felt>>(
    iter: &mut It,
) -> Result<MessageToL1, OsOutputError> {
    let from_address = wrap_missing_as(iter.next(), "MessageToL1::from_address")?;
    let to_address = wrap_missing_as(iter.next(), "MessageToL1::to_address")?;
    let payload_size = wrap_missing_as(iter.next(), "MessageToL1::payload_size")?;
    let payload = L2ToL1Payload(iter.take(payload_size).collect());

    Ok(MessageToL1 { from_address, to_address, payload })
}

// TODO(Tzahi): Replace with starknet_api struct after it is updated.
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug)]
// An L1 to L2 message header, the message payload is concatenated to the end of the header.
pub struct MessageToL2 {
    // The L1 address of the contract sending the message.
    pub(crate) from_address: EthAddress,
    // The L2 address of the contract receiving the message.
    pub(crate) to_address: ContractAddress,
    pub(crate) nonce: Nonce,
    pub(crate) selector: EntryPointSelector,
    pub(crate) payload: L1ToL2Payload,
}

impl TryFromOutputIter for MessageToL2 {
    fn try_from_output_iter<It: Iterator<Item = Felt>>(
        iter: &mut It,
    ) -> Result<Self, OsOutputError> {
        let from_address = wrap_missing_as(iter.next(), "MessageToL2::from_address")?;
        let to_address = wrap_missing_as(iter.next(), "MessageToL2::to_address")?;
        let nonce = Nonce(wrap_missing(iter.next(), "MessageToL2::nonce")?);
        let selector = EntryPointSelector(wrap_missing(iter.next(), "MessageToL2::selector")?);
        let payload_size = wrap_missing_as(iter.next(), "MessageToL2::payload_size")?;
        let payload = L1ToL2Payload(iter.take(payload_size).collect());

        Ok(Self { from_address, to_address, nonce, selector, payload })
    }
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, PartialEq)]
/// A representation of the state diff of an OS run, with 4 variants that depends on the use_kzg_da
/// and the full_output flags of the OS run.
pub enum OsStateDiff {
    // An explicit state diff (no KZG commitment applied) in the full output format (with previous
    // values for every storage/class hash change). The expected input format for the
    // aggregator.
    Full(FullOsStateDiff),
    // An explicit state diff (no KZG commitment applied) in the partial output format (no
    // previous values).
    Partial(PartialOsStateDiff),
    // A commitment to the state diff (with KZG commitment applied) in the full output format.
    FullCommitment(FullCommitmentOsStateDiff),
    // A commitment to the state diff (with KZG commitment applied) in the partial output format.
    PartialCommitment(PartialCommitmentOsStateDiff),
}

struct OutputIterParsedData {
    common_os_output: CommonOsOutput,
    kzg_commitment_info: Option<Vec<Felt>>,
    full_output: bool,
}

impl TryFromOutputIter for OutputIterParsedData {
    fn try_from_output_iter<It: Iterator<Item = Felt>>(
        output_iter: &mut It,
    ) -> Result<Self, OsOutputError> {
        let initial_root = wrap_missing(output_iter.next(), "initial_root")?;
        let final_root = wrap_missing(output_iter.next(), "final_root")?;
        let prev_block_number = wrap_missing_as(output_iter.next(), "prev_block_number")?;
        let new_block_number =
            BlockNumber(wrap_missing_as(output_iter.next(), "new_block_number")?);
        let prev_block_hash = wrap_missing(output_iter.next(), "prev_block_hash")?;
        let new_block_hash = wrap_missing(output_iter.next(), "new_block_hash")?;
        let os_program_hash = wrap_missing(output_iter.next(), "os_program_hash")?;
        let starknet_os_config_hash = wrap_missing(output_iter.next(), "starknet_os_config_hash")?;
        let use_kzg_da = wrap_missing_as_bool(output_iter.next(), "use_kzg_da")?;
        let full_output = wrap_missing_as_bool(output_iter.next(), "full_output")?;

        let kzg_commitment_info = if use_kzg_da {
            // Read KZG data into a vec.
            let kzg_z = wrap_missing(output_iter.next(), "kzg_z")?;
            let n_blobs: usize = wrap_missing_as(output_iter.next(), "n_blobs")?;
            let commitments = output_iter.take(2 * 2 * n_blobs);
            Some([kzg_z, n_blobs.into()].into_iter().chain(commitments).collect::<Vec<_>>())
        } else {
            None
        };

        // Messages to L1 and L2.
        let mut messages_to_l1_segment_size =
            wrap_missing_as(output_iter.next(), "messages_to_l1_segment_size")?;
        let mut messages_to_l1_iter = output_iter.take(messages_to_l1_segment_size).peekable();
        let mut messages_to_l1 = Vec::<MessageToL1>::new();

        while messages_to_l1_iter.peek().is_some() {
            let message = message_l1_from_output_iter(&mut messages_to_l1_iter)?;
            messages_to_l1_segment_size -= message.payload.0.len() + MESSAGE_TO_L1_CONST_FIELD_SIZE;
            messages_to_l1.push(message);
        }
        assert_eq!(
            messages_to_l1_segment_size, 0,
            "Expected messages to L1 segment to be consumed, but {} felts were left.",
            messages_to_l1_segment_size
        );

        let mut messages_to_l2_segment_size =
            wrap_missing_as(output_iter.next(), "messages_to_l2_segment_size")?;
        let mut messages_to_l2_iter = output_iter.take(messages_to_l2_segment_size).peekable();
        let mut messages_to_l2 = Vec::<MessageToL2>::new();

        while messages_to_l2_iter.peek().is_some() {
            let message = MessageToL2::try_from_output_iter(&mut messages_to_l2_iter)?;
            messages_to_l2_segment_size -= message.payload.0.len() + MESSAGE_TO_L2_CONST_FIELD_SIZE;
            messages_to_l2.push(message);
        }
        Ok(Self {
            common_os_output: CommonOsOutput {
                initial_root,
                final_root,
                prev_block_number,
                new_block_number,
                prev_block_hash,
                new_block_hash,
                os_program_hash,
                starknet_os_config_hash,
                messages_to_l1,
                messages_to_l2,
            },
            kzg_commitment_info,
            full_output,
        })
    }
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug)]
pub struct CommonOsOutput {
    // The root before.
    pub initial_root: StarkHash,
    // The root after.
    pub final_root: StarkHash,
    // The previous block number.
    pub prev_block_number: PreviousBlockNumber,
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
    // Messages from L2 to L1.
    pub messages_to_l1: Vec<MessageToL1>,
    // Messages from L1 to L2.
    pub messages_to_l2: Vec<MessageToL2>,
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug)]
/// A specific structured os output (with FullOsStateDiff).
/// The aggregator inputs are expected to be in this format.
pub(crate) struct FullOsOutput {
    pub common_os_output: CommonOsOutput,
    pub state_diff: FullOsStateDiff,
}

impl TryFrom<OsOutput> for FullOsOutput {
    type Error = OsOutputError;

    fn try_from(output: OsOutput) -> Result<Self, Self::Error> {
        Ok(Self {
            common_os_output: output.common_os_output,
            state_diff: match output.state_diff {
                OsStateDiff::Full(state_diff) => state_diff,
                _ => return Err(OsOutputError::ConvertToFullOutput),
            },
        })
    }
}

impl FullOsOutput {
    pub fn _use_kzg_da(&self) -> bool {
        false
    }

    pub fn _full_output(&self) -> bool {
        true
    }
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug)]
/// A general structure os output (with OsStateDiff).
pub struct OsOutput {
    pub common_os_output: CommonOsOutput,
    pub state_diff: OsStateDiff,
}

impl TryFromOutputIter for OsOutput {
    fn try_from_output_iter<It: Iterator<Item = Felt>>(
        output_iter: &mut It,
    ) -> Result<Self, OsOutputError> {
        let OutputIterParsedData { common_os_output, kzg_commitment_info, full_output } =
            OutputIterParsedData::try_from_output_iter(output_iter)?;

        let state_diff = match (kzg_commitment_info, full_output) {
            (Some(info), true) => OsStateDiff::FullCommitment(FullCommitmentOsStateDiff(info)),
            (Some(info), false) => {
                OsStateDiff::PartialCommitment(PartialCommitmentOsStateDiff(info))
            }
            (None, true) => OsStateDiff::Full(FullOsStateDiff::try_from_output_iter(output_iter)?),
            (None, false) => {
                OsStateDiff::Partial(PartialOsStateDiff::try_from_output_iter(output_iter)?)
            }
        };

        Ok(Self { common_os_output, state_diff })
    }
}

impl OsOutput {
    pub fn use_kzg_da(&self) -> bool {
        match self.state_diff {
            OsStateDiff::FullCommitment(_) | OsStateDiff::PartialCommitment(_) => true,
            OsStateDiff::Full(_) | OsStateDiff::Partial(_) => false,
        }
    }

    pub fn full_output(&self) -> bool {
        match self.state_diff {
            OsStateDiff::Full(_) | OsStateDiff::FullCommitment(_) => true,
            OsStateDiff::Partial(_) | OsStateDiff::PartialCommitment(_) => false,
        }
    }
}

#[derive(Debug)]
pub struct StarknetOsRunnerOutput {
    pub raw_os_output: Vec<Felt>,
    #[cfg(feature = "include_program_output")]
    pub os_output: OsOutput,
    pub cairo_pie: CairoPie,
    pub da_segment: Option<Vec<Felt>>,
    pub metrics: OsMetrics,
    #[cfg(any(test, feature = "testing"))]
    pub unused_hints: std::collections::HashSet<crate::hints::enum_definition::AllHints>,
}

pub struct StarknetAggregatorRunnerOutput {
    // TODO(Tzahi): Define a struct for the output.
    #[cfg(feature = "include_program_output")]
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
    let range_of_output = vm
        .get_continuous_range(output_address, output_size)
        .map_err(VirtualMachineError::Memory)?;
    range_of_output
        .into_iter()
        .map(|x| match x {
            MaybeRelocatable::Int(val) => Ok(val),
            MaybeRelocatable::RelocatableValue(val) => Err(StarknetOsError::VirtualMachineError(
                VirtualMachineError::ExpectedIntAtRange(Box::new(Some(val.into()))),
            )),
        })
        .collect()
}
