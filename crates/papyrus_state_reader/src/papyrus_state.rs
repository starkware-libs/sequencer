use std::sync::Arc;

use blockifier::execution::contract_class::{
    CompiledClassV0,
    CompiledClassV1,
    RunnableCompiledClass,
};
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::errors::{couple_casm_and_sierra, StateError};
use blockifier::state::global_cache::CachedClass;
use blockifier::state::state_api::{StateReader, StateResult};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use futures::executor::block_on;
use log;
use papyrus_storage::compiled_class::CasmStorageReader;
use papyrus_storage::db::RO;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::StorageReader;
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedClass;
use starknet_api::state::{SierraContractClass, StateNumber, StorageKey};
use starknet_class_manager_types::SharedClassManagerClient;
use starknet_types_core::felt::Felt;

#[cfg(test)]
#[path = "papyrus_state_test.rs"]
mod test;

type RawPapyrusReader<'env> = papyrus_storage::StorageTxn<'env, RO>;

pub struct PapyrusReader {
    storage_reader: StorageReader,
    latest_block: BlockNumber,
    contract_class_manager: ContractClassManager,
    // Reader is `None` for reader invoked through `native_blockifier`.
    class_reader: Option<SharedClassManagerClient>,
}

impl PapyrusReader {
    pub fn new(
        storage_reader: StorageReader,
        latest_block: BlockNumber,
        contract_class_manager: ContractClassManager,
    ) -> Self {
        Self {
            storage_reader,
            latest_block,
            contract_class_manager,
            // TODO(Elin): integrate class manager client.
            class_reader: None,
        }
    }

    fn reader(&self) -> StateResult<RawPapyrusReader<'_>> {
        self.storage_reader
            .begin_ro_txn()
            .map_err(|error| StateError::StateReadError(error.to_string()))
    }

    /// Returns a V1 contract with Sierra if V1 contract is found, or a V0 contract without Sierra
    /// if a V1 contract is not found, or an `Error` otherwise.
    fn get_compiled_class_from_db(&self, class_hash: ClassHash) -> StateResult<CachedClass> {
        let state_number = StateNumber(self.latest_block);
        let class_declaration_block_number = self
            .reader()?
            .get_state_reader()
            .and_then(|sr| sr.get_class_definition_block_number(&class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?;
        let class_is_declared: bool = matches!(class_declaration_block_number,
                        Some(block_number) if block_number <= state_number.0);

        if class_is_declared {
            // Cairo 1.
            let (casm_compiled_class, sierra) = self.read_casm_and_sierra(class_hash)?;
            let sierra_version = SierraVersion::extract_from_program(&sierra.sierra_program)?;
            return Ok(CachedClass::V1(
                CompiledClassV1::try_from((casm_compiled_class, sierra_version))?,
                Arc::new(sierra),
            ));
        }

        // Possibly Cairo 0.
        let v0_compiled_class = self.read_deprecated_casm(class_hash)?;
        match v0_compiled_class {
            Some(starknet_api_contract_class) => {
                Ok(CachedClass::V0(CompiledClassV0::try_from(starknet_api_contract_class)?))
            }
            None => Err(StateError::UndeclaredClassHash(class_hash)),
        }
    }

    fn read_casm_and_sierra(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<(CasmContractClass, SierraContractClass)> {
        let Some(class_reader) = &self.class_reader else {
            let (option_casm, option_sierra) = self
                .reader()?
                .get_casm_and_sierra(&class_hash)
                .map_err(|err| StateError::StateReadError(err.to_string()))?;
            let (casm, sierra) = couple_casm_and_sierra(class_hash, option_casm, option_sierra)?
                .expect(
                    "Should be able to fetch a Casm and Sierra class if its definition exists,
                    database is inconsistent.",
                );

            return Ok((casm, sierra));
        };

        let casm = block_on(class_reader.get_executable(class_hash))
            .map_err(|e| StateError::StateReadError(e.to_string()))?
            .ok_or(StateError::UndeclaredClassHash(class_hash))?;
        let ContractClass::V1((casm, _sierra_version)) = casm else {
            panic!("Class hash {class_hash} originated from a Cairo 1 contract.");
        };
        // TODO(Elin): consider not reading Sierra if compilation is disabled.
        let sierra = block_on(class_reader.get_sierra(class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?
            .ok_or(StateError::UndeclaredClassHash(class_hash))?;

        Ok((casm, sierra))
    }

    fn read_deprecated_casm(&self, class_hash: ClassHash) -> StateResult<Option<DeprecatedClass>> {
        let Some(class_reader) = &self.class_reader else {
            let state_number = StateNumber(self.latest_block);
            let option_casm = self
                .reader()?
                .get_state_reader()
                .and_then(|sr| sr.get_deprecated_class_definition_at(state_number, &class_hash))
                .map_err(|err| StateError::StateReadError(err.to_string()))?;

            return Ok(option_casm);
        };

        let casm = block_on(class_reader.get_executable(class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?
            .ok_or(StateError::UndeclaredClassHash(class_hash))?;
        let ContractClass::V0(casm) = casm else {
            panic!("Class hash {class_hash} originated from a Cairo 0 contract.");
        };

        Ok(Some(casm))
    }
}

// Currently unused - will soon replace the same `impl` for `PapyrusStateReader`.
impl StateReader for PapyrusReader {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        let state_number = StateNumber(self.latest_block);
        self.reader()?
            .get_state_reader()
            .and_then(|sr| sr.get_storage_at(state_number, &contract_address, &key))
            .map_err(|error| StateError::StateReadError(error.to_string()))
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        let state_number = StateNumber(self.latest_block);
        match self
            .reader()?
            .get_state_reader()
            .and_then(|sr| sr.get_nonce_at(state_number, &contract_address))
        {
            Ok(Some(nonce)) => Ok(nonce),
            Ok(None) => Ok(Nonce::default()),
            Err(err) => Err(StateError::StateReadError(err.to_string())),
        }
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        let state_number = StateNumber(self.latest_block);
        match self
            .reader()?
            .get_state_reader()
            .and_then(|sr| sr.get_class_hash_at(state_number, &contract_address))
        {
            Ok(Some(class_hash)) => Ok(class_hash),
            Ok(None) => Ok(ClassHash::default()),
            Err(err) => Err(StateError::StateReadError(err.to_string())),
        }
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        // Assumption: the global cache is cleared upon reverted blocks.

        // TODO(Yoni): move this logic to a separate reader. Move tests from papyrus_state.
        if let Some(runnable_class) = self.contract_class_manager.get_runnable(&class_hash) {
            return Ok(runnable_class);
        }

        let cached_class = self.get_compiled_class_from_db(class_hash)?;
        self.contract_class_manager.set_and_compile(class_hash, cached_class.clone());
        // Access the cache again in case the class was compiled.
        Ok(self.contract_class_manager.get_runnable(&class_hash).unwrap_or_else(|| {
            // Edge case that should not be happen if the cache size is big enough.
            // TODO(Yoni): consider having an atomic set-and-get.
            log::error!("Class is missing immediately after being cached.");
            cached_class.to_runnable()
        }))
    }

    fn get_compiled_class_hash(&self, _class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        todo!()
    }
}
