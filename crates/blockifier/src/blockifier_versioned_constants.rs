use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;
use std::path::Path;
use std::sync::Arc;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use num_rational::Ratio;
use num_traits::Inv;
use semver::Version;
use serde::de::Error as DeserializationError;
use serde::{Deserialize, Deserializer, Serialize};
use starknet_api::block::{GasPrice, StarknetVersion};
use starknet_api::contract_class::SierraVersion;
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector};
use starknet_api::define_versioned_constants;
use starknet_api::executable_transaction::TransactionType;
use starknet_api::execution_resources::{GasAmount, GasVector};
use starknet_api::transaction::fields::{hex_to_tip, GasVectorComputationMode, Tip};
use strum::IntoEnumIterator;
use thiserror::Error;

use crate::execution::common_hints::ExecutionMode;
use crate::execution::execution_utils::poseidon_hash_many_cost;
use crate::execution::syscalls::vm_syscall_utils::{SyscallSelector, SyscallUsageMap};
use crate::fee::resources::StarknetResources;
use crate::transaction::objects::ExecutionResourcesTraits;
use crate::utils::get_gas_cost_from_vm_resources;

#[cfg(test)]
#[path = "versioned_constants_test.rs"]
pub mod test;

define_versioned_constants!(
    VersionedConstants,
    RawVersionedConstants,
    VersionedConstantsError,
    (V0_13_0, "../resources/blockifier_versioned_constants_0_13_0.json"),
    (V0_13_1, "../resources/blockifier_versioned_constants_0_13_1.json"),
    (V0_13_1_1, "../resources/blockifier_versioned_constants_0_13_1_1.json"),
    (V0_13_2, "../resources/blockifier_versioned_constants_0_13_2.json"),
    (V0_13_2_1, "../resources/blockifier_versioned_constants_0_13_2_1.json"),
    (V0_13_3, "../resources/blockifier_versioned_constants_0_13_3.json"),
    (V0_13_4, "../resources/blockifier_versioned_constants_0_13_4.json"),
    (V0_13_5, "../resources/blockifier_versioned_constants_0_13_5.json"),
    (V0_13_6, "../resources/blockifier_versioned_constants_0_13_6.json"),
    (V0_14_0, "../resources/blockifier_versioned_constants_0_14_0.json"),
    (V0_15_0, "../resources/blockifier_versioned_constants_0_15_0.json"),
);

pub type SyscallGasCostsMap = HashMap<SyscallSelector, RawSyscallGasCost>;

/// Representation of the JSON data of versioned constants. Used as an intermediate struct for
/// serde.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct RawVersionedConstants {
    // Limits.
    pub tx_event_limits: EventLimits,
    pub gateway: VersionedConstantsGatewayLimits,
    pub invoke_tx_max_n_steps: u32,
    pub validate_max_n_steps: u32,
    pub max_recursion_depth: usize,

    // Costs.
    pub deprecated_l2_resource_gas_costs: ArchivalDataGasCosts,
    pub archival_data_gas_costs: ArchivalDataGasCosts,
    pub allocation_cost: AllocationCost,
    pub vm_resource_fee_cost: VmResourceCosts,

    // Feature flags.
    pub disable_cairo0_redeclaration: bool,
    pub enable_stateful_compression: bool,
    pub comprehensive_state_diff: bool,
    pub block_direct_execute_call: bool,
    pub ignore_inner_event_resources: bool,
    pub disable_deploy_in_validation_mode: bool,
    pub enable_reverts: bool,
    pub min_sierra_version_for_sierra_gas: SierraVersion,
    pub enable_tip: bool,
    pub segment_arena_cells: bool,

    // OS.
    pub os_constants: RawOsConstants,
    pub os_resources: RawOsResources,
}

#[cfg_attr(any(test, feature = "testing"), derive(Serialize))]
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct RawOsConstants {
    // Selectors.
    pub constructor_entry_point_selector: EntryPointSelector,
    pub default_entry_point_selector: EntryPointSelector,
    pub execute_entry_point_selector: EntryPointSelector,
    pub transfer_entry_point_selector: EntryPointSelector,
    pub validate_declare_entry_point_selector: EntryPointSelector,
    pub validate_deploy_entry_point_selector: EntryPointSelector,
    pub validate_entry_point_selector: EntryPointSelector,

    // Entry point type identifiers (in the OS).
    pub entry_point_type_constructor: u8,
    pub entry_point_type_external: u8,
    pub entry_point_type_l1_handler: u8,

    // Validation.
    pub validate_rounding_consts: ValidateRoundingConsts,
    pub validated: String,

    // Execution limits.
    pub execute_max_sierra_gas: GasAmount,
    pub validate_max_sierra_gas: GasAmount,

    // Error strings.
    pub error_block_number_out_of_range: String,
    pub error_invalid_input_len: String,
    pub error_invalid_argument: String,
    pub error_out_of_gas: String,
    pub error_entry_point_failed: String,
    pub error_entry_point_not_found: String,

    // Resource bounds names.
    pub l1_gas: String,
    pub l2_gas: String,
    pub l1_data_gas: String,

    // Resource bounds indices.
    pub l1_gas_index: usize,
    pub l1_data_gas_index: usize,
    pub l2_gas_index: usize,

    // Costs.
    pub memory_hole_gas_cost: GasAmount,
    pub builtin_gas_costs: BuiltinGasCosts,
    pub step_gas_cost: u64,
    pub syscall_base_gas_cost: RawStepGasCost,
    // Deprecated field for computation of syscall gas costs in old blocks.
    // New VCs set this to null.
    pub syscall_gas_costs: Option<SyscallGasCostsMap>,

    // Initial costs.
    pub entry_point_initial_budget: RawStepGasCost,
    pub default_initial_gas_cost: RawStepGasCost,

    // L1 handler.
    pub l1_handler_version: u8,
    pub l1_handler_max_amount_bounds: GasVector,

    // Miscellaneous.
    pub nop_entry_point_offset: i8,
    pub os_contract_addresses: OsContractAddresses,
    pub sierra_array_len_bound: u64,
    pub stored_block_hash_buffer: u8,

    // Deprecated contract logic support.
    pub v1_bound_accounts_cairo0: Vec<ClassHash>,
    pub v1_bound_accounts_cairo1: Vec<ClassHash>,
    #[serde(deserialize_with = "hex_to_tip")]
    pub v1_bound_accounts_max_tip: Tip,
    pub data_gas_accounts: Vec<ClassHash>,
}

#[cfg_attr(any(test, feature = "testing"), derive(Serialize))]
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct RawStepGasCost {
    pub step_gas_cost: GasAmount,
}

pub type ResourceCost = Ratio<u64>;

// TODO(Dori): Delete this ratio-converter function once event keys / data length are no longer 128
// bits   (no other usage is expected).
pub fn resource_cost_to_u128_ratio(cost: ResourceCost) -> Ratio<u128> {
    Ratio::new((*cost.numer()).into(), (*cost.denom()).into())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, PartialOrd)]
pub struct CompilerVersion(pub Version);
impl Default for CompilerVersion {
    fn default() -> Self {
        Self(Version::new(0, 0, 0))
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct VmResourceCosts {
    pub n_steps: ResourceCost,
    #[serde(deserialize_with = "builtin_map_from_string_map")]
    pub builtins: HashMap<BuiltinName, ResourceCost>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct AllocationCost {
    pub blob_cost: GasVector,
    pub gas_cost: GasVector,
}

impl AllocationCost {
    pub const ZERO: AllocationCost =
        AllocationCost { blob_cost: GasVector::ZERO, gas_cost: GasVector::ZERO };

    pub fn get_cost(&self, use_kzg_da: bool) -> &GasVector {
        if use_kzg_da { &self.blob_cost } else { &self.gas_cost }
    }
}

// TODO(Dori): This (along with the Serialize impl) is implemented in pub(crate) scope in the VM
// (named   serde_generic_map_impl); use it if and when it's public.
fn builtin_map_from_string_map<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<HashMap<BuiltinName, ResourceCost>, D::Error> {
    HashMap::<String, ResourceCost>::deserialize(d)?
        .into_iter()
        .map(|(k, v)| BuiltinName::from_str_with_suffix(&k).map(|k| (k, v)))
        .collect::<Option<HashMap<_, _>>>()
        .ok_or(D::Error::custom("Invalid builtin name"))
}

/// Contains constants for the Blockifier that may vary between versions.
/// Additional constants in the JSON file, not used by Blockifier but included for transparency, are
/// automatically ignored during deserialization.
/// Instances of this struct for specific Starknet versions can be selected by using the above enum.
#[cfg_attr(any(test, feature = "testing"), derive(PartialEq))]
#[derive(Clone, Debug, Default)]
pub struct VersionedConstants {
    // Limits.
    pub tx_event_limits: EventLimits,
    pub invoke_tx_max_n_steps: u32,
    pub deprecated_l2_resource_gas_costs: ArchivalDataGasCosts,
    pub archival_data_gas_costs: ArchivalDataGasCosts,
    pub max_recursion_depth: usize,
    pub validate_max_n_steps: u32,
    pub min_sierra_version_for_sierra_gas: SierraVersion,
    // BACKWARD COMPATIBILITY: If true, the segment_arena builtin instance counter will be
    // multiplied by 3. This offsets a bug in the old vm where the counter counted the number of
    // cells used by instances of the builtin, instead of the number of instances.
    pub segment_arena_cells: bool,

    // Transactions settings.
    pub disable_cairo0_redeclaration: bool,
    pub enable_stateful_compression: bool,
    pub comprehensive_state_diff: bool,
    pub block_direct_execute_call: bool,
    pub ignore_inner_event_resources: bool,
    pub disable_deploy_in_validation_mode: bool,

    // Compiler settings.
    pub enable_reverts: bool,

    // Cairo OS constants.
    // Note: if loaded from a json file, there are some assumptions made on its structure.
    // See the struct's docstring for more details.
    pub os_constants: Arc<OsConstants>,

    // Fee related.
    pub(crate) vm_resource_fee_cost: Arc<VmResourceCosts>,
    pub enable_tip: bool,
    // Cost of allocating a storage cell.
    pub allocation_cost: AllocationCost,

    // Resources.
    os_resources: Arc<OsResources>,

    // Just to make sure the value exists, but don't use the actual values.
    #[allow(dead_code)]
    gateway: VersionedConstantsGatewayLimits,
}

impl From<RawVersionedConstants> for VersionedConstants {
    fn from(raw_vc: RawVersionedConstants) -> Self {
        let os_constants = OsConstants::from_raw(&raw_vc.os_constants, &raw_vc.os_resources);
        let os_resources = OsResources::from_raw(&raw_vc.os_resources);
        Self {
            tx_event_limits: raw_vc.tx_event_limits,
            invoke_tx_max_n_steps: raw_vc.invoke_tx_max_n_steps,
            deprecated_l2_resource_gas_costs: raw_vc.deprecated_l2_resource_gas_costs,
            archival_data_gas_costs: raw_vc.archival_data_gas_costs,
            max_recursion_depth: raw_vc.max_recursion_depth,
            validate_max_n_steps: raw_vc.validate_max_n_steps,
            min_sierra_version_for_sierra_gas: raw_vc.min_sierra_version_for_sierra_gas,
            segment_arena_cells: raw_vc.segment_arena_cells,
            disable_cairo0_redeclaration: raw_vc.disable_cairo0_redeclaration,
            enable_stateful_compression: raw_vc.enable_stateful_compression,
            comprehensive_state_diff: raw_vc.comprehensive_state_diff,
            block_direct_execute_call: raw_vc.block_direct_execute_call,
            ignore_inner_event_resources: raw_vc.ignore_inner_event_resources,
            disable_deploy_in_validation_mode: raw_vc.disable_deploy_in_validation_mode,
            enable_reverts: raw_vc.enable_reverts,
            os_constants: Arc::new(os_constants),
            vm_resource_fee_cost: Arc::new(raw_vc.vm_resource_fee_cost),
            enable_tip: raw_vc.enable_tip,
            allocation_cost: raw_vc.allocation_cost,
            os_resources: Arc::new(os_resources),
            gateway: raw_vc.gateway,
        }
    }
}

impl VersionedConstants {
    pub fn from_path(path: &Path) -> VersionedConstantsResult<Self> {
        let raw_vc: RawVersionedConstants = serde_json::from_reader(std::fs::File::open(path)?)?;
        Ok(raw_vc.into())
    }

    /// Converts from L1 gas price to L2 gas price with **upward rounding**, based on the
    /// conversion of a Cairo step from Sierra gas to L1 gas.
    pub fn convert_l1_to_l2_gas_price_round_up(&self, l1_gas_price: GasPrice) -> GasPrice {
        (*(resource_cost_to_u128_ratio(self.sierra_gas_in_l1_gas_amount()) * l1_gas_price.0)
            .ceil()
            .numer())
        .into()
    }

    /// Converts L1 gas amount to Sierra (L2) gas amount with **upward rounding**.
    pub fn l1_gas_to_sierra_gas_amount_round_up(&self, l1_gas_amount: GasAmount) -> GasAmount {
        // The amount ratio is the inverse of the price ratio.
        (*(self.sierra_gas_in_l1_gas_amount().inv() * l1_gas_amount.0).ceil().numer()).into()
    }

    /// Converts Sierra (L2) gas amount to L1 gas amount with **upward rounding**.
    pub fn sierra_gas_to_l1_gas_amount_round_up(&self, l2_gas_amount: GasAmount) -> GasAmount {
        (*(self.sierra_gas_in_l1_gas_amount() * l2_gas_amount.0).ceil().numer()).into()
    }

    /// Returns the equivalent L1 gas amount of one unit of Sierra gas.
    /// The conversion is based on the pricing of a single Cairo step.
    fn sierra_gas_in_l1_gas_amount(&self) -> ResourceCost {
        Ratio::new(1, self.os_constants.gas_costs.base.step_gas_cost)
            * self.vm_resource_fee_cost().n_steps
    }

    /// Default initial gas amount when L2 gas is not provided.
    pub fn initial_gas_no_user_l2_bound(&self) -> GasAmount {
        (self
            .os_constants
            .execute_max_sierra_gas
            .checked_add(self.os_constants.validate_max_sierra_gas))
        .expect("The default initial gas cost should be less than the maximum gas amount.")
    }

    /// Returns the maximum gas amount according to the given mode.
    pub fn sierra_gas_limit(&self, mode: &ExecutionMode) -> GasAmount {
        match mode {
            ExecutionMode::Validate => self.os_constants.validate_max_sierra_gas,
            ExecutionMode::Execute => self.os_constants.execute_max_sierra_gas,
        }
    }

    /// Returns the default initial gas for VM mode transactions.
    pub fn infinite_gas_for_vm_mode(&self) -> u64 {
        self.os_constants.gas_costs.base.default_initial_gas_cost
    }

    pub fn vm_resource_fee_cost(&self) -> &VmResourceCosts {
        &self.vm_resource_fee_cost
    }

    pub fn os_resources_for_tx_type(
        &self,
        tx_type: &TransactionType,
        calldata_length: usize,
    ) -> ExecutionResources {
        self.os_resources.resources_for_tx_type(tx_type, calldata_length)
    }

    pub fn os_kzg_da_resources(&self, data_segment_length: usize) -> ExecutionResources {
        self.os_resources.os_kzg_da_resources(data_segment_length)
    }

    pub fn get_additional_os_tx_resources(
        &self,
        tx_type: TransactionType,
        starknet_resources: &StarknetResources,
        use_kzg_da: bool,
    ) -> ExecutionResources {
        self.os_resources.get_additional_os_tx_resources(
            tx_type,
            starknet_resources.archival_data.calldata_length,
            starknet_resources.state.get_onchain_data_segment_length(),
            use_kzg_da,
        )
    }

    pub fn get_additional_os_syscall_resources(
        &self,
        syscalls_usage: &SyscallUsageMap,
    ) -> ExecutionResources {
        self.os_resources.get_additional_os_syscall_resources(syscalls_usage)
    }

    pub fn get_validate_block_number_rounding(&self) -> u64 {
        self.os_constants.validate_rounding_consts.validate_block_number_rounding
    }

    pub fn get_validate_timestamp_rounding(&self) -> u64 {
        self.os_constants.validate_rounding_consts.validate_timestamp_rounding
    }

    #[cfg(any(feature = "testing", test))]
    pub fn create_for_account_testing() -> Self {
        let step_cost = ResourceCost::from_integer(1);
        let vm_resource_fee_cost = Arc::new(VmResourceCosts {
            n_steps: step_cost,
            builtins: HashMap::from([
                (BuiltinName::pedersen, ResourceCost::from_integer(1)),
                (BuiltinName::range_check, ResourceCost::from_integer(1)),
                (BuiltinName::ecdsa, ResourceCost::from_integer(1)),
                (BuiltinName::bitwise, ResourceCost::from_integer(1)),
                (BuiltinName::poseidon, ResourceCost::from_integer(1)),
                (BuiltinName::output, ResourceCost::from_integer(1)),
                (BuiltinName::ec_op, ResourceCost::from_integer(1)),
                (BuiltinName::range_check96, ResourceCost::from_integer(1)),
                (BuiltinName::add_mod, ResourceCost::from_integer(1)),
                (BuiltinName::mul_mod, ResourceCost::from_integer(1)),
            ]),
        });

        // Maintain the ratio between L1 gas price and L2 gas price.
        let latest = Self::create_for_testing();
        let latest_step_cost = latest.vm_resource_fee_cost.n_steps;
        let mut archival_data_gas_costs = latest.archival_data_gas_costs;
        archival_data_gas_costs.gas_per_code_byte *= latest_step_cost / step_cost;
        archival_data_gas_costs.gas_per_data_felt *= latest_step_cost / step_cost;
        Self { vm_resource_fee_cost, archival_data_gas_costs, ..latest }
    }

    // TODO(Arni): Consider replacing each call to this function with `latest_with_overrides`, and
    // squashing the functions together.
    /// Returns the latest versioned constants, applying the given overrides.
    pub fn get_versioned_constants(
        versioned_constants_overrides: VersionedConstantsOverrides,
    ) -> Self {
        let VersionedConstantsOverrides {
            validate_max_n_steps,
            max_recursion_depth,
            invoke_tx_max_n_steps,
            max_n_events,
        } = versioned_constants_overrides;
        let latest_constants = Self::latest_constants().clone();
        let tx_event_limits =
            EventLimits { max_n_emitted_events: max_n_events, ..latest_constants.tx_event_limits };
        Self {
            validate_max_n_steps,
            max_recursion_depth,
            invoke_tx_max_n_steps,
            tx_event_limits,
            ..latest_constants
        }
    }

    pub fn get_archival_data_gas_costs(
        &self,
        mode: &GasVectorComputationMode,
    ) -> &ArchivalDataGasCosts {
        match mode {
            GasVectorComputationMode::All => &self.archival_data_gas_costs,
            GasVectorComputationMode::NoL2Gas => &self.deprecated_l2_resource_gas_costs,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct ArchivalDataGasCosts {
    // TODO(barak, 18/03/2024): Once we start charging per byte change to milligas_per_data_byte,
    // divide the value by 32 in the JSON file.
    pub gas_per_data_felt: ResourceCost,
    pub event_key_factor: ResourceCost,
    // TODO(avi, 15/04/2024): This constant was changed to 32 milligas in the JSON file, but the
    // actual number we wanted is 1/32 gas per byte. Change the value to 1/32 in the next version
    // where rational numbers are supported.
    pub gas_per_code_byte: ResourceCost,
}

pub struct CairoNativeStackConfig {
    pub gas_to_stack_ratio: Ratio<u64>,
    pub max_stack_size: u64,
    pub min_stack_red_zone: u64,
    pub buffer_size: u64,
}

impl CairoNativeStackConfig {
    /// Rounds up the given size to the nearest multiple of MB.
    pub fn round_up_to_mb(size: u64) -> u64 {
        const MB: u64 = 1024 * 1024;
        size.div_ceil(MB) * MB
    }

    /// Returns the stack size sufficient for running Cairo Native.
    /// Rounds up to the nearest multiple of MB.
    pub fn get_stack_size_red_zone(&self, remaining_gas: u64) -> u64 {
        let stack_size_based_on_gas =
            (self.gas_to_stack_ratio * Ratio::new(remaining_gas, 1)).to_integer();
        // Ensure the computed stack size is within the allowed range.
        CairoNativeStackConfig::round_up_to_mb(
            stack_size_based_on_gas.clamp(self.min_stack_red_zone, self.max_stack_size),
        )
    }

    pub fn get_target_stack_size(&self, red_zone: u64) -> u64 {
        // Stack size should be a multiple of page size, since `stacker::grow` works with this unit.
        CairoNativeStackConfig::round_up_to_mb(red_zone + self.buffer_size)
    }
}

#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VersionedConstantsGatewayLimits {
    pub max_calldata_length: usize,
    pub max_contract_bytecode_size: usize,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct EventLimits {
    pub max_data_length: usize,
    pub max_keys_length: usize,
    pub max_n_emitted_events: usize,
}

#[derive(Error, Debug)]
pub enum RawOsResourcesError {
    #[error("os_resources.execute_syscalls are missing a selector: {0:?}")]
    MissingSelector(SyscallSelector),
    #[error("os_resources.execute_tx_inner is missing transaction_type: {0:?}")]
    MissingTxType(TransactionType),
    #[error("unknown os resource {0}")]
    UnknownResource(String),
}

#[cfg_attr(any(test, feature = "testing"), derive(PartialEq))]
#[derive(Clone, Debug, Default)]
pub struct OsResources {
    // Mapping from every syscall to its execution resources in the OS (e.g., amount of Cairo
    // steps).
    // TODO(Arni, 14/6/2023): Update `GetBlockHash` values.
    // TODO(ilya): Consider moving the resources of a keccak round to a seperate dict.
    execute_syscalls: HashMap<SyscallSelector, ResourcesParams>,
    // Mapping from every transaction to its extra execution resources in the OS,
    // i.e., resources that don't count during the execution itself.
    // For each transaction the OS uses a constant amount of VM resources, and an
    // additional variable amount that depends on the calldata length.
    execute_txs_inner: HashMap<TransactionType, ResourcesParams>,

    // Resources needed for the OS to compute the KZG commitment info, as a factor of the data
    // segment length. Does not include poseidon_hash_many cost.
    compute_os_kzg_commitment_info: ExecutionResources,
}

fn validate_all_tx_types<V>(
    tx_type_map: &HashMap<TransactionType, V>,
) -> Result<(), RawOsResourcesError> {
    for tx_type in TransactionType::iter() {
        if !tx_type_map.contains_key(&tx_type) {
            return Err(RawOsResourcesError::MissingTxType(tx_type));
        }
    }
    Ok(())
}

fn validate_all_selectors<V>(
    selector_map: &HashMap<SyscallSelector, V>,
) -> Result<(), RawOsResourcesError> {
    for syscall_handler in SyscallSelector::iter() {
        if !selector_map.contains_key(&syscall_handler) {
            return Err(RawOsResourcesError::MissingSelector(syscall_handler));
        }
    }
    Ok(())
}

fn validate_builtins_known<'a, B: Iterator<Item = &'a BuiltinName>>(
    builtin_names: B,
) -> Result<(), RawOsResourcesError> {
    let known_builtin_names: HashSet<&str> = [
        BuiltinName::output,
        BuiltinName::pedersen,
        BuiltinName::range_check,
        BuiltinName::ecdsa,
        BuiltinName::bitwise,
        BuiltinName::ec_op,
        BuiltinName::keccak,
        BuiltinName::poseidon,
        BuiltinName::segment_arena,
    ]
    .iter()
    .map(|builtin| builtin.to_str_with_suffix())
    .collect();

    for builtin_name in builtin_names {
        if !(known_builtin_names.contains(builtin_name.to_str_with_suffix())) {
            return Err(RawOsResourcesError::UnknownResource(builtin_name.to_string()));
        }
    }
    Ok(())
}

impl OsResources {
    fn from_raw(raw_os_resources: &RawOsResources) -> Self {
        Self {
            execute_syscalls: raw_os_resources
                .execute_syscalls
                .iter()
                .map(|(k, v)| (*k, ResourcesParams::from(v)))
                .collect(),
            execute_txs_inner: raw_os_resources
                .execute_txs_inner
                .iter()
                .map(|(k, v)| (*k, ResourcesParams::from(v)))
                .collect(),
            compute_os_kzg_commitment_info: raw_os_resources.compute_os_kzg_commitment_info.clone(),
        }
    }

    /// Calculates the additional resources needed for the OS to run the given transaction;
    /// i.e., the resources of the Starknet OS function `execute_transactions_inner`.
    /// Also adds the resources needed for the fee transfer execution, performed in the end·
    /// of every transaction.
    fn get_additional_os_tx_resources(
        &self,
        tx_type: TransactionType,
        calldata_length: usize,
        data_segment_length: usize,
        use_kzg_da: bool,
    ) -> ExecutionResources {
        let mut os_additional_vm_resources = self.resources_for_tx_type(&tx_type, calldata_length);

        if use_kzg_da {
            os_additional_vm_resources += &self.os_kzg_da_resources(data_segment_length);
        }

        os_additional_vm_resources
    }

    /// Calculates the additional resources needed for the OS to run the given syscalls;
    /// i.e., the resources of the Starknet OS function `execute_syscalls`.
    fn get_additional_os_syscall_resources(
        &self,
        syscalls_usage: &SyscallUsageMap,
    ) -> ExecutionResources {
        let mut os_additional_resources = ExecutionResources::default();
        for (syscall_selector, syscall_usage) in syscalls_usage {
            if syscall_selector == &SyscallSelector::Keccak {
                let keccak_base_resources =
                    self.execute_syscalls.get(syscall_selector).unwrap_or_else(|| {
                        panic!("OS resources of syscall '{syscall_selector:?}' are unknown.")
                    });
                os_additional_resources += &keccak_base_resources.constant;
            }
            let syscall_selector = if syscall_selector == &SyscallSelector::Keccak {
                &SyscallSelector::KeccakRound
            } else {
                syscall_selector
            };
            let syscall_resources =
                self.execute_syscalls.get(syscall_selector).unwrap_or_else(|| {
                    panic!("OS resources of syscall '{syscall_selector:?}' are unknown.")
                });
            let calldata_factor = CallDataFactor::from(&syscall_resources.calldata_factor);
            os_additional_resources += &(&(&syscall_resources.constant * syscall_usage.call_count)
                + &calldata_factor.calculate_resources(syscall_usage.linear_factor));
        }

        os_additional_resources
    }

    fn resources_params_for_tx_type(&self, tx_type: &TransactionType) -> &ResourcesParams {
        self.execute_txs_inner
            .get(tx_type)
            .unwrap_or_else(|| panic!("should contain transaction type '{tx_type:?}'."))
    }

    fn resources_for_tx_type(
        &self,
        tx_type: &TransactionType,
        calldata_length: usize,
    ) -> ExecutionResources {
        let resources_vector = self.resources_params_for_tx_type(tx_type);
        &resources_vector.constant
            + &CallDataFactor::from(&resources_vector.calldata_factor)
                .calculate_resources(calldata_length)
    }

    fn os_kzg_da_resources(&self, data_segment_length: usize) -> ExecutionResources {
        // BACKWARD COMPATIBILITY: we set compute_os_kzg_commitment_info to empty in older versions
        // where this was not yet computed.
        let empty_resources = ExecutionResources::default();
        if self.compute_os_kzg_commitment_info == empty_resources {
            return empty_resources;
        }
        &(&self.compute_os_kzg_commitment_info * data_segment_length)
            + &poseidon_hash_many_cost(data_segment_length)
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
// Serde trick for adding validations via a customr deserializer, without forgoing the derive.
// See: https://github.com/serde-rs/serde/issues/1220.
#[serde(remote = "Self")]
pub struct RawOsResources {
    pub execute_syscalls: HashMap<SyscallSelector, VariableResourceParams>,
    pub execute_txs_inner: HashMap<TransactionType, VariableResourceParams>,
    pub compute_os_kzg_commitment_info: ExecutionResources,
}

impl RawOsResources {
    pub fn validate(&self) -> Result<(), RawOsResourcesError> {
        validate_all_tx_types(&self.execute_txs_inner)?;
        validate_all_selectors(&self.execute_syscalls)?;

        // Extract all `ExecutionResources` objects from the resource params.
        fn resources_params_exec_resources(
            resources_params: &VariableResourceParams,
        ) -> Vec<&ExecutionResources> {
            match resources_params {
                VariableResourceParams::Constant(constant) => vec![constant],
                VariableResourceParams::WithFactor(ResourcesParams {
                    constant,
                    calldata_factor:
                        VariableCallDataFactor::Scaled(CallDataFactor { resources, .. }),
                })
                | VariableResourceParams::WithFactor(ResourcesParams {
                    constant,
                    calldata_factor: VariableCallDataFactor::Unscaled(resources),
                }) => {
                    vec![constant, resources]
                }
            }
        }

        let execution_resources = self
            .execute_txs_inner
            .values()
            .flat_map(resources_params_exec_resources)
            .chain(self.execute_syscalls.values().flat_map(resources_params_exec_resources))
            .chain(std::iter::once(&self.compute_os_kzg_commitment_info));
        let builtin_names =
            execution_resources.flat_map(|resources| resources.builtin_instance_counter.keys());
        validate_builtins_known(builtin_names)?;

        Ok(())
    }
}

impl<'de> Deserialize<'de> for RawOsResources {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw_os_resources = Self::deserialize(deserializer)?;
        raw_os_resources
            .validate()
            .map_err(|error| DeserializationError::custom(format!("ValidationError: {error}")))?;
        Ok(raw_os_resources)
    }
}

#[cfg_attr(any(test, feature = "testing"), derive(Serialize))]
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged, deny_unknown_fields)]
pub enum RawSyscallGasCost {
    Flat(u64),
    Structured(RawStructuredDeprecatedSyscallGasCost),
}

#[cfg_attr(any(test, feature = "testing"), derive(Serialize))]
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RawStructuredDeprecatedSyscallGasCost {
    #[serde(default)]
    pub step_gas_cost: u64,
    #[serde(default)]
    pub range_check: u64,
    #[serde(default)]
    pub bitwise: u64,
    #[serde(default)]
    pub syscall_base_gas_cost: u64,
    #[serde(default)]
    pub memory_hole_gas_cost: u64,
}

#[derive(PartialEq, Debug, Clone, Copy, Serialize, Default)]
pub struct SyscallGasCost {
    base: u64,
    linear_factor: u64,
}

impl SyscallGasCost {
    pub fn new_from_base_cost(base: u64) -> Self {
        Self { base, linear_factor: 0 }
    }

    pub fn get_syscall_cost(&self, linear_length: u64) -> u64 {
        self.base + self.linear_factor * linear_length
    }

    pub fn base_syscall_cost(&self) -> u64 {
        assert!(self.linear_factor == 0, "The syscall has a linear factor cost to be considered.");
        self.base
    }

    pub fn linear_syscall_cost(&self) -> u64 {
        self.linear_factor
    }
}

#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
#[derive(Debug, Default, PartialEq)]
pub struct SyscallGasCosts {
    pub call_contract: SyscallGasCost,
    pub deploy: SyscallGasCost,
    pub get_block_hash: SyscallGasCost,
    pub get_execution_info: SyscallGasCost,
    pub library_call: SyscallGasCost,
    pub replace_class: SyscallGasCost,
    pub storage_read: SyscallGasCost,
    pub storage_write: SyscallGasCost,
    pub get_class_hash_at: SyscallGasCost,
    pub emit_event: SyscallGasCost,
    pub send_message_to_l1: SyscallGasCost,
    pub secp256k1_add: SyscallGasCost,
    pub secp256k1_get_point_from_x: SyscallGasCost,
    pub secp256k1_get_xy: SyscallGasCost,
    pub secp256k1_mul: SyscallGasCost,
    pub secp256k1_new: SyscallGasCost,
    pub secp256r1_add: SyscallGasCost,
    pub secp256r1_get_point_from_x: SyscallGasCost,
    pub secp256r1_get_xy: SyscallGasCost,
    pub secp256r1_mul: SyscallGasCost,
    pub secp256r1_new: SyscallGasCost,
    pub keccak: SyscallGasCost,
    pub keccak_round: SyscallGasCost,
    pub meta_tx_v0: SyscallGasCost,
    pub sha256_process_block: SyscallGasCost,
}

impl SyscallGasCosts {
    pub fn get_syscall_gas_cost(
        &self,
        selector: &SyscallSelector,
    ) -> Result<SyscallGasCost, GasCostsError> {
        let gas_cost = match *selector {
            SyscallSelector::CallContract => self.call_contract,
            SyscallSelector::Deploy => self.deploy,
            SyscallSelector::EmitEvent => self.emit_event,
            SyscallSelector::GetBlockHash => self.get_block_hash,
            SyscallSelector::GetExecutionInfo => self.get_execution_info,
            SyscallSelector::GetClassHashAt => self.get_class_hash_at,
            SyscallSelector::KeccakRound => self.keccak_round,
            SyscallSelector::Keccak => self.keccak,
            SyscallSelector::Sha256ProcessBlock => self.sha256_process_block,
            SyscallSelector::LibraryCall => self.library_call,
            SyscallSelector::MetaTxV0 => self.meta_tx_v0,
            SyscallSelector::ReplaceClass => self.replace_class,
            SyscallSelector::Secp256k1Add => self.secp256k1_add,
            SyscallSelector::Secp256k1GetPointFromX => self.secp256k1_get_point_from_x,
            SyscallSelector::Secp256k1GetXy => self.secp256k1_get_xy,
            SyscallSelector::Secp256k1Mul => self.secp256k1_mul,
            SyscallSelector::Secp256k1New => self.secp256k1_new,
            SyscallSelector::Secp256r1Add => self.secp256r1_add,
            SyscallSelector::Secp256r1GetPointFromX => self.secp256r1_get_point_from_x,
            SyscallSelector::Secp256r1GetXy => self.secp256r1_get_xy,
            SyscallSelector::Secp256r1Mul => self.secp256r1_mul,
            SyscallSelector::Secp256r1New => self.secp256r1_new,
            SyscallSelector::SendMessageToL1 => self.send_message_to_l1,
            SyscallSelector::StorageRead => self.storage_read,
            SyscallSelector::StorageWrite => self.storage_write,
            SyscallSelector::DelegateCall
            | SyscallSelector::DelegateL1Handler
            | SyscallSelector::GetBlockNumber
            | SyscallSelector::GetBlockTimestamp
            | SyscallSelector::GetCallerAddress
            | SyscallSelector::GetContractAddress
            | SyscallSelector::GetTxInfo
            | SyscallSelector::GetSequencerAddress
            | SyscallSelector::GetTxSignature
            | SyscallSelector::LibraryCallL1Handler => {
                return Err(GasCostsError::DeprecatedSyscall { selector: *selector });
            }
        };

        Ok(gas_cost)
    }
}

#[cfg_attr(any(test, feature = "testing"), derive(Clone, Copy))]
#[derive(Debug, Default, PartialEq)]
pub struct BaseGasCosts {
    pub step_gas_cost: u64,
    pub memory_hole_gas_cost: u64,
    // An estimation of the initial gas for a transaction to run with. This solution is
    // temporary and this value will be deduced from the transaction's fields.
    pub default_initial_gas_cost: u64,
    // Compiler gas costs.
    pub entry_point_initial_budget: u64,
    pub syscall_base_gas_cost: u64,
}

#[cfg_attr(any(test, feature = "testing"), derive(Serialize))]
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
pub struct BuiltinGasCosts {
    // Range check has a hard-coded cost higher than its proof percentage to avoid the overhead of
    // retrieving its price from the table.
    pub range_check: u64,
    pub range_check96: u64,
    // Priced builtins.
    pub keccak: u64,
    pub pedersen: u64,
    pub bitwise: u64,
    pub ecop: u64,
    pub poseidon: u64,
    pub add_mod: u64,
    pub mul_mod: u64,
    pub ecdsa: u64,
}

impl BuiltinGasCosts {
    pub fn get_builtin_gas_cost(&self, builtin: &BuiltinName) -> Result<u64, GasCostsError> {
        let gas_cost = match *builtin {
            BuiltinName::range_check => self.range_check,
            BuiltinName::pedersen => self.pedersen,
            BuiltinName::bitwise => self.bitwise,
            BuiltinName::ec_op => self.ecop,
            BuiltinName::keccak => self.keccak,
            BuiltinName::poseidon => self.poseidon,
            BuiltinName::range_check96 => self.range_check96,
            BuiltinName::add_mod => self.add_mod,
            BuiltinName::mul_mod => self.mul_mod,
            BuiltinName::ecdsa => self.ecdsa,
            BuiltinName::segment_arena => return Err(GasCostsError::VirtualBuiltin),
            BuiltinName::output => {
                return Err(GasCostsError::UnsupportedBuiltinInCairo1 { builtin: *builtin });
            }
        };

        Ok(gas_cost)
    }
}

/// Gas cost constants. For more documentation see in core/os/constants.cairo.
#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
#[derive(Debug, Default, PartialEq)]
pub struct GasCosts {
    pub base: BaseGasCosts,
    pub builtins: BuiltinGasCosts,
    pub syscalls: SyscallGasCosts,
}

impl GasCosts {
    fn from_raw(os_constants: &RawOsConstants, os_resources: &RawOsResources) -> Self {
        let step_gas_cost = os_constants.step_gas_cost;

        // Explicitly destructure initial costs, to make sure all inner costs are accounted for.
        let RawStepGasCost { step_gas_cost: default_initial_gas_cost_in_steps } =
            os_constants.default_initial_gas_cost;
        let RawStepGasCost { step_gas_cost: entry_point_initial_budget_in_steps } =
            os_constants.entry_point_initial_budget;
        let RawStepGasCost { step_gas_cost: syscall_base_gas_cost_in_steps } =
            os_constants.syscall_base_gas_cost;
        let base_costs = BaseGasCosts {
            step_gas_cost,
            memory_hole_gas_cost: os_constants.memory_hole_gas_cost.0,
            default_initial_gas_cost: step_gas_cost * default_initial_gas_cost_in_steps.0,
            entry_point_initial_budget: step_gas_cost * entry_point_initial_budget_in_steps.0,
            syscall_base_gas_cost: step_gas_cost * syscall_base_gas_cost_in_steps.0,
        };

        let summarize = |selector: SyscallSelector| match os_constants.syscall_gas_costs {
            Some(ref syscall_gas_costs) => Self::old_syscall_gas_cost_summary(
                &base_costs,
                selector,
                syscall_gas_costs,
                &os_constants.builtin_gas_costs,
            ),
            None => Self::new_syscall_gas_cost_summary(
                &base_costs,
                selector,
                &os_constants.builtin_gas_costs,
                os_resources,
            ),
        };

        let syscalls = SyscallGasCosts {
            call_contract: summarize(SyscallSelector::CallContract),
            deploy: summarize(SyscallSelector::Deploy),
            get_block_hash: summarize(SyscallSelector::GetBlockHash),
            get_execution_info: summarize(SyscallSelector::GetExecutionInfo),
            library_call: summarize(SyscallSelector::LibraryCall),
            replace_class: summarize(SyscallSelector::ReplaceClass),
            storage_read: summarize(SyscallSelector::StorageRead),
            storage_write: summarize(SyscallSelector::StorageWrite),
            get_class_hash_at: summarize(SyscallSelector::GetClassHashAt),
            emit_event: summarize(SyscallSelector::EmitEvent),
            send_message_to_l1: summarize(SyscallSelector::SendMessageToL1),
            secp256k1_add: summarize(SyscallSelector::Secp256k1Add),
            secp256k1_get_point_from_x: summarize(SyscallSelector::Secp256k1GetPointFromX),
            secp256k1_get_xy: summarize(SyscallSelector::Secp256k1GetXy),
            secp256k1_mul: summarize(SyscallSelector::Secp256k1Mul),
            secp256k1_new: summarize(SyscallSelector::Secp256k1New),
            secp256r1_add: summarize(SyscallSelector::Secp256r1Add),
            secp256r1_get_point_from_x: summarize(SyscallSelector::Secp256r1GetPointFromX),
            secp256r1_get_xy: summarize(SyscallSelector::Secp256r1GetXy),
            secp256r1_mul: summarize(SyscallSelector::Secp256r1Mul),
            secp256r1_new: summarize(SyscallSelector::Secp256r1New),
            keccak: summarize(SyscallSelector::Keccak),
            keccak_round: summarize(SyscallSelector::KeccakRound),
            meta_tx_v0: summarize(SyscallSelector::MetaTxV0),
            sha256_process_block: summarize(SyscallSelector::Sha256ProcessBlock),
        };

        Self { syscalls, base: base_costs, builtins: os_constants.builtin_gas_costs }
    }

    fn old_syscall_gas_cost_summary(
        base_costs: &BaseGasCosts,
        selector: SyscallSelector,
        syscall_gas_costs: &SyscallGasCostsMap,
        builtin_costs: &BuiltinGasCosts,
    ) -> SyscallGasCost {
        let raw_cost = syscall_gas_costs.get(&selector).unwrap_or_else(|| {
            panic!("{selector:?} missing from syscall_gas_costs map. Map: {syscall_gas_costs:?}")
        });
        SyscallGasCost::new_from_base_cost(match raw_cost {
            RawSyscallGasCost::Flat(flat_cost) => *flat_cost,
            RawSyscallGasCost::Structured(RawStructuredDeprecatedSyscallGasCost {
                step_gas_cost,
                range_check,
                bitwise,
                syscall_base_gas_cost,
                memory_hole_gas_cost,
            }) => {
                step_gas_cost * base_costs.step_gas_cost
                    + range_check * builtin_costs.range_check
                    + bitwise * builtin_costs.bitwise
                    + syscall_base_gas_cost * base_costs.syscall_base_gas_cost
                    + memory_hole_gas_cost * base_costs.memory_hole_gas_cost
            }
        })
    }

    fn new_syscall_gas_cost_summary(
        base_costs: &BaseGasCosts,
        selector: SyscallSelector,
        builtin_costs: &BuiltinGasCosts,
        os_resources: &RawOsResources,
    ) -> SyscallGasCost {
        let vm_resources: ResourcesParams = os_resources
            .execute_syscalls
            .get(&selector)
            .expect("Fetching the execution resources of a syscall should not fail.")
            .into();

        let mut base_gas =
            get_gas_cost_from_vm_resources(&vm_resources.constant, base_costs, builtin_costs);

        // The minimum total cost is `syscall_base_gas_cost`, which is pre-charged by the compiler.
        base_gas = std::cmp::max(base_costs.syscall_base_gas_cost, base_gas);
        let linear_gas_cost = get_gas_cost_from_vm_resources(
            &match vm_resources.calldata_factor {
                VariableCallDataFactor::Scaled(CallDataFactor { resources, scaling_factor }) => {
                    assert!(
                        scaling_factor == 1,
                        "The scaling factor of the syscall should be 1, but it is {scaling_factor}"
                    );
                    resources
                }
                VariableCallDataFactor::Unscaled(resources) => resources,
            },
            base_costs,
            builtin_costs,
        );

        // Sum up the two methods to get the final cost.
        SyscallGasCost { base: base_gas, linear_factor: linear_gas_cost }
    }
}

#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
#[derive(Debug, Default, PartialEq)]
pub struct OsConstants {
    pub gas_costs: GasCosts,

    // Selectors.
    pub constructor_entry_point_selector: EntryPointSelector,
    pub default_entry_point_selector: EntryPointSelector,
    pub execute_entry_point_selector: EntryPointSelector,
    pub transfer_entry_point_selector: EntryPointSelector,
    pub validate_declare_entry_point_selector: EntryPointSelector,
    pub validate_deploy_entry_point_selector: EntryPointSelector,
    pub validate_entry_point_selector: EntryPointSelector,

    // Execution limits.
    pub validate_max_sierra_gas: GasAmount,
    pub execute_max_sierra_gas: GasAmount,

    // Validation.
    pub validate_rounding_consts: ValidateRoundingConsts,
    pub validated: String,

    // Error strings.
    // TODO(Nimrod): Use these strings instead of the constants in `hint_processor.rs`.
    pub error_block_number_out_of_range: String,
    pub error_invalid_input_len: String,
    pub error_invalid_argument: String,
    pub error_out_of_gas: String,
    pub error_entry_point_failed: String,
    pub error_entry_point_not_found: String,

    // Resource bounds names.
    pub l1_gas: String,
    pub l2_gas: String,
    pub l1_data_gas: String,

    // Resource bounds indices.
    pub l1_gas_index: usize,
    pub l1_data_gas_index: usize,
    pub l2_gas_index: usize,

    // Initial costs.
    pub entry_point_initial_budget: GasAmount,
    pub default_initial_gas_cost: GasAmount,

    // L1 handler.
    pub l1_handler_version: u8,
    pub l1_handler_max_amount_bounds: GasVector,

    // Miscellaneous.
    pub nop_entry_point_offset: i8,
    pub os_contract_addresses: OsContractAddresses,
    pub sierra_array_len_bound: u64,
    pub stored_block_hash_buffer: u8,

    // Entry point type identifiers (in the OS).
    pub entry_point_type_constructor: u8,
    pub entry_point_type_external: u8,
    pub entry_point_type_l1_handler: u8,

    // Deprecated contract logic support.
    pub v1_bound_accounts_cairo0: Vec<ClassHash>,
    pub v1_bound_accounts_cairo1: Vec<ClassHash>,
    pub v1_bound_accounts_max_tip: Tip,
    pub data_gas_accounts: Vec<ClassHash>,
}

impl OsConstants {
    fn from_raw(raw_constants: &RawOsConstants, raw_resources: &RawOsResources) -> Self {
        let gas_costs = GasCosts::from_raw(raw_constants, raw_resources);

        // Preprocess inital budget costs.
        let RawStepGasCost { step_gas_cost: entry_point_initial_budget_steps } =
            raw_constants.entry_point_initial_budget;
        let RawStepGasCost { step_gas_cost: default_initial_gas_cost_steps } =
            raw_constants.default_initial_gas_cost;
        let entry_point_initial_budget = entry_point_initial_budget_steps
            .checked_factor_mul(gas_costs.base.step_gas_cost)
            .expect("The entry point initial budget - in gas - should not overflow.");
        let default_initial_gas_cost = default_initial_gas_cost_steps
            .checked_factor_mul(gas_costs.base.step_gas_cost)
            .expect("The default initial gas should not overflow.");

        Self {
            gas_costs,
            constructor_entry_point_selector: raw_constants.constructor_entry_point_selector,
            default_entry_point_selector: raw_constants.default_entry_point_selector,
            execute_entry_point_selector: raw_constants.execute_entry_point_selector,
            transfer_entry_point_selector: raw_constants.transfer_entry_point_selector,
            validate_declare_entry_point_selector: raw_constants
                .validate_declare_entry_point_selector,
            validate_deploy_entry_point_selector: raw_constants
                .validate_deploy_entry_point_selector,
            validate_entry_point_selector: raw_constants.validate_entry_point_selector,
            validate_max_sierra_gas: raw_constants.validate_max_sierra_gas,
            execute_max_sierra_gas: raw_constants.execute_max_sierra_gas,
            validate_rounding_consts: raw_constants.validate_rounding_consts,
            validated: raw_constants.validated.clone(),
            error_block_number_out_of_range: raw_constants.error_block_number_out_of_range.clone(),
            error_invalid_input_len: raw_constants.error_invalid_input_len.clone(),
            error_invalid_argument: raw_constants.error_invalid_argument.clone(),
            error_out_of_gas: raw_constants.error_out_of_gas.clone(),
            error_entry_point_failed: raw_constants.error_entry_point_failed.clone(),
            error_entry_point_not_found: raw_constants.error_entry_point_not_found.clone(),
            l1_gas: raw_constants.l1_gas.clone(),
            l2_gas: raw_constants.l2_gas.clone(),
            l1_data_gas: raw_constants.l1_data_gas.clone(),
            l1_gas_index: raw_constants.l1_gas_index,
            l1_data_gas_index: raw_constants.l1_data_gas_index,
            l2_gas_index: raw_constants.l2_gas_index,
            entry_point_initial_budget,
            default_initial_gas_cost,
            l1_handler_version: raw_constants.l1_handler_version,
            l1_handler_max_amount_bounds: raw_constants.l1_handler_max_amount_bounds,
            nop_entry_point_offset: raw_constants.nop_entry_point_offset,
            os_contract_addresses: raw_constants.os_contract_addresses,
            sierra_array_len_bound: raw_constants.sierra_array_len_bound,
            stored_block_hash_buffer: raw_constants.stored_block_hash_buffer,
            entry_point_type_constructor: raw_constants.entry_point_type_constructor,
            entry_point_type_external: raw_constants.entry_point_type_external,
            entry_point_type_l1_handler: raw_constants.entry_point_type_l1_handler,
            v1_bound_accounts_cairo0: raw_constants.v1_bound_accounts_cairo0.clone(),
            v1_bound_accounts_cairo1: raw_constants.v1_bound_accounts_cairo1.clone(),
            v1_bound_accounts_max_tip: raw_constants.v1_bound_accounts_max_tip,
            data_gas_accounts: raw_constants.data_gas_accounts.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct OsContractAddresses {
    block_hash_contract_address: u8,
    alias_contract_address: u8,
    reserved_contract_address: u8,
}

impl OsContractAddresses {
    pub fn block_hash_contract_address(&self) -> ContractAddress {
        ContractAddress::from(self.block_hash_contract_address)
    }

    pub fn alias_contract_address(&self) -> ContractAddress {
        ContractAddress::from(self.alias_contract_address)
    }

    pub fn reserved_contract_address(&self) -> ContractAddress {
        ContractAddress::from(self.reserved_contract_address)
    }
}

impl Default for OsContractAddresses {
    fn default() -> Self {
        VersionedConstants::latest_constants().os_constants.os_contract_addresses
    }
}

#[derive(Debug, Error)]
pub enum VersionedConstantsError {
    #[error(transparent)]
    IoError(#[from] io::Error),
    #[error("JSON file cannot be serialized into VersionedConstants: {0}")]
    ParseError(#[from] serde_json::Error),
    #[error("Invalid version: {version:?}")]
    InvalidVersion { version: String },
    #[error("Invalid Starknet version: {0}")]
    InvalidStarknetVersion(StarknetVersion),
}

pub type VersionedConstantsResult<T> = Result<T, VersionedConstantsError>;

#[derive(Debug, Error)]
pub enum GasCostsError {
    #[error("used syscall: {:?} is not supported in a Cairo 0 contract.", selector)]
    DeprecatedSyscall { selector: SyscallSelector },
    #[error("used builtin: {:?} is not supported in a Cairo 1 contract.", builtin)]
    UnsupportedBuiltinInCairo1 { builtin: BuiltinName },
    #[error("a virtual builtin does not have a gas cost.")]
    VirtualBuiltin,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CallDataFactor {
    pub resources: ExecutionResources,
    pub scaling_factor: usize,
}

impl Default for CallDataFactor {
    fn default() -> Self {
        Self { resources: ExecutionResources::default(), scaling_factor: 1 }
    }
}

impl CallDataFactor {
    pub fn calculate_resources(&self, linear_factor: usize) -> ExecutionResources {
        (&self.resources * linear_factor).div_ceil(self.scaling_factor).clone()
    }
}

#[cfg_attr(any(test, feature = "testing"), derive(PartialEq))]
#[derive(Deserialize, Debug, Clone)]
#[serde(untagged, deny_unknown_fields)]
pub enum VariableCallDataFactor {
    Scaled(CallDataFactor),
    Unscaled(ExecutionResources),
}

impl Default for VariableCallDataFactor {
    fn default() -> Self {
        Self::Scaled(CallDataFactor::default())
    }
}

impl From<&VariableCallDataFactor> for CallDataFactor {
    fn from(value: &VariableCallDataFactor) -> Self {
        match value {
            VariableCallDataFactor::Scaled(calldata_factor) => calldata_factor.clone(),
            VariableCallDataFactor::Unscaled(resources) => {
                Self { resources: resources.clone(), scaling_factor: 1 }
            }
        }
    }
}

#[cfg_attr(any(test, feature = "testing"), derive(PartialEq))]
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourcesParams {
    pub constant: ExecutionResources,
    pub calldata_factor: VariableCallDataFactor,
}

#[cfg_attr(any(test, feature = "testing"), derive(PartialEq))]
#[derive(Deserialize, Debug, Clone)]
#[serde(untagged, deny_unknown_fields)]
pub enum VariableResourceParams {
    Constant(ExecutionResources),
    WithFactor(ResourcesParams),
}

impl From<&VariableResourceParams> for ResourcesParams {
    fn from(value: &VariableResourceParams) -> Self {
        match value {
            VariableResourceParams::WithFactor(raw_params) => raw_params.clone(),
            VariableResourceParams::Constant(constant) => {
                Self { constant: constant.clone(), calldata_factor: Default::default() }
            }
        }
    }
}

#[cfg_attr(any(test, feature = "testing"), derive(Serialize))]
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct ValidateRoundingConsts {
    // Flooring factor for block number in validate mode.
    pub validate_block_number_rounding: u64,
    // Flooring factor for timestamp in validate mode.
    pub validate_timestamp_rounding: u64,
}

impl Default for ValidateRoundingConsts {
    fn default() -> Self {
        Self { validate_block_number_rounding: 1, validate_timestamp_rounding: 1 }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VersionedConstantsOverrides {
    pub validate_max_n_steps: u32,
    pub max_recursion_depth: usize,
    pub invoke_tx_max_n_steps: u32,
    pub max_n_events: usize,
}

impl Default for VersionedConstantsOverrides {
    fn default() -> Self {
        let latest_versioned_constants = VersionedConstants::latest_constants();
        Self {
            validate_max_n_steps: latest_versioned_constants.validate_max_n_steps,
            max_recursion_depth: latest_versioned_constants.max_recursion_depth,
            invoke_tx_max_n_steps: latest_versioned_constants.invoke_tx_max_n_steps,
            max_n_events: latest_versioned_constants.tx_event_limits.max_n_emitted_events,
        }
    }
}

impl SerializeConfig for VersionedConstantsOverrides {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "validate_max_n_steps",
                &self.validate_max_n_steps,
                "Maximum number of steps the validation function is allowed to run.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_recursion_depth",
                &self.max_recursion_depth,
                "Maximum recursion depth for nested calls during blockifier validation.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "invoke_tx_max_n_steps",
                &self.invoke_tx_max_n_steps,
                "Maximum number of steps the invoke function is allowed to run.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_n_events",
                &self.max_n_events,
                "Maximum number of events that can be emitted from the transation.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
