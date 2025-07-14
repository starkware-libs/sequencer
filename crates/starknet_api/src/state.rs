#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

use std::fmt::Debug;
use std::sync::LazyLock;

use cairo_lang_starknet_classes::contract_class::ContractEntryPoint as CairoLangContractEntryPoint;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use sha3::Digest;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Poseidon, StarkHash as SNTypsCoreStarkHash};

use crate::block::{BlockHash, BlockNumber};
use crate::contract_class::EntryPointType;
use crate::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    EntryPointSelector,
    GlobalRoot,
    Nonce,
    PatriciaKey,
};
use crate::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use crate::hash::{PoseidonHash, StarkHash};
use crate::rpc_transaction::EntryPointByType;
use crate::{impl_from_through_intermediate, StarknetApiError};

pub type DeclaredClasses = IndexMap<ClassHash, SierraContractClass>;
pub type DeprecatedDeclaredClasses = IndexMap<ClassHash, DeprecatedContractClass>;

static API_VERSION: LazyLock<Felt> =
    LazyLock::new(|| Felt::from_bytes_be_slice(b"CONTRACT_CLASS_V0.1.0"));

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
    pub storage_diffs: IndexMap<ContractAddress, IndexMap<StorageKey, Felt>>,
    pub declared_classes: IndexMap<ClassHash, (CompiledClassHash, SierraContractClass)>,
    pub deprecated_declared_classes: IndexMap<ClassHash, DeprecatedContractClass>,
    pub nonces: IndexMap<ContractAddress, Nonce>,
}

// Invariant: Addresses are strictly increasing.
// The invariant is enforced as [`ThinStateDiff`] is created only from [`starknet_api`][`StateDiff`]
// where the addresses are strictly increasing.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct ThinStateDiff {
    pub deployed_contracts: IndexMap<ContractAddress, ClassHash>,
    pub storage_diffs: IndexMap<ContractAddress, IndexMap<StorageKey, Felt>>,
    pub declared_classes: IndexMap<ClassHash, CompiledClassHash>,
    pub deprecated_declared_classes: Vec<ClassHash>,
    pub nonces: IndexMap<ContractAddress, Nonce>,
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

impl From<StorageKey> for Felt {
    fn from(storage_key: StorageKey) -> Felt {
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

impl StorageKey {
    pub fn next_storage_key(&self) -> Result<StorageKey, StarknetApiError> {
        Ok(StorageKey(PatriciaKey::try_from(*self.0.key() + Felt::ONE)?))
    }
}

impl_from_through_intermediate!(u128, StorageKey, u8, u16, u32, u64);

/// A contract class.
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize, Hash)]
pub struct SierraContractClass {
    pub sierra_program: Vec<Felt>,
    pub contract_class_version: String,
    pub entry_points_by_type: EntryPointByType,
    pub abi: String,
}

impl Default for SierraContractClass {
    fn default() -> Self {
        Self {
            sierra_program: [Felt::ONE, Felt::TWO, Felt::THREE].to_vec(),
            contract_class_version: Default::default(),
            entry_points_by_type: Default::default(),
            abi: Default::default(),
        }
    }
}

impl SierraContractClass {
    pub fn calculate_class_hash(&self) -> ClassHash {
        let external_entry_points_hash = entry_points_hash(self, &EntryPointType::External);
        let l1_handler_entry_points_hash = entry_points_hash(self, &EntryPointType::L1Handler);
        let constructor_entry_points_hash = entry_points_hash(self, &EntryPointType::Constructor);
        let abi_keccak = sha3::Keccak256::default().chain_update(self.abi.as_bytes()).finalize();
        let abi_hash = truncated_keccak(abi_keccak.into());
        let program_hash = Poseidon::hash_array(self.sierra_program.as_slice());

        let class_hash = Poseidon::hash_array(&[
            *API_VERSION,
            external_entry_points_hash.0,
            l1_handler_entry_points_hash.0,
            constructor_entry_points_hash.0,
            abi_hash,
            program_hash,
        ]);
        ClassHash(class_hash)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ContractClassComponentHashes {
    contract_class_version: Felt,
    external_functions_hash: PoseidonHash,
    l1_handlers_hash: PoseidonHash,
    constructors_hash: PoseidonHash,
    abi_hash: Felt,
    sierra_program_hash: Felt,
}

impl ContractClassComponentHashes {
    pub fn flatten(&self) -> Vec<Felt> {
        vec![
            self.contract_class_version,
            self.external_functions_hash.0,
            self.l1_handlers_hash.0,
            self.constructors_hash.0,
            self.abi_hash,
            self.sierra_program_hash,
        ]
    }
}

#[cfg(any(test, feature = "testing"))]
impl From<cairo_lang_starknet_classes::contract_class::ContractClass> for SierraContractClass {
    fn from(
        cairo_lang_contract_class: cairo_lang_starknet_classes::contract_class::ContractClass,
    ) -> Self {
        Self {
            sierra_program: cairo_lang_contract_class
                .sierra_program
                .into_iter()
                .map(|big_uint_as_hex| Felt::from(big_uint_as_hex.value))
                .collect(),
            contract_class_version: cairo_lang_contract_class.contract_class_version,
            entry_points_by_type: cairo_lang_contract_class.entry_points_by_type.into(),
            abi: cairo_lang_contract_class.abi.map(|abi| abi.json()).unwrap_or_default(),
        }
    }
}

/// An entry point of a [SierraContractClass](`SierraContractClass`).
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EntryPoint {
    pub function_idx: FunctionIndex,
    pub selector: EntryPointSelector,
}

impl From<CairoLangContractEntryPoint> for EntryPoint {
    fn from(entry_point: CairoLangContractEntryPoint) -> Self {
        Self {
            function_idx: FunctionIndex(entry_point.function_idx),
            selector: EntryPointSelector(entry_point.selector.into()),
        }
    }
}

#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct FunctionIndex(pub usize);

fn entry_points_hash(
    class: &SierraContractClass,
    entry_point_type: &EntryPointType,
) -> PoseidonHash {
    PoseidonHash(Poseidon::hash_array(
        class
            .entry_points_by_type
            .to_hash_map()
            .get(entry_point_type)
            .unwrap_or(&vec![])
            .iter()
            .flat_map(|ep| [ep.selector.0, usize_into_felt(ep.function_idx.0)])
            .collect::<Vec<_>>()
            .as_slice(),
    ))
}

// Python code masks with (2**250 - 1) which starts 0x03 and is followed by 31 0xff in be.
// Truncation is needed not to overflow the field element.
pub fn truncated_keccak(mut plain: [u8; 32]) -> Felt {
    plain[0] &= 0x03;
    Felt::from_bytes_be(&plain)
}

fn usize_into_felt(u: usize) -> Felt {
    u128::try_from(u).expect("Expect at most 128 bits").into()
}
