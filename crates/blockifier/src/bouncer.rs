use std::collections::{BTreeMap, HashMap, HashSet};
use std::num::NonZeroU64;

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use serde::{Deserialize, Serialize};
use starknet_api::core::ClassHash;
use starknet_api::execution_resources::GasAmount;

use crate::blockifier::transaction_executor::{
    CompiledClassHashV2ToV1,
    TransactionExecutorError,
    TransactionExecutorResult,
};
use crate::blockifier_versioned_constants::{BuiltinGasCosts, VersionedConstants};
use crate::execution::call_info::{
    cairo_primitive_counter_map,
    BuiltinCounterMap,
    CairoPrimitiveCounterMap,
    ExecutionSummary,
    ExtendedExecutionResources,
};
use crate::fee::gas_usage::get_onchain_data_segment_length;
use crate::fee::resources::TransactionResources;
use crate::metrics::record_exceeded_bouncer_resources;
use crate::state::cached_state::{StateChangesKeys, StorageEntry};
use crate::state::state_api::StateReader;
use crate::transaction::errors::TransactionExecutionError;
use crate::transaction::objects::{ExecutionResourcesTraits, TransactionExecutionResult};
use crate::utils::{add_maps, should_migrate, u64_from_usize, usize_from_u64};

#[cfg(test)]
#[path = "bouncer_test.rs"]
mod test;

macro_rules! impl_field_wise_ops {
    ($type:ty, $($field:ident),+) => {
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

        const fn field_names() -> &'static [&'static str] {
            &[$(stringify!($field)),+]
        }
    };
}

macro_rules! impl_variant_names_from_field_names {
    ($type:ty) => {
        impl strum::VariantNames for $type {
            const VARIANTS: &'static [&'static str] = <$type>::field_names();
        }
    };
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct BouncerConfig {
    pub block_max_capacity: BouncerWeights,
    pub builtin_instance_limits: BuiltinInstanceLimits,
}

impl BouncerConfig {
    pub fn empty() -> Self {
        Self {
            block_max_capacity: BouncerWeights::empty(),
            builtin_instance_limits: BuiltinInstanceLimits::default(),
        }
    }

    pub fn max() -> Self {
        // Keep proving_gas at the default value so the induced builtin gas costs stay in a
        // sane range; other capacities are MAX so block-full never triggers in tests.
        Self {
            block_max_capacity: BouncerWeights {
                proving_gas: BouncerWeights::default().proving_gas,
                ..BouncerWeights::max()
            },
            builtin_instance_limits: BuiltinInstanceLimits::default(),
        }
    }

    /// Per-Cairo-primitive proving-gas costs derived from the configured instance limits and
    /// the block-wide proving-gas budget.
    pub fn builtin_gas_costs(&self) -> BuiltinGasCosts {
        self.builtin_instance_limits.induced_gas_costs(self.block_max_capacity.proving_gas)
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
    pub proving_gas: GasAmount,
    /// Receipt-based L2 gas, including execution gas + state allocation costs + DA costs.
    /// Used to close blocks on the economic gas metric. Diverges from sierra_gas because
    /// it includes allocation_cost for new storage keys and other non-execution costs.
    // NOTE: Must stay in sync with orchestrator_versioned_constants' max_block_size.
    pub receipt_l2_gas: GasAmount,
}

impl_variant_names_from_field_names!(BouncerWeights);

impl BouncerWeights {
    impl_field_wise_ops!(
        BouncerWeights,
        l1_gas,
        message_segment_length,
        n_events,
        n_txs,
        state_diff_size,
        sierra_gas,
        proving_gas,
        receipt_l2_gas
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
            receipt_l2_gas: GasAmount::MAX,
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
            receipt_l2_gas: GasAmount::ZERO,
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
            sierra_gas: GasAmount(5000000000),
            proving_gas: GasAmount(5000000000),
            // NOTE: Must stay in sync with orchestrator_versioned_constants' max_block_size.
            receipt_l2_gas: GasAmount(5800000000),
        }
    }
}

impl std::fmt::Display for BouncerWeights {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BouncerWeights {{ l1_gas: {}, message_segment_length: {}, n_events: {}, n_txs: {}, \
             state_diff_size: {}, sierra_gas: {}, proving_gas: {}, receipt_l2_gas: {} }}",
            self.l1_gas,
            self.message_segment_length,
            self.n_events,
            self.n_txs,
            self.state_diff_size,
            self.sierra_gas,
            self.proving_gas,
            self.receipt_l2_gas,
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
        class_hash_to_resources: &HashMap<ClassHash, ExtendedExecutionResources>,
        gas_without_casm_hash_computation: GasAmount,
        resources_to_gas_fn: F,
    ) -> Self
    where
        F: Fn(&ExtendedExecutionResources) -> GasAmount,
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
    resources: ExtendedExecutionResources,
}

impl CasmHashMigrationData {
    fn empty() -> Self {
        Self {
            class_hashes_to_migrate: HashMap::new(),
            resources: ExtendedExecutionResources::default(),
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
    /// costs.
    fn to_gas(
        &self,
        builtin_gas_costs: &BuiltinGasCosts,
        versioned_constants: &VersionedConstants,
    ) -> GasAmount {
        extended_execution_resources_to_gas(&self.resources, builtin_gas_costs, versioned_constants)
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

/// Per-block upper bounds on the number of instances of each Cairo primitive (builtin or
/// opcode). The bouncer's per-primitive proving-gas cost is derived from these limits and the
/// block-wide proving-gas budget via `induced_gas_costs`. Limits are `NonZeroU64` so zero
/// values are rejected at deserialization (and at construction in tests) instead of being
/// caught by a runtime assertion in the derivation.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct BuiltinInstanceLimits {
    pub pedersen: NonZeroU64,
    pub range_check: NonZeroU64,
    pub range_check96: NonZeroU64,
    pub poseidon: NonZeroU64,
    pub ecdsa: NonZeroU64,
    pub ecop: NonZeroU64,
    pub bitwise: NonZeroU64,
    pub keccak: NonZeroU64,
    pub add_mod: NonZeroU64,
    pub mul_mod: NonZeroU64,
    pub blake: NonZeroU64,
}

impl BuiltinInstanceLimits {
    /// Induces the per-instance proving-gas cost of each Cairo primitive from its per-block
    /// instance limit: `cost = floor(proving_gas / limit)`.
    pub fn induced_gas_costs(&self, proving_gas: GasAmount) -> BuiltinGasCosts {
        let derive = |limit: NonZeroU64| -> u64 { proving_gas.0 / limit.get() };
        BuiltinGasCosts {
            pedersen: derive(self.pedersen),
            range_check: derive(self.range_check),
            range_check96: derive(self.range_check96),
            poseidon: derive(self.poseidon),
            ecdsa: derive(self.ecdsa),
            ecop: derive(self.ecop),
            bitwise: derive(self.bitwise),
            keccak: derive(self.keccak),
            add_mod: derive(self.add_mod),
            mul_mod: derive(self.mul_mod),
            blake: derive(self.blake),
        }
    }
}

impl Default for BuiltinInstanceLimits {
    fn default() -> Self {
        let nz = |n: u64| NonZeroU64::new(n).expect("BuiltinInstanceLimits default must be > 0");
        Self {
            pedersen: nz(2_000_000),
            range_check: nz(66_666_666),
            range_check96: nz(33_519_553),
            poseidon: nz(600_000),
            ecdsa: nz(3_000),
            ecop: nz(130_000),
            bitwise: nz(10_500_000),
            keccak: nz(10_000),
            add_mod: nz(3_000_000),
            mul_mod: nz(3_000_000),
            blake: nz(1_800_000),
        }
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
    // TODO(Dan): refactor to reduce the number of arguments.
    #[allow(clippy::too_many_arguments)]
    pub fn try_update<S: StateReader>(
        &mut self,
        state_reader: &S,
        tx_state_changes_keys: &StateChangesKeys,
        tx_execution_summary: &ExecutionSummary,
        tx_builtin_counters: &CairoPrimitiveCounterMap,
        tx_resources: &TransactionResources,
        versioned_constants: &VersionedConstants,
        receipt_l2_gas: GasAmount,
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
            receipt_l2_gas,
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
            let exceeded_weights =
                self.bouncer_config.get_exceeded_weights(next_accumulated_weights);
            log::debug!(
                "Transaction cannot be added to the current block, block capacity reached; \
                 transaction weights: {:?}, block weights: {:?}. Block max capacity reached on \
                 fields: {}",
                tx_weights.bouncer_weights,
                self.get_bouncer_weights(),
                exceeded_weights
            );
            record_exceeded_bouncer_resources(&exceeded_weights);
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
        // Also, `get_patricia_update_resources` relies on this property - each cell must
        // be counted at most once as modified.
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
fn proving_gas_from_cairo_primitives_and_sierra_gas(
    sierra_gas: GasAmount,
    cairo_primitives_counters: &CairoPrimitiveCounterMap,
    proving_builtin_gas_costs: &BuiltinGasCosts,
    sierra_builtin_gas_costs: &BuiltinGasCosts,
) -> GasAmount {
    let cairo_primitives_proving_gas =
        cairo_primitives_to_gas(cairo_primitives_counters, proving_builtin_gas_costs);
    let steps_proving_gas =
        sierra_gas_to_steps_gas(sierra_gas, cairo_primitives_counters, sierra_builtin_gas_costs);

    steps_proving_gas.checked_add_panic_on_overflow(cairo_primitives_proving_gas)
}

/// Converts extended execution resources to gas with configurable builtin gas calculation.
pub fn extended_execution_resources_to_gas(
    resources: &ExtendedExecutionResources,
    cairo_primitives_gas_costs: &BuiltinGasCosts,
    versioned_constants: &VersionedConstants,
) -> GasAmount {
    let cairo_primitives_gas_cost =
        cairo_primitives_to_gas(&resources.prover_cairo_primitives(), cairo_primitives_gas_costs);
    let n_steps_gas_cost =
        n_steps_to_gas(resources.vm_resources.total_n_steps(), versioned_constants);
    let n_memory_holes_gas_cost =
        memory_holes_to_gas(resources.vm_resources.n_memory_holes, versioned_constants);

    n_steps_gas_cost
        .checked_add_panic_on_overflow(n_memory_holes_gas_cost)
        .checked_add_panic_on_overflow(cairo_primitives_gas_cost)
}

/// Computes the steps gas by subtracting the builtins' contribution from the Sierra gas.
pub fn sierra_gas_to_steps_gas(
    sierra_gas: GasAmount,
    cairo_primitives_counters: &CairoPrimitiveCounterMap,
    sierra_builtin_gas_costs: &BuiltinGasCosts,
) -> GasAmount {
    let cairo_primitives_gas =
        cairo_primitives_to_gas(cairo_primitives_counters, sierra_builtin_gas_costs);

    sierra_gas.checked_sub(cairo_primitives_gas).unwrap_or_else(|| {
        log::debug!(
            "Sierra gas underflow: cairo primitives gas exceeds total. Sierra gas: \
             {sierra_gas:?}, Cairo primitives gas: {cairo_primitives_gas:?}, Cairo primitives: \
             {cairo_primitives_counters:?}"
        );
        GasAmount::ZERO
    })
}

pub fn cairo_primitives_to_gas(
    cairo_primitives_counters: &CairoPrimitiveCounterMap,
    // NOTE: 'blake' is currently the only supported opcode, by being included in the
    // builtin_gas_costs.
    cairo_primitives_gas_costs: &BuiltinGasCosts,
) -> GasAmount {
    let cairo_primitives_gas =
        cairo_primitives_counters.iter().fold(0u64, |accumulated_gas, (name, &count)| {
            let cairo_primitive_weight =
                cairo_primitives_gas_costs.get_cairo_primitive_gas_cost(name).unwrap();
            cairo_primitive_weight
                .checked_mul(u64_from_usize(count))
                .and_then(|builtin_gas| accumulated_gas.checked_add(builtin_gas))
                .unwrap_or_else(|| {
                    panic!(
                        "Overflow while converting cairo primitives counters to gas.\nCairo \
                         primitive: {name:?}, Weight: {cairo_primitive_weight}, Count: {count}, \
                         Accumulated gas: {accumulated_gas}"
                    )
                })
        });

    GasAmount(cairo_primitives_gas)
}

fn add_casm_hash_computation_gas_cost(
    class_hash_to_casm_hash_computation_resources: &HashMap<ClassHash, ExtendedExecutionResources>,
    gas_without_casm_hash_computation: GasAmount,
    builtin_gas_cost: &BuiltinGasCosts,
    versioned_constants: &VersionedConstants,
) -> (GasAmount, CasmHashComputationData) {
    let casm_hash_computation_data_gas = CasmHashComputationData::from_resources(
        class_hash_to_casm_hash_computation_resources,
        gas_without_casm_hash_computation,
        |resources| {
            extended_execution_resources_to_gas(resources, builtin_gas_cost, versioned_constants)
        },
    );
    (casm_hash_computation_data_gas.total_gas(), casm_hash_computation_data_gas)
}

fn compute_sierra_gas(
    vm_resources: &ExtendedExecutionResources,
    sierra_builtin_gas_costs: &BuiltinGasCosts,
    versioned_constants: &VersionedConstants,
    tx_resources: &TransactionResources,
    migration_gas: GasAmount,
    class_hash_to_casm_hash_computation_resources: &HashMap<ClassHash, ExtendedExecutionResources>,
) -> (GasAmount, CasmHashComputationData, GasAmount) {
    let mut vm_resources_sierra_gas = extended_execution_resources_to_gas(
        vm_resources,
        sierra_builtin_gas_costs,
        versioned_constants,
    );
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
        );
    (total_sierra_gas, casm_hash_computation_data_sierra_gas, vm_resources_sierra_gas)
}

fn compute_proving_gas(
    cairo_primitives_counters: &CairoPrimitiveCounterMap,
    vm_resources_sierra_gas: GasAmount,
    versioned_constants: &VersionedConstants,
    proving_builtin_gas_costs: &BuiltinGasCosts,
    sierra_builtin_gas_costs: &BuiltinGasCosts,
    migration_gas: GasAmount,
    class_hash_to_casm_hash_computation_resources: &HashMap<ClassHash, ExtendedExecutionResources>,
) -> (GasAmount, CasmHashComputationData) {
    let vm_resources_proving_gas = proving_gas_from_cairo_primitives_and_sierra_gas(
        vm_resources_sierra_gas,
        cairo_primitives_counters,
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
    tx_cairo_primitives_counters: &CairoPrimitiveCounterMap,
    bouncer_config: &BouncerConfig,
    receipt_l2_gas: GasAmount,
) -> TransactionExecutionResult<TxWeights> {
    let message_resources = &tx_resources.starknet_resources.messages;
    let message_starknet_l1gas = usize_from_u64(message_resources.get_starknet_gas_cost().l1_gas.0)
        .expect("This conversion should not fail as the value is a converted usize.");

    // Casm hash resources.
    let class_hash_to_casm_hash_computation_resources =
        map_class_hash_to_casm_hash_computation_resources(state_reader, executed_class_hashes)?;

    // Patricia update + transaction resources.
    let patricia_update_resources = get_patricia_update_resources(
        n_visited_storage_entries,
        // TODO(Yoni): consider counting here the global contract tree and the aliases as well.
        state_changes_keys.storage_keys.len(),
    );
    let vm_resources =
        &tx_resources.computation.total_extended_vm_resources() + &patricia_update_resources;

    // Builtin gas costs for stone and for stwo.
    let sierra_builtin_gas_costs = &versioned_constants.os_constants.gas_costs.builtins;
    let proving_builtin_gas_costs = &bouncer_config.builtin_gas_costs();

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

    // Migration occurs once per contract and is not included in the CASM hash computation, which
    // is performed every time a contract is loaded.
    let sierra_migration_gas = migration_data.to_gas(sierra_builtin_gas_costs, versioned_constants);
    let proving_migration_gas =
        migration_data.to_gas(proving_builtin_gas_costs, versioned_constants);

    // Sierra gas computation.
    let (total_sierra_gas, casm_hash_computation_data_sierra_gas, vm_resources_sierra_gas) =
        compute_sierra_gas(
            &vm_resources,
            sierra_builtin_gas_costs,
            versioned_constants,
            tx_resources,
            sierra_migration_gas,
            &class_hash_to_casm_hash_computation_resources,
        );

    // Proving gas computation.
    let cairo_primitives_for_proving_gas = get_cairo_primitives_for_proving_gas_computation(
        patricia_update_resources.prover_builtins(),
        tx_resources.computation.os_vm_resources.prover_builtins(),
        tx_cairo_primitives_counters,
    );

    let (total_proving_gas, casm_hash_computation_data_proving_gas) = compute_proving_gas(
        &cairo_primitives_for_proving_gas,
        vm_resources_sierra_gas,
        versioned_constants,
        proving_builtin_gas_costs,
        sierra_builtin_gas_costs,
        proving_migration_gas,
        &class_hash_to_casm_hash_computation_resources,
    );

    let bouncer_weights = BouncerWeights {
        l1_gas: message_starknet_l1gas,
        message_segment_length: message_resources.message_segment_length,
        n_events: tx_resources.starknet_resources.archival_data.event_summary.n_events,
        state_diff_size: get_onchain_data_segment_length(&total_state_changes_keys.count()),
        sierra_gas: total_sierra_gas,
        n_txs: 1,
        proving_gas: total_proving_gas,
        receipt_l2_gas,
    };

    Ok(TxWeights {
        bouncer_weights,
        casm_hash_computation_data_sierra_gas,
        casm_hash_computation_data_proving_gas,
        class_hashes_to_migrate: migration_data.class_hashes_to_migrate,
    })
}

/// Aggregates Cairo primitives (builtins and opcodes) for proving gas computation.
///
/// The Patricia tree updates and OS computation only track builtin usage- they do not
/// consume opcodes. The transaction resources comes from VM execution and includes both builtin
// and opcode counters.
fn get_cairo_primitives_for_proving_gas_computation(
    patricia_update_builtins: BuiltinCounterMap,
    os_computation_builtins: BuiltinCounterMap,
    tx_cairo_primitives: &CairoPrimitiveCounterMap,
) -> CairoPrimitiveCounterMap {
    let mut cairo_primitives = cairo_primitive_counter_map(patricia_update_builtins);
    add_maps(&mut cairo_primitives, &cairo_primitive_counter_map(os_computation_builtins));
    add_maps(&mut cairo_primitives, tx_cairo_primitives);

    cairo_primitives
}

/// Returns a mapping from each class hash to its estimated Cairo resources for Casm hash
/// computation (done by the OS).
pub fn map_class_hash_to_casm_hash_computation_resources<S: StateReader>(
    state_reader: &S,
    executed_class_hashes: &HashSet<ClassHash>,
) -> TransactionExecutionResult<HashMap<ClassHash, ExtendedExecutionResources>> {
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
///      n_first_time_modified_storage_entries (if the first access to a cell was write) and may not
///      (if a cell was read by a previous transaction and is now modified for the first time).
///   2. In practice, the OS performs a multi-update, which is more efficient than performing
///      separate updates. However, we use this conservative estimate for simplicity.
pub fn get_patricia_update_resources(
    n_visited_storage_entries: usize,
    n_first_time_modified_storage_entries: usize,
) -> ExecutionResources {
    // The height of a Patricia tree with N uniformly distributed leaves is ~log(N).
    const TREE_HEIGHT_UPPER_BOUND: usize = 24;
    // TODO(Yoni, 1/5/2024): re-estimate this.
    const STEPS_IN_TREE_PER_HEIGHT: usize = 16;
    const PEDERSENS_PER_HEIGHT: usize = 1;

    let resources_per_tree_access = ExecutionResources {
        n_steps: TREE_HEIGHT_UPPER_BOUND * STEPS_IN_TREE_PER_HEIGHT,
        builtin_instance_counter: BTreeMap::from([(
            BuiltinName::pedersen,
            TREE_HEIGHT_UPPER_BOUND * PEDERSENS_PER_HEIGHT,
        )]),
        n_memory_holes: 0,
    };

    // One traversal per access (read or write), and an additional one per write access.
    &resources_per_tree_access * (n_visited_storage_entries + n_first_time_modified_storage_entries)
}

// TODO(Dan): refactor to reduce the number of arguments.
#[allow(clippy::too_many_arguments)]
pub fn verify_tx_weights_within_max_capacity<S: StateReader>(
    state_reader: &S,
    tx_execution_summary: &ExecutionSummary,
    tx_builtin_counters: &CairoPrimitiveCounterMap,
    tx_resources: &TransactionResources,
    tx_state_changes_keys: &StateChangesKeys,
    bouncer_config: &BouncerConfig,
    versioned_constants: &VersionedConstants,
    receipt_l2_gas: GasAmount,
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
        receipt_l2_gas,
    )?
    .bouncer_weights;

    bouncer_config.within_max_capacity_or_err(tx_weights)
}
