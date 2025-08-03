use std::cell::{Ref, RefCell};
use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use starknet_api::abi::abi_utils::get_fee_token_var_address;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::context::TransactionContext;
use crate::execution::contract_class::RunnableCompiledClass;
use crate::state::errors::StateError;
use crate::state::state_api::{State, StateReader, StateResult, UpdatableState};
use crate::transaction::objects::TransactionExecutionInfo;
use crate::utils::{strict_subtract_mappings, subtract_mappings};

#[cfg(test)]
#[path = "cached_state_test.rs"]
mod test;

pub type ContractClassMapping = HashMap<ClassHash, RunnableCompiledClass>;

/// Caches read and write requests.
///
/// Writer functionality is builtin, whereas Reader functionality is injected through
/// initialization.
#[cfg_attr(any(test, feature = "reexecution", feature = "testing"), derive(Clone))]
#[derive(Debug)]
pub struct CachedState<S: StateReader> {
    pub state: S,
    // Invariant: read/write access is managed by CachedState.
    // Using interior mutability to update caches during `State`'s immutable getters.
    pub cache: RefCell<StateCache>,
    pub class_hash_to_class: RefCell<ContractClassMapping>,
}

impl<S: StateReader> CachedState<S> {
    pub fn new(state: S) -> Self {
        Self {
            state,
            cache: RefCell::new(StateCache::default()),
            class_hash_to_class: RefCell::new(HashMap::default()),
        }
    }

    /// Returns the state diff resulting from the performed writes, with respect to the parent
    /// state.
    pub fn to_state_diff(&mut self) -> StateResult<StateChanges> {
        self.update_initial_values_of_write_only_access()?;
        Ok(self.cache.borrow().to_state_diff())
    }

    pub fn borrow_updated_state_cache(&mut self) -> StateResult<Ref<'_, StateCache>> {
        self.update_initial_values_of_write_only_access()?;
        Ok(self.cache.borrow())
    }

    pub fn update_cache(
        &mut self,
        write_updates: &StateMaps,
        local_contract_cache_updates: ContractClassMapping,
    ) {
        // Check consistency between declared_contracts and class_hash_to_class.
        for (&key, &value) in &write_updates.declared_contracts {
            assert_eq!(value, local_contract_cache_updates.contains_key(&key));
        }
        let mut cache = self.cache.borrow_mut();
        cache.writes.extend(write_updates);
        self.class_hash_to_class.get_mut().extend(local_contract_cache_updates);
    }

    /// Updates cache with initial cell values for write-only access.
    /// If written values match the original, the cell is unchanged and not counted as a
    /// storage-change for fee calculation.
    /// Note: in valid flows, all other read mappings must be filled at this point:
    ///   * Nonce: read previous before incrementing.
    ///   * Class hash: Deploy: verify the address is not occupied; Replace class: verify the
    ///     contract is deployed before running any code.
    ///   * Compiled class hash: verify the class is not declared through `get_compiled_class`.
    ///
    /// TODO(Noa, 30/07/23): Consider adding DB getters in bulk (via a DB read transaction).
    fn update_initial_values_of_write_only_access(&mut self) -> StateResult<()> {
        let cache = &mut *self.cache.borrow_mut();

        // Eliminate storage writes that are identical to the initial value (no change).
        for contract_storage_key in cache.writes.storage.keys() {
            if !cache.initial_reads.storage.contains_key(contract_storage_key) {
                // First access to this cell was write; cache initial value.
                cache.initial_reads.storage.insert(
                    *contract_storage_key,
                    self.state.get_storage_at(contract_storage_key.0, contract_storage_key.1)?,
                );
            }
        }
        Ok(())
    }

    pub fn writes_contract_addresses(&self) -> HashSet<ContractAddress> {
        self.cache.borrow().writes.get_contract_addresses()
    }

    // TODO(Aner): move under OS cfg flag.
    // TODO(Aner): Try to avoid cloning.
    pub fn writes_compiled_class_hashes(&self) -> HashMap<ClassHash, CompiledClassHash> {
        self.cache.borrow().writes.compiled_class_hashes.clone()
    }
}

impl<S: StateReader> UpdatableState for CachedState<S> {
    fn apply_writes(&mut self, writes: &StateMaps, class_hash_to_class: &ContractClassMapping) {
        // TODO(Noa,15/5/24): Reconsider the clone.
        self.update_cache(writes, class_hash_to_class.clone());
    }
}

#[cfg(any(feature = "testing", test))]
impl<S: StateReader> From<S> for CachedState<S> {
    fn from(state_reader: S) -> Self {
        CachedState::new(state_reader)
    }
}

impl<S: StateReader> StateReader for CachedState<S> {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        let mut cache = self.cache.borrow_mut();

        if cache.get_storage_at(contract_address, key).is_none() {
            let storage_value = self.state.get_storage_at(contract_address, key)?;
            cache.set_storage_initial_value(contract_address, key, storage_value);
        }

        let value = cache.get_storage_at(contract_address, key).unwrap_or_else(|| {
            panic!("Cannot retrieve '{contract_address:?}' and '{key:?}' from the cache.")
        });
        Ok(*value)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        let mut cache = self.cache.borrow_mut();

        if cache.get_nonce_at(contract_address).is_none() {
            let nonce = self.state.get_nonce_at(contract_address)?;
            cache.set_nonce_initial_value(contract_address, nonce);
        }

        let nonce = cache
            .get_nonce_at(contract_address)
            .unwrap_or_else(|| panic!("Cannot retrieve '{contract_address:?}' from the cache."));

        Ok(*nonce)
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        let mut cache = self.cache.borrow_mut();

        if cache.get_class_hash_at(contract_address).is_none() {
            let class_hash = self.state.get_class_hash_at(contract_address)?;
            cache.set_class_hash_initial_value(contract_address, class_hash);
        }

        let class_hash = cache
            .get_class_hash_at(contract_address)
            .unwrap_or_else(|| panic!("Cannot retrieve '{contract_address:?}' from the cache."));
        Ok(*class_hash)
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        let mut cache = self.cache.borrow_mut();
        let class_hash_to_class = &mut *self.class_hash_to_class.borrow_mut();

        if let std::collections::hash_map::Entry::Vacant(vacant_entry) =
            class_hash_to_class.entry(class_hash)
        {
            match self.state.get_compiled_class(class_hash) {
                Err(StateError::UndeclaredClassHash(class_hash)) => {
                    cache.set_declared_contract_initial_value(class_hash, false);
                    cache.set_compiled_class_hash_initial_value(
                        class_hash,
                        CompiledClassHash(Felt::ZERO),
                    );
                    Err(StateError::UndeclaredClassHash(class_hash))?;
                }
                Err(error) => Err(error)?,
                Ok(contract_class) => {
                    cache.set_declared_contract_initial_value(class_hash, true);
                    vacant_entry.insert(contract_class);
                }
            }
        }

        let contract_class = class_hash_to_class
            .get(&class_hash)
            .cloned()
            .expect("The class hash must appear in the cache.");

        Ok(contract_class)
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        let mut cache = self.cache.borrow_mut();

        if cache.get_compiled_class_hash(class_hash).is_none() {
            let compiled_class_hash = self.state.get_compiled_class_hash(class_hash)?;
            cache.set_compiled_class_hash_initial_value(class_hash, compiled_class_hash);
        }

        let compiled_class_hash = cache
            .get_compiled_class_hash(class_hash)
            .unwrap_or_else(|| panic!("Cannot retrieve '{class_hash:?}' from the cache."));
        Ok(*compiled_class_hash)
    }
}

impl<S: StateReader> State for CachedState<S> {
    fn set_storage_at(
        &mut self,
        contract_address: ContractAddress,
        key: StorageKey,
        value: Felt,
    ) -> StateResult<()> {
        self.cache.get_mut().set_storage_value(contract_address, key, value);

        Ok(())
    }

    fn increment_nonce(&mut self, contract_address: ContractAddress) -> StateResult<()> {
        let current_nonce = self.get_nonce_at(contract_address)?;
        let next_nonce = Nonce(current_nonce.0 + Felt::ONE);
        self.cache.get_mut().set_nonce_value(contract_address, next_nonce);

        Ok(())
    }

    fn set_class_hash_at(
        &mut self,
        contract_address: ContractAddress,
        class_hash: ClassHash,
    ) -> StateResult<()> {
        if contract_address == ContractAddress::default() {
            return Err(StateError::OutOfRangeContractAddress);
        }

        self.cache.get_mut().set_class_hash_write(contract_address, class_hash);
        Ok(())
    }

    fn set_contract_class(
        &mut self,
        class_hash: ClassHash,
        contract_class: RunnableCompiledClass,
    ) -> StateResult<()> {
        self.class_hash_to_class.get_mut().insert(class_hash, contract_class);
        let mut cache = self.cache.borrow_mut();
        cache.declare_contract(class_hash);
        Ok(())
    }

    fn set_compiled_class_hash(
        &mut self,
        class_hash: ClassHash,
        compiled_class_hash: CompiledClassHash,
    ) -> StateResult<()> {
        self.cache.get_mut().set_compiled_class_hash_write(class_hash, compiled_class_hash);
        Ok(())
    }
}

#[cfg(any(feature = "testing", test))]
impl Default for CachedState<crate::test_utils::dict_state_reader::DictStateReader> {
    fn default() -> Self {
        Self {
            state: Default::default(),
            cache: Default::default(),
            class_hash_to_class: Default::default(),
        }
    }
}

#[cfg(feature = "reexecution")]
impl<S: StateReader> CachedState<S> {
    pub fn get_initial_reads(&self) -> StateResult<StateMaps> {
        Ok(self.cache.borrow().initial_reads.clone())
    }
}

pub type StorageEntry = (ContractAddress, StorageKey);

#[derive(Debug, Default, derive_more::IntoIterator)]
pub struct StorageView(pub HashMap<StorageEntry, Felt>);

/// Converts a `CachedState`'s storage mapping into a `StateDiff`'s storage mapping.
impl From<StorageView> for IndexMap<ContractAddress, IndexMap<StorageKey, Felt>> {
    fn from(storage_view: StorageView) -> Self {
        let mut storage_updates = Self::new();
        for ((address, key), value) in storage_view.into_iter() {
            storage_updates
                .entry(address)
                .and_modify(|map| {
                    map.insert(key, value);
                })
                .or_insert_with(|| IndexMap::from([(key, value)]));
        }

        storage_updates
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StateMaps {
    pub nonces: HashMap<ContractAddress, Nonce>,
    pub class_hashes: HashMap<ContractAddress, ClassHash>,
    pub storage: HashMap<StorageEntry, Felt>,
    pub compiled_class_hashes: HashMap<ClassHash, CompiledClassHash>,
    pub declared_contracts: HashMap<ClassHash, bool>,
}

impl StateMaps {
    pub fn extend(&mut self, other: &Self) {
        self.nonces.extend(&other.nonces);
        self.class_hashes.extend(&other.class_hashes);
        self.storage.extend(&other.storage);
        self.compiled_class_hashes.extend(&other.compiled_class_hashes);
        self.declared_contracts.extend(&other.declared_contracts)
    }

    /// Subtracts other's mappings from self.
    /// Assumes (and enforces) other's keys contains self's.
    pub fn diff(&self, other: &Self) -> Self {
        Self {
            nonces: strict_subtract_mappings(&self.nonces, &other.nonces),
            class_hashes: strict_subtract_mappings(&self.class_hashes, &other.class_hashes),
            storage: strict_subtract_mappings(&self.storage, &other.storage),
            compiled_class_hashes: strict_subtract_mappings(
                &self.compiled_class_hashes,
                &other.compiled_class_hashes,
            ),
            // TODO(Yoni, 1/8/2024): consider forbid redeclaration of Cairo 0, to be able to use
            // strict subtraction here, for completeness.
            declared_contracts: subtract_mappings(
                &self.declared_contracts,
                &other.declared_contracts,
            ),
        }
    }

    pub fn get_contract_addresses(&self) -> HashSet<ContractAddress> {
        // Storage updates.
        let mut modified_contracts: HashSet<ContractAddress> =
            self.storage.keys().map(|address_key_pair| address_key_pair.0).collect();
        // Nonce updates.
        modified_contracts.extend(self.nonces.keys());
        // Class hash updates (deployed contracts + replace_class syscall).
        modified_contracts.extend(self.class_hashes.keys());

        modified_contracts
    }

    pub fn keys(&self) -> StateChangesKeys {
        StateChangesKeys {
            modified_contracts: self.get_contract_addresses(),
            nonce_keys: self.nonces.keys().cloned().collect(),
            class_hash_keys: self.class_hashes.keys().cloned().collect(),
            storage_keys: self.storage.keys().cloned().collect(),
            compiled_class_hash_keys: self.compiled_class_hashes.keys().cloned().collect(),
        }
    }
}

/// Caches read and write requests.
/// The tracked changes are needed for block state commitment.

// Invariant: keys cannot be deleted from fields (only used internally by the cached state).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StateCache {
    // Reader's cached information; initial values, read before any write operation (per cell).
    pub(crate) initial_reads: StateMaps,

    // Writer's cached information.
    pub(crate) writes: StateMaps,
}

impl StateCache {
    /// Returns the state diff resulting from the performed writes, with respect to the initial
    /// reads. Assumes (and enforces) all initial reads are cached.
    pub fn to_state_diff(&self) -> StateChanges {
        let state_maps = self.writes.diff(&self.initial_reads);
        let allocated_keys =
            AllocatedKeys::from_storage_diff(&self.writes.storage, &self.initial_reads.storage);
        StateChanges { state_maps, allocated_keys }
    }

    /// Squashes the given state caches into a single one and returns the state diff. Note that the
    /// order of the state caches is important.
    pub fn squash_state_caches(state_caches: Vec<&Self>) -> Self {
        let mut squashed_state_cache = StateCache::default();

        // Gives priority to early initial reads.
        state_caches.iter().rev().for_each(|state_cache| {
            squashed_state_cache.initial_reads.extend(&state_cache.initial_reads)
        });
        // Gives priority to late writes.
        state_caches
            .iter()
            .for_each(|state_cache| squashed_state_cache.writes.extend(&state_cache.writes));
        squashed_state_cache
    }

    /// Squashes the given state caches into a single one and returns the state diff. Note that the
    /// order of the state caches is important.
    /// If 'comprehensive_state_diff' is false, opposite updates may not be canceled out. Used for
    /// backward compatibility.
    pub fn squash_state_diff(
        state_caches: Vec<&Self>,
        comprehensive_state_diff: bool,
    ) -> StateChanges {
        if comprehensive_state_diff {
            return Self::squash_state_caches(state_caches).to_state_diff();
        }

        // Backward compatibility.
        let mut merged_state_changes = StateChanges::default();
        for state_cache in state_caches {
            let state_change = state_cache.to_state_diff();
            merged_state_changes.state_maps.extend(&state_change.state_maps);
            merged_state_changes.allocated_keys.0.extend(&state_change.allocated_keys.0);
        }
        merged_state_changes
    }

    pub fn extended_state_diff(&self) -> StateMaps {
        let mut reads = self.initial_reads.clone();
        reads.extend(&self.writes);
        reads
    }

    fn declare_contract(&mut self, class_hash: ClassHash) {
        self.writes.declared_contracts.insert(class_hash, true);
    }

    fn set_declared_contract_initial_value(&mut self, class_hash: ClassHash, is_declared: bool) {
        self.initial_reads.declared_contracts.insert(class_hash, is_declared);
    }

    fn get_storage_at(&self, contract_address: ContractAddress, key: StorageKey) -> Option<&Felt> {
        let contract_storage_key = (contract_address, key);
        self.writes
            .storage
            .get(&contract_storage_key)
            .or_else(|| self.initial_reads.storage.get(&contract_storage_key))
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> Option<&Nonce> {
        self.writes
            .nonces
            .get(&contract_address)
            .or_else(|| self.initial_reads.nonces.get(&contract_address))
    }

    pub fn set_storage_initial_value(
        &mut self,
        contract_address: ContractAddress,
        key: StorageKey,
        value: Felt,
    ) {
        let contract_storage_key = (contract_address, key);
        self.initial_reads.storage.insert(contract_storage_key, value);
    }

    fn set_storage_value(
        &mut self,
        contract_address: ContractAddress,
        key: StorageKey,
        value: Felt,
    ) {
        let contract_storage_key = (contract_address, key);
        self.writes.storage.insert(contract_storage_key, value);
    }

    fn set_nonce_initial_value(&mut self, contract_address: ContractAddress, nonce: Nonce) {
        self.initial_reads.nonces.insert(contract_address, nonce);
    }

    fn set_nonce_value(&mut self, contract_address: ContractAddress, nonce: Nonce) {
        self.writes.nonces.insert(contract_address, nonce);
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> Option<&ClassHash> {
        self.writes
            .class_hashes
            .get(&contract_address)
            .or_else(|| self.initial_reads.class_hashes.get(&contract_address))
    }

    fn set_class_hash_initial_value(
        &mut self,
        contract_address: ContractAddress,
        class_hash: ClassHash,
    ) {
        self.initial_reads.class_hashes.insert(contract_address, class_hash);
    }

    fn set_class_hash_write(&mut self, contract_address: ContractAddress, class_hash: ClassHash) {
        self.writes.class_hashes.insert(contract_address, class_hash);
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> Option<&CompiledClassHash> {
        self.writes
            .compiled_class_hashes
            .get(&class_hash)
            .or_else(|| self.initial_reads.compiled_class_hashes.get(&class_hash))
    }

    fn set_compiled_class_hash_initial_value(
        &mut self,
        class_hash: ClassHash,
        compiled_class_hash: CompiledClassHash,
    ) {
        self.initial_reads.compiled_class_hashes.insert(class_hash, compiled_class_hash);
    }

    fn set_compiled_class_hash_write(
        &mut self,
        class_hash: ClassHash,
        compiled_class_hash: CompiledClassHash,
    ) {
        self.writes.compiled_class_hashes.insert(class_hash, compiled_class_hash);
    }
}

/// Wraps a mutable reference to a `State` object, exposing its API.
/// Used to pass ownership to a `CachedState`.
pub struct MutRefState<'a, S: StateReader + ?Sized>(&'a mut S);

impl<'a, S: StateReader + ?Sized> MutRefState<'a, S> {
    pub fn new(state: &'a mut S) -> Self {
        Self(state)
    }
}

/// Proxies inner object to expose `State` functionality.
impl<S: StateReader + ?Sized> StateReader for MutRefState<'_, S> {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        self.0.get_storage_at(contract_address, key)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        self.0.get_nonce_at(contract_address)
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        self.0.get_class_hash_at(contract_address)
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        self.0.get_compiled_class(class_hash)
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        self.0.get_compiled_class_hash(class_hash)
    }
}

pub type TransactionalState<'a, U> = CachedState<MutRefState<'a, U>>;

impl<S: StateReader> TransactionalState<'_, S> {
    /// Creates a transactional instance from the given updatable state.
    /// It allows performing buffered modifying actions on the given state, which
    /// will either all happen (will be updated in the state and committed)
    /// or none of them (will be discarded).
    pub fn create_transactional(state: &mut S) -> TransactionalState<'_, S> {
        CachedState::new(MutRefState::new(state))
    }

    /// Drops `self`.
    pub fn abort(self) {}
}

/// Adds the ability to perform a transactional execution.
impl<U: UpdatableState> TransactionalState<'_, U> {
    /// Commits changes in the child (wrapping) state to its parent.
    pub fn commit(self) {
        let state = self.state.0;
        let child_cache = self.cache.into_inner();
        state.apply_writes(&child_cache.writes, &self.class_hash_to_class.into_inner())
    }
}

type StorageDiff = IndexMap<ContractAddress, IndexMap<StorageKey, Felt>>;

/// Holds uncommitted changes induced on Starknet contracts.
#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommitmentStateDiff {
    // Contract instance attributes (per address).
    pub address_to_class_hash: IndexMap<ContractAddress, ClassHash>,
    pub address_to_nonce: IndexMap<ContractAddress, Nonce>,
    pub storage_updates: IndexMap<ContractAddress, IndexMap<StorageKey, Felt>>,

    // Global attributes.
    pub class_hash_to_compiled_class_hash: IndexMap<ClassHash, CompiledClassHash>,
}

impl From<StateMaps> for CommitmentStateDiff {
    fn from(diff: StateMaps) -> Self {
        Self {
            address_to_class_hash: IndexMap::from_iter(diff.class_hashes),
            storage_updates: StorageDiff::from(StorageView(diff.storage)),
            class_hash_to_compiled_class_hash: IndexMap::from_iter(diff.compiled_class_hashes),
            address_to_nonce: IndexMap::from_iter(diff.nonces),
        }
    }
}

/// Used to track the state diff size, which is determined by the number of new keys.
/// Also, can be used to accuratly measure the contribution of a single (say, transactional)
/// state to a cumulative state diff - provides set-like functionallities for this porpuse.
///
/// Note: Cancelling writes (0 -> 1 -> 0) are neglected here.
#[cfg_attr(any(feature = "testing", test), derive(Clone))]
#[derive(Debug, Default, Eq, PartialEq)]
pub struct StateChangesKeys {
    nonce_keys: HashSet<ContractAddress>,
    class_hash_keys: HashSet<ContractAddress>,
    storage_keys: HashSet<StorageEntry>,
    compiled_class_hash_keys: HashSet<ClassHash>,
    // Note: this field may not be consistent with the above keys; specifically, it may be
    // strictlly contained in them. For example, as a result of a `difference` operation.
    modified_contracts: HashSet<ContractAddress>,
}

impl StateChangesKeys {
    // For each set member, collects the values that are in `self` but not in `other`.
    // The output represents the residual contribution of `self` to `other`'s corresponding
    // state diff.
    pub fn difference(&self, other: &Self) -> Self {
        Self {
            nonce_keys: self.nonce_keys.difference(&other.nonce_keys).cloned().collect(),
            class_hash_keys: self
                .class_hash_keys
                .difference(&other.class_hash_keys)
                .cloned()
                .collect(),
            storage_keys: self.storage_keys.difference(&other.storage_keys).cloned().collect(),
            compiled_class_hash_keys: self
                .compiled_class_hash_keys
                .difference(&other.compiled_class_hash_keys)
                .cloned()
                .collect(),
            modified_contracts: self
                .modified_contracts
                .difference(&other.modified_contracts)
                .cloned()
                .collect(),
        }
    }

    pub fn extend(&mut self, other: &Self) {
        self.nonce_keys.extend(&other.nonce_keys);
        self.class_hash_keys.extend(&other.class_hash_keys);
        self.storage_keys.extend(&other.storage_keys);
        self.compiled_class_hash_keys.extend(&other.compiled_class_hash_keys);
        self.modified_contracts.extend(&other.modified_contracts);
    }

    pub fn update_sequencer_key_in_storage(
        &mut self,
        tx_context: &TransactionContext,
        tx_result: &TransactionExecutionInfo,
        concurrency_mode: bool,
    ) {
        let actual_fee = tx_result.receipt.fee.0;
        let sequencer_address = tx_context.block_context.block_info.sequencer_address;
        if concurrency_mode
            && !tx_context.is_sequencer_the_sender()
            && tx_result.fee_transfer_call_info.is_some()
            && actual_fee > 0
        {
            // Add the deleted sequencer balance key to the storage keys.
            let sequencer_balance_low = get_fee_token_var_address(sequencer_address);
            self.storage_keys.insert((tx_context.fee_token_address(), sequencer_balance_low));
        }
    }

    pub fn count(&self) -> StateChangesCount {
        // nonce_keys effect is captured by modified_contracts; it is not used but kept for
        // completeness of this struct.
        StateChangesCount {
            n_storage_updates: self.storage_keys.len(),
            n_class_hash_updates: self.class_hash_keys.len(),
            n_compiled_class_hash_updates: self.compiled_class_hash_keys.len(),
            n_modified_contracts: self.modified_contracts.len(),
        }
    }

    #[cfg(any(feature = "testing", test))]
    pub fn create_for_testing(nonce_keys: HashSet<ContractAddress>) -> Self {
        Self { nonce_keys, ..Default::default() }
    }
}

/// Holds the set of allocated storage keys.
/// Ignores all but storage entry allocations - newly allocated contract addresses and
/// class hashes are paid for separately.
#[cfg_attr(any(feature = "testing", test), derive(Clone))]
#[derive(Debug, Default, Eq, PartialEq)]
pub struct AllocatedKeys(HashSet<StorageEntry>);

impl AllocatedKeys {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Collects entries that turn zero -> nonzero.
    pub fn from_storage_diff(
        updated_storage: &HashMap<StorageEntry, Felt>,
        base_storage: &HashMap<StorageEntry, Felt>,
    ) -> Self {
        Self(
            updated_storage
                .iter()
                .filter_map(|(k, v)| {
                    let base_value = base_storage.get(k).unwrap_or(&Felt::ZERO);
                    if *v != Felt::ZERO && *base_value == Felt::ZERO { Some(*k) } else { None }
                })
                .collect(),
        )
    }
}

/// Holds the state changes.
#[cfg_attr(any(feature = "testing", test), derive(Clone))]
#[derive(Debug, Default, Eq, PartialEq)]
pub struct StateChanges {
    pub state_maps: StateMaps,
    pub allocated_keys: AllocatedKeys,
}

impl StateChanges {
    pub fn count_for_fee_charge(
        &self,
        sender_address: Option<ContractAddress>,
        fee_token_address: ContractAddress,
    ) -> StateChangesCountForFee {
        let mut modified_contracts = self.state_maps.get_contract_addresses();

        // For account transactions, we need to compute the transaction fee before we can execute
        // the fee transfer, and the fee should cover the state changes that happen in the
        // fee transfer. The fee transfer is going to update the balance of the sequencer
        // and the balance of the sender contract, but we don't charge the sender for the
        // sequencer balance change as it is amortized across the block.
        let mut n_storage_updates = self.state_maps.storage.len();
        if let Some(sender_address) = sender_address {
            let sender_balance_key = get_fee_token_var_address(sender_address);
            if !self.state_maps.storage.contains_key(&(fee_token_address, sender_balance_key)) {
                n_storage_updates += 1;
            }
        }

        // Exclude the fee token contract modification, since it’s charged once throughout the
        // block.
        modified_contracts.remove(&fee_token_address);

        StateChangesCountForFee {
            state_changes_count: StateChangesCount {
                n_storage_updates,
                n_class_hash_updates: self.state_maps.class_hashes.len(),
                n_compiled_class_hash_updates: self.state_maps.compiled_class_hashes.len(),
                n_modified_contracts: modified_contracts.len(),
            },
            n_allocated_keys: self.allocated_keys.len(),
        }
    }
}

/// Holds the number of state changes.
#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StateChangesCount {
    pub n_storage_updates: usize,
    pub n_class_hash_updates: usize,
    pub n_compiled_class_hash_updates: usize,
    pub n_modified_contracts: usize,
}

/// Holds the number of state changes for fee.
#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StateChangesCountForFee {
    pub state_changes_count: StateChangesCount,
    pub n_allocated_keys: usize,
}
