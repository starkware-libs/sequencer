use std::collections::HashMap;

use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{SierraContractClass, StorageKey};
use starknet_types_core::felt::Felt;

use crate::execution::contract_class::RunnableCompiledClass;
use crate::state::cached_state::StorageEntry;
use crate::state::errors::StateError;
use crate::state::state_api::{StateReader, StateResult};
use crate::test_utils::contracts::FeatureContractData;

/// A simple implementation of `StateReader` using `HashMap`s as storage.
#[derive(Clone, Debug, Default)]
pub struct DictStateReader {
    pub storage_view: HashMap<StorageEntry, Felt>,
    pub address_to_nonce: HashMap<ContractAddress, Nonce>,
    pub address_to_class_hash: HashMap<ContractAddress, ClassHash>,
    pub class_hash_to_class: HashMap<ClassHash, RunnableCompiledClass>,
    pub class_hash_to_sierra: HashMap<ClassHash, SierraContractClass>,
    pub class_hash_to_compiled_class_hash: HashMap<ClassHash, CompiledClassHash>,
}

impl DictStateReader {
    pub fn add_class(&mut self, contract: &FeatureContractData) {
        self.class_hash_to_class.insert(contract.class_hash, contract.runnable_class.clone());

        match contract.runnable_class {
            RunnableCompiledClass::V0(_) => {
                assert!(
                    contract.sierra.is_none(),
                    "Sierra class should not be provided for Cairo0"
                );
            }
            RunnableCompiledClass::V1(_) => {
                assert!(contract.sierra.is_some(), "Sierra class is required for Cairo1");
                self.class_hash_to_sierra
                    .insert(contract.class_hash, contract.sierra.clone().unwrap());
            }
            #[cfg(feature = "cairo_native")]
            RunnableCompiledClass::V1Native(_) => {
                // Do nothing, Sierra class is not required for native classes.

                // TODO(AvivG): Currently, Sierra is passed for native classes even though it's not
                // required. Consider ignoring it or enforcing None for clarity and
                // correctness.
            }
        }
    }
}

impl StateReader for DictStateReader {
    // TODO(AvivG): implement get_sierra for DictStateReader.
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        let contract_storage_key = (contract_address, key);
        let value = self.storage_view.get(&contract_storage_key).copied().unwrap_or_default();
        Ok(value)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        let nonce = self.address_to_nonce.get(&contract_address).copied().unwrap_or_default();
        Ok(nonce)
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        let contract_class = self.class_hash_to_class.get(&class_hash).cloned();
        match contract_class {
            Some(contract_class) => Ok(contract_class),
            _ => Err(StateError::UndeclaredClassHash(class_hash)),
        }
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        let class_hash =
            self.address_to_class_hash.get(&contract_address).copied().unwrap_or_default();
        Ok(class_hash)
    }

    fn get_compiled_class_hash(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<starknet_api::core::CompiledClassHash> {
        let compiled_class_hash =
            self.class_hash_to_compiled_class_hash.get(&class_hash).copied().unwrap_or_default();
        Ok(compiled_class_hash)
    }

    fn get_sierra(&self, class_hash: ClassHash) -> StateResult<SierraContractClass> {
        let runnable = self.get_compiled_class(class_hash)?;

        assert!(
            matches!(runnable, RunnableCompiledClass::V1(_)),
            "Expected Cairo 1 class for Sierra retrieval"
        );

        self.class_hash_to_sierra
            .get(&class_hash)
            .cloned()
            .ok_or_else(|| panic!("Missing Sierra class for class hash: {class_hash:?}"))
    }
}
