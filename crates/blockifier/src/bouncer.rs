use std::collections::{BTreeMap, HashMap, HashSet};

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use serde::{Deserialize, Serialize};
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_api::core::ClassHash;
use starknet_api::execution_resources::GasAmount;

use crate::blockifier::transaction_executor::{
    CompiledClassHashV2ToV1,
    TransactionExecutorError,
    TransactionExecutorResult,
};
use crate::blockifier_versioned_constants::{BuiltinGasCosts, VersionedConstants};
use crate::execution::call_info::{BuiltinCounterMap, ExecutionSummary};
use crate::execution::casm_hash_estimation::EstimatedExecutionResources;
use crate::fee::gas_usage::get_onchain_data_segment_length;
use crate::fee::resources::TransactionResources;
use crate::state::cached_state::{StateChangesKeys, StorageEntry};
use crate::state::state_api::StateReader;
use crate::transaction::errors::TransactionExecutionError;
use crate::transaction::objects::{ExecutionResourcesTraits, TransactionExecutionResult};
use crate::utils::{add_maps, should_migrate, u64_from_usize, usize_from_u64};

#[cfg(test)]
#[path = "bouncer_test.rs"]
mod test;

macro_rules! impl_field_wise_ops {
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

        // Returns a comma-separated string of exceeded fields.
        pub fn get_exceeded_weights(self: Self, other: Self) -> String {
            let mut exceeded = Vec::new();
            $(
                if other.$field > self.$field {
                    exceeded.push(stringify!($field));
                }
            )+
            exceeded.join(", ")
        }
    };
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BouncerConfig {
    pub block_max_capacity: BouncerWeights,
    pub builtin_weights: BuiltinWeights,
    pub blake_weight: usize,
}

impl Default for BouncerConfig {
    fn default() -> Self {
        Self {
            block_max_capacity: BouncerWeights::default(),
            builtin_weights: BuiltinWeights::default(),
            blake_weight: 6320,
        }
    }
}

impl BouncerConfig {
    pub fn empty() -> Self {
        Self {
            block_max_capacity: BouncerWeights::empty(),
            builtin_weights: BuiltinWeights::empty(),
            blake_weight: 0,
        }
    }

    pub fn max() -> Self {
        Self { block_max_capacity: BouncerWeights::max(), ..Default::default() }
    }

    pub fn has_room(&self, weights: BouncerWeights) -> bool {
        self.block_max_capacity.has_room(weights)
    }

    pub fn get_exceeded_weights(&self, weights: BouncerWeights) -> String {
        self.block_max_capacity.get_exceeded_weights(weights)
    }

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
        let mut dump =
            prepend_sub_config_name(self.block_max_capacity.dump(), "block_max_capacity");
        dump.append(&mut prepend_sub_config_name(self.builtin_weights.dump(), "builtin_weights"));
        dump.append(&mut BTreeMap::from([ser_param(
            "blake_weight",
            &self.blake_weight,
            "blake opcode gas weight.",
            ParamPrivacyInput::Public,
        )]));
        dump
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
    // NOTE: Must stay in sync with orchestrator_versioned_constants' max_block_size.
    pub sierra_gas: GasAmount,
    pub n_txs: usize,
    pub proving_gas: GasAmount,
}

impl BouncerWeights {
    impl_field_wise_ops!(
        l1_gas,
        message_segment_length,
        n_events,
        n_txs,
        state_diff_size,
        sierra_gas,
        proving_gas
    );

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
            proving_gas: GasAmount::MAX,
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
            proving_gas: GasAmount::ZERO,
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
            n_txs: 600,
            state_diff_size: 4000,
            // NOTE: Must stay in sync with orchestrator_versioned_constants' max_block_size.
            sierra_gas: GasAmount(6000000000),
            proving_gas: GasAmount(6000000000),
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
        dump.append(&mut BTreeMap::from([ser_param(
            "proving_gas",
            &self.proving_gas,
            "An upper bound on the total builtins and steps gas usage used in a block.",
            ParamPrivacyInput::Public,
        )]));
        dump
    }
}

impl std::fmt::Display for BouncerWeights {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BouncerWeights {{ l1_gas: {}, message_segment_length: {}, n_events: {}, n_txs: {}, \
             state_diff_size: {}, sierra_gas: {}, proving_gas: {} }}",
            self.l1_gas,
            self.message_segment_length,
            self.n_events,
            self.n_txs,
            self.state_diff_size,
            self.sierra_gas,
            self.proving_gas,
        )
    }
}

#[derive(Debug, PartialEq, Default, Clone, Deserialize, Serialize)]
pub struct CasmHashComputationData {
    pub class_hash_to_casm_hash_computation_gas: HashMap<ClassHash, GasAmount>,
    pub gas_without_casm_hash_computation: GasAmount,
}

impl CasmHashComputationData {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn extend(&mut self, other: CasmHashComputationData) {
        self.class_hash_to_casm_hash_computation_gas
            .extend(other.class_hash_to_casm_hash_computation_gas);
        self.gas_without_casm_hash_computation = self
            .gas_without_casm_hash_computation
            .checked_add_panic_on_overflow(other.gas_without_casm_hash_computation)
    }

    /// Creates CasmHashComputationData by mapping resources to gas using a provided function.
    /// This method encapsulates the pattern used for both Sierra gas and proving gas computation.
    pub fn from_resources<F>(
        class_hash_to_resources: &HashMap<ClassHash, EstimatedExecutionResources>,
        gas_without_casm_hash_computation: GasAmount,
        resources_to_gas_fn: F,
    ) -> Self
    where
        F: Fn(&EstimatedExecutionResources) -> GasAmount,
    {
        Self {
            class_hash_to_casm_hash_computation_gas: class_hash_to_resources
                .iter()
                .map(|(&class_hash, resources)| {
                    let gas = resources_to_gas_fn(resources);
                    (class_hash, gas)
                })
                .collect(),
            gas_without_casm_hash_computation,
        }
    }

    pub fn total_gas(&self) -> GasAmount {
        self.class_hash_to_casm_hash_computation_gas
            .values()
            .fold(self.gas_without_casm_hash_computation, |acc, &gas| {
                acc.checked_add_panic_on_overflow(gas)
            })
    }
}

/// Aggregates compiled class hash migration data for executed classes.
///
/// Tracks which classes need migration from V1 to V2 compiled hashes and
/// accumulates the estimated execution resources required to perform the migration.
struct CasmHashMigrationData {
    pub(crate) class_hashes_to_migrate: HashMap<ClassHash, CompiledClassHashV2ToV1>,
    resources: EstimatedExecutionResources,
}

impl CasmHashMigrationData {
    fn empty() -> Self {
        Self {
            class_hashes_to_migrate: HashMap::new(),
            resources: EstimatedExecutionResources::new(HashVersion::V2),
        }
    }

    /// Builds a migration aggregation from the current state.
    /// Returns empty if migration is disabled.
    /// Otherwise, iterates over `executed_class_hashes`, selects classes that should migrate
    /// via `should_migrate`, and accumulates their migration resources.
    fn from_state<S: StateReader>(
        state_reader: &S,
        executed_class_hashes: &HashSet<ClassHash>,
        versioned_constants: &VersionedConstants,
    ) -> TransactionExecutionResult<Self> {
        if !versioned_constants.enable_casm_hash_migration {
            return Ok(Self::empty());
        }

        executed_class_hashes.iter().try_fold(Self::empty(), |mut migration_data, &class_hash| {
            if let Some((class_hash, casm_hash_v2_to_v1)) =
                should_migrate(state_reader, class_hash)?
            {
                // Add class hash mapping to the migration data.
                migration_data.class_hashes_to_migrate.insert(class_hash, casm_hash_v2_to_v1);

                // Accumulate the class's migration resources.
                let class = state_reader.get_compiled_class(class_hash)?;
                migration_data.resources +=
                    &class.estimate_compiled_class_hash_migration_resources();
            }
            Ok(migration_data)
        })
    }

    /// Converts the aggregated migration resources into gas amounts using the provided builtin gas
    /// costs and `blake_opcode_gas`.
    fn to_gas(
        &self,
        builtin_gas_costs: &BuiltinGasCosts,
        versioned_constants: &VersionedConstants,
        blake_opcode_gas: usize,
    ) -> GasAmount {
        self.resources.to_gas(builtin_gas_costs, blake_opcode_gas, versioned_constants)
    }
}

#[derive(Debug, Default, PartialEq)]
#[cfg_attr(test, derive(Clone))]
pub struct TxWeights {
    pub bouncer_weights: BouncerWeights,
    pub casm_hash_computation_data_sierra_gas: CasmHashComputationData,
    pub casm_hash_computation_data_proving_gas: CasmHashComputationData,
    pub class_hashes_to_migrate: HashMap<ClassHash, CompiledClassHashV2ToV1>,
}

impl TxWeights {
    fn empty() -> Self {
        Self {
            bouncer_weights: BouncerWeights::empty(),
            casm_hash_computation_data_sierra_gas: CasmHashComputationData::empty(),
            casm_hash_computation_data_proving_gas: CasmHashComputationData::empty(),
            class_hashes_to_migrate: HashMap::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct BuiltinWeights {
    pub gas_costs: BuiltinGasCosts,
}

impl BuiltinWeights {
    pub fn empty() -> Self {
        Self {
            gas_costs: BuiltinGasCosts {
                pedersen: 0,
                range_check: 0,
                ecdsa: 0,
                bitwise: 0,
                poseidon: 0,
                keccak: 0,
                ecop: 0,
                mul_mod: 0,
                add_mod: 0,
                range_check96: 0,
            },
        }
    }
}

impl Default for BuiltinWeights {
    fn default() -> Self {
        Self {
            gas_costs: BuiltinGasCosts {
                pedersen: 5722,
                range_check: 70,
                ecdsa: 2000000,
                ecop: 857850,
                bitwise: 583,
                keccak: 600000,
                poseidon: 11450,
                add_mod: 360,
                mul_mod: 604,
                range_check96: 56,
            },
        }
    }
}

impl SerializeConfig for BuiltinWeights {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from([ser_param(
            "gas_costs.pedersen",
            &self.gas_costs.pedersen,
            "Pedersen gas weight.",
            ParamPrivacyInput::Public,
        )]);
        dump.append(&mut BTreeMap::from([ser_param(
            "gas_costs.range_check",
            &self.gas_costs.range_check,
            "Range_check gas weight.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "gas_costs.range_check96",
            &self.gas_costs.range_check96,
            "range_check96 gas weight.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "gas_costs.poseidon",
            &self.gas_costs.poseidon,
            "Poseidon gas weight.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "gas_costs.ecdsa",
            &self.gas_costs.ecdsa,
            "Ecdsa gas weight.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "gas_costs.ecop",
            &self.gas_costs.ecop,
            "Ec_op gas weight.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "gas_costs.add_mod",
            &self.gas_costs.add_mod,
            "Add_mod gas weight.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "gas_costs.mul_mod",
            &self.gas_costs.mul_mod,
            "Mul_mod gas weight.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "gas_costs.keccak",
            &self.gas_costs.keccak,
            "Keccak gas weight.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "gas_costs.bitwise",
            &self.gas_costs.bitwise,
            "Bitwise gas weight.",
            ParamPrivacyInput::Public,
        )]));

        dump
    }
}

#[derive(Debug, PartialEq)]
#[cfg_attr(test, derive(Clone))]
pub struct Bouncer {
    // Additional info; maintained and used to calculate the residual contribution of a transaction
    // to the accumulated weights.
    pub visited_storage_entries: HashSet<StorageEntry>,
    pub state_changes_keys: StateChangesKeys,
    pub bouncer_config: BouncerConfig,
    accumulated_weights: TxWeights,
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
            accumulated_weights: TxWeights::empty(),
        }
    }

    pub fn get_bouncer_weights(&self) -> &BouncerWeights {
        &self.accumulated_weights.bouncer_weights
    }

    pub fn get_mut_casm_hash_computation_data_sierra_gas(
        &mut self,
    ) -> &mut CasmHashComputationData {
        &mut self.accumulated_weights.casm_hash_computation_data_sierra_gas
    }

    pub fn get_mut_casm_hash_computation_data_proving_gas(
        &mut self,
    ) -> &mut CasmHashComputationData {
        &mut self.accumulated_weights.casm_hash_computation_data_proving_gas
    }

    pub fn get_mut_class_hashes_to_migrate(
        &mut self,
    ) -> &mut HashMap<ClassHash, CompiledClassHashV2ToV1> {
        &mut self.accumulated_weights.class_hashes_to_migrate
    }

    pub fn get_executed_class_hashes(&self) -> HashSet<ClassHash> {
        self.accumulated_weights
            .casm_hash_computation_data_sierra_gas
            .class_hash_to_casm_hash_computation_gas
            .keys()
            .cloned()
            .collect()
    }

    /// Updates the bouncer with a new transaction.
    pub fn try_update<S: StateReader>(
        &mut self,
        state_reader: &S,
        tx_state_changes_keys: &StateChangesKeys,
        tx_execution_summary: &ExecutionSummary,
        tx_builtin_counters: &BuiltinCounterMap,
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
            tx_builtin_counters,
            &self.bouncer_config,
        )?;

        let tx_bouncer_weights = tx_weights.bouncer_weights;

        // Check if the transaction can fit the current block available capacity.
        let err_msg = format!(
            "Addition overflow. Transaction weights: {tx_bouncer_weights:?}, block weights: {:?}.",
            self.get_bouncer_weights()
        );
        let next_accumulated_weights =
            self.get_bouncer_weights().checked_add(tx_bouncer_weights).expect(&err_msg);
        if !self.bouncer_config.has_room(next_accumulated_weights) {
            log::debug!(
                "Transaction cannot be added to the current block, block capacity reached; \
                 transaction weights: {:?}, block weights: {:?}. Block max capacity reached on \
                 fields: {}",
                tx_weights.bouncer_weights,
                self.get_bouncer_weights(),
                self.bouncer_config.get_exceeded_weights(next_accumulated_weights)
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
            self.get_bouncer_weights()
        );
        self.accumulated_weights.bouncer_weights = self
            .accumulated_weights
            .bouncer_weights
            .checked_add(tx_weights.bouncer_weights)
            .expect(&err_msg);
        self.accumulated_weights
            .casm_hash_computation_data_sierra_gas
            .extend(tx_weights.casm_hash_computation_data_sierra_gas);
        self.accumulated_weights
            .casm_hash_computation_data_proving_gas
            .extend(tx_weights.casm_hash_computation_data_proving_gas);
        self.visited_storage_entries.extend(&tx_execution_summary.visited_storage_entries);
        // Note: cancelling writes (0 -> 1 -> 0) will not be removed, but it's fine since fee was
        // charged for them.
        self.state_changes_keys.extend(state_changes_keys);
        self.accumulated_weights.class_hashes_to_migrate.extend(tx_weights.class_hashes_to_migrate);
    }

    #[cfg(test)]
    pub fn set_bouncer_weights(&mut self, weights: BouncerWeights) {
        self.accumulated_weights.bouncer_weights = weights;
    }
}

/// Converts 'amount' of resource units into Sierra gas, given a per-unit rate.
fn vm_resource_to_gas_amount(amount: usize, gas_per_unit: u64, name: &str) -> GasAmount {
    let amount_u64 = u64_from_usize(amount);
    let gas = amount_u64.checked_mul(gas_per_unit).unwrap_or_else(|| {
        panic!(
            "Multiplication overflow converting {name} to gas. units: {amount_u64}, gas per unit: \
             {gas_per_unit}."
        )
    });

    GasAmount(gas)
}

fn n_steps_to_gas(n_steps: usize, versioned_constants: &VersionedConstants) -> GasAmount {
    let gas_per_step = versioned_constants.os_constants.gas_costs.base.step_gas_cost;
    vm_resource_to_gas_amount(n_steps, gas_per_step, "steps")
}

fn memory_holes_to_gas(
    n_memory_holes: usize,
    versioned_constants: &VersionedConstants,
) -> GasAmount {
    let gas_per_memory_hole = versioned_constants.os_constants.gas_costs.base.memory_hole_gas_cost;
    vm_resource_to_gas_amount(n_memory_holes, gas_per_memory_hole, "memory_holes")
}

/// Calculates proving gas from builtin counters and Sierra gas.
fn proving_gas_from_builtins_and_sierra_gas(
    sierra_gas: GasAmount,
    builtin_counters: &BuiltinCounterMap,
    proving_builtin_gas_costs: &BuiltinGasCosts,
    sierra_builtin_gas_costs: &BuiltinGasCosts,
) -> GasAmount {
    let builtins_proving_gas = builtins_to_gas(builtin_counters, proving_builtin_gas_costs);
    let steps_proving_gas =
        sierra_gas_to_steps_gas(sierra_gas, builtin_counters, sierra_builtin_gas_costs);

    steps_proving_gas.checked_add_panic_on_overflow(builtins_proving_gas)
}

/// Generic function to convert VM resources to gas with configurable builtin gas calculation
pub fn vm_resources_to_gas(
    resources: &ExecutionResources,
    builtin_gas_cost: &BuiltinGasCosts,
    versioned_constants: &VersionedConstants,
) -> GasAmount {
    let builtins_gas_cost = builtins_to_gas(&resources.prover_builtins(), builtin_gas_cost);
    let n_steps_gas_cost = n_steps_to_gas(resources.total_n_steps(), versioned_constants);
    let n_memory_holes_gas_cost =
        memory_holes_to_gas(resources.n_memory_holes, versioned_constants);

    n_steps_gas_cost
        .checked_add_panic_on_overflow(n_memory_holes_gas_cost)
        .checked_add_panic_on_overflow(builtins_gas_cost)
}

/// Computes the steps gas by subtracting the builtins' contribution from the Sierra gas.
pub fn sierra_gas_to_steps_gas(
    sierra_gas: GasAmount,
    builtin_counters: &BuiltinCounterMap,
    sierra_builtin_gas_costs: &BuiltinGasCosts,
) -> GasAmount {
    let builtins_gas_cost = builtins_to_gas(builtin_counters, sierra_builtin_gas_costs);

    sierra_gas.checked_sub(builtins_gas_cost).unwrap_or_else(|| {
        log::debug!(
            "Sierra gas underflow: builtins gas exceeds total. Sierra gas: {sierra_gas:?}, \
             Builtins gas: {builtins_gas_cost:?}, Builtins: {builtin_counters:?}"
        );
        GasAmount::ZERO
    })
}

pub fn builtins_to_gas(
    builtin_counters: &BuiltinCounterMap,
    builtin_gas_costs: &BuiltinGasCosts,
) -> GasAmount {
    let builtin_gas = builtin_counters.iter().fold(0u64, |accumulated_gas, (name, &count)| {
        let builtin_weight = builtin_gas_costs.get_builtin_gas_cost(name).unwrap();
        builtin_weight
            .checked_mul(u64_from_usize(count))
            .and_then(|builtin_gas| accumulated_gas.checked_add(builtin_gas))
            .unwrap_or_else(|| {
                panic!(
                    "Overflow while converting builtin counters to gas.\nBuiltin: {name}, Weight: \
                     {builtin_weight}, Count: {count}, Accumulated gas: {accumulated_gas}"
                )
            })
    });

    GasAmount(builtin_gas)
}

fn add_casm_hash_computation_gas_cost(
    class_hash_to_casm_hash_computation_resources: &HashMap<ClassHash, EstimatedExecutionResources>,
    gas_without_casm_hash_computation: GasAmount,
    builtin_gas_cost: &BuiltinGasCosts,
    versioned_constants: &VersionedConstants,
    blake_opcode_gas: usize,
) -> (GasAmount, CasmHashComputationData) {
    let casm_hash_computation_data_gas = CasmHashComputationData::from_resources(
        class_hash_to_casm_hash_computation_resources,
        gas_without_casm_hash_computation,
        |resources| resources.to_gas(builtin_gas_cost, blake_opcode_gas, versioned_constants),
    );
    (casm_hash_computation_data_gas.total_gas(), casm_hash_computation_data_gas)
}

fn compute_sierra_gas(
    vm_resources: &ExecutionResources,
    sierra_builtin_gas_costs: &BuiltinGasCosts,
    versioned_constants: &VersionedConstants,
    tx_resources: &TransactionResources,
    migration_gas: GasAmount,
    class_hash_to_casm_hash_computation_resources: &HashMap<ClassHash, EstimatedExecutionResources>,
    blake_opcode_gas: usize,
) -> (GasAmount, CasmHashComputationData, GasAmount) {
    let mut vm_resources_sierra_gas =
        vm_resources_to_gas(vm_resources, sierra_builtin_gas_costs, versioned_constants);
    let sierra_gas = tx_resources.computation.sierra_gas;

    vm_resources_sierra_gas = vm_resources_sierra_gas.checked_add_panic_on_overflow(sierra_gas);

    let sierra_gas_without_casm_hash_computation =
        vm_resources_sierra_gas.checked_add_panic_on_overflow(migration_gas);

    let (total_sierra_gas, casm_hash_computation_data_sierra_gas) =
        add_casm_hash_computation_gas_cost(
            class_hash_to_casm_hash_computation_resources,
            sierra_gas_without_casm_hash_computation,
            sierra_builtin_gas_costs,
            versioned_constants,
            // Sierra gas represents `stone` proving costs. However, a Blake opcode cannot be
            // executed in `stone`, (i.e. this version is not supported by `stone`). For
            // simplicity, the Blake `stwo` cost is used for the sierra gas estimation.
            blake_opcode_gas,
        );
    (total_sierra_gas, casm_hash_computation_data_sierra_gas, vm_resources_sierra_gas)
}

#[allow(clippy::too_many_arguments)]
fn compute_proving_gas(
    builtin_counters: &BuiltinCounterMap,
    vm_resources_sierra_gas: GasAmount,
    versioned_constants: &VersionedConstants,
    proving_builtin_gas_costs: &BuiltinGasCosts,
    sierra_builtin_gas_costs: &BuiltinGasCosts,
    migration_gas: GasAmount,
    class_hash_to_casm_hash_computation_resources: &HashMap<ClassHash, EstimatedExecutionResources>,
    blake_opcode_gas: usize,
) -> (GasAmount, CasmHashComputationData) {
    let vm_resources_proving_gas = proving_gas_from_builtins_and_sierra_gas(
        vm_resources_sierra_gas,
        builtin_counters,
        proving_builtin_gas_costs,
        sierra_builtin_gas_costs,
    );

    let proving_gas_without_casm_hash_computation =
        vm_resources_proving_gas.checked_add_panic_on_overflow(migration_gas);

    add_casm_hash_computation_gas_cost(
        class_hash_to_casm_hash_computation_resources,
        proving_gas_without_casm_hash_computation,
        proving_builtin_gas_costs,
        versioned_constants,
        blake_opcode_gas,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn get_tx_weights<S: StateReader>(
    state_reader: &S,
    executed_class_hashes: &HashSet<ClassHash>,
    n_visited_storage_entries: usize,
    tx_resources: &TransactionResources,
    state_changes_keys: &StateChangesKeys,
    versioned_constants: &VersionedConstants,
    tx_builtin_counters: &BuiltinCounterMap,
    bouncer_config: &BouncerConfig,
) -> TransactionExecutionResult<TxWeights> {
    let message_resources = &tx_resources.starknet_resources.messages;
    let message_starknet_l1gas = usize_from_u64(message_resources.get_starknet_gas_cost().l1_gas.0)
        .expect("This conversion should not fail as the value is a converted usize.");

    // Casm hash resources.
    let class_hash_to_casm_hash_computation_resources =
        map_class_hash_to_casm_hash_computation_resources(state_reader, executed_class_hashes)?;

    // Patricia update + transaction resources.
    let patrticia_update_resources = get_patricia_update_resources(
        n_visited_storage_entries,
        // TODO(Yoni) consider counting here the global contract tree and the aliases as well.
        state_changes_keys.storage_keys.len(),
    );
    let vm_resources = &patrticia_update_resources + &tx_resources.computation.total_vm_resources();

    // Builtin gas costs for stone and for stwo.
    let sierra_builtin_gas_costs = &versioned_constants.os_constants.gas_costs.builtins;
    let proving_builtin_gas_costs = &bouncer_config.builtin_weights.gas_costs;

    // Casm hash migration resources.
    let migration_data = CasmHashMigrationData::from_state(
        state_reader,
        executed_class_hashes,
        versioned_constants,
    )?;
    // Total state changes keys are the sum of marginal state changes keys and the
    // migration state changes.
    let mut total_state_changes_keys = StateChangesKeys {
        compiled_class_hash_keys: migration_data.class_hashes_to_migrate.keys().cloned().collect(),
        ..Default::default()
    };
    total_state_changes_keys.extend(state_changes_keys);

    let blake_opcode_gas = bouncer_config.blake_weight;

    // Migration occurs once per contract and is not included in the CASM hash computation, which
    // is performed every time a contract is loaded.
    let sierra_migration_gas =
        migration_data.to_gas(sierra_builtin_gas_costs, versioned_constants, blake_opcode_gas);
    let proving_migration_gas =
        migration_data.to_gas(proving_builtin_gas_costs, versioned_constants, blake_opcode_gas);

    // Sierra gas computation.
    let (total_sierra_gas, casm_hash_computation_data_sierra_gas, vm_resources_sierra_gas) =
        compute_sierra_gas(
            &vm_resources,
            sierra_builtin_gas_costs,
            versioned_constants,
            tx_resources,
            sierra_migration_gas,
            &class_hash_to_casm_hash_computation_resources,
            blake_opcode_gas,
        );

    // Proving gas computation.
    // Exclude tx_vm_resources to prevent double-counting in tx_builtin_counters.
    let mut vm_resources_builtins_for_proving_gas_computation =
        (&patrticia_update_resources + &tx_resources.computation.os_vm_resources).prover_builtins();
    // Use tx_builtin_counters to count the Sierra gas executed entry points as well.
    add_maps(&mut vm_resources_builtins_for_proving_gas_computation, tx_builtin_counters);

    let (total_proving_gas, casm_hash_computation_data_proving_gas) = compute_proving_gas(
        &vm_resources_builtins_for_proving_gas_computation,
        vm_resources_sierra_gas,
        versioned_constants,
        proving_builtin_gas_costs,
        sierra_builtin_gas_costs,
        proving_migration_gas,
        &class_hash_to_casm_hash_computation_resources,
        blake_opcode_gas,
    );

    let bouncer_weights = BouncerWeights {
        l1_gas: message_starknet_l1gas,
        message_segment_length: message_resources.message_segment_length,
        n_events: tx_resources.starknet_resources.archival_data.event_summary.n_events,
        state_diff_size: get_onchain_data_segment_length(&total_state_changes_keys.count()),
        sierra_gas: total_sierra_gas,
        n_txs: 1,
        proving_gas: total_proving_gas,
    };

    Ok(TxWeights {
        bouncer_weights,
        casm_hash_computation_data_sierra_gas,
        casm_hash_computation_data_proving_gas,
        class_hashes_to_migrate: migration_data.class_hashes_to_migrate,
    })
}

/// Returns a mapping from each class hash to its estimated Cairo resources for Casm hash
/// computation (done by the OS).
pub fn map_class_hash_to_casm_hash_computation_resources<S: StateReader>(
    state_reader: &S,
    executed_class_hashes: &HashSet<ClassHash>,
) -> TransactionExecutionResult<HashMap<ClassHash, EstimatedExecutionResources>> {
    executed_class_hashes
        .iter()
        .map(|class_hash| {
            let class = state_reader.get_compiled_class(*class_hash)?;
            Ok((*class_hash, class.estimate_casm_hash_computation_resources()))
        })
        .collect()
}

/// Returns the estimated Cairo resources for Patricia tree updates given the accessed and
/// modified storage entries.
///
/// Each access (read or write) requires a traversal of the previous tree, and a write access
/// requires an additional traversal of the new tree.
///
/// Note:
///   1. n_visited_storage_entries includes both read and write accesses, and may overlap with
///      n_modified_storage_entries (if the first access to a cell was write) and my not (if a
///      cell was read by a previous transaction and is now modified).
///   2. In practice, the OS performs a multi-update, which is more efficient than performing
///      separate updates. However, we use this conservative estimate for simplicity.
pub fn get_patricia_update_resources(
    n_visited_storage_entries: usize,
    n_modified_storage_entries: usize,
) -> ExecutionResources {
    // The height of a Patricia tree with N uniformly distributed leaves is ~log(N).
    const TREE_HEIGHT_UPPER_BOUND: usize = 24;
    // TODO(Yoni, 1/5/2024): re-estimate this.
    const STEPS_IN_TREE_PER_HEIGHT: usize = 16;
    const PEDERSENS_PER_HEIGHT: usize = 1;

    let resources_per_tree_access = ExecutionResources {
        n_steps: TREE_HEIGHT_UPPER_BOUND * STEPS_IN_TREE_PER_HEIGHT,
        builtin_instance_counter: HashMap::from([(
            BuiltinName::pedersen,
            TREE_HEIGHT_UPPER_BOUND * PEDERSENS_PER_HEIGHT,
        )]),
        n_memory_holes: 0,
    };

    // One traversal per access (read or write), and an additional one per write access.
    &resources_per_tree_access * (n_visited_storage_entries + n_modified_storage_entries)
}

pub fn verify_tx_weights_within_max_capacity<S: StateReader>(
    state_reader: &S,
    tx_execution_summary: &ExecutionSummary,
    tx_builtin_counters: &BuiltinCounterMap,
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
        tx_builtin_counters,
        bouncer_config,
    )?
    .bouncer_weights;

    bouncer_config.within_max_capacity_or_err(tx_weights)
}
