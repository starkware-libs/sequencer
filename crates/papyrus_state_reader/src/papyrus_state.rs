use blockifier::execution::contract_class::{
    CompiledClassV0,
    CompiledClassV1,
    RunnableCompiledClass,
    VersionedRunnableCompiledClass,
};
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::errors::{couple_casm_and_sierra, StateError};
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

    /// Returns a V1 contract if found, or a V0 contract if a V1 contract is not
    /// found, or an `Error` otherwise.
    fn get_compiled_class_inner(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<VersionedRunnableCompiledClass> {
        // TODO(AVIV): Return RunnableCompiledClass.
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
            let runnable_compiled = RunnableCompiledClass::V1(CompiledClassV1::try_from((
                casm_compiled_class,
                sierra_version.clone(),
            ))?);
            return Ok(VersionedRunnableCompiledClass::Cairo1((runnable_compiled, sierra_version)));
        }

        let v0_compiled_class = self
            .reader()?
            .get_state_reader()
            .and_then(|sr| sr.get_deprecated_class_definition_at(state_number, &class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?;

        match v0_compiled_class {
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
        let cached_casm = self.contract_class_manager.get_casm(&class_hash);

        match cached_casm {
            None => {
                let versioned_compiled_class_from_db = self.get_compiled_class_inner(class_hash)?;
                let cachable_casm =
                    CachedCasm::WithoutSierra(versioned_compiled_class_from_db.clone());
                // The class was declared in a previous (finalized) state; update the global cache.
                self.contract_class_manager.set_casm(class_hash, cachable_casm);
                Ok(RunnableCompiledClass::from(versioned_compiled_class_from_db))
            }
            Some(CachedCasm::WithoutSierra(versioned_compiled_class)) => {
                Ok(RunnableCompiledClass::from(versioned_compiled_class))
            }
            Some(CachedCasm::WithSierra) => {
                todo!("Add this flow when Sierra to Native compilation is added to PapyrusReader.")
            }
        }
    }

    fn get_compiled_class_hash(&self, _class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        todo!()
    }
}
