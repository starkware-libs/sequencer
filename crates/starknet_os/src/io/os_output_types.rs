use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::{Felt, NonZeroFelt};

use crate::io::os_output::{
    felt_as_bool,
    try_into_custom_error,
    wrap_missing,
    wrap_missing_as,
    OsOutputError,
};

#[cfg(test)]
#[path = "os_output_types_test.rs"]
mod os_output_types_test;

// Defined in output.cairo
const N_UPDATES_BOUND: NonZeroFelt =
    NonZeroFelt::from_felt_unchecked(Felt::from_hex_unchecked("10000000000000000")); // 2^64.
const N_UPDATES_SMALL_PACKING_BOUND: NonZeroFelt =
    NonZeroFelt::from_felt_unchecked(Felt::from_hex_unchecked("100")); // 2^8.
const FLAG_BOUND: NonZeroFelt = NonZeroFelt::TWO;

// Cairo DictAccess types for concrete objects.

pub(crate) trait TryFromOutputIter {
    fn try_from_output_iter<It: Iterator<Item = Felt> + ?Sized>(
        iter: &mut It,
    ) -> Result<Self, OsOutputError>
    where
        Self: Sized;
}

impl<T: TryFromOutputIter> TryFromOutputIter for Vec<T> {
    fn try_from_output_iter<It: Iterator<Item = Felt> + ?Sized>(
        iter: &mut It,
    ) -> Result<Self, OsOutputError> {
        let n_items =
            wrap_missing_as(iter.next(), &format!("n_items of {}", std::any::type_name::<T>()))?;
        let mut items = Vec::with_capacity(n_items);
        for _ in 0..n_items {
            items.push(T::try_from_output_iter(iter)?);
        }
        Ok(items)
    }
}
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

impl TryFromOutputIter for FullContractStorageUpdate {
    fn try_from_output_iter<It: Iterator<Item = Felt> + ?Sized>(
        iter: &mut It,
    ) -> Result<Self, OsOutputError> {
        Ok(Self {
            key: wrap_missing_as(iter.next(), "storage key")?,
            prev_value: wrap_missing_as(iter.next(), "previous storage value")?,
            new_value: wrap_missing_as(iter.next(), "storage value")?,
        })
    }
}

impl TryFromOutputIter for PartialContractStorageUpdate {
    fn try_from_output_iter<It: Iterator<Item = Felt> + ?Sized>(
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

impl TryFromOutputIter for FullCompiledClassHashUpdate {
    fn try_from_output_iter<It: Iterator<Item = Felt> + ?Sized>(
        iter: &mut It,
    ) -> Result<Self, OsOutputError> {
        Ok(Self {
            class_hash: ClassHash(wrap_missing_as(iter.next(), "class_hash")?),
            prev_compiled_class_hash: CompiledClassHash(wrap_missing_as(
                iter.next(),
                "prev_compiled_class_hash",
            )?),
            next_compiled_class_hash: CompiledClassHash(wrap_missing_as(
                iter.next(),
                "next_compiled_class_hash",
            )?),
        })
    }
}

impl TryFromOutputIter for PartialCompiledClassHashUpdate {
    fn try_from_output_iter<It: Iterator<Item = Felt> + ?Sized>(
        iter: &mut It,
    ) -> Result<Self, OsOutputError> {
        Ok(Self {
            class_hash: ClassHash(wrap_missing_as(iter.next(), "class_hash")?),
            next_compiled_class_hash: CompiledClassHash(wrap_missing_as(
                iter.next(),
                "new_compiled_class_hash",
            )?),
        })
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
    // The storage changes of the contract (includes the previous and new value).
    pub(crate) storage_changes: Vec<FullContractStorageUpdate>,
}

impl TryFromOutputIter for FullContractChanges {
    fn try_from_output_iter<It: Iterator<Item = Felt> + ?Sized>(
        iter: &mut It,
    ) -> Result<Self, OsOutputError> {
        Ok(Self {
            addr: wrap_missing_as(iter.next(), "addr")?,
            prev_nonce: Nonce(wrap_missing(iter.next(), "prev_nonce")?),
            new_nonce: Nonce(wrap_missing_as(iter.next(), "new_nonce")?),
            prev_class_hash: ClassHash(wrap_missing_as(iter.next(), "prev_class_hash")?),
            new_class_hash: ClassHash(wrap_missing_as(iter.next(), "new_class_hash")?),
            storage_changes: Vec::<FullContractStorageUpdate>::try_from_output_iter(iter)?,
        })
    }
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

impl TryFromOutputIter for PartialContractChanges {
    fn try_from_output_iter<It: Iterator<Item = Felt> + ?Sized>(
        iter: &mut It,
    ) -> Result<Self, OsOutputError> {
        let addr = wrap_missing_as(iter.next(), "addr")?;
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
            new_nonce,
            new_class_hash,
            storage_changes: {
                let n_changes = try_into_custom_error(n_changes, "n_changes")?;
                let mut storage_changes = Vec::with_capacity(n_changes);
                for _ in 0..n_changes {
                    storage_changes.push(PartialContractStorageUpdate::try_from_output_iter(iter)?);
                }
                storage_changes
            },
        })
    }
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

impl TryFromOutputIter for FullOsStateDiff {
    fn try_from_output_iter<It: Iterator<Item = Felt> + ?Sized>(
        iter: &mut It,
    ) -> Result<Self, OsOutputError> {
        Ok(Self {
            contracts: Vec::<FullContractChanges>::try_from_output_iter(iter)?,
            classes: Vec::<FullCompiledClassHashUpdate>::try_from_output_iter(iter)?,
        })
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

impl TryFromOutputIter for PartialOsStateDiff {
    fn try_from_output_iter<It: Iterator<Item = Felt> + ?Sized>(
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
