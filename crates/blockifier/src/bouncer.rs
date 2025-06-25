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
use crate::utils::{add_maps, u64_from_usize, usize_from_u64};

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
    pub builtin_weights: BuiltinWeights,
}

impl BouncerConfig {
    pub fn empty() -> Self {
        Self {
            block_max_capacity: BouncerWeights::empty(),
            builtin_weights: BuiltinWeights::empty(),
        }
    }

    pub fn max() -> Self {
        Self {
            block_max_capacity: BouncerWeights::max(),
            builtin_weights: BuiltinWeights::default(),
        }
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
        let mut dump =
            prepend_sub_config_name(self.block_max_capacity.dump(), "block_max_capacity");
        dump.append(&mut prepend_sub_config_name(self.builtin_weights.dump(), "builtin_weights"));
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
    pub sierra_gas: GasAmount,
    pub n_txs: usize,
    pub proving_gas: GasAmount,
}

impl BouncerWeights {
    impl_checked_ops!(
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
            sierra_gas: GasAmount(4000000000),
            proving_gas: GasAmount(2000000000),
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
            .checked_add(other.gas_without_casm_hash_computation)
            .unwrap_or_else(|| {
                panic!(
                    "Addition overflow while adding sierra gas. current gas: {}, try to add
                 gas: {}.",
                    self.gas_without_casm_hash_computation, other.gas_without_casm_hash_computation
                )
            });
    }
}

pub struct TxWeights {
    pub bouncer_weights: BouncerWeights,
    pub casm_hash_computation_data_sierra_gas: CasmHashComputationData,
    pub casm_hash_computation_data_proving_gas: CasmHashComputationData,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
// TODO(Meshi): Consider code sharing with the BuiltinGasCosts struct.
pub struct BuiltinWeights {
    pub pedersen: usize,
    pub range_check: usize,
    pub ecdsa: usize,
    pub bitwise: usize,
    pub poseidon: usize,
    pub keccak: usize,
    pub ec_op: usize,
    pub mul_mod: usize,
    pub add_mod: usize,
    pub range_check96: usize,
}

impl BuiltinWeights {
    pub fn empty() -> Self {
        Self {
            pedersen: 0,
            range_check: 0,
            ecdsa: 0,
            bitwise: 0,
            poseidon: 0,
            keccak: 0,
            ec_op: 0,
            mul_mod: 0,
            add_mod: 0,
            range_check96: 0,
        }
    }

    // TODO(Meshi): Consider code sharing with the builtins_to_sierra_gas function.
    pub fn calc_proving_gas_from_builtin_counter(
        &self,
        builtin_counts: &BuiltinCounterMap,
    ) -> GasAmount {
        let builtin_gas = builtin_counts.iter().fold(0_usize, |accumulated_gas, (name, &count)| {
            let builtin_weight = self.builtin_weight(name);
            builtin_weight
                .checked_mul(count)
                .and_then(|builtin_gas| accumulated_gas.checked_add(builtin_gas))
                .unwrap_or_else(|| {
                    panic!(
                        "Overflow while converting builtin counters to gas.\nBuiltin: {name}, \
                         Weight: {builtin_weight}, Count: {count}, Accumulated gas: \
                         {accumulated_gas}"
                    )
                })
        });

        GasAmount(u64_from_usize(builtin_gas))
    }

    pub fn builtin_weight(&self, builtin_name: &BuiltinName) -> usize {
        match builtin_name {
            BuiltinName::pedersen => self.pedersen,
            BuiltinName::range_check => self.range_check,
            BuiltinName::ecdsa => self.ecdsa,
            BuiltinName::bitwise => self.bitwise,
            BuiltinName::poseidon => self.poseidon,
            BuiltinName::keccak => self.keccak,
            BuiltinName::ec_op => self.ec_op,
            BuiltinName::mul_mod => self.mul_mod,
            BuiltinName::add_mod => self.add_mod,
            BuiltinName::range_check96 => self.range_check96,
            _ => panic!("Builtin name {builtin_name} is not supported in the bouncer weights."),
        }
    }
}

impl Default for BuiltinWeights {
    fn default() -> Self {
        Self {
            pedersen: 4050,
            range_check: 70,
            ecdsa: 675904,
            ec_op: 285950,
            bitwise: 583,
            keccak: 204283,
            poseidon: 3928,
            add_mod: 250,
            mul_mod: 604,
            range_check96: 56,
        }
    }
}

impl SerializeConfig for BuiltinWeights {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from([ser_param(
            "pedersen",
            &self.pedersen,
            "Pedersen gas weight.",
            ParamPrivacyInput::Public,
        )]);
        dump.append(&mut BTreeMap::from([ser_param(
            "range_check",
            &self.range_check,
            "Range_check gas weight.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "range_check96",
            &self.range_check96,
            "range_check96 gas weight.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "poseidon",
            &self.poseidon,
            "Poseidon gas weight.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "ecdsa",
            &self.ecdsa,
            "Ecdsa gas weight.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "ec_op",
            &self.ec_op,
            "Ec_op gas weight.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "add_mod",
            &self.add_mod,
            "Add_mod gas weight.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "mul_mod",
            &self.mul_mod,
            "Mul_mod gas weight.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "keccak",
            &self.keccak,
            "Keccak gas weight.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "bitwise",
            &self.bitwise,
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
    pub casm_hash_computation_data_sierra_gas: CasmHashComputationData,
    pub casm_hash_computation_data_proving_gas: CasmHashComputationData,

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
            casm_hash_computation_data_sierra_gas: CasmHashComputationData::empty(),
            casm_hash_computation_data_proving_gas: CasmHashComputationData::empty(),
        }
    }

    pub fn get_accumulated_weights(&self) -> &BouncerWeights {
        &self.accumulated_weights
    }

    pub fn get_executed_class_hashes(&self) -> HashSet<ClassHash> {
        self.casm_hash_computation_data_sierra_gas
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
            &tx_execution_summary.builtin_counters,
            &self.bouncer_config.builtin_weights,
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
        self.casm_hash_computation_data_sierra_gas
            .extend(tx_weights.casm_hash_computation_data_sierra_gas);
        self.casm_hash_computation_data_proving_gas
            .extend(tx_weights.casm_hash_computation_data_proving_gas);
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

/// Converts 'amount' of resource units into Sierra gas, given a per-unit rate.
fn vm_resource_to_gas_amount(amount: usize, gas_per_unit: u64, name: &str) -> GasAmount {
    let amount_u64 = u64_from_usize(amount);
    let gas = amount_u64.checked_mul(gas_per_unit).unwrap_or_else(|| {
        panic!(
            "Multiplication overflow converting {name} to gas. units: {}, gas per unit: {}.",
            amount_u64, gas_per_unit
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

fn vm_resources_to_sierra_gas(
    resources: &ExecutionResources,
    versioned_constants: &VersionedConstants,
) -> GasAmount {
    let builtins_gas_cost =
        builtins_to_sierra_gas(&resources.prover_builtins(), versioned_constants);
    let n_steps_gas_cost = n_steps_to_gas(resources.total_n_steps(), versioned_constants);
    let n_memory_holes_gas_cost =
        memory_holes_to_gas(resources.n_memory_holes, versioned_constants);

    n_steps_gas_cost
        .checked_add(n_memory_holes_gas_cost)
        .and_then(|sum| sum.checked_add(builtins_gas_cost))
        .unwrap_or_else(|| {
            panic!(
                "Addition overflow while converting vm resources to gas. steps gas: {}, memory \
                 holes gas: {}, builtins gas: {}.",
                n_steps_gas_cost, builtins_gas_cost, n_memory_holes_gas_cost
            )
        })
}

/// Computes the steps gas by subtracting the builtins' contribution from the Sierra gas.
pub fn sierra_gas_to_steps_gas(
    sierra_gas: GasAmount,
    versioned_constants: &VersionedConstants,
    builtin_counters: &BuiltinCounterMap,
) -> GasAmount {
    let builtins_gas_cost = builtins_to_sierra_gas(builtin_counters, versioned_constants);

    sierra_gas.checked_sub(builtins_gas_cost).unwrap_or_else(|| {
        panic!(
            "Invalid gas subtraction: builtins gas exceeds total sierra gas. Sierra gas: {:?}, \
             Builtins gas: {:?}, Builtins: {:?}",
            sierra_gas, builtins_gas_cost, builtin_counters
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
// TODO(Noa):Fix.
#[allow(clippy::too_many_arguments)]
pub fn get_tx_weights<S: StateReader>(
    state_reader: &S,
    executed_class_hashes: &HashSet<ClassHash>,
    n_visited_storage_entries: usize,
    tx_resources: &TransactionResources,
    state_changes_keys: &StateChangesKeys,
    versioned_constants: &VersionedConstants,
    tx_builtin_counters: &BuiltinCounterMap,
    builtin_weights: &BuiltinWeights,
) -> TransactionExecutionResult<TxWeights> {
    let message_resources = &tx_resources.starknet_resources.messages;
    let message_starknet_l1gas = usize_from_u64(message_resources.get_starknet_gas_cost().l1_gas.0)
        .expect("This conversion should not fail as the value is a converted usize.");

    // Casm hash resources.
    let class_hash_to_casm_hash_computation_resources =
        map_class_hash_to_casm_hash_computation_resources(state_reader, executed_class_hashes)?;
    let total_casm_hash_computation_resources = class_hash_to_casm_hash_computation_resources
        .values()
        .fold(ExecutionResources::default(), |acc, resources| &acc + resources);

    // Patricia update + transaction resources.
    let patrticia_update_resources = get_particia_update_resources(n_visited_storage_entries);
    let vm_resources = &patrticia_update_resources + &tx_resources.computation.total_vm_resources();

    // Sierra gas computation.
    let vm_resources_sierra_gas = vm_resources_to_sierra_gas(&vm_resources, versioned_constants);
    let sierra_gas = tx_resources.computation.sierra_gas;
    let gas_without_casm_hash_computation =
        sierra_gas.checked_add(vm_resources_sierra_gas).unwrap_or_else(|| {
            panic!(
                "Addition overflow while adding sierra gas. current gas: {}, try to add
                 gas: {}.",
                sierra_gas, vm_resources_sierra_gas
            )
        });
    let total_casm_hash_computation_sierra_gas =
        vm_resources_to_sierra_gas(&total_casm_hash_computation_resources, versioned_constants);
    let total_sierra_gas = gas_without_casm_hash_computation
        .checked_add(total_casm_hash_computation_sierra_gas)
        .unwrap_or_else(|| {
            panic!(
                "Addition overflow while adding sierra gas. current gas: {}, try to add
                 gas: {}.",
                gas_without_casm_hash_computation, total_casm_hash_computation_sierra_gas
            )
        });

    let casm_hash_computation_data_sierra_gas = CasmHashComputationData {
        class_hash_to_casm_hash_computation_gas: class_hash_to_casm_hash_computation_resources
            .into_iter()
            .map(|(class_hash, resources)| {
                let gas = vm_resources_to_sierra_gas(&resources, versioned_constants);
                (class_hash, gas)
            })
            .collect(),
        gas_without_casm_hash_computation,
    };

    // Proving gas computation.
    let mut total_builtin_counters = patrticia_update_resources.prover_builtins();
    add_maps(&mut total_builtin_counters, tx_builtin_counters);
    // The transaction builtin counters does not include the transaction overhead ('additional')
    // resources.
    add_maps(
        &mut total_builtin_counters,
        &tx_resources.computation.os_vm_resources.prover_builtins(),
    );
    let total_casm_hash_computation_resources_builtins =
        total_casm_hash_computation_resources.prover_builtins();
    add_maps(&mut total_builtin_counters, &total_casm_hash_computation_resources_builtins);
    let builtins_proving_gas =
        builtin_weights.calc_proving_gas_from_builtin_counter(&total_builtin_counters);
    let steps_proving_gas =
        sierra_gas_to_steps_gas(total_sierra_gas, versioned_constants, &total_builtin_counters);
    let total_proving_gas =
        steps_proving_gas.checked_add(builtins_proving_gas).unwrap_or_else(|| {
            panic!(
                "Addition overflow while calculating the proving gas. steps gas: {}, builtins as \
                 gas: {}.",
                steps_proving_gas, builtins_proving_gas
            )
        });

    // TODO(Aviv): compute the actual casm proving gas
    let casm_hash_computation_data_proving_gas = CasmHashComputationData::empty();

    let bouncer_weights = BouncerWeights {
        l1_gas: message_starknet_l1gas,
        message_segment_length: message_resources.message_segment_length,
        n_events: tx_resources.starknet_resources.archival_data.event_summary.n_events,
        state_diff_size: get_onchain_data_segment_length(&state_changes_keys.count()),
        sierra_gas: total_sierra_gas,
        n_txs: 1,
        proving_gas: total_proving_gas,
    };

    Ok(TxWeights {
        bouncer_weights,
        casm_hash_computation_data_sierra_gas,
        casm_hash_computation_data_proving_gas,
    })
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
        &tx_execution_summary.builtin_counters,
        &bouncer_config.builtin_weights,
    )?
    .bouncer_weights;

    bouncer_config.within_max_capacity_or_err(tx_weights)
}
