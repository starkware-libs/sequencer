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
<<<<<<< HEAD
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
||||||| 91889fd5e
=======
use log;
>>>>>>> origin/main-v0.13.4
use papyrus_storage::compiled_class::CasmStorageReader;
use papyrus_storage::db::RO;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::StorageReader;
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::SierraVersion;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedClass;
use starknet_api::state::{SierraContractClass, StateNumber, StorageKey};
use starknet_class_manager_types::{EmptyClassManagerClient, SharedClassManagerClient};
use starknet_types_core::felt::Felt;

#[cfg(test)]
#[path = "papyrus_state_test.rs"]
mod test;

type RawPapyrusReader<'env> = papyrus_storage::StorageTxn<'env, RO>;

pub struct PapyrusReader {
    storage_reader: StorageReader,
    latest_block: BlockNumber,
    contract_class_manager: ContractClassManager,
    _class_reader: SharedClassManagerClient,
}

impl PapyrusReader {
    pub fn new(
        storage_reader: StorageReader,
        latest_block: BlockNumber,
        contract_class_manager: ContractClassManager,
    ) -> Self {
        // TODO(Elin): integrate class manager client.
        let _class_reader = Arc::new(EmptyClassManagerClient);
        Self { storage_reader, latest_block, contract_class_manager, _class_reader }
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
<<<<<<< HEAD
            let (casm_compiled_class, sierra) = self.read_casm_and_sierra(class_hash)?;
||||||| 91889fd5e
            let (option_casm, option_sierra) = self
                .reader()?
                .get_casm_and_sierra(&class_hash)
                .map_err(|err| StateError::StateReadError(err.to_string()))?;
            let (casm_compiled_class, sierra) =
                couple_casm_and_sierra(class_hash, option_casm, option_sierra)?.expect(
                    "Should be able to fetch a Casm and Sierra class if its definition exists, \
                     database is inconsistent.",
                );
=======
            // Cairo 1.
            let (option_casm, option_sierra) = self
                .reader()?
                .get_casm_and_sierra(&class_hash)
                .map_err(|err| StateError::StateReadError(err.to_string()))?;
            let (casm_compiled_class, sierra) =
                couple_casm_and_sierra(class_hash, option_casm, option_sierra)?.expect(
                    "Should be able to fetch a Casm and Sierra class if its definition exists, \
                     database is inconsistent.",
                );
>>>>>>> origin/main-v0.13.4
            let sierra_version = SierraVersion::extract_from_program(&sierra.sierra_program)?;
            return Ok(CachedClass::V1(
                CompiledClassV1::try_from((casm_compiled_class, sierra_version))?,
                Arc::new(sierra),
            ));
        }

<<<<<<< HEAD
        let v0_compiled_class = self.read_deprecated_casm(class_hash)?;
||||||| 91889fd5e
        let v0_compiled_class = self
            .reader()?
            .get_state_reader()
            .and_then(|sr| sr.get_deprecated_class_definition_at(state_number, &class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?;

=======
        // Possibly Cairo 0.
        let v0_compiled_class = self
            .reader()?
            .get_state_reader()
            .and_then(|sr| sr.get_deprecated_class_definition_at(state_number, &class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?;

>>>>>>> origin/main-v0.13.4
        match v0_compiled_class {
            Some(starknet_api_contract_class) => {
                Ok(CachedClass::V0(CompiledClassV0::try_from(starknet_api_contract_class)?))
            }
            None => Err(StateError::UndeclaredClassHash(class_hash)),
        }
    }
<<<<<<< HEAD

    /// Returns cached casm from cache if exists, otherwise fetches it from state.
    fn get_cached_casm(&self, class_hash: ClassHash) -> StateResult<CachedCasm> {
        match self.contract_class_manager.get_casm(&class_hash) {
            Some(contract_class) => Ok(contract_class),
            None => {
                let runnable_casm_from_db = self.get_compiled_class_inner(class_hash)?;
                self.contract_class_manager.set_casm(class_hash, runnable_casm_from_db.clone());
                Ok(runnable_casm_from_db)
            }
        }
    }

    fn get_casm(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        Ok(self.get_cached_casm(class_hash)?.to_runnable_casm())
    }

    #[cfg(feature = "cairo_native")]
    // Handles `get_compiled_class` under the assumption that native compilation has finished.
    // Returns the native compiled class if the compilation succeeded and the runnable casm upon
    // failure.
    fn get_compiled_class_after_waiting_on_native_compilation(
        &self,
        class_hash: ClassHash,
        casm: RunnableCompiledClass,
    ) -> RunnableCompiledClass {
        assert!(
            self.contract_class_manager.wait_on_native_compilation(),
            "this function should only be called when the waiting on native compilation flag is \
             on."
        );
        let cached_native = self
            .contract_class_manager
            .get_native(&class_hash)
            .expect("Should have native in cache in sync compilation flow.");
        match cached_native {
            CachedCairoNative::Compiled(compiled_native) => {
                RunnableCompiledClass::from(compiled_native)
            }
            CachedCairoNative::CompilationFailed => casm,
        }
    }

    fn read_casm_and_sierra(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<(CasmContractClass, SierraContractClass)> {
        let (option_casm, option_sierra) = self
            .reader()?
            .get_casm_and_sierra(&class_hash)
            .map_err(|err| StateError::StateReadError(err.to_string()))?;
        let (casm, sierra) = couple_casm_and_sierra(class_hash, option_casm, option_sierra)?
            .expect(
                "Should be able to fetch a Casm and Sierra class if its definition exists,
                database is inconsistent.",
            );

        Ok((casm, sierra))
    }

    fn read_deprecated_casm(&self, class_hash: ClassHash) -> StateResult<Option<DeprecatedClass>> {
        let state_number = StateNumber(self.latest_block);
        let option_casm = self
            .reader()?
            .get_state_reader()
            .and_then(|sr| sr.get_deprecated_class_definition_at(state_number, &class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?;

        Ok(option_casm)
    }
||||||| 91889fd5e

    /// Returns cached casm from cache if exists, otherwise fetches it from state.
    fn get_cached_casm(&self, class_hash: ClassHash) -> StateResult<CachedCasm> {
        match self.contract_class_manager.get_casm(&class_hash) {
            Some(contract_class) => Ok(contract_class),
            None => {
                let runnable_casm_from_db = self.get_compiled_class_inner(class_hash)?;
                self.contract_class_manager.set_casm(class_hash, runnable_casm_from_db.clone());
                Ok(runnable_casm_from_db)
            }
        }
    }

    fn get_casm(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        Ok(self.get_cached_casm(class_hash)?.to_runnable_casm())
    }

    #[cfg(feature = "cairo_native")]
    // Handles `get_compiled_class` under the assumption that native compilation has finished.
    // Returns the native compiled class if the compilation succeeded and the runnable casm upon
    // failure.
    fn get_compiled_class_after_waiting_on_native_compilation(
        &self,
        class_hash: ClassHash,
        casm: RunnableCompiledClass,
    ) -> RunnableCompiledClass {
        assert!(
            self.contract_class_manager.wait_on_native_compilation(),
            "this function should only be called when the waiting on native compilation flag is \
             on."
        );
        let cached_native = self
            .contract_class_manager
            .get_native(&class_hash)
            .expect("Should have native in cache in sync compilation flow.");
        match cached_native {
            CachedCairoNative::Compiled(compiled_native) => {
                RunnableCompiledClass::from(compiled_native)
            }
            CachedCairoNative::CompilationFailed => casm,
        }
    }
=======
>>>>>>> origin/main-v0.13.4
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
            // TODO(Yoni) consider having an atomic set-and-get.
            log::error!("Class is missing immediately after being cached.");
            cached_class.to_runnable()
        }))
    }

    fn get_compiled_class_hash(&self, _class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        todo!()
    }
}
