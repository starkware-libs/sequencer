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

    /// Returns a V1 contract if found, or a V0 contract if a V1 contract is not
    /// found, or an `Error` otherwise.
    fn get_compiled_class_inner(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<RunnableCompiledClass> {
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
            let casm_compiled_class_v1 =
                CompiledClassV1::try_from((casm_compiled_class, sierra_version))?;

            let runnable_compiled = RunnableCompiledClass::V1(casm_compiled_class_v1.clone());
            #[cfg(feature = "cairo_native")]
            self.contract_class_manager.cache_request_contracts(&(
                class_hash,
                Arc::new(sierra),
                casm_compiled_class_v1,
            ));
            return Ok(runnable_compiled);
        }

        let v0_compiled_class = self
            .reader()?
            .get_state_reader()
            .and_then(|sr| sr.get_deprecated_class_definition_at(state_number, &class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?;

        match v0_compiled_class {
            Some(starknet_api_contract_class) => Ok(RunnableCompiledClass::V0(
                CompiledClassV0::try_from(starknet_api_contract_class)?,
            )),
            None => Err(StateError::UndeclaredClassHash(class_hash)),
        }
    }

    fn get_compiled_class_non_native_flow(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<RunnableCompiledClass> {
        let versioned_contract_class = self.contract_class_manager.get_casm(&class_hash);
        match versioned_contract_class {
            Some(contract_class) => Ok(contract_class),
            None => Ok(self.get_compiled_class_inner(class_hash)?),
        }
    }

    #[cfg(feature = "cairo_native")]
    fn get_casm_and_sierra_when_casm_in_cache(
        &self,
        class_hash: ClassHash,
        runnable_casm: RunnableCompiledClass,
    ) -> StateResult<()> {
        match runnable_casm {
            // Cairo0 does not have a sierra class.
            RunnableCompiledClass::V0(_) => Ok(()),
            RunnableCompiledClass::V1(_) => {
                let sierra_option = self.contract_class_manager.get_sierra(&class_hash);
                match sierra_option {
                    Some(sierra) => {
                        self.contract_class_manager.set_sierra(class_hash, sierra);
                        Ok(())
                    }
                    None => {
                        // We assume that the class is already declared because the casm was in the
                        // cache.
                        let (_, option_sierra) = self
                            .reader()?
                            .get_casm_and_sierra(&class_hash)
                            .map_err(|err| StateError::StateReadError(err.to_string()))?;
                        let sierra = option_sierra.expect(
                            "Should be able to fetch a Sierra class if its definition exists, \
                             database is inconsistent.",
                        );
                        self.contract_class_manager.set_sierra(class_hash, Arc::new(sierra));
                        Ok(())
                    }
                }
            }
            #[cfg(feature = "cairo_native")]
            RunnableCompiledClass::V1Native(_) => {
                panic!("should not get here with Native runnable version")
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

        #[cfg(not(feature = "cairo_native"))]
        {
            let versioned_contract_class_from_db =
                self.get_compiled_class_non_native_flow(class_hash)?;
            // The class was declared in a previous (finalized) state; update the global
            // cache.
            self.contract_class_manager
                .set_casm(class_hash, versioned_contract_class_from_db.clone());
            Ok(versioned_contract_class_from_db)
        }

        #[cfg(feature = "cairo_native")]
        {
            // if we turned off the cairo native compilation, we use the non cairo native flow.
            if !self.contract_class_manager.run_cairo_native() {
                return self.get_compiled_class_non_native_flow(class_hash);
            }

            let native_versioned_contract_class =
                self.contract_class_manager.get_native(&class_hash);
            match native_versioned_contract_class {
                // we already compiled to native and have the cached version
                Some(cached_native) => {
                    match cached_native {
                        CachedCairoNative::Compiled(compiled_native) => {
                            Ok(RunnableCompiledClass::from(compiled_native))
                        }
                        // for some reason the compilation failed, we use the non cairo native flow.
                        CachedCairoNative::CompilationFailed => {
                            self.get_compiled_class_non_native_flow(class_hash)
                        }
                    }
                }

                // we don't have the cached version, we need to compile it
                None => {
                    let compiled_casm = self.contract_class_manager.get_casm(&class_hash);
                    // Fetch the casm and sierra for the cairo native compilation request.
                    let runnable_casm = match compiled_casm {
                        Some(versioned_casm) => {
                            self.get_casm_and_sierra_when_casm_in_cache(
                                class_hash,
                                versioned_casm.clone(),
                            )?;
                            versioned_casm
                        }
                        // We do not have the casm in the cache, needs to fetch it from
                        // the db.
                        None => self.get_compiled_class_inner(class_hash)?,
                    };

                    let sierra = self.contract_class_manager.get_sierra(&class_hash);
                    match runnable_casm {
                        RunnableCompiledClass::V0(_) => Ok(runnable_casm),
                        RunnableCompiledClass::V1(casm) => {
                            let sierra = sierra.expect("Sierra should be exists in this flow");
                            self.contract_class_manager
                                .send_compilation_request((class_hash, sierra, casm.clone()));
                            if self.contract_class_manager.wait_on_native_compilation() {
                                let compiled_native = self.contract_class_manager.get_native(&class_hash).expect(
                                    "Should have the compiled native contract if we wait on the compilation",
                                );
                                match compiled_native {
                                    CachedCairoNative::Compiled(compiled_native) => {
                                        return Ok(RunnableCompiledClass::from(compiled_native));
                                    }
                                    CachedCairoNative::CompilationFailed => {
                                        return Ok(RunnableCompiledClass::V1(casm));
                                    }
                                }

                            }
                            Ok(RunnableCompiledClass::V1(casm))
                        }
                        RunnableCompiledClass::V1Native(_) => {
                            panic!("should not get here with Native runnable version")
                        }
                    }
                }
            }
        }
    }

    fn get_compiled_class_hash(&self, _class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        todo!()
    }
}
