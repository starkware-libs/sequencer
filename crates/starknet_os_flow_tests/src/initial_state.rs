#![allow(dead_code)]
use std::collections::HashMap;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::SierraContractClass;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia_storage::map_storage::MapStorage;

use crate::state_trait::FlowTestState;

/// Gathers the information needed to execute a flow test.
pub(crate) struct InitialStateData<S: FlowTestState> {
    pub(crate) updatable_state: S,
    pub(crate) fact_storage: MapStorage,
    // Current patricia roots.
    pub(crate) contracts_trie_root_hash: HashOutput,
    pub(crate) classes_trie_root_hash: HashOutput,
    // Cairo contracts that run during the OS execution.
    pub(crate) contracts: HashMap<CompiledClassHash, CasmContractClass>,
    pub(crate) deprecated_contracts: HashMap<CompiledClassHash, DeprecatedContractClass>,
    // Sierra contracts that are declared during the OS execution.
    pub(crate) class_hash_to_sierra: HashMap<ClassHash, SierraContractClass>,
}
