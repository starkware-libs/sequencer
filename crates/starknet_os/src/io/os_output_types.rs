use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::io::os_output::OsOutputError;

// Cairo DictAccess types for concrete objects.

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, PartialEq)]
/// Represents a full contract storage update.
pub(crate) struct FullContractStorageUpdate {
    pub(crate) key: StorageKey,
    pub(crate) prev_value: Felt,
    pub(crate) new_value: Felt,
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, PartialEq)]
pub(crate) struct PartialContractStorageUpdate {
    pub(crate) key: StorageKey,
    pub(crate) new_value: Felt,
}

impl FullContractStorageUpdate {
    pub fn _from_output_iter<It: Iterator<Item = Felt> + ?Sized>(
        _iter: &mut It,
    ) -> Result<Self, OsOutputError> {
        unimplemented!()
    }
}

impl PartialContractStorageUpdate {
    pub fn _from_output_iter<It: Iterator<Item = Felt> + ?Sized>(
        _iter: &mut It,
    ) -> Result<Self, OsOutputError> {
        unimplemented!()
    }
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, PartialEq)]
pub struct FullCompiledClassHashUpdate {
    pub(crate) class_hash: ClassHash,
    pub(crate) prev_compiled_class_hash: CompiledClassHash,
    pub(crate) next_compiled_class_hash: CompiledClassHash,
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, PartialEq)]
pub struct PartialCompiledClassHashUpdate {
    pub(crate) class_hash: ClassHash,
    pub(crate) next_compiled_class_hash: CompiledClassHash,
}

impl FullCompiledClassHashUpdate {
    pub fn from_output_iter<It: Iterator<Item = Felt> + ?Sized>(
        _iter: &mut It,
    ) -> Result<Self, OsOutputError> {
        unimplemented!()
    }
}

impl PartialCompiledClassHashUpdate {
    pub fn from_output_iter<It: Iterator<Item = Felt> + ?Sized>(
        _iter: &mut It,
    ) -> Result<Self, OsOutputError> {
        unimplemented!()
    }
}

// TODO(Tzahi): replace ContractChanges with the next two structs.
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, PartialEq)]
/// Represents the changes in a contract instance, in a full format.
pub struct FullContractChanges {
    // The address of the contract.
    pub(crate) addr: ContractAddress,
    // The previous nonce of the contract.
    pub(crate) prev_nonce: Nonce,
    // The new nonce of the contract.
    pub(crate) new_nonce: Nonce,
    // The previous class hash.
    pub(crate) prev_class_hash: ClassHash,
    // The new class hash.
    pub(crate) new_class_hash: ClassHash,
    // The storage changes of the contract (includes the previous and new value).
    pub(crate) storage_changes: Vec<FullContractStorageUpdate>,
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, PartialEq)]
/// Represents the changes in a contract instance, in a partial format.
pub struct PartialContractChanges {
    // The address of the contract.
    pub(crate) addr: ContractAddress,
    // The new nonce of the contract (for account contracts, if changed).
    pub(crate) new_nonce: Option<Nonce>,
    // The new class hash (if changed).
    pub(crate) new_class_hash: Option<ClassHash>,
    // The storage changes of the contract (includes only the new value).
    pub(crate) storage_changes: Vec<PartialContractStorageUpdate>,
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, PartialEq)]
// An explicit state diff (no KZG commitment applied) in the full output format (with previous
// values for every storage/class hash change). The expected input format for the aggregator.
/// Matches the SquashedOsStateUpdate cairo struct.
pub struct FullOsStateDiff {
    // Changed contracts.
    pub contracts: Vec<FullContractChanges>,
    // Declared classes. Represents the updates of a mapping from class hash to previous and new
    // compiled class hash.
    pub classes: Vec<FullCompiledClassHashUpdate>,
}

impl FullOsStateDiff {
    pub fn from_output_iter<It: Iterator<Item = Felt>>(
        _output_iter: &mut It,
    ) -> Result<Self, OsOutputError> {
        unimplemented!()
    }
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, PartialEq)]
// An explicit state diff (no KZG commitment applied) in the partial output format (no
// previous values).
pub struct PartialOsStateDiff {
    // Changed contracts.
    pub contracts: Vec<PartialContractChanges>,
    // Declared classes. Represents the updates of a mapping from class hash to the new compiled
    // class hash.
    pub classes: Vec<PartialCompiledClassHashUpdate>,
}

impl PartialOsStateDiff {
    pub fn from_output_iter<It: Iterator<Item = Felt>>(
        _output_iter: &mut It,
    ) -> Result<Self, OsOutputError> {
        unimplemented!()
    }
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, PartialEq)]
// A commitment to the state diff (with KZG commitment applied) in the full output format.
pub struct FullCommitmentOsStateDiff(pub(crate) Vec<Felt>);

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, PartialEq)]
// A commitment to the state diff (with KZG commitment applied) in the partial output format.
pub struct PartialCommitmentOsStateDiff(pub(crate) Vec<Felt>);
