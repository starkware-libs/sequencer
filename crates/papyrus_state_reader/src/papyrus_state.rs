#[cfg(feature = "cairo_native")]
use std::sync::Arc;

use blockifier::execution::contract_class::{
    CompiledClassV0,
    CompiledClassV1,
    RunnableCompiledClass,
};
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::errors::{couple_casm_and_sierra, StateError};
#[cfg(feature = "cairo_native")]
use blockifier::state::global_cache::CachedCairoNative;
use blockifier::state::global_cache::CachedCasm;
use blockifier::state::state_api::{StateReader, StateResult};
use papyrus_storage::compiled_class::CasmStorageReader;
use papyrus_storage::db::RO;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::StorageReader;
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::SierraVersion;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{StateNumber, StorageKey};
use starknet_types_core::felt::Felt;

#[cfg(test)]
#[path = "papyrus_state_test.rs"]
mod test;

type RawPapyrusReader<'env> = papyrus_storage::StorageTxn<'env, RO>;

pub struct PapyrusReader {
    storage_reader: StorageReader,
    latest_block: BlockNumber,
    contract_class_manager: ContractClassManager,
}

impl PapyrusReader {
    pub fn new(
        storage_reader: StorageReader,
        latest_block: BlockNumber,
        contract_class_manager: ContractClassManager,
    ) -> Self {
        Self { storage_reader, latest_block, contract_class_manager }
    }

    fn reader(&self) -> StateResult<RawPapyrusReader<'_>> {
        self.storage_reader
            .begin_ro_txn()
            .map_err(|error| StateError::StateReadError(error.to_string()))
    }

    /// Returns a V1 contract with Sierra if V1 contract is found, or a V0 contract without Sierra
    /// if a V1 contract is not found, or an `Error` otherwise.
    fn get_compiled_class_inner(&self, class_hash: ClassHash) -> StateResult<CachedCasm> {
        let state_number = StateNumber(self.latest_block);
        let class_declaration_block_number = self
            .reader()?
            .get_state_reader()
            .and_then(|sr| sr.get_class_definition_block_number(&class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?;
        let class_is_declared: bool = matches!(class_declaration_block_number,
                        Some(block_number) if block_number <= state_number.0);

        if class_is_declared {
            let (option_casm, option_sierra) = self
                .reader()?
                .get_casm_and_sierra(&class_hash)
                .map_err(|err| StateError::StateReadError(err.to_string()))?;
            let (casm_compiled_class, sierra) =
                couple_casm_and_sierra(class_hash, option_casm, option_sierra)?.expect(
                    "Should be able to fetch a Casm and Sierra class if its definition exists, \
                     database is inconsistent.",
                );
            let sierra_version = SierraVersion::extract_from_program(&sierra.sierra_program)?;
            let runnable_casm = RunnableCompiledClass::V1(CompiledClassV1::try_from((
                casm_compiled_class,
                sierra_version,
            ))?);
            #[cfg(not(feature = "cairo_native"))]
            let cached_casm = CachedCasm::WithoutSierra(runnable_casm);

            #[cfg(feature = "cairo_native")]
            let cached_casm = if !self.contract_class_manager.run_cairo_native() {
                CachedCasm::WithoutSierra(runnable_casm)
            } else {
                CachedCasm::WithSierra(runnable_casm, Arc::new(sierra))
            };

            return Ok(cached_casm);
        }

        let v0_compiled_class = self
            .reader()?
            .get_state_reader()
            .and_then(|sr| sr.get_deprecated_class_definition_at(state_number, &class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?;

        match v0_compiled_class {
            Some(starknet_api_contract_class) => {
                let runnable_casm = RunnableCompiledClass::V0(CompiledClassV0::try_from(
                    starknet_api_contract_class,
                )?);
                Ok(CachedCasm::WithoutSierra(runnable_casm))
            }
            None => Err(StateError::UndeclaredClassHash(class_hash)),
        }
    }

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

        #[cfg(not(feature = "cairo_native"))]
        return self.get_casm(class_hash);

        #[cfg(feature = "cairo_native")]
        {
            if !self.contract_class_manager.run_cairo_native() {
                // Cairo native is disabled - fetch and return the casm.
                return self.get_casm(class_hash);
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
                        return self.get_casm(class_hash);
                    }
                }
            };

            // Native not found in cache. Get the cached casm.
            let cached_casm = self.get_cached_casm(class_hash)?;

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
