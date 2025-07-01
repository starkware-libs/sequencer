#[cfg(feature = "cairo_native")]
use std::collections::HashMap;
use std::sync::Arc;
#[cfg(feature = "cairo_native")]
use std::sync::{LazyLock, RwLock};

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
#[cfg(feature = "cairo_native")]
use cairo_lang_starknet_classes::contract_class::ContractClass as SierraContractClass;
#[cfg(feature = "cairo_native")]
use cairo_native::executor::AotContractExecutor;
use serde_json::Value;
use starknet_api::block::BlockInfo;
use starknet_api::contract_address;
#[cfg(feature = "cairo_native")]
use starknet_api::contract_class::SierraVersion;
use starknet_api::core::{ChainId, ClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::execution_resources::GasAmount;
use starknet_api::test_utils::{TEST_ERC20_CONTRACT_ADDRESS, TEST_ERC20_CONTRACT_ADDRESS2};

use crate::blockifier::config::{CairoNativeRunConfig, ContractClassManagerConfig};
use crate::bouncer::{BouncerConfig, BouncerWeights};
use crate::context::{BlockContext, ChainInfo, FeeTokenAddresses, TransactionContext};
use crate::execution::call_info::{CallExecution, CallInfo, Retdata};
use crate::execution::common_hints::ExecutionMode;
#[cfg(feature = "cairo_native")]
use crate::execution::contract_class::CompiledClassV1;
use crate::execution::entry_point::{
    CallEntryPoint,
    EntryPointExecutionContext,
    EntryPointExecutionResult,
    SierraGasRevertTracker,
};
#[cfg(feature = "cairo_native")]
use crate::execution::native::contract_class::NativeCompiledClassV1;
use crate::state::contract_class_manager::ContractClassManager;
use crate::state::state_api::State;
use crate::test_utils::{get_raw_contract_class, update_json_value};
use crate::transaction::objects::{
    CurrentTransactionInfo,
    DeprecatedTransactionInfo,
    TransactionInfo,
};
use crate::versioned_constants::{
    GasCosts,
    OsConstants,
    VersionedConstants,
    VERSIONED_CONSTANTS_LATEST_JSON,
};

impl CallEntryPoint {
    /// Executes the call directly, without account context. Limits the number of steps by resource
    /// bounds.
    pub fn execute_directly(self, state: &mut dyn State) -> EntryPointExecutionResult<CallInfo> {
        // Do not limit steps by resources as we use default resources.
        let limit_steps_by_resources = false;
        self.execute_directly_given_tx_info(
            state,
            TransactionInfo::Current(CurrentTransactionInfo::create_for_testing()),
            limit_steps_by_resources,
            ExecutionMode::Execute,
        )
    }

    pub fn execute_directly_given_block_context(
        self,
        state: &mut dyn State,
        block_context: BlockContext,
    ) -> EntryPointExecutionResult<CallInfo> {
        // Do not limit steps by resources as we use default resources.
        let limit_steps_by_resources = false;
        let tx_context = TransactionContext {
            block_context,
            tx_info: TransactionInfo::Current(CurrentTransactionInfo::create_for_testing()),
        };

        let mut context = EntryPointExecutionContext::new(
            Arc::new(tx_context),
            ExecutionMode::Execute,
            limit_steps_by_resources,
            SierraGasRevertTracker::new(GasAmount(self.initial_gas)),
        );
        let mut remaining_gas = self.initial_gas;
        self.execute(state, &mut context, &mut remaining_gas)
    }

    pub fn execute_directly_given_tx_info(
        self,
        state: &mut dyn State,
        tx_info: TransactionInfo,
        limit_steps_by_resources: bool,
        execution_mode: ExecutionMode,
    ) -> EntryPointExecutionResult<CallInfo> {
        let tx_context =
            TransactionContext { block_context: BlockContext::create_for_testing(), tx_info };
        let mut context = EntryPointExecutionContext::new(
            Arc::new(tx_context),
            execution_mode,
            limit_steps_by_resources,
            SierraGasRevertTracker::new(GasAmount(self.initial_gas)),
        );
        let mut remaining_gas = self.initial_gas;
        self.execute(state, &mut context, &mut remaining_gas)
    }

    /// Executes the call directly in validate mode, without account context. Limits the number of
    /// steps by resource bounds.
    pub fn execute_directly_in_validate_mode(
        self,
        state: &mut dyn State,
    ) -> EntryPointExecutionResult<CallInfo> {
        let limit_steps_by_resources = false; // Do not limit steps by resources as we use default reasources.
        self.execute_directly_given_tx_info(
            state,
            // TODO(Yoni, 1/12/2024): change the default to V3.
            TransactionInfo::Deprecated(DeprecatedTransactionInfo::default()),
            limit_steps_by_resources,
            ExecutionMode::Validate,
        )
    }
}

impl CallInfo {
    pub fn with_some_class_hash(mut self) -> Self {
        self.call.class_hash = Some(ClassHash::default());
        self
    }
}

impl VersionedConstants {
    pub fn create_for_testing() -> Self {
        Self::latest_constants().clone()
    }
}

impl GasCosts {
    pub fn create_for_testing_from_subset(subset_of_os_constants: &str) -> Self {
        let subset_of_os_constants: Value = serde_json::from_str(subset_of_os_constants).unwrap();
        let mut os_constants: Value =
            serde_json::from_str::<Value>(VERSIONED_CONSTANTS_LATEST_JSON.as_str())
                .unwrap()
                .get("os_constants")
                .unwrap()
                .clone();
        update_json_value(&mut os_constants, subset_of_os_constants);
        let os_constants: OsConstants = serde_json::from_value(os_constants).unwrap();
        os_constants.gas_costs
    }
}

impl ChainInfo {
    pub fn create_for_testing() -> Self {
        Self {
            chain_id: ChainId::create_for_testing(),
            fee_token_addresses: FeeTokenAddresses {
                eth_fee_token_address: contract_address!(TEST_ERC20_CONTRACT_ADDRESS),
                strk_fee_token_address: contract_address!(TEST_ERC20_CONTRACT_ADDRESS2),
            },
        }
    }
}

impl BlockContext {
    pub fn create_for_testing() -> Self {
        Self {
            block_info: BlockInfo::create_for_testing(),
            chain_info: ChainInfo::create_for_testing(),
            versioned_constants: VersionedConstants::create_for_testing(),
            bouncer_config: BouncerConfig::max(),
        }
    }

    pub fn create_for_account_testing() -> Self {
        Self {
            block_info: BlockInfo::create_for_testing(),
            chain_info: ChainInfo::create_for_testing(),
            versioned_constants: VersionedConstants::create_for_account_testing(),
            bouncer_config: BouncerConfig::max(),
        }
    }

    pub fn create_for_bouncer_testing(max_n_events_in_block: usize) -> Self {
        Self {
            bouncer_config: BouncerConfig {
                block_max_capacity: BouncerWeights {
                    n_events: max_n_events_in_block,
                    ..BouncerWeights::max()
                },
                ..BouncerConfig::max()
            },
            ..Self::create_for_account_testing()
        }
    }

    pub fn create_for_account_testing_with_kzg(use_kzg_da: bool) -> Self {
        Self {
            block_info: BlockInfo::create_for_testing_with_kzg(use_kzg_da),
            ..Self::create_for_account_testing()
        }
    }
}

impl CallExecution {
    pub fn from_retdata(retdata: Retdata) -> Self {
        Self { retdata, ..Default::default() }
    }
}

impl ContractClassManager {
    pub fn create_for_testing(native_config: CairoNativeRunConfig) -> Self {
        let config = ContractClassManagerConfig {
            cairo_native_run_config: native_config,
            ..Default::default()
        };
        ContractClassManager::start(config)
    }
}
// Contract loaders.

// TODO(Noa): Consider using PathBuf.
pub trait LoadContractFromFile: serde::de::DeserializeOwned {
    fn from_file(contract_path: &str) -> Self {
        let raw_contract_class = get_raw_contract_class(contract_path);
        serde_json::from_str(&raw_contract_class).unwrap()
    }
}

impl LoadContractFromFile for CasmContractClass {}
impl LoadContractFromFile for DeprecatedContractClass {}

#[cfg(feature = "cairo_native")]
static COMPILED_NATIVE_CONTRACT_CACHE: LazyLock<RwLock<HashMap<String, NativeCompiledClassV1>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

#[cfg(feature = "cairo_native")]
impl NativeCompiledClassV1 {
    /// Convenience function to construct a NativeCompiledClassV1 from a raw contract class.
    /// If control over the compilation is desired use [Self::new] instead.
    pub fn try_from_json_string(raw_sierra_contract_class: &str) -> Self {
        let sierra_contract_class: SierraContractClass =
            serde_json::from_str(raw_sierra_contract_class).unwrap();

        let sierra_program = sierra_contract_class
            .extract_sierra_program()
            .expect("Cannot extract sierra program from sierra contract class");

        let sierra_version_values = sierra_contract_class
            .sierra_program
            .iter()
            .take(3)
            .map(|x| x.value.clone())
            .collect::<Vec<_>>();

        let sierra_version = SierraVersion::extract_from_program(&sierra_version_values)
            .expect("Cannot extract sierra version from sierra program");

        let executor = AotContractExecutor::new(
            &sierra_program,
            &sierra_contract_class.entry_points_by_type,
            sierra_version.clone().into(),
            cairo_native::OptLevel::Default,
        )
        .expect("Cannot compile sierra into native");

        // Compile the sierra contract class into casm
        let casm_contract_class =
            CasmContractClass::from_contract_class(sierra_contract_class, false, usize::MAX)
                .expect("Cannot compile sierra contract class into casm contract class");
        let casm = CompiledClassV1::try_from((casm_contract_class, sierra_version))
            .expect("Cannot get CompiledClassV1 from CasmContractClass");

        NativeCompiledClassV1::new(executor, casm)
    }

    pub fn from_file(contract_path: &str) -> Self {
        let raw_contract_class = get_raw_contract_class(contract_path);
        Self::try_from_json_string(&raw_contract_class)
    }

    /// Compile a contract from a file or get it from the cache.
    pub fn compile_or_get_cached(path: &str) -> Self {
        let cache = COMPILED_NATIVE_CONTRACT_CACHE.read().unwrap();
        if let Some(cached_class) = cache.get(path) {
            return cached_class.clone();
        }
        std::mem::drop(cache);

        let class = NativeCompiledClassV1::from_file(path);
        let mut cache = COMPILED_NATIVE_CONTRACT_CACHE.write().unwrap();
        cache.insert(path.to_string(), class.clone());
        class
    }
}
