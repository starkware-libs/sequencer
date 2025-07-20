use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::{fs, io};

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use indexmap::{IndexMap, IndexSet};
use num_rational::Ratio;
use num_traits::Inv;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use paste::paste;
use semver::Version;
use serde::de::Error as DeserializationError;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Number, Value};
use starknet_api::block::{GasPrice, StarknetVersion};
use starknet_api::contract_class::SierraVersion;
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::execution_resources::{GasAmount, GasVector};
use starknet_api::transaction::fields::{GasVectorComputationMode, Tip};
use starknet_infra_utils::compile_time_cargo_manifest_dir;
use strum::IntoEnumIterator;
use thiserror::Error;

use crate::execution::common_hints::ExecutionMode;
use crate::execution::execution_utils::poseidon_hash_many_cost;
use crate::execution::syscalls::hint_processor::SyscallUsageMap;
use crate::execution::syscalls::SyscallSelector;
use crate::fee::resources::StarknetResources;
use crate::transaction::transaction_types::TransactionType;
use crate::utils::get_gas_cost_from_vm_resources;

#[cfg(test)]
#[path = "versioned_constants_test.rs"]
pub mod test;

/// Auto-generate getters for listed versioned constants versions.
macro_rules! define_versioned_constants {
    ($(($variant:ident, $path_to_json:expr)),* $(,)?) => {
        // Static (lazy) instances of the versioned constants.
        // For internal use only; for access to a static instance use the `StarknetVersion` enum.
        paste! {
            $(
                pub(crate) const [<VERSIONED_CONSTANTS_ $variant:upper _JSON>]: &str =
                    include_str!($path_to_json);
                pub static [<VERSIONED_CONSTANTS_ $variant:upper>]: LazyLock<VersionedConstants> = LazyLock::new(|| {
                    serde_json::from_str([<VERSIONED_CONSTANTS_ $variant:upper _JSON>])
                        .expect(&format!("Versioned constants {} is malformed.", $path_to_json))
                });
            )*
        }

        /// API to access a static instance of the versioned constants.
        impl TryFrom<StarknetVersion> for &'static VersionedConstants {
            type Error = VersionedConstantsError;

            fn try_from(version: StarknetVersion) -> VersionedConstantsResult<Self> {
                match version {
                    $(
                        StarknetVersion::$variant => {
                           Ok(& paste! { [<VERSIONED_CONSTANTS_ $variant:upper>] })
                        }
                    )*
                    _ => Err(VersionedConstantsError::InvalidStarknetVersion(version)),
                }
            }
        }

        impl VersionedConstants {
            pub fn path_to_json(version: &StarknetVersion) -> VersionedConstantsResult<&'static str> {
                match version {
                    $(StarknetVersion::$variant => Ok($path_to_json),)*
                    _ => Err(VersionedConstantsError::InvalidStarknetVersion(*version)),
                }
            }

            /// Gets the constants that shipped with the current version of the Blockifier.
            /// To use custom constants, initialize the struct from a file using `from_path`.
            pub fn latest_constants() -> &'static Self {
                Self::get(&StarknetVersion::LATEST)
                    .expect("Latest version should support VC.")
            }

            /// Gets the constants for the specified Starknet version.
            pub fn get(version: &StarknetVersion) -> VersionedConstantsResult<&'static Self> {
                match version {
                    $(
                        StarknetVersion::$variant => Ok(
                            & paste! { [<VERSIONED_CONSTANTS_ $variant:upper>] }
                        ),
                    )*
                    _ => Err(VersionedConstantsError::InvalidStarknetVersion(*version)),
                }
            }
        }

        pub static VERSIONED_CONSTANTS_LATEST_JSON: LazyLock<String> = LazyLock::new(|| {
            let latest_variant = StarknetVersion::LATEST;
            let path_to_json: PathBuf = [
                compile_time_cargo_manifest_dir!(),
                "src".into(),
                VersionedConstants::path_to_json(&latest_variant)
                    .expect("Latest variant should have a path to json.").into()
            ].iter().collect();
            fs::read_to_string(path_to_json.clone())
                .expect(&format!("Failed to read file {}.", path_to_json.display()))
        });
    };
}

define_versioned_constants! {
    (V0_13_0, "../resources/versioned_constants_0_13_0.json"),
    (V0_13_1, "../resources/versioned_constants_0_13_1.json"),
    (V0_13_1_1, "../resources/versioned_constants_0_13_1_1.json"),
    (V0_13_2, "../resources/versioned_constants_0_13_2.json"),
    (V0_13_2_1, "../resources/versioned_constants_0_13_2_1.json"),
    (V0_13_3, "../resources/versioned_constants_0_13_3.json"),
    (V0_13_4, "../resources/versioned_constants_0_13_4.json"),
    (V0_13_5, "../resources/versioned_constants_0_13_5.json"),
}

pub type ResourceCost = Ratio<u64>;

// TODO: Delete this ratio-converter function once event keys / data length are no longer 128 bits
//   (no other usage is expected).
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

#[derive(Clone, Debug, Default, Deserialize)]
pub struct VmResourceCosts {
    pub n_steps: ResourceCost,
    #[serde(deserialize_with = "builtin_map_from_string_map")]
    pub builtins: HashMap<BuiltinName, ResourceCost>,
}

#[derive(Clone, Debug, Default, Deserialize)]
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

// TODO: This (along with the Serialize impl) is implemented in pub(crate) scope in the VM (named
//   serde_generic_map_impl); use it if and when it's public.
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
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(remote = "Self")]
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
    pub ignore_inner_event_resources: bool,

    // Compiler settings.
    pub enable_reverts: bool,

    // Cairo OS constants.
    // Note: if loaded from a json file, there are some assumptions made on its structure.
    // See the struct's docstring for more details.
    pub os_constants: Arc<OsConstants>,

    // Fee related.
    pub(crate) vm_resource_fee_cost: Arc<VmResourceCosts>,
    // Cost of allocating a storage cell.
    pub allocation_cost: AllocationCost,

    // Resources.
    os_resources: Arc<OsResources>,

    // Just to make sure the value exists, but don't use the actual values.
    #[allow(dead_code)]
    gateway: serde::de::IgnoredAny,
}

impl VersionedConstants {
    pub fn from_path(path: &Path) -> VersionedConstantsResult<Self> {
        Ok(serde_json::from_reader(std::fs::File::open(path)?)?)
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

    pub fn toggle_sierra_gas(&self, use_sierra_gas: bool) -> Self {
        match use_sierra_gas {
            true => {
                log::debug!("Forcing Sierra gas to be used.");
                VersionedConstants {
                    min_sierra_version_for_sierra_gas: SierraVersion::new(0, 0, 0),
                    ..self.clone()
                }
            }
            false => {
                log::debug!("Forcing Sierra gas to be disabled.");
                VersionedConstants {
                    min_sierra_version_for_sierra_gas: SierraVersion::new(100, 0, 0),
                    ..self.clone()
                }
            }
        }
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

    /// Calculates the syscall gas cost from the OS resources.
    pub fn get_syscall_gas_cost(&self, syscall_selector: &SyscallSelector) -> SyscallGasCost {
        let gas_costs = &self.os_constants.gas_costs;
        let vm_resources = &self
            .os_resources
            .execute_syscalls
            .get(syscall_selector)
            .expect("Fetching the execution resources of a syscall should not fail.");

        let mut base_gas_cost = get_gas_cost_from_vm_resources(&vm_resources.constant, gas_costs);

        // The minimum total cost is `syscall_base_gas_cost`, which is pre-charged by the compiler.
        base_gas_cost = std::cmp::max(gas_costs.base.syscall_base_gas_cost, base_gas_cost);
        let linear_gas_cost =
            get_gas_cost_from_vm_resources(&vm_resources.calldata_factor, gas_costs);
        SyscallGasCost { base: base_gas_cost, linear_factor: linear_gas_cost }
    }
}

impl<'de> Deserialize<'de> for VersionedConstants {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut versioned_constants = Self::deserialize(deserializer)?;

        let syscall_gas_costs = &(versioned_constants.os_constants.gas_costs.syscalls);
        if syscall_gas_costs == &SyscallGasCosts::default() {
            let syscalls = SyscallGasCosts {
                call_contract: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::CallContract),
                deploy: versioned_constants.get_syscall_gas_cost(&SyscallSelector::Deploy),
                get_block_hash: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::GetBlockHash),
                get_execution_info: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::GetExecutionInfo),
                library_call: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::LibraryCall),
                replace_class: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::ReplaceClass),
                storage_read: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::StorageRead),
                storage_write: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::StorageWrite),
                get_class_hash_at: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::GetClassHashAt),
                emit_event: versioned_constants.get_syscall_gas_cost(&SyscallSelector::EmitEvent),
                send_message_to_l1: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::SendMessageToL1),
                secp256k1_add: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::Secp256k1Add),
                secp256k1_get_point_from_x: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::Secp256k1GetPointFromX),
                secp256k1_get_xy: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::Secp256k1GetXy),
                secp256k1_mul: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::Secp256k1Mul),
                secp256k1_new: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::Secp256k1New),
                secp256r1_add: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::Secp256r1Add),
                secp256r1_get_point_from_x: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::Secp256r1GetPointFromX),
                secp256r1_get_xy: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::Secp256r1GetXy),
                secp256r1_mul: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::Secp256r1Mul),
                secp256r1_new: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::Secp256r1New),
                keccak: versioned_constants.get_syscall_gas_cost(&SyscallSelector::Keccak),
                keccak_round_cost: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::KeccakRound),
                sha256_process_block: versioned_constants
                    .get_syscall_gas_cost(&SyscallSelector::Sha256ProcessBlock),
            };

            Arc::get_mut(&mut versioned_constants.os_constants)
                .expect("Failed to get mutable reference")
                .gas_costs
                .syscalls = syscalls;
        }

        Ok(versioned_constants)
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

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct EventLimits {
    pub max_data_length: usize,
    pub max_keys_length: usize,
    pub max_n_emitted_events: usize,
}

#[derive(Clone, Debug, Default, Deserialize)]
// Serde trick for adding validations via a customr deserializer, without forgoing the derive.
// See: https://github.com/serde-rs/serde/issues/1220.
#[serde(remote = "Self")]
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
    execute_txs_inner: HashMap<TransactionType, ResourcesByVersion>,

    // Resources needed for the OS to compute the KZG commitment info, as a factor of the data
    // segment length. Does not include poseidon_hash_many cost.
    compute_os_kzg_commitment_info: ExecutionResources,
}

impl OsResources {
    pub fn validate<'de, D: Deserializer<'de>>(
        &self,
    ) -> Result<(), <D as Deserializer<'de>>::Error> {
        for tx_type in TransactionType::iter() {
            if !self.execute_txs_inner.contains_key(&tx_type) {
                return Err(DeserializationError::custom(format!(
                    "ValidationError: os_resources.execute_tx_inner is missing transaction_type: \
                     {tx_type:?}"
                )));
            }
        }

        for syscall_handler in SyscallSelector::iter() {
            if !self.execute_syscalls.contains_key(&syscall_handler) {
                return Err(DeserializationError::custom(format!(
                    "ValidationError: os_resources.execute_syscalls are missing syscall handler: \
                     {syscall_handler:?}"
                )));
            }
        }

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

        let execution_resources = self
            .execute_txs_inner
            .values()
            .flat_map(|resources_vector| {
                [
                    &resources_vector.deprecated_resources.constant,
                    &resources_vector.deprecated_resources.calldata_factor,
                ]
            })
            .chain(self.execute_syscalls.values().flat_map(|resources_params| {
                [&resources_params.constant, &resources_params.calldata_factor]
            }))
            .chain(std::iter::once(&self.compute_os_kzg_commitment_info));
        let builtin_names =
            execution_resources.flat_map(|resources| resources.builtin_instance_counter.keys());
        for builtin_name in builtin_names {
            if !(known_builtin_names.contains(builtin_name.to_str_with_suffix())) {
                return Err(DeserializationError::custom(format!(
                    "ValidationError: unknown os resource {builtin_name}"
                )));
            }
        }

        Ok(())
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
            os_additional_resources += &(&(&syscall_resources.constant * syscall_usage.call_count)
                + &(&syscall_resources.calldata_factor * syscall_usage.linear_factor));
        }

        os_additional_resources
    }

    fn resources_params_for_tx_type(&self, tx_type: &TransactionType) -> &ResourcesParams {
        &(self
            .execute_txs_inner
            .get(tx_type)
            .unwrap_or_else(|| panic!("should contain transaction type '{tx_type:?}'."))
            .deprecated_resources)
    }

    fn resources_for_tx_type(
        &self,
        tx_type: &TransactionType,
        calldata_length: usize,
    ) -> ExecutionResources {
        let resources_vector = self.resources_params_for_tx_type(tx_type);
        &resources_vector.constant + &(&(resources_vector.calldata_factor) * calldata_length)
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

impl<'de> Deserialize<'de> for OsResources {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let os_resources = Self::deserialize(deserializer)?;

        // Validations.

        #[cfg(not(test))]
        os_resources.validate::<D>()?;

        Ok(os_resources)
    }
}

#[derive(Deserialize, PartialEq, Debug, Clone, Copy, Serialize, Default)]
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
}

#[cfg_attr(any(test, feature = "testing"), derive(Clone, Copy))]
#[derive(Debug, Default, Deserialize, PartialEq)]
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
    pub keccak_round_cost: SyscallGasCost,
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
            SyscallSelector::KeccakRound => self.keccak_round_cost,
            SyscallSelector::Keccak => self.keccak,
            SyscallSelector::Sha256ProcessBlock => self.sha256_process_block,
            SyscallSelector::LibraryCall => self.library_call,
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
#[derive(Debug, Default, Deserialize)]
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

#[cfg_attr(any(test, feature = "testing"), derive(Clone, Copy))]
#[derive(Debug, Default, Deserialize)]
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
#[cfg_attr(any(test, feature = "testing"), derive(Clone, Copy))]
#[derive(Debug, Default, Deserialize)]
pub struct GasCosts {
    pub base: BaseGasCosts,
    pub builtins: BuiltinGasCosts,
    pub syscalls: SyscallGasCosts,
}

// Below, serde first deserializes the json into a regular IndexMap wrapped by the newtype
// `OsConstantsRawJson`, then calls the `try_from` of the newtype, which handles the
// conversion into actual values.
// TODO(Dori): consider encoding the * and + operations inside the json file, instead of hardcoded
// below in the `try_from`.
#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
#[derive(Debug, Default, Deserialize)]
#[serde(try_from = "OsConstantsRawJson")]
pub struct OsConstants {
    pub gas_costs: GasCosts,
    pub validate_rounding_consts: ValidateRoundingConsts,
    pub os_contract_addresses: OsContractAddresses,
    pub validate_max_sierra_gas: GasAmount,
    pub execute_max_sierra_gas: GasAmount,
    pub v1_bound_accounts_cairo0: Vec<ClassHash>,
    pub v1_bound_accounts_cairo1: Vec<ClassHash>,
    pub v1_bound_accounts_max_tip: Tip,
}

impl OsConstants {
    // List of os constants to be ignored
    // during the creation of the struct containing the base gas costs.

    const ADDITIONAL_FIELDS: [&'static str; 32] = [
        "builtin_gas_costs",
        "constructor_entry_point_selector",
        "default_entry_point_selector",
        "entry_point_type_constructor",
        "entry_point_type_external",
        "entry_point_type_l1_handler",
        "error_block_number_out_of_range",
        "error_invalid_input_len",
        "error_invalid_argument",
        "error_entry_point_failed",
        "error_entry_point_not_found",
        "error_out_of_gas",
        "execute_entry_point_selector",
        "execute_max_sierra_gas",
        "l1_gas",
        "l1_gas_index",
        "l1_handler_version",
        "l2_gas",
        "l2_gas_index",
        "l1_data_gas",
        "l1_data_gas_index",
        "nop_entry_point_offset",
        "sierra_array_len_bound",
        "stored_block_hash_buffer",
        "syscall_gas_costs",
        "transfer_entry_point_selector",
        "validate_declare_entry_point_selector",
        "validate_deploy_entry_point_selector",
        "validate_entry_point_selector",
        "validate_max_sierra_gas",
        "validate_rounding_consts",
        "validated",
    ];
}

impl TryFrom<&OsConstantsRawJson> for GasCosts {
    type Error = OsConstantsSerdeError;

    fn try_from(raw_json_data: &OsConstantsRawJson) -> Result<Self, Self::Error> {
        let base_value: Value = serde_json::to_value(&raw_json_data.parse_base()?)?;
        let base: BaseGasCosts = serde_json::from_value(base_value)?;
        let builtins_value: Value = serde_json::to_value(&raw_json_data.parse_builtin()?)?;
        let builtins: BuiltinGasCosts = serde_json::from_value(builtins_value)?;
        if (raw_json_data.raw_json_file_as_dict).contains_key("syscall_gas_costs") {
            let syscalls_value: Value =
                serde_json::to_value(&raw_json_data.parse_syscalls(&base, &builtins)?)?;
            let syscalls: SyscallGasCosts = serde_json::from_value(syscalls_value)?;
            Ok(GasCosts { base, builtins, syscalls })
        } else {
            Ok(GasCosts { base, builtins, syscalls: SyscallGasCosts::default() })
        }
    }
}

impl TryFrom<OsConstantsRawJson> for OsConstants {
    type Error = OsConstantsSerdeError;

    fn try_from(raw_json_data: OsConstantsRawJson) -> Result<Self, Self::Error> {
        let gas_costs = GasCosts::try_from(&raw_json_data)?;
        let validate_rounding_consts = raw_json_data.validate_rounding_consts;
        let os_contract_addresses = raw_json_data.os_contract_addresses;
        let key = "validate_max_sierra_gas";
        let validate_max_sierra_gas = GasAmount(serde_json::from_value(
            raw_json_data
                .raw_json_file_as_dict
                .get(key)
                .ok_or_else(|| OsConstantsSerdeError::KeyNotFoundInFile(key.to_string()))?
                .clone(),
        )?);
        let key = "execute_max_sierra_gas";
        let execute_max_sierra_gas = GasAmount(serde_json::from_value(
            raw_json_data
                .raw_json_file_as_dict
                .get(key)
                .ok_or_else(|| OsConstantsSerdeError::KeyNotFoundInFile(key.to_string()))?
                .clone(),
        )?);
        let v1_bound_accounts_cairo0 = raw_json_data.v1_bound_accounts_cairo0;
        let v1_bound_accounts_cairo1 = raw_json_data.v1_bound_accounts_cairo1;
        let v1_bound_accounts_max_tip = raw_json_data.v1_bound_accounts_max_tip;
        let os_constants = OsConstants {
            gas_costs,
            validate_rounding_consts,
            os_contract_addresses,
            validate_max_sierra_gas,
            execute_max_sierra_gas,
            v1_bound_accounts_cairo0,
            v1_bound_accounts_cairo1,
            v1_bound_accounts_max_tip,
        };
        Ok(os_constants)
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
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

// Intermediate representation of the JSON file in order to make the deserialization easier, using a
// regular try_from.
#[derive(Debug, Deserialize)]
struct OsConstantsRawJson {
    #[serde(flatten)]
    raw_json_file_as_dict: IndexMap<String, Value>,
    #[serde(default)]
    validate_rounding_consts: ValidateRoundingConsts,
    os_contract_addresses: OsContractAddresses,
    v1_bound_accounts_cairo0: Vec<ClassHash>,
    v1_bound_accounts_cairo1: Vec<ClassHash>,
    v1_bound_accounts_max_tip: Tip,
}

impl OsConstantsRawJson {
    fn parse_base(&self) -> Result<IndexMap<String, u64>, OsConstantsSerdeError> {
        let mut base = IndexMap::new();
        let additional_fields: IndexSet<_> =
            OsConstants::ADDITIONAL_FIELDS.iter().copied().collect();
        for (key, value) in &self.raw_json_file_as_dict {
            if additional_fields.contains(key.as_str()) {
                // Ignore additional constants.
                continue;
            }

            self.recursive_add_to_base(key, value, &mut base)?;
        }

        Ok(base)
    }

    fn parse_syscalls(
        &self,
        base: &BaseGasCosts,
        builtins: &BuiltinGasCosts,
    ) -> Result<IndexMap<String, SyscallGasCost>, OsConstantsSerdeError> {
        let mut gas_costs = IndexMap::new();
        let key = "syscall_gas_costs";
        let syscalls: IndexMap<String, Value> = serde_json::from_value(
            (self
                .raw_json_file_as_dict
                .get(key)
                .ok_or_else(|| OsConstantsSerdeError::KeyNotFoundInFile(key.to_string()))?)
            .clone(),
        )?;
        for (key, value) in syscalls {
            self.add_to_syscalls(&key, &value, &mut gas_costs, base, builtins)?;
        }
        Ok(gas_costs)
    }

    fn parse_builtin(&self) -> Result<IndexMap<String, u64>, OsConstantsSerdeError> {
        let mut gas_costs = IndexMap::new();
        let key = "builtin_gas_costs";
        let builtins: IndexMap<String, Value> = serde_json::from_value(
            (self
                .raw_json_file_as_dict
                .get(key)
                .ok_or_else(|| OsConstantsSerdeError::KeyNotFoundInFile(key.to_string()))?)
            .clone(),
        )?;
        for (key, value) in builtins {
            match value {
                Value::Number(n) => {
                    let cost = n.as_u64().ok_or_else(|| OsConstantsSerdeError::OutOfRange {
                        key: key.to_string(),
                        value: n.clone(),
                    })?;
                    gas_costs.insert(key.to_string(), cost);
                }
                _ => return Err(OsConstantsSerdeError::UnhandledValueType(value.clone())),
            }
        }
        Ok(gas_costs)
    }

    /// Recursively adds a key to gas costs, calculating its value after processing any nested keys.
    // Invariant: there is no circular dependency between key definitions.
    fn recursive_add_to_base(
        &self,
        key: &str,
        value: &Value,
        gas_costs: &mut IndexMap<String, u64>,
    ) -> Result<(), OsConstantsSerdeError> {
        if gas_costs.contains_key(key) {
            return Ok(());
        }

        match value {
            Value::Number(n) => {
                let value = n.as_u64().ok_or_else(|| OsConstantsSerdeError::OutOfRange {
                    key: key.to_string(),
                    value: n.clone(),
                })?;
                gas_costs.insert(key.to_string(), value);
            }
            Value::Object(obj) => {
                // Converts:
                // `k_1: {k_2: factor_1, k_3: factor_2}`
                // into:
                // k_1 = k_2 * factor_1 + k_3 * factor_2
                let mut value = 0;
                for (inner_key, factor) in obj {
                    let inner_value =
                        &self.raw_json_file_as_dict.get(inner_key).ok_or_else(|| {
                            OsConstantsSerdeError::KeyNotFound {
                                key: key.to_string(),
                                inner_key: inner_key.clone(),
                            }
                        })?;
                    self.recursive_add_to_base(inner_key, inner_value, gas_costs)?;
                    let inner_key_value = gas_costs.get(inner_key).ok_or_else(|| {
                        OsConstantsSerdeError::KeyNotFound {
                            key: key.to_string(),
                            inner_key: inner_key.to_string(),
                        }
                    })?;
                    let factor =
                        factor.as_u64().ok_or_else(|| OsConstantsSerdeError::OutOfRangeFactor {
                            key: key.to_string(),
                            value: factor.clone(),
                        })?;
                    value += inner_key_value * factor;
                }
                gas_costs.insert(key.to_string(), value);
            }
            Value::String(_) => {
                panic!(
                    "String values should have been previously filtered out in the whitelist \
                     check and should not be depended on"
                )
            }
            _ => return Err(OsConstantsSerdeError::UnhandledValueType(value.clone())),
        }

        Ok(())
    }

    fn add_to_syscalls(
        &self,
        key: &str,
        value: &Value,
        syscalls: &mut IndexMap<String, SyscallGasCost>,
        base: &BaseGasCosts,
        builtins: &BuiltinGasCosts,
    ) -> Result<(), OsConstantsSerdeError> {
        let mut cost = 0;
        match value {
            Value::Object(obj) => {
                for (inner_key, factor) in obj {
                    let inner_value = match inner_key.as_str() {
                        "step_gas_cost" => base.step_gas_cost,
                        "memory_hole_gas_cost" => base.memory_hole_gas_cost,
                        "default_initial_gas_cost" => base.default_initial_gas_cost,
                        "entry_point_initial_budget" => base.entry_point_initial_budget,
                        "syscall_base_gas_cost" => base.syscall_base_gas_cost,
                        "range_check" => builtins.range_check,
                        "keccak" => builtins.keccak,
                        "pedersen" => builtins.pedersen,
                        "bitwise" => builtins.bitwise,
                        "ecop" => builtins.ecop,
                        "poseidon" => builtins.poseidon,
                        "add_mod" => builtins.add_mod,
                        "mul_mod" => builtins.mul_mod,
                        "ecdsa" => builtins.ecdsa,
                        _ => {
                            return Err(OsConstantsSerdeError::KeyNotFound {
                                key: key.to_string(),
                                inner_key: inner_key.clone(),
                            });
                        }
                    };
                    let factor =
                        factor.as_u64().ok_or_else(|| OsConstantsSerdeError::OutOfRangeFactor {
                            key: key.to_string(),
                            value: factor.clone(),
                        })?;
                    cost += inner_value * factor;
                }
                syscalls.insert(key.to_string(), SyscallGasCost::new_from_base_cost(cost));
            }
            Value::Number(n) => {
                cost = n.as_u64().ok_or_else(|| OsConstantsSerdeError::OutOfRange {
                    key: key.to_string(),
                    value: n.clone(),
                })?;
                syscalls.insert(key.to_string(), SyscallGasCost::new_from_base_cost(cost));
            }
            _ => return Err(OsConstantsSerdeError::UnhandledValueType(value.clone())),
        }
        Ok(())
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
pub enum OsConstantsSerdeError {
    #[error("Value cannot be cast into u64: {0}")]
    InvalidFactorFormat(Value),
    #[error("Unknown key '{inner_key}' used to create value for '{key}'")]
    KeyNotFound { key: String, inner_key: String },
    #[error("Key'{0}' is not found")]
    KeyNotFoundInFile(String),
    #[error("Value {value} for key '{key}' is out of range and cannot be cast into u64")]
    OutOfRange { key: String, value: Number },
    #[error(
        "Value {value} used to create value for key '{key}' is out of range and cannot be cast \
         into u64"
    )]
    OutOfRangeFactor { key: String, value: Value },
    #[error(transparent)]
    ParseError(#[from] serde_json::Error),
    #[error("Unhandled value type: {0}")]
    UnhandledValueType(Value),
    #[error("Validation failed: {0}")]
    ValidationError(String),
}

#[derive(Debug, Error)]
pub enum GasCostsError {
    #[error("used syscall: {:?} is not supported in a Cairo 0 contract.", selector)]
    DeprecatedSyscall { selector: SyscallSelector },
    #[error("used builtin: {:?} is not supported in a Cairo 1 contract.", builtin)]
    UnsupportedBuiltinInCairo1 { builtin: BuiltinName },
    #[error("a virtual builtin does not have a gas cost.")]
    VirtualBuiltin,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(try_from = "ResourceParamsRaw")]
pub struct ResourcesParams {
    pub constant: ExecutionResources,
    pub calldata_factor: ExecutionResources,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct ResourceParamsRaw {
    #[serde(flatten)]
    raw_resource_params_as_dict: Map<String, Value>,
}

impl TryFrom<ResourceParamsRaw> for ResourcesParams {
    type Error = VersionedConstantsError;

    fn try_from(mut json_data: ResourceParamsRaw) -> VersionedConstantsResult<Self> {
        let constant_value = json_data.raw_resource_params_as_dict.remove("constant");
        let calldata_factor_value = json_data.raw_resource_params_as_dict.remove("calldata_factor");

        let (constant, calldata_factor) = match (constant_value, calldata_factor_value) {
            (Some(constant), Some(calldata_factor)) => (constant, calldata_factor),
            (Some(_), None) => {
                return Err(serde_json::Error::custom(
                    "Malformed JSON: If `constant` is present, then so should `calldata_factor`",
                ))?;
            }
            (None, _) => {
                // If `constant` is not found, use the entire map for `constant` and default
                // `calldata_factor`
                let entire_value = std::mem::take(&mut json_data.raw_resource_params_as_dict);
                (Value::Object(entire_value), serde_json::to_value(ExecutionResources::default())?)
            }
        };

        Ok(Self {
            constant: serde_json::from_value(constant)?,
            calldata_factor: serde_json::from_value(calldata_factor)?,
        })
    }
}

#[cfg_attr(any(test, feature = "testing"), derive(Copy))]
#[derive(Clone, Debug, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
pub struct ResourcesByVersion {
    pub resources: ResourcesParams,
    pub deprecated_resources: ResourcesParams,
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
