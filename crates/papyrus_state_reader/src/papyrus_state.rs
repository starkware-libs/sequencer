use blockifier::execution::contract_class::{
    CompiledClassV0,
    CompiledClassV1,
    RunnableCompiledClass,
    VersionedRunnableCompiledClass,
};
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::errors::{couple_casm_and_sierra, StateError};
use blockifier::state::global_cache::CachedCairoNative;
use blockifier::state::state_api::{StateReader, StateResult};
use blockifier::transaction::test_utils::versioned_constants;
use blockifier::versioned_constants::VersionedConstants;
use papyrus_storage::compiled_class::CasmStorageReader;
use papyrus_storage::db::RO;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::StorageReader;
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::SierraVersion;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{SierraContractClass, StateNumber, StorageKey};
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

#[cfg(feature = "cairo_native")]
type InnerCallReturnType = (VersionedRunnableCompiledClass, Option<SierraContractClass>);
#[cfg(not(feature = "cairo_native"))]
type InnerCallReturnType = VersionedRunnableCompiledClass;

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
    fn get_compiled_class_inner(&self, class_hash: ClassHash) -> StateResult<InnerCallReturnType> {
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
            let runnable_compiled =
                RunnableCompiledClass::V1(CompiledClassV1::try_from(casm_compiled_class)?);

            #[cfg(feature = "cairo_native")]
            return Ok(
                VersionedRunnableCompiledClass::Cairo1((runnable_compiled, sierra_version)),
                sierra,
            );
            #[cfg(not(feature = "cairo_native"))]
            return Ok(VersionedRunnableCompiledClass::Cairo1((runnable_compiled, sierra_version)));
        }

        let v0_compiled_class = self
            .reader()?
            .get_state_reader()
            .and_then(|sr| sr.get_deprecated_class_definition_at(state_number, &class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?;

        match v0_compiled_class {
            #[cfg(feature = "cairo_native")]
            Some(starknet_api_contract_class) => Ok(
                VersionedRunnableCompiledClass::Cairo0(
                    CompiledClassV0::try_from(starknet_api_contract_class)?.into(),
                ),
                None,
            ),
            #[cfg(not(feature = "cairo_native"))]
            Some(starknet_api_contract_class) => Ok(VersionedRunnableCompiledClass::Cairo0(
                CompiledClassV0::try_from(starknet_api_contract_class)?.into(),
            )),
            None => Err(StateError::UndeclaredClassHash(class_hash)),
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
        #[cfg(feature = "cairo_native")]
        {
            let versioned_constants = VersionedConstants::latest_constants();
            let versioned_contract_class = self.contract_class_manager.get_native(&class_hash);
            match versioned_contract_class {
                Some(cached_native) => {
                    match cached_native {
                        CachedCairoNative::Compiled(compiled_native) => {
                            return Ok(RunnableCompiledClass::from(compiled_native));
                        }
                        CachedCairoNative::CompilationFailed => {
                            let casm = self.contract_class_manager.get_casm(&class_hash);
                            match casm {
                                Some(contract_class) => {
                                    Ok(RunnableCompiledClass::from(contract_class))
                                }
                                None => {
                                    let versioned_contract_class_from_db =
                                        self.get_compiled_class_inner(class_hash)?;
                                    // The class was declared in a previous (finalized) state;
                                    // update the global cache.
                                    self.contract_class_manager.set_casm(
                                        class_hash,
                                        versioned_contract_class_from_db.clone(),
                                    );
                                    Ok(RunnableCompiledClass::from(
                                        versioned_contract_class_from_db,
                                    ))
                                }
                            }
                        }
                    }
                }
                None => {
                    versioned_contract_class_from_db = self.get_compiled_class_inner(class_hash)?;
                    let compiled_casm = self.contract_class_manager.get_casm(&class_hash);
                    let (casm, sierra) = match compiled_casm {
                        Some(casm) => (
                            casm,
                            self.contract_class_manager
                                .get_sierra(&class_hash)
                                .expect("Sierra class not found"),
                        ),
                        None => self.get_compiled_class_inner(class_hash),
                    };

                    match sierra {
                        None => {
                            self.contract_class_manager.set_casm(class_hash, casm);
                            Ok(RunnableCompiledClass::from(casm))
                        }
                        Some(sierra) => match casm {
                            VersionedRunnableCompiledClass::Cairo0(casm) => {
                                panic!("Casm V0 is not supported with Sierra");
                            }
                            VersionedRunnableCompiledClass::Cairo1((casm, sierra_version)) => {
                                if sierra_version
                                    < versioned_constants.min_compiler_version_for_sierra_gas
                                {
                                    self.contract_class_manager.set_casm(class_hash, casm);
                                    return Ok(RunnableCompiledClass::from(casm));
                                }

                                self.contract_class_manager.send_compilation_request(
                                    CompilationRequest::new(class_hash, Arc::new(sierra), casm),
                                );
                                Ok(RunnableCompiledClass::from(casm))
                            }
                        },
                    }
                }
            }
        }

        #[cfg(not(feature = "cairo_native"))]
        let versioned_contract_class = self.contract_class_manager.get_casm(&class_hash);

        match versioned_contract_class {
            Some(contract_class) => Ok(RunnableCompiledClass::from(contract_class)),
            None => {
                let versioned_contract_class_from_db = self.get_compiled_class_inner(class_hash)?;
                // The class was declared in a previous (finalized) state; update the global cache.
                self.contract_class_manager
                    .set_casm(class_hash, versioned_contract_class_from_db.clone());
                Ok(RunnableCompiledClass::from(versioned_contract_class_from_db))
            }
        }
    }

    fn get_compiled_class_hash(&self, _class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        todo!()
    }
}
