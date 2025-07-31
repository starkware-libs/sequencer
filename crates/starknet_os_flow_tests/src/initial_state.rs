#![allow(dead_code)]
use std::collections::HashMap;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::ContractClassComponentHashes;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia_storage::map_storage::MapStorage;

use crate::state_trait::FlowTestState;

/// Gathers the information needed to execute a flow test.
pub(crate) struct InitialStateData<S: FlowTestState> {
    pub(crate) initial_state: InitialState<S>,
    pub(crate) execution_contracts: OsExecutionContracts,
}

pub(crate) struct OsExecutionContracts {
    // Cairo contracts that are executed during the OS execution.
    pub(crate) executed_contracts: ExecutedContracts,
    // Cairo 1 contracts that are declared during the OS execution.
    pub(crate) declared_class_hash_to_component_hashes:
        HashMap<ClassHash, ContractClassComponentHashes>,
}

pub(crate) struct ExecutedContracts {
    pub(crate) contracts: HashMap<CompiledClassHash, CasmContractClass>,
    pub(crate) deprecated_contracts: HashMap<CompiledClassHash, DeprecatedContractClass>,
}

pub(crate) struct InitialState<S: FlowTestState> {
    pub(crate) updatable_state: S,
    pub(crate) commitment_storage: MapStorage,
    // Current patricia roots.
    pub(crate) contracts_trie_root_hash: HashOutput,
    pub(crate) classes_trie_root_hash: HashOutput,
}
