use std::collections::{BTreeMap, HashMap, HashSet};

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use serde::{Deserialize, Serialize};
use starknet_api::core::ClassHash;
use starknet_api::execution_resources::GasAmount;

use crate::blockifier::transaction_executor::{
    TransactionExecutorError,
    TransactionExecutorResult,
};
use crate::blockifier_versioned_constants::VersionedConstants;
use crate::execution::call_info::ExecutionSummary;
use crate::fee::gas_usage::get_onchain_data_segment_length;
use crate::fee::resources::TransactionResources;
use crate::state::cached_state::{StateChangesKeys, StorageEntry};
use crate::state::state_api::StateReader;
use crate::transaction::errors::TransactionExecutionError;
use crate::transaction::objects::{ExecutionResourcesTraits, TransactionExecutionResult};
use crate::utils::{
    safe_add_gas_panic_on_overflow,
    should_migrate,
    u64_from_usize,
    usize_from_u64,
};

#[cfg(test)]
#[path = "bouncer_test.rs"]
mod test;

macro_rules! impl_checked_ops {
    ($($field:ident),+) => {
        pub fn checked_sub(self: Self, other: Self) -> Option<Self> {
            Some(
                Self {
                    $(
                        $field: self.$field.checked_sub(other.$field)?,
                    )+
                }
            )
        }

        pub fn checked_add(self: Self, other: Self) -> Option<Self> {
            Some(
                Self {
                    $(
                        $field: self.$field.checked_add(other.$field)?,
                    )+
                }
            )
        }
    };
}

pub type BuiltinCounterMap = HashMap<BuiltinName, usize>;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct BouncerConfig {
    pub block_max_capacity: BouncerWeights,
}

impl BouncerConfig {
    pub fn empty() -> Self {
        Self { block_max_capacity: BouncerWeights::empty() }
    }

    pub fn max() -> Self {
        Self { block_max_capacity: BouncerWeights::max() }
    }

    pub fn has_room(&self, weights: BouncerWeights) -> bool {
        self.block_max_capacity.has_room(weights)
    }

    #[allow(clippy::result_large_err)]
    pub fn within_max_capacity_or_err(
        &self,
        weights: BouncerWeights,
    ) -> TransactionExecutionResult<()> {
        if self.block_max_capacity.has_room(weights) {
            Ok(())
        } else {
            Err(TransactionExecutionError::TransactionTooLarge {
                max_capacity: Box::new(self.block_max_capacity),
                tx_size: Box::new(weights),
            })
        }
    }
}

impl SerializeConfig for BouncerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        prepend_sub_config_name(self.block_max_capacity.dump(), "block_max_capacity")
    }
}

#[cfg_attr(any(test, feature = "testing"), derive(derive_more::Add, derive_more::AddAssign))]
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
/// Represents the execution resources counted throughout block creation.
pub struct BouncerWeights {
    pub l1_gas: usize,
    pub message_segment_length: usize,
    pub n_events: usize,
    pub state_diff_size: usize,
    pub sierra_gas: GasAmount,
    pub n_txs: usize,
}

impl BouncerWeights {
    impl_checked_ops!(l1_gas, message_segment_length, n_events, state_diff_size, sierra_gas, n_txs);

    pub fn has_room(&self, other: Self) -> bool {
        self.checked_sub(other).is_some()
    }

    pub fn max() -> Self {
        Self {
            l1_gas: usize::MAX,
            message_segment_length: usize::MAX,
            n_events: usize::MAX,
            state_diff_size: usize::MAX,
            sierra_gas: GasAmount::MAX,
            n_txs: usize::MAX,
        }
    }

    pub fn empty() -> Self {
        Self {
            l1_gas: 0,
            message_segment_length: 0,
            n_events: 0,
            state_diff_size: 0,
            sierra_gas: GasAmount::ZERO,
            n_txs: 0,
        }
    }
}

impl Default for BouncerWeights {
    // TODO(Yael): update the default values once the actual values are known.
    fn default() -> Self {
        Self {
            l1_gas: 2500000,
            message_segment_length: 3700,
            n_events: 5000,
            state_diff_size: 4000,
            sierra_gas: GasAmount(400000000),
            n_txs: 600,
        }
    }
}

impl SerializeConfig for BouncerWeights {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from([ser_param(
            "l1_gas",
            &self.l1_gas,
            "An upper bound on the total l1_gas used in a block.",
            ParamPrivacyInput::Public,
        )]);
        dump.append(&mut BTreeMap::from([ser_param(
            "message_segment_length",
            &self.message_segment_length,
            "An upper bound on the message segment length in a block.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "n_events",
            &self.n_events,
            "An upper bound on the total number of events generated in a block.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "state_diff_size",
            &self.state_diff_size,
            "An upper bound on the total state diff size in a block.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "sierra_gas",
            &self.sierra_gas,
            "An upper bound on the total sierra_gas used in a block.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "n_txs",
            &self.n_txs,
            "An upper bound on the total number of transactions in a block.",
            ParamPrivacyInput::Public,
        )]));
        dump
    }
}

impl std::fmt::Display for BouncerWeights {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BouncerWeights {{ l1_gas: {}, message_segment_length: {}, n_events: {}, \
             state_diff_size: {}, sierra_gas: {}, n_txs: {} }}",
            self.l1_gas,
            self.message_segment_length,
            self.n_events,
            self.state_diff_size,
            self.sierra_gas,
            self.n_txs
        )
    }
}

#[derive(Debug, PartialEq, Default, Clone, Deserialize, Serialize)]
pub struct CasmHashComputationData {
    pub class_hash_to_casm_hash_computation_gas: HashMap<ClassHash, GasAmount>,
    pub sierra_gas_without_casm_hash_computation: GasAmount,
    pub class_hashes_for_migration: HashSet<ClassHash>,
}

impl CasmHashComputationData {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn extend(&mut self, other: CasmHashComputationData) {
        self.class_hash_to_casm_hash_computation_gas
            .extend(other.class_hash_to_casm_hash_computation_gas);
        self.sierra_gas_without_casm_hash_computation = safe_add_gas_panic_on_overflow(
            self.sierra_gas_without_casm_hash_computation,
            other.sierra_gas_without_casm_hash_computation,
        )
    }
}

pub struct TxWeights {
    pub bouncer_weights: BouncerWeights,
    pub casm_hash_computation_data: CasmHashComputationData,
}

#[derive(Debug, PartialEq)]
#[cfg_attr(test, derive(Clone))]
pub struct Bouncer {
    // Additional info; maintained and used to calculate the residual contribution of a transaction
    // to the accumulated weights.
    pub visited_storage_entries: HashSet<StorageEntry>,
    pub state_changes_keys: StateChangesKeys,
    pub casm_hash_computation_data: CasmHashComputationData,

    pub bouncer_config: BouncerConfig,
    accumulated_weights: BouncerWeights,
}

impl Bouncer {
    pub fn new(bouncer_config: BouncerConfig) -> Self {
        Bouncer { bouncer_config, ..Self::empty() }
    }

    pub fn empty() -> Self {
        Bouncer {
            visited_storage_entries: HashSet::default(),
            state_changes_keys: StateChangesKeys::default(),
            bouncer_config: BouncerConfig::empty(),
            accumulated_weights: BouncerWeights::empty(),
            casm_hash_computation_data: CasmHashComputationData::empty(),
        }
    }

    pub fn get_accumulated_weights(&self) -> &BouncerWeights {
        &self.accumulated_weights
    }

    pub fn get_executed_class_hashes(&self) -> HashSet<ClassHash> {
        self.casm_hash_computation_data
            .class_hash_to_casm_hash_computation_gas
            .keys()
            .cloned()
            .collect()
    }

    /// Updates the bouncer with a new transaction.
    #[allow(clippy::result_large_err)]
    pub fn try_update<S: StateReader>(
        &mut self,
        state_reader: &S,
        tx_state_changes_keys: &StateChangesKeys,
        tx_execution_summary: &ExecutionSummary,
        tx_resources: &TransactionResources,
        versioned_constants: &VersionedConstants,
    ) -> TransactionExecutorResult<()> {
        // The countings here should be linear in the transactional state changes and execution info
        // rather than the cumulative state attributes.
        let marginal_state_changes_keys =
            tx_state_changes_keys.difference(&self.state_changes_keys);
        let marginal_executed_class_hashes = tx_execution_summary
            .executed_class_hashes
            .difference(&self.get_executed_class_hashes())
            .cloned()
            .collect();
        let n_marginal_visited_storage_entries = tx_execution_summary
            .visited_storage_entries
            .difference(&self.visited_storage_entries)
            .count();
        let tx_weights = get_tx_weights(
            state_reader,
            &marginal_executed_class_hashes,
            n_marginal_visited_storage_entries,
            tx_resources,
            &marginal_state_changes_keys,
            versioned_constants,
        )?;
        let tx_bouncer_weights = tx_weights.bouncer_weights;

        // Check if the transaction can fit the current block available capacity.
        let err_msg = format!(
            "Addition overflow. Transaction weights: {tx_bouncer_weights:?}, block weights: {:?}.",
            self.accumulated_weights
        );
        if !self
            .bouncer_config
            .has_room(self.accumulated_weights.checked_add(tx_bouncer_weights).expect(&err_msg))
        {
            log::debug!(
                "Transaction cannot be added to the current block, block capacity reached; \
                 transaction weights: {:?}, block weights: {:?}.",
                tx_weights.bouncer_weights,
                self.accumulated_weights
            );
            Err(TransactionExecutorError::BlockFull)?
        }

        self.update(tx_weights, tx_execution_summary, &marginal_state_changes_keys);

        Ok(())
    }

    fn update(
        &mut self,
        tx_weights: TxWeights,
        tx_execution_summary: &ExecutionSummary,
        state_changes_keys: &StateChangesKeys,
    ) {
        let bouncer_weights = &tx_weights.bouncer_weights;
        let err_msg = format!(
            "Addition overflow. Transaction weights: {bouncer_weights:?}, block weights: {:?}.",
            self.accumulated_weights
        );
        self.accumulated_weights =
            self.accumulated_weights.checked_add(tx_weights.bouncer_weights).expect(&err_msg);
        self.casm_hash_computation_data.extend(tx_weights.casm_hash_computation_data);
        self.visited_storage_entries.extend(&tx_execution_summary.visited_storage_entries);
        // Note: cancelling writes (0 -> 1 -> 0) will not be removed, but it's fine since fee was
        // charged for them.
        self.state_changes_keys.extend(state_changes_keys);
    }

    #[cfg(test)]
    pub fn set_accumulated_weights(&mut self, weights: BouncerWeights) {
        self.accumulated_weights = weights;
    }
}

fn n_steps_to_sierra_gas(n_steps: usize, versioned_constants: &VersionedConstants) -> GasAmount {
    let n_steps_u64 = u64_from_usize(n_steps);
    let gas_per_step = versioned_constants.os_constants.gas_costs.base.step_gas_cost;
    let n_steps_gas_cost = n_steps_u64.checked_mul(gas_per_step).unwrap_or_else(|| {
        panic!(
            "Multiplication overflow while converting steps to gas. steps: {}, gas per step: {}.",
            n_steps, gas_per_step
        )
    });
    GasAmount(n_steps_gas_cost)
}

fn vm_resources_to_sierra_gas(
    resources: ExecutionResources,
    versioned_constants: &VersionedConstants,
) -> GasAmount {
    let builtins_gas_cost =
        builtins_to_sierra_gas(&resources.prover_builtins(), versioned_constants);
    let n_steps_gas_cost = n_steps_to_sierra_gas(resources.total_n_steps(), versioned_constants);
    n_steps_gas_cost.checked_add(builtins_gas_cost).unwrap_or_else(|| {
        panic!(
            "Addition overflow while converting vm resources to gas. steps gas: {}, builtins gas: \
             {}.",
            n_steps_gas_cost, builtins_gas_cost
        )
    })
}

pub fn builtins_to_sierra_gas(
    builtin_counts: &BuiltinCounterMap,
    versioned_constants: &VersionedConstants,
) -> GasAmount {
    let gas_costs = &versioned_constants.os_constants.gas_costs.builtins;

    let total_gas = builtin_counts
        .iter()
        .try_fold(0u64, |accumulated_gas, (&builtin, &count)| {
            let builtin_gas_cost = gas_costs
                .get_builtin_gas_cost(&builtin)
                .unwrap_or_else(|err| panic!("Failed to get gas cost: {}", err));
            let builtin_count_u64 = u64_from_usize(count);
            let builtin_total_cost = builtin_count_u64.checked_mul(builtin_gas_cost)?;
            accumulated_gas.checked_add(builtin_total_cost)
        })
        .unwrap_or_else(|| {
            panic!(
                "Overflow occurred while converting built-in resources to gas. Builtins: {:?}",
                builtin_counts
            )
        });

    GasAmount(total_gas)
}

#[allow(clippy::result_large_err)]
pub fn get_tx_weights<S: StateReader>(
    state_reader: &S,
    executed_class_hashes: &HashSet<ClassHash>,
    n_visited_storage_entries: usize,
    tx_resources: &TransactionResources,
    state_changes_keys: &StateChangesKeys,
    versioned_constants: &VersionedConstants,
) -> TransactionExecutionResult<TxWeights> {
    let message_resources = &tx_resources.starknet_resources.messages;
    let message_starknet_l1gas = usize_from_u64(message_resources.get_starknet_gas_cost().l1_gas.0)
        .expect("This conversion should not fail as the value is a converted usize.");

    let patrticia_update_resources = get_particia_update_resources(n_visited_storage_entries);

    let vm_resources = &patrticia_update_resources + &tx_resources.computation.vm_resources;
    let vm_resources_gas = vm_resources_to_sierra_gas(vm_resources, versioned_constants);
    let sierra_gas = tx_resources.computation.sierra_gas;
    let sierra_gas_without_casm_hash_computation =
        safe_add_gas_panic_on_overflow(sierra_gas, vm_resources_gas);

    let (class_hashes_for_migration, migration_gas) =
        get_migration_resources(state_reader, executed_class_hashes);
    let class_hash_to_casm_hash_computation_resources =
        map_class_hash_to_casm_hash_computation_resources(state_reader, executed_class_hashes)?;
    let class_hash_to_casm_hash_computation_gas: HashMap<_, _> =
        class_hash_to_casm_hash_computation_resources
            .into_iter()
            .map(|(class_hash, resources)| {
                let gas = vm_resources_to_sierra_gas(resources, versioned_constants);
                (class_hash, gas)
            })
            .collect();

    let total_casm_hash_computation_gas = class_hash_to_casm_hash_computation_gas
        .values()
        .fold(migration_gas, |acc, gas| safe_add_gas_panic_on_overflow(acc, *gas));

    let total_sierra_gas = safe_add_gas_panic_on_overflow(
        sierra_gas_without_casm_hash_computation,
        total_casm_hash_computation_gas,
    );

    let casm_hash_computation_data = CasmHashComputationData {
        class_hash_to_casm_hash_computation_gas,
        sierra_gas_without_casm_hash_computation,
        class_hashes_for_migration,
    };

    let bouncer_weights = BouncerWeights {
        l1_gas: message_starknet_l1gas,
        message_segment_length: message_resources.message_segment_length,
        n_events: tx_resources.starknet_resources.archival_data.event_summary.n_events,
        state_diff_size: get_onchain_data_segment_length(&state_changes_keys.count()),
        sierra_gas: total_sierra_gas,
        n_txs: 1,
    };

    Ok(TxWeights { bouncer_weights, casm_hash_computation_data })
}

/// Returns a mapping from each class hash to its estimated Cairo resources for Casm hash
/// computation (done by the OS).
#[allow(clippy::result_large_err)]
pub fn map_class_hash_to_casm_hash_computation_resources<S: StateReader>(
    state_reader: &S,
    executed_class_hashes: &HashSet<ClassHash>,
) -> TransactionExecutionResult<HashMap<ClassHash, ExecutionResources>> {
    executed_class_hashes
        .iter()
        .map(|class_hash| {
            let class = state_reader.get_compiled_class(*class_hash)?;
            Ok((*class_hash, class.estimate_casm_hash_computation_resources()))
        })
        .collect()
}

/// Returns the estimated Cairo resources for Patricia tree updates, or hash invocations
/// (done by the OS), required for accessing (read/write) the given storage entries.
// For each tree: n_visited_leaves * log(n_initialized_leaves)
// as the height of a Patricia tree with N uniformly distributed leaves is ~log(N),
// and number of visited leaves includes reads and writes.
pub fn get_particia_update_resources(n_visited_storage_entries: usize) -> ExecutionResources {
    const TREE_HEIGHT_UPPER_BOUND: usize = 24;
    let n_updates = n_visited_storage_entries * TREE_HEIGHT_UPPER_BOUND;

    ExecutionResources {
        // TODO(Yoni, 1/5/2024): re-estimate this.
        n_steps: 32 * n_updates,
        // For each Patricia update there are two hash calculations.
        builtin_instance_counter: HashMap::from([(BuiltinName::pedersen, 2 * n_updates)]),
        n_memory_holes: 0,
    }
}

#[allow(clippy::result_large_err)]
pub fn verify_tx_weights_within_max_capacity<S: StateReader>(
    state_reader: &S,
    tx_execution_summary: &ExecutionSummary,
    tx_resources: &TransactionResources,
    tx_state_changes_keys: &StateChangesKeys,
    bouncer_config: &BouncerConfig,
    versioned_constants: &VersionedConstants,
) -> TransactionExecutionResult<()> {
    let tx_weights = get_tx_weights(
        state_reader,
        &tx_execution_summary.executed_class_hashes,
        tx_execution_summary.visited_storage_entries.len(),
        tx_resources,
        tx_state_changes_keys,
        versioned_constants,
    )?
    .bouncer_weights;

    bouncer_config.within_max_capacity_or_err(tx_weights)
}

fn get_migration_resources<S: StateReader>(
    state_reader: &S,
    executed_class_hashes: &HashSet<ClassHash>,
) -> (HashSet<ClassHash>, GasAmount) {
    executed_class_hashes
        .iter()
        .filter(|&class_hash_ref| should_migrate(state_reader, *class_hash_ref))
        .map(|class_hash| {
            let class = state_reader.get_compiled_class(*class_hash).expect("Failed to get class");
            (*class_hash, class.estimate_compiled_class_hash_migration_resources())
        })
        .fold((HashSet::new(), GasAmount::ZERO), |(mut hashes, gas), (hash, new_gas)| {
            hashes.insert(hash);
            (hashes, safe_add_gas_panic_on_overflow(gas, new_gas))
        })
}
