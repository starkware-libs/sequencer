#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

use std::collections::HashMap;
use std::fmt::Debug;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::block::{BlockHash, BlockNumber};
use crate::core::{
    ClassHash, CompiledClassHash, ContractAddress, EntryPointSelector, GlobalRoot, Nonce,
    PatriciaKey,
};
use crate::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use crate::hash::{StarkFelt, StarkHash};
use crate::{impl_from_through_intermediate, StarknetApiError};

pub type DeclaredClasses = IndexMap<ClassHash, ContractClass>;
pub type DeprecatedDeclaredClasses = IndexMap<ClassHash, DeprecatedContractClass>;

/// The differences between two states before and after a block with hash block_hash
/// and their respective roots.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct StateUpdate {
    pub block_hash: BlockHash,
    pub new_root: GlobalRoot,
    pub old_root: GlobalRoot,
    pub state_diff: StateDiff,
}

/// The differences between two states.
// Invariant: Addresses are strictly increasing.
// Invariant: Class hashes of declared_classes and deprecated_declared_classes are exclusive.
// TODO(yair): Enforce this invariant.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct StateDiff {
    pub deployed_contracts: IndexMap<ContractAddress, ClassHash>,
    pub storage_diffs: IndexMap<ContractAddress, IndexMap<StorageKey, StarkFelt>>,
    pub declared_classes: IndexMap<ClassHash, (CompiledClassHash, ContractClass)>,
    pub deprecated_declared_classes: IndexMap<ClassHash, DeprecatedContractClass>,
    pub nonces: IndexMap<ContractAddress, Nonce>,
    pub replaced_classes: IndexMap<ContractAddress, ClassHash>,
}

// Invariant: Addresses are strictly increasing.
// The invariant is enforced as [`ThinStateDiff`] is created only from [`starknet_api`][`StateDiff`]
// where the addresses are strictly increasing.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct ThinStateDiff {
    pub deployed_contracts: IndexMap<ContractAddress, ClassHash>,
    pub storage_diffs: IndexMap<ContractAddress, IndexMap<StorageKey, StarkFelt>>,
    pub declared_classes: IndexMap<ClassHash, CompiledClassHash>,
    pub deprecated_declared_classes: Vec<ClassHash>,
    pub nonces: IndexMap<ContractAddress, Nonce>,
    pub replaced_classes: IndexMap<ContractAddress, ClassHash>,
}

impl ThinStateDiff {
    // Returns also the declared classes without cloning them.
    pub fn from_state_diff(diff: StateDiff) -> (Self, DeclaredClasses, DeprecatedDeclaredClasses) {
        (
            Self {
                deployed_contracts: diff.deployed_contracts,
                storage_diffs: diff.storage_diffs,
                declared_classes: diff
                    .declared_classes
                    .iter()
                    .map(|(class_hash, (compiled_hash, _class))| (*class_hash, *compiled_hash))
                    .collect(),
                deprecated_declared_classes: diff
                    .deprecated_declared_classes
                    .keys()
                    .copied()
                    .collect(),
                nonces: diff.nonces,
                replaced_classes: diff.replaced_classes,
            },
            diff.declared_classes
                .into_iter()
                .map(|(class_hash, (_compiled_class_hash, class))| (class_hash, class))
                .collect(),
            diff.deprecated_declared_classes,
        )
    }

    /// This has the same value as `state_diff_length` in the corresponding `BlockHeader`.
    pub fn len(&self) -> usize {
        let mut result = 0usize;
        result += self.deployed_contracts.len();
        result += self.declared_classes.len();
        result += self.deprecated_declared_classes.len();
        result += self.nonces.len();
        result += self.replaced_classes.len();

        for (_contract_address, storage_diffs) in &self.storage_diffs {
            result += storage_diffs.len();
        }
        result
    }

    pub fn is_empty(&self) -> bool {
        self.deployed_contracts.is_empty()
            && self.declared_classes.is_empty()
            && self.deprecated_declared_classes.is_empty()
            && self.nonces.is_empty()
            && self.replaced_classes.is_empty()
            && self
                .storage_diffs
                .iter()
                .all(|(_contract_address, storage_diffs)| storage_diffs.is_empty())
    }
}

impl From<StateDiff> for ThinStateDiff {
    fn from(diff: StateDiff) -> Self {
        Self::from_state_diff(diff).0
    }
}

/// The sequential numbering of the states between blocks.
// Example:
// States: S0       S1       S2
// Blocks      B0->     B1->
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct StateNumber(pub BlockNumber);

impl StateNumber {
    /// The state at the beginning of the block.
    pub fn right_before_block(block_number: BlockNumber) -> StateNumber {
        StateNumber(block_number)
    }

    /// The state at the end of the block, or None if it's is out of range.
    pub fn right_after_block(block_number: BlockNumber) -> Option<StateNumber> {
        Some(StateNumber(block_number.next()?))
    }

    /// The state at the end of the block, without checking if it's in range.
    pub fn unchecked_right_after_block(block_number: BlockNumber) -> StateNumber {
        StateNumber(block_number.unchecked_next())
    }

    pub fn is_before(&self, block_number: BlockNumber) -> bool {
        self.0 <= block_number
    }

    pub fn is_after(&self, block_number: BlockNumber) -> bool {
        !self.is_before(block_number)
    }

    pub fn block_after(&self) -> BlockNumber {
        self.0
    }
}

/// A storage key in a contract.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
    derive_more::Deref,
)]
pub struct StorageKey(pub PatriciaKey);

impl From<StorageKey> for StarkFelt {
    fn from(storage_key: StorageKey) -> StarkFelt {
        **storage_key
    }
}

impl TryFrom<StarkHash> for StorageKey {
    type Error = StarknetApiError;

    fn try_from(val: StarkHash) -> Result<Self, Self::Error> {
        Ok(Self(PatriciaKey::try_from(val)?))
    }
}

impl From<u128> for StorageKey {
    fn from(val: u128) -> Self {
        StorageKey(PatriciaKey::from(val))
    }
}

impl_from_through_intermediate!(u128, StorageKey, u8, u16, u32, u64);

/// A contract class.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClass {
    pub sierra_program: Vec<StarkFelt>,
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
    pub abi: String,
}

#[derive(
    Debug, Default, Clone, Copy, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(deny_unknown_fields)]
pub enum EntryPointType {
    /// A constructor entry point.
    #[serde(rename = "CONSTRUCTOR")]
    Constructor,
    /// An external entry point.
    #[serde(rename = "EXTERNAL")]
    #[default]
    External,
    /// An L1 handler entry point.
    #[serde(rename = "L1_HANDLER")]
    L1Handler,
}

/// An entry point of a [ContractClass](`crate::state::ContractClass`).
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EntryPoint {
    pub function_idx: FunctionIndex,
    pub selector: EntryPointSelector,
}

#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct FunctionIndex(pub usize);
