use std::collections::HashMap;

use blockifier::state::cached_state::StateMaps;
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
use starknet_api::transaction::{L1ToL2Payload, L2ToL1Payload, MessageToL1};
use starknet_types_core::felt::{Felt, NonZeroFelt};

use crate::errors::StarknetOsError;
use crate::hints::hint_implementation::stateless_compression::utils::decompress;
use crate::io::os_output_types::{
    FullCommitmentOsStateDiff,
    FullContractStorageUpdate,
    FullOsStateDiff,
    PartialCommitmentOsStateDiff,
    PartialOsStateDiff,
};
use crate::metrics::OsMetrics;

#[cfg(test)]
#[path = "os_output_test.rs"]
mod os_output_test;

// Cairo DictAccess types for concrete objects.
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
    #[error("Invalid output in field: {value_name}. Val: {val}. Error: {message}")]
    InvalidOsOutputField { value_name: String, val: Felt, message: String },
    #[error("Failed to convert to FullOsOutput. State diff variant is of a different type")]
    ConvertToFullOutput,
}

pub(crate) fn wrap_missing<T>(val: Option<T>, val_name: &str) -> Result<T, OsOutputError> {
    val.ok_or_else(|| OsOutputError::MissingFieldInOutput(val_name.to_string()))
}

fn try_into_custom_error<T: TryFrom<Felt>>(val: Felt, val_name: &str) -> Result<T, OsOutputError>
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
    from_address: EthAddress,
    // The L2 address of the contract receiving the message.
    to_address: ContractAddress,
    nonce: Nonce,
    selector: EntryPointSelector,
    payload: L1ToL2Payload,
}

impl MessageToL2 {
    pub fn from_output_iter<It: Iterator<Item = Felt>>(
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
/// Represents the changes in a contract instance.
pub struct ContractChanges {
    // The address of the contract.
    addr: ContractAddress,
    // The previous nonce of the contract (for account contracts, if full output).
    prev_nonce: Option<Nonce>,
    // The new nonce of the contract (for account contracts, if changed or full output).
    new_nonce: Option<Nonce>,
    // The previous class hash (if full output).
    prev_class_hash: Option<ClassHash>,
    // The new class hash (if changed or full output).
    new_class_hash: Option<ClassHash>,
    // A map from storage key to its prev value (optional) and new value.
    storage_changes: Vec<FullContractStorageUpdate>,
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
                storage_changes: {
                    let n_changes = wrap_missing_as(iter.next(), "n_changes")?;
                    let mut changes = Vec::with_capacity(n_changes);
                    for _ in 0..n_changes {
                        changes.push(FullContractStorageUpdate::from_output_iter(iter)?);
                    }
                    changes
                },
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
        let (nonce, _n_changes) = nonce_n_changes.div_rem(&n_updates_bound);

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
            // Should be similar to the full_output code,only with partial updates.
            storage_changes: vec![],
        })
    }
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, PartialEq)]
pub enum OsStateDiff {
    // State diff of an OS run with use_kzg_da=false and full_output=true
    // (expected input of the aggregator).
    Full(FullOsStateDiff),
    // State diff of an OS run with full_output=false.
    Partial(PartialOsStateDiff),
    FullCommitment(FullCommitmentOsStateDiff),
    PartialCommitment(PartialCommitmentOsStateDiff),
}

// TODO(Tzahi): Remove after all derived methods for OsStateDiff are implemented.
// Not in use - kept here as a reference for PR reviews.
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, PartialEq)]
pub struct DeprecatedOsStateDiff {
    // Contracts that were changed.
    pub contracts: Vec<ContractChanges>,
    // Classes that were declared. Represents the updates of a mapping from class hash to previous
    // (optional) and new compiled class hash.
    pub classes: Vec<CompiledClassHashUpdate>,
}

impl DeprecatedOsStateDiff {
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

    /// Returns the state diff as a [StateMaps] object.
    pub fn as_state_maps(&self) -> StateMaps {
        let class_hashes = self
            .contracts
            .iter()
            .filter_map(|contract| {
                contract.new_class_hash.map(|class_hash| (contract.addr, class_hash))
            })
            .collect();
        let nonces = self
            .contracts
            .iter()
            .filter_map(|contract| contract.new_nonce.map(|nonce| (contract.addr, nonce)))
            .collect();
        let mut storage = HashMap::new();
        for contract in &self.contracts {
            for FullContractStorageUpdate { key, new_value, .. } in &contract.storage_changes {
                storage.insert((contract.addr, *key), *new_value);
            }
        }
        let compiled_class_hashes = self
            .classes
            .iter()
            .map(|(class_hash, (_prev_compiled_class_hash, new_compiled_class_hash))| {
                (*class_hash, *new_compiled_class_hash)
            })
            .collect();
        let declared_contracts = HashMap::new();
        StateMaps { nonces, class_hashes, storage, compiled_class_hashes, declared_contracts }
    }
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug)]
pub struct DifflessOsOutput {
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
    // Messages from L2 to L1.
    pub messages_to_l1: Vec<MessageToL1>,
    // Messages from L1 to L2.
    pub messages_to_l2: Vec<MessageToL2>,
}

impl DifflessOsOutput {
    pub fn from_output_iter<It: Iterator<Item = Felt>>(
        mut output_iter: It,
    ) -> Result<(Self, Option<Vec<Felt>>, bool), OsOutputError> {
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

        let kzg_commitment_info = if use_kzg_da {
            // Read KZG data into a vec.
            let kzg_z = wrap_missing(output_iter.next(), "kzg_z")?;
            let n_blobs: usize = wrap_missing_as(output_iter.next(), "n_blobs")?;
            let commitments = output_iter.by_ref().take(2 * 2 * n_blobs);
            Some([kzg_z, n_blobs.into()].into_iter().chain(commitments).collect::<Vec<_>>())
        } else {
            None
        };

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
        Ok((
            Self {
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
        ))
    }
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug)]
/// A specific structured os output (with FullOsStateDiff).
/// The aggregator inputs are expected to be in this format.
pub struct FullOsOutput {
    pub diff_less_os_output: DifflessOsOutput,
    pub state_diff: FullOsStateDiff,
}

impl TryFrom<OsOutput> for FullOsOutput {
    type Error = OsOutputError;

    fn try_from(output: OsOutput) -> Result<Self, Self::Error> {
        Ok(Self {
            diff_less_os_output: output.diffless_os_output,
            state_diff: match output.state_diff {
                OsStateDiff::Full(state_diff) => state_diff,
                _ => return Err(OsOutputError::ConvertToFullOutput),
            },
        })
    }
}
impl FullOsOutput {
    pub fn use_kzg_da(&self) -> bool {
        false
    }

    pub fn full_output(&self) -> bool {
        true
    }
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug)]
/// A general structure os output (with OsStateDiff).
pub struct OsOutput {
    pub diffless_os_output: DifflessOsOutput,
    pub state_diff: OsStateDiff,
}

impl OsOutput {
    pub fn from_output_iter<It: Iterator<Item = Felt>>(
        mut output_iter: It,
    ) -> Result<Self, OsOutputError> {
        let (diff_less_os_output, kzg_commitment_info, full_output) =
            DifflessOsOutput::from_output_iter(&mut output_iter)?;

        let state_diff = match (kzg_commitment_info, full_output) {
            (Some(info), true) => OsStateDiff::FullCommitment(FullCommitmentOsStateDiff(info)),
            (Some(info), false) => {
                OsStateDiff::PartialCommitment(PartialCommitmentOsStateDiff(info))
            }
            (None, true) => OsStateDiff::Full(FullOsStateDiff::from_output_iter(&mut output_iter)?),
            (None, false) => {
                OsStateDiff::Partial(PartialOsStateDiff::from_output_iter(&mut output_iter)?)
            }
        };

        Ok(Self { diffless_os_output: diff_less_os_output, state_diff })
    }

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
