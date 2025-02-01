#[cfg(feature = "cairo_native")]
use std::sync::Arc;

use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::contract_class_manager::{CasmReader, ContractClassManager};
use blockifier::state::errors::{couple_casm_and_sierra, StateError};
#[cfg(feature = "cairo_native")]
use blockifier::state::global_cache::CachedCairoNative;
use blockifier::state::global_cache::CachedCasm;
use blockifier::state::state_api::{StateReader, StateResult};
use futures::executor::block_on;
use papyrus_storage::compiled_class::CasmStorageReader;
use papyrus_storage::db::RO;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::StorageReader;
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{StateNumber, StorageKey};
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
    casm_reader: PapyrusCasmReader,
}

impl PapyrusReader {
    pub fn new(
        storage_reader: StorageReader,
        latest_block: BlockNumber,
        contract_class_manager: ContractClassManager,
    ) -> Self {
        let casm_reader = PapyrusCasmReader { reader: storage_reader.clone(), latest_block };
        Self { storage_reader, latest_block, contract_class_manager, casm_reader }
    }

    fn reader(&self) -> StateResult<RawPapyrusReader<'_>> {
        self.storage_reader
            .begin_ro_txn()
            .map_err(|error| StateError::StateReadError(error.to_string()))
    }

    /// Returns a V1 contract with Sierra if V1 contract is found, or a V0 contract without Sierra
    /// if a V1 contract is not found, or an `Error` otherwise.
    fn read_casm(&self, class_hash: ClassHash) -> StateResult<CachedCasm> {
        let cached_casm = self.casm_reader.read_casm(class_hash)?;

        #[cfg(feature = "cairo_native")]
        if !self.run_cairo_native() {
            let cached_casm = CachedCasm::WithoutSierra(cached_casm.to_runnable_casm()); // FIXME
        }

        Ok(cached_casm)
    }

    /// Returns cached casm from cache if exists, otherwise fetches it from state.
    fn get_cached_casm(&self, class_hash: ClassHash) -> StateResult<CachedCasm> {
        match self.contract_class_manager.get_casm(&class_hash) {
            Some(contract_class) => Ok(contract_class),
            None => {
                let runnable_casm_from_db = self.read_casm(class_hash)?;
                self.set_casm(class_hash, runnable_casm_from_db.clone());
                Ok(runnable_casm_from_db)
            }
        }
    }

    pub fn get_runnable_casm(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        Ok(self.get_cached_casm(class_hash)?.to_runnable_casm())
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
        // self.contract_class_manager.get_compiled_class(class_hash)

        #[cfg(not(feature = "cairo_native"))]
        return self.contract_class_manager.get_runnable_casm(class_hash);

        #[cfg(feature = "cairo_native")]
        {
            if !self.contract_class_manager.run_cairo_native() {
                // Cairo native is disabled - fetch and return the casm.
                return self.get_runnable_casm(class_hash);
            }

            // Try fetching native from cache.
            if let Some(cached_native) = self.contract_class_manager.get_native(&class_hash) {
                match cached_native {
                    CachedCairoNative::Compiled(compiled_native) => {
                        return Ok(RunnableCompiledClass::from(compiled_native));
                    }
                    CachedCairoNative::CompilationFailed => {
                        // The compilation previously failed. Make no further compilation attempts.
                        // Fetch and return the casm.
                        return self.get_runnable_casm(class_hash);
                    }
                }
            };

            // Native not found in cache. Get the cached casm.
            let cached_casm = self.contract_class_manager.get_cached_casm(class_hash)?;

            // If the fetched casm includes a Sierra, send a compilation request.
            // Return the casm.
            // NOTE: We assume that whenever the fetched casm does not include a Sierra, compilation
            // to native is not required.
            match cached_casm {
                CachedCasm::WithSierra(runnable_casm, sierra) => {
                    if let RunnableCompiledClass::V1(casm_v1) = runnable_casm.clone() {
                        self.contract_class_manager.send_compilation_request((
                            class_hash,
                            sierra.clone(),
                            casm_v1.clone(),
                        ));
                        if self.contract_class_manager.wait_on_native_compilation() {
                            // With this config, sending a compilation request blocks the sender
                            // until compilation completes. Retry fetching Native from cache.
                            return Ok(self
                                .contract_class_manager
                                .get_compiled_class_after_waiting_on_native_compilation(
                                    class_hash,
                                    runnable_casm,
                                ));
                        }
                    } else {
                        panic!(
                            "A Sierra file was saved in cache for a Cairo0 contract - class hash \
                             {class_hash}. This is probably a bug as no Sierra file exists for a \
                             Cairo0 contract."
                        );
                    }

                    Ok(runnable_casm)
                }
                CachedCasm::WithoutSierra(runnable_casm) => Ok(runnable_casm),
            }
        }
    }

    fn get_compiled_class_hash(&self, _class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        todo!()
    }
}

pub struct PapyrusCasmReader {
    pub reader: StorageReader,
    pub latest_block: BlockNumber,
}

impl PapyrusCasmReader {
    fn reader(&self) -> StateResult<RawPapyrusReader<'_>> {
        self.reader.begin_ro_txn().map_err(|error| StateError::StateReadError(error.to_string()))
    }
}

impl CasmReader for PapyrusCasmReader {
    fn read_casm(&self, class_hash: ClassHash) -> StateResult<CachedCasm> {
        let state_number = StateNumber(self.latest_block);
        let reader = self.reader()?;
        let state_reader =
            reader.get_state_reader().map_err(|err| StateError::StateReadError(err.to_string()))?;

        let class_declaration_block_number = state_reader
            .get_class_definition_block_number(&class_hash)
            .map_err(|err| StateError::StateReadError(err.to_string()))?;
        let cairo1_declared_class: bool = matches!(class_declaration_block_number,
                        Some(block_number) if block_number <= state_number.0);

        if !cairo1_declared_class {
            // Either a non-declared V1 class, or a deprecated (V0) class.
            let deprecated_casm = state_reader
                .get_deprecated_class_definition_at(state_number, &class_hash)
                .map_err(|err| StateError::StateReadError(err.to_string()))?
                .ok_or(StateError::UndeclaredClassHash(class_hash))?;
            let deprecated_casm = ContractClass::V0(deprecated_casm);
            let runnable_casm = RunnableCompiledClass::try_from(deprecated_casm)?;

            return Ok(CachedCasm::WithoutSierra(runnable_casm));
        }

        let (option_casm, option_sierra) = reader
            .get_casm_and_sierra(&class_hash)
            .map_err(|err| StateError::StateReadError(err.to_string()))?;
        let (casm_compiled_class, sierra) =
            couple_casm_and_sierra(class_hash, option_casm, option_sierra)?.expect(
                "Should be able to fetch a Casm and Sierra class if its definition exists,
                database is inconsistent.",
            );
        let sierra_version = SierraVersion::extract_from_program(&sierra.sierra_program)?;
        let versioned_casm = ContractClass::V1((casm_compiled_class, sierra_version));
        let runnable_casm = RunnableCompiledClass::try_from(versioned_casm)?;

        #[cfg(not(feature = "cairo_native"))]
        let cached_casm = CachedCasm::WithoutSierra(runnable_casm);

        #[cfg(feature = "cairo_native")]
        let cached_casm = CachedCasm::WithSierra(runnable_casm, Arc::new(sierra));

        Ok(cached_casm)
    }
}

struct NextCasmReader {
    reader: StorageReader,
    latest_block: BlockNumber,
    class_manager: SharedClassManagerClient,
}

impl NextCasmReader {
    fn reader(&self) -> StateResult<RawPapyrusReader<'_>> {
        self.reader.begin_ro_txn().map_err(|error| StateError::StateReadError(error.to_string()))
    }
}

impl CasmReader for NextCasmReader {
    fn read_casm(&self, class_hash: ClassHash) -> StateResult<CachedCasm> {
        let state_number = StateNumber(self.latest_block);
        let reader = self.reader()?;
        let state_reader =
            reader.get_state_reader().map_err(|err| StateError::StateReadError(err.to_string()))?;

        let class_declaration_block_number = state_reader
            .get_class_definition_block_number(&class_hash)
            .map_err(|err| StateError::StateReadError(err.to_string()))?;
        let _cairo1_declared_class: bool = matches!(class_declaration_block_number,
                        Some(block_number) if block_number <= state_number.0);

        let casm = block_on(self.class_manager.get_executable(class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?;
        // TODO: handle no casm.
        let runnable_casm = RunnableCompiledClass::try_from(casm.clone())?; // FIXME
        let cached_casm = match casm {
            ContractClass::V0(_) => CachedCasm::WithoutSierra(runnable_casm),
            ContractClass::V1(_) => {
                #[cfg(not(feature = "cairo_native"))]
                let cached_casm = CachedCasm::WithoutSierra(runnable_casm);

                #[cfg(feature = "cairo_native")]
                {
                    let sierra = block_on(self.class_manager.get_sierra(class_hash))
                        .map_err(|err| StateError::StateReadError(err.to_string()))?;
                    let cached_casm = CachedCasm::WithSierra(runnable_casm, Arc::new(sierra));
                }

                cached_casm
            }
        };

        Ok(cached_casm)
    }
}
