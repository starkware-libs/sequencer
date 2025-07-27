use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::io::os_output::{wrap_missing, wrap_missing_as, OsOutputError};

// Cairo DictAccess types for concrete objects.

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, PartialEq)]
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
    pub fn from_output_iter<It: Iterator<Item = Felt> + ?Sized>(
        iter: &mut It,
    ) -> Result<Self, OsOutputError> {
        Ok(Self {
            key: wrap_missing_as(iter.next(), "storage key")?,
            prev_value: wrap_missing_as(iter.next(), "previous storage value")?,
            new_value: wrap_missing_as(iter.next(), "storage value")?,
        })
    }
}

impl PartialContractStorageUpdate {
    pub fn from_output_iter<It: Iterator<Item = Felt> + ?Sized>(
        iter: &mut It,
    ) -> Result<Self, OsOutputError> {
        Ok(Self {
            key: wrap_missing_as(iter.next(), "storage key")?,
            new_value: wrap_missing(iter.next(), "storage value")?,
        })
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
    // A map from storage key to its prev value and new value.
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
    // A map from storage key to its prev value (optional) and new value.
    pub(crate) storage_changes: Vec<PartialContractStorageUpdate>,
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, PartialEq)]
/// State diff of an OS run with use_kzg_da=false and full_output=true
/// (expected input of the aggregator).
/// Matches the SquashedOsStateUpdate cairo struct.
pub struct FullOsStateDiff {
    // Contracts that were changed.
    pub contracts: Vec<FullContractChanges>,
    // Classes that were declared. Represents the updates of a mapping from class hash to previous
    // (optional) and new compiled class hash.
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
/// State diff of an OS run with use_kzg_da=false and full_output=false.
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
/// State diff of an OS run with use_kzg_da=true and full_output=true.
pub struct FullCommitmentOsStateDiff(pub(crate) Vec<Felt>);

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, PartialEq)]
/// State diff of an OS run with use_kzg_da=true and full_output=false.
pub struct PartialCommitmentOsStateDiff(pub(crate) Vec<Felt>);
