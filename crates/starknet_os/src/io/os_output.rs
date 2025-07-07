use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::errors::memory_errors::MemoryError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;
use cairo_vm::vm::runners::builtin_runner::BuiltinRunner;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use cairo_vm::vm::vm_core::VirtualMachine;
use num_traits::ToPrimitive;
use starknet_api::block::BlockNumber;
use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    EntryPointSelector,
    EthAddress,
    Nonce,
};
use starknet_api::hash::StarkHash;
use starknet_api::state::StorageKey;
use starknet_api::transaction::{L1ToL2Payload, L2ToL1Payload, MessageToL1};
use starknet_types_core::felt::{Felt, NonZeroFelt};

use crate::errors::StarknetOsError;
use crate::hints::hint_implementation::stateless_compression::utils::decompress;
use crate::metrics::OsMetrics;

#[cfg(test)]
#[path = "os_output_test.rs"]
mod os_output_test;

// Cairo DictAccess types for concrete objects.
type ContractStorageUpdate = (StorageKey, (Option<Felt>, Felt));
type CompiledClassHashUpdate = (ClassHash, (Option<CompiledClassHash>, CompiledClassHash));

// Defined in output.cairo
const N_UPDATES_BOUND: NonZeroFelt =
    NonZeroFelt::from_felt_unchecked(Felt::from_hex_unchecked("10000000000000000")); // 2^64.
const N_UPDATES_SMALL_PACKING_BOUND: NonZeroFelt =
    NonZeroFelt::from_felt_unchecked(Felt::from_hex_unchecked("100")); // 2^8.
const FLAG_BOUND: NonZeroFelt = NonZeroFelt::TWO;

const MESSAGE_TO_L1_CONST_FIELD_SIZE: usize = 3; // from_address, to_address, payload_size.
// from_address, to_address, nonce, selector, payload_size.
const MESSAGE_TO_L2_CONST_FIELD_SIZE: usize = 5;
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

fn try_into_custom_error<T: TryFrom<Felt>>(val: Felt, val_name: &str) -> Result<T, OsOutputError>
where
    <T as TryFrom<Felt>>::Error: std::fmt::Display,
{
    val.try_into().map_err(|e: <T as TryFrom<Felt>>::Error| {
        OsOutputError::InvalidOsOutputField(val_name.to_string(), e.to_string())
    })
}

fn wrap_missing_as<T: TryFrom<Felt>>(val: Option<Felt>, val_name: &str) -> Result<T, OsOutputError>
where
    <T as TryFrom<Felt>>::Error: std::fmt::Display,
{
    try_into_custom_error(wrap_missing(val, val_name)?, val_name)
}

fn felt_as_bool(felt_val: Felt, val_name: &str) -> Result<bool, OsOutputError> {
    if felt_val == Felt::ZERO || felt_val == Felt::ONE {
        return Ok(felt_val == Felt::ONE);
    }
    Err(OsOutputError::InvalidOsOutputField(
        val_name.to_string(),
        format!("Expected a bool felt, got {felt_val}"),
    ))
}
fn wrap_missing_as_bool(val: Option<Felt>, val_name: &str) -> Result<bool, OsOutputError> {
    let felt_val = wrap_missing(val, val_name)?;
    if felt_val == Felt::ZERO || felt_val == Felt::ONE {
        return Ok(felt_val == Felt::ONE);
    }
    Err(OsOutputError::InvalidOsOutputField(
        val_name.to_string(),
        format!("Expected a bool felt, got {felt_val}"),
    ))
}

pub fn message_l1_from_output_iter<It: Iterator<Item = Felt>>(
    iter: &mut It,
) -> Result<MessageToL1, OsOutputError> {
    let from_address = wrap_missing_as(iter.next(), "from_address")?;
    let to_address = wrap_missing_as(iter.next(), "to_address")?;
    let payload_size = wrap_missing_as(iter.next(), "payload_size")?;
    let payload = L2ToL1Payload(iter.take(payload_size).collect());

    Ok(MessageToL1 { from_address, to_address, payload })
}

// TODO(Tzahi): Replace with starknet_api struct after it is updated.
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug)]
// An L1 to L2 message header, the message payload is concatenated to the end of the header.
pub struct MessageToL2 {
    // The L1 address of the contract sending the message.
    pub from_address: EthAddress,
    // The L2 address of the contract receiving the message.
    pub to_address: ContractAddress,
    pub nonce: Nonce,
    pub selector: EntryPointSelector,
    pub payload: L1ToL2Payload,
}

impl MessageToL2 {
    pub fn from_output_iter<It: Iterator<Item = Felt>>(
        iter: &mut It,
    ) -> Result<Self, OsOutputError> {
        let from_address = wrap_missing_as(iter.next(), "from_address")?;
        let to_address = wrap_missing_as(iter.next(), "to_address")?;
        let nonce = Nonce(wrap_missing(iter.next(), "nonce")?);
        let selector = EntryPointSelector(wrap_missing(iter.next(), "selector")?);
        let payload_size = wrap_missing_as(iter.next(), "payload_size")?;
        let payload = L1ToL2Payload(iter.take(payload_size).collect());

        Ok(Self { from_address, to_address, nonce, selector, payload })
    }
}

fn parse_storage_changes<It: Iterator<Item = Felt> + ?Sized>(
    n_changes: usize,
    iter: &mut It,
    full_output: bool,
) -> Result<Vec<ContractStorageUpdate>, OsOutputError> {
    (0..n_changes)
        .map(|_| {
            let key = wrap_missing_as(iter.next(), "storage key")?;
            let prev_value = if full_output {
                Some(wrap_missing(iter.next(), "previous storage value")?)
            } else {
                None
            };
            let new_value = wrap_missing(iter.next(), "storage value")?;
            // Wrapped in Ok to be able to use ? operator in the closure.
            Ok((key, (prev_value, new_value)))
        })
        .collect()
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug)]
/// Represents the changes in a contract instance.
pub struct ContractChanges {
    // The address of the contract.
    pub addr: ContractAddress,
    // The previous nonce of the contract (for account contracts, if full output).
    pub prev_nonce: Option<Nonce>,
    // The new nonce of the contract (for account contracts, if changed or full output).
    pub new_nonce: Option<Nonce>,
    // The previous class hash (if full output).
    pub prev_class_hash: Option<ClassHash>,
    // The new class hash (if changed or full output).
    pub new_class_hash: Option<ClassHash>,
    // A map from storage key to its prev value (optional) and new value.
    pub storage_changes: Vec<ContractStorageUpdate>,
}

impl ContractChanges {
    pub fn from_iter<It: Iterator<Item = Felt> + ?Sized>(
        iter: &mut It,
        full_output: bool,
    ) -> Result<Self, OsOutputError> {
        let addr = wrap_missing_as(iter.next(), "addr")?;
        if full_output {
            return Ok(Self {
                addr,
                prev_nonce: Some(Nonce(wrap_missing(iter.next(), "prev_nonce")?)),
                new_nonce: Some(Nonce(wrap_missing_as(iter.next(), "new_nonce")?)),
                prev_class_hash: Some(ClassHash(wrap_missing_as(iter.next(), "prev_class_hash")?)),
                new_class_hash: Some(ClassHash(wrap_missing_as(iter.next(), "new_class_hash")?)),
                storage_changes: parse_storage_changes(
                    wrap_missing_as(iter.next(), "storage_changes")?,
                    iter,
                    full_output,
                )?,
            });
        }
        // Parse packed info.
        let nonce_n_changes_two_flags = wrap_missing(iter.next(), "nonce_n_changes_two_flags")?;

        // Parse flags.
        let (nonce_n_changes_one_flag, class_updated_felt) =
            nonce_n_changes_two_flags.div_rem(&FLAG_BOUND);
        let class_updated = felt_as_bool(class_updated_felt, "class_updated")?;
        let (nonce_n_changes, is_n_updates_small_felt) =
            nonce_n_changes_one_flag.div_rem(&FLAG_BOUND);
        let is_n_updates_small = felt_as_bool(is_n_updates_small_felt, "is_n_updates_small")?;

        // Parse n_changes.
        let n_updates_bound =
            if is_n_updates_small { N_UPDATES_SMALL_PACKING_BOUND } else { N_UPDATES_BOUND };
        let (nonce, n_changes) = nonce_n_changes.div_rem(&n_updates_bound);

        // Parse nonce.
        let new_nonce = if nonce == Felt::ZERO { None } else { Some(Nonce(nonce)) };

        let new_class_hash = if class_updated {
            Some(ClassHash(wrap_missing(iter.next(), "new_class_hash")?))
        } else {
            None
        };
        Ok(Self {
            addr,
            prev_nonce: None,
            new_nonce,
            prev_class_hash: None,
            new_class_hash,
            storage_changes: parse_storage_changes(
                try_into_custom_error(n_changes, "n_changes")?,
                iter,
                full_output,
            )?,
        })
    }
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug)]
pub struct OsStateDiff {
    // Contracts that were changed.
    pub contracts: Vec<ContractChanges>,
    // Classes that were declared. Represents the updates of a mapping from class hash to previous
    // (optional) and new compiled class hash.
    pub classes: Vec<CompiledClassHashUpdate>,
}

impl OsStateDiff {
    pub fn from_iter<It: Iterator<Item = Felt>>(
        output_iter: &mut It,
        full_output: bool,
    ) -> Result<Self, OsOutputError> {
        let state_diff;
        let iter: &mut dyn Iterator<Item = Felt> = if !full_output {
            state_diff = decompress(output_iter);
            &mut state_diff.into_iter().chain(output_iter)
        } else {
            output_iter
        };
        // Contracts changes.
        let n_contracts = wrap_missing_as(iter.next(), "OsStateDiff.n_contracts")?;
        let mut contracts = Vec::with_capacity(n_contracts);
        for _ in 0..n_contracts {
            contracts.push(ContractChanges::from_iter(iter, full_output)?);
        }

        // Classes changes.
        let n_classes = wrap_missing_as(iter.next(), "OsStateDiff.n_classes")?;
        let mut classes = Vec::with_capacity(n_classes);
        for _ in 0..n_classes {
            let class_hash = ClassHash(wrap_missing(iter.next(), "class_hash")?);
            let prev_compiled_class_hash = if full_output {
                Some(CompiledClassHash(wrap_missing(iter.next(), "prev_compiled_class_hash")?))
            } else {
                None
            };
            let new_compiled_class_hash =
                CompiledClassHash(wrap_missing(iter.next(), "new_compiled_class_hash")?);
            classes.push((class_hash, (prev_compiled_class_hash, new_compiled_class_hash)));
        }
        Ok(Self { contracts, classes })
    }
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug)]
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
        let prev_block_number =
            BlockNumber(wrap_missing_as(output_iter.next(), "prev_block_number")?);
        let new_block_number =
            BlockNumber(wrap_missing_as(output_iter.next(), "new_block_number")?);
        let prev_block_hash = wrap_missing(output_iter.next(), "prev_block_hash")?;
        let new_block_hash = wrap_missing(output_iter.next(), "new_block_hash")?;
        let os_program_hash = wrap_missing(output_iter.next(), "os_program_hash")?;
        let starknet_os_config_hash = wrap_missing(output_iter.next(), "starknet_os_config_hash")?;
        let use_kzg_da = wrap_missing_as_bool(output_iter.next(), "use_kzg_da")?;
        let full_output = wrap_missing_as_bool(output_iter.next(), "full_output")?;

        if use_kzg_da {
            // Skip KZG data.

            let _kzg_z = wrap_missing(output_iter.next(), "kzg_z")?;
            let n_blobs: usize = wrap_missing_as(output_iter.next(), "n_blobs")?;
            // Skip 'n_blobs' commitments and evaluations.
            output_iter.nth((2 * 2 * n_blobs) - 1);
        }

        // Messages to L1 and L2.
        let mut messages_to_l1_segment_size =
            wrap_missing_as(output_iter.next(), "messages_to_l1_segment_size")?;
        let mut messages_to_l1_iter =
            output_iter.by_ref().take(messages_to_l1_segment_size).peekable();
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
        let mut messages_to_l2_iter =
            output_iter.by_ref().take(messages_to_l2_segment_size).peekable();
        let mut messages_to_l2 = Vec::<MessageToL2>::new();

        while messages_to_l2_iter.peek().is_some() {
            let message = MessageToL2::from_output_iter(&mut messages_to_l2_iter)?;
            messages_to_l2_segment_size -= message.payload.0.len() + MESSAGE_TO_L2_CONST_FIELD_SIZE;
            messages_to_l2.push(message);
        }

        // State diff.
        let state_diff = if use_kzg_da {
            None
        } else {
            Some(OsStateDiff::from_iter(&mut output_iter, full_output)?)
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
    #[cfg(feature = "include_program_output")]
    pub os_output: OsOutput,
    pub cairo_pie: CairoPie,
    pub da_segment: Option<Vec<Felt>>,
    pub metrics: OsMetrics,
    #[cfg(any(test, feature = "testing"))]
    pub unused_hints: std::collections::HashSet<crate::hints::enum_definition::AllHints>,
}

pub struct StarknetAggregatorRunnerOutput {
    // TODO(Aner): Define a struct for the output.
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
