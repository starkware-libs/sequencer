use std::sync::Arc;

use apollo_class_manager_types::SharedClassManagerClient;
use apollo_storage::class::ClassStorageReader;
use apollo_storage::compiled_class::CasmStorageReader;
use apollo_storage::db::RO;
use apollo_storage::state::StateStorageReader;
use apollo_storage::StorageReader;
use blockifier::execution::contract_class::{
    CompiledClassV0,
    CompiledClassV1,
    RunnableCompiledClass,
};
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::errors::{couple_casm_and_sierra, StateError};
use blockifier::state::global_cache::CachedClass;
use blockifier::state::state_api::{StateReader, StateResult};
use blockifier::state::state_reader_and_contract_manager::FetchCompiliedClasses;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use log;
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedClass;
use starknet_api::state::{SierraContractClass, StateNumber, StorageKey};
use starknet_types_core::felt::Felt;

use crate::metrics::{CLASS_CACHE_HITS, CLASS_CACHE_MISSES};

#[cfg(test)]
#[path = "papyrus_state_test.rs"]
mod test;

type RawPapyrusReader<'env> = apollo_storage::StorageTxn<'env, RO>;

pub struct ClassReader {
    pub reader: SharedClassManagerClient,
    // Used to invoke async functions from sync reader code.
    pub runtime: tokio::runtime::Handle,
}

impl ClassReader {
    fn read_executable(&self, class_hash: ClassHash) -> StateResult<ContractClass> {
        let casm = self
            .runtime
            .block_on(self.reader.get_executable(class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?
            .ok_or(StateError::UndeclaredClassHash(class_hash))?;

        Ok(casm)
    }

    fn read_casm(&self, class_hash: ClassHash) -> StateResult<CasmContractClass> {
        let casm = self.read_executable(class_hash)?;
        let ContractClass::V1((casm, _sierra_version)) = casm else {
            panic!("Class hash {class_hash} originated from a Cairo 1 contract.");
        };

        Ok(casm)
    }

    fn read_sierra(&self, class_hash: ClassHash) -> StateResult<SierraContractClass> {
        let sierra = self
            .runtime
            .block_on(self.reader.get_sierra(class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?
            .ok_or(StateError::UndeclaredClassHash(class_hash))?;

        Ok(sierra)
    }

    // TODO(Elin): make `read[_optional_deprecated]_casm` symmetrical and independent of invocation
    // order.
    fn read_optional_deprecated_casm(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<Option<DeprecatedClass>> {
        let casm = self.read_executable(class_hash)?;
        if let ContractClass::V0(casm) = casm { Ok(Some(casm)) } else { Ok(None) }
    }
}

pub struct PapyrusReader {
    storage_reader: StorageReader,
    latest_block: BlockNumber,
    // TODO(AvivG): remove class_manager once cairo_native logic moves to
    // StateReaderAndContractManger.
    contract_class_manager: ContractClassManager,
    // Reader is `None` for reader invoked through `native_blockifier`.
    class_reader: Option<ClassReader>,
}

impl PapyrusReader {
    pub fn new_with_class_manager(
        storage_reader: StorageReader,
        latest_block: BlockNumber,
        contract_class_manager: ContractClassManager,
        class_reader: Option<ClassReader>,
    ) -> Self {
        Self { storage_reader, latest_block, contract_class_manager, class_reader }
    }

    pub fn new(
        storage_reader: StorageReader,
        latest_block: BlockNumber,
        contract_class_manager: ContractClassManager,
    ) -> Self {
        Self { storage_reader, latest_block, contract_class_manager, class_reader: None }
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

        // TODO(Elin): consider not reading Sierra if compilation is disabled.
        Ok((class_reader.read_casm(class_hash)?, class_reader.read_sierra(class_hash)?))
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

        class_reader.read_optional_deprecated_casm(class_hash)
    }

    fn update_native_metrics(&self, _runnable_class: &RunnableCompiledClass) {
        #[cfg(feature = "cairo_native")]
        {
            if matches!(_runnable_class, RunnableCompiledClass::V1Native(_)) {
                crate::metrics::NATIVE_CLASS_RETURNED.increment(1);
            }
        }
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
            CLASS_CACHE_HITS.increment(1);
            self.update_native_metrics(&runnable_class);
            return Ok(runnable_class);
        }
        CLASS_CACHE_MISSES.increment(1);

        let cached_class = self.get_compiled_class_from_db(class_hash)?;
        self.contract_class_manager.set_and_compile(class_hash, cached_class.clone());
        // Access the cache again in case the class was compiled.
        let runnable_class =
            self.contract_class_manager.get_runnable(&class_hash).unwrap_or_else(|| {
                // Edge case that should not be happen if the cache size is big enough.
                // TODO(Yoni): consider having an atomic set-and-get.
                log::error!("Class is missing immediately after being cached.");
                cached_class.to_runnable()
            });
        self.update_native_metrics(&runnable_class);
        Ok(runnable_class)
    }

    fn get_compiled_class_hash(&self, _class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        todo!()
    }
}

impl FetchCompiliedClasses for PapyrusReader {
    fn get_compiled_classes(&self, class_hash: ClassHash) -> StateResult<CachedClass> {
        self.get_compiled_class_from_db(class_hash)
    }
}
