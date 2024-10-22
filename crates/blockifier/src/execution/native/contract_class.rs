use std::ops::Deref;
use std::sync::Arc;

use cairo_lang_sierra::ids::FunctionId;
use cairo_lang_starknet_classes::contract_class::{
    ContractClass as SierraContractClass,
    ContractEntryPoint as SierraContractEntryPoint,
};
use cairo_lang_utils::bigint::BigUintAsHex;
#[allow(unused_imports)]
use cairo_native::executor::AotNativeExecutor;
use starknet_api::core::EntryPointSelector;

use crate::execution::contract_class::{EntryPointsByType, HasSelector};
use crate::execution::entry_point::CallEntryPoint;
use crate::execution::errors::PreExecutionError;
use crate::execution::native::utils::contract_entrypoint_to_entrypoint_selector;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeContractClassV1(pub Arc<NativeContractClassV1Inner>);
impl Deref for NativeContractClassV1 {
    type Target = NativeContractClassV1Inner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl NativeContractClassV1 {
    pub(crate) fn constructor_selector(&self) -> Option<EntryPointSelector> {
        self.entry_points_by_type.constructor.first().map(|ep| ep.selector)
    }

    /// Initialize a compiled contract class for native.
    ///
    /// executor must be derived from sierra_program which in turn must be derived from
    /// sierra_contract_class.
    pub fn new(
        executor: AotNativeExecutor,
        sierra_contract_class: SierraContractClass,
        casm: ContractClassV1,
    ) -> NativeContractClassV1 {
        let contract = NativeContractClassV1Inner::new(executor, sierra_contract_class, casm);

        Self(Arc::new(contract))
    }

    /// Returns an entry point into the natively compiled contract.
    pub fn get_entry_point(&self, call: &CallEntryPoint) -> Result<FunctionId, PreExecutionError> {
        self.entry_points_by_type.get_entry_point(call).map(|ep| ep.function_id)
    }
}

#[derive(Debug)]
pub struct NativeContractClassV1Inner {
    pub executor: AotNativeExecutor,
    entry_points_by_type: EntryPointsByType<NativeEntryPoint>,
    casm: ContractClassV1,
}

impl NativeContractClassV1Inner {
    fn new(
        executor: AotNativeExecutor,
        sierra_contract_class: SierraContractClass,
        casm: ContractClassV1,
    ) -> Self {
        NativeContractClassV1Inner {
            executor,
            entry_points_by_type: EntryPointsByType::from(&sierra_contract_class),
            casm,
        }
    }
}

// The location where the compiled contract is loaded into memory will not
// be the same therefore we exclude it from the comparison.
impl PartialEq for NativeContractClassV1Inner {
    fn eq(&self, other: &Self) -> bool {
        self.entry_points_by_type == other.entry_points_by_type && self.casm == other.casm
    }
}

impl Eq for NativeContractClassV1Inner {}

impl From<&SierraContractClass> for EntryPointsByType<NativeEntryPoint> {
    fn from(sierra_contract_class: &SierraContractClass) -> Self {
        let program =
            sierra_contract_class.extract_sierra_program().expect("Can't get sierra program.");

        let func_ids = program.funcs.iter().map(|func| &func.id).collect::<Vec<&FunctionId>>();

        let entry_points_by_type = &sierra_contract_class.entry_points_by_type;

        EntryPointsByType::<NativeEntryPoint> {
            constructor: sierra_eps_to_native_eps(&func_ids, &entry_points_by_type.constructor),
            external: sierra_eps_to_native_eps(&func_ids, &entry_points_by_type.external),
            l1_handler: sierra_eps_to_native_eps(&func_ids, &entry_points_by_type.l1_handler),
        }
    }
}

fn sierra_eps_to_native_eps(
    func_ids: &[&FunctionId],
    sierra_eps: &[SierraContractEntryPoint],
) -> Vec<NativeEntryPoint> {
    sierra_eps.iter().map(|sierra_ep| NativeEntryPoint::from(func_ids, sierra_ep)).collect()
}

#[derive(Clone, Debug, PartialEq)]
/// Provides a relation between a function in a contract and a compiled contract.
pub struct NativeEntryPoint {
    /// The selector is the key to find the function in the contract.
    selector: EntryPointSelector,
    /// And the function_id is the key to find the function in the compiled contract.
    function_id: FunctionId,
}

impl NativeEntryPoint {
    fn from(func_ids: &[&FunctionId], sierra_ep: &SierraContractEntryPoint) -> NativeEntryPoint {
        let &function_id = func_ids.get(sierra_ep.function_idx).expect("Can't find function id.");
        NativeEntryPoint {
            selector: contract_entrypoint_to_entrypoint_selector(sierra_ep),
            function_id: function_id.clone(),
        }
    }
}

impl HasSelector for NativeEntryPoint {
    fn selector(&self) -> &EntryPointSelector {
        &self.selector
    }
}
