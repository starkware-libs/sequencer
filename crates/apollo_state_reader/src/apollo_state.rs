use std::sync::Arc;

use apollo_class_manager_types::SharedClassManagerClient;
use apollo_storage::class_hash::ClassHashStorageReader;
use apollo_storage::compiled_class::CasmStorageReader;
use apollo_storage::db::RO;
use apollo_storage::state::StateStorageReader;
use apollo_storage::StorageReader;
use blockifier::execution::contract_class::{
    CompiledClassV0,
    CompiledClassV1,
    RunnableCompiledClass,
};
use blockifier::state::errors::{couple_casm_and_sierra, StateError};
use blockifier::state::global_cache::CompiledClasses;
use blockifier::state::state_api::{StateReader, StateResult};
use blockifier::state::state_reader_and_contract_manager::FetchCompiledClasses;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedClass;
use starknet_api::state::{SierraContractClass, StateNumber, StorageKey};
use starknet_types_core::felt::Felt;
use tracing::error;

#[cfg(test)]
#[path = "apollo_state_test.rs"]
mod test;

type RawApolloReader<'env> = apollo_storage::StorageTxn<'env, RO>;

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

    fn read_sierra(&self, class_hash: ClassHash) -> StateResult<SierraContractClass> {
        let sierra = self
            .runtime
            .block_on(self.reader.get_sierra(class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?
            .ok_or(StateError::UndeclaredClassHash(class_hash))?;

        Ok(sierra)
    }

    /// Returns the compiled class hash v2 for the given class hash.
    fn read_compiled_class_hash_v2(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<Option<CompiledClassHash>> {
        let compiled_class_hash_v2 = self
            .runtime
            .block_on(self.reader.get_executable_class_hash_v2(class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?;
        Ok(compiled_class_hash_v2)
    }
}

pub struct ApolloReader {
    storage_reader: StorageReader,
    latest_block: BlockNumber,
    // Reader is `None` for reader invoked through `native_blockifier`.
    class_reader: Option<ClassReader>,
}

impl ApolloReader {
    pub fn new_with_class_reader(
        storage_reader: StorageReader,
        latest_block: BlockNumber,
        class_reader: Option<ClassReader>,
    ) -> Self {
        Self { storage_reader, latest_block, class_reader }
    }

    pub fn new(storage_reader: StorageReader, latest_block: BlockNumber) -> Self {
        Self { storage_reader, latest_block, class_reader: None }
    }

    fn reader(&self) -> StateResult<RawApolloReader<'_>> {
        self.storage_reader
            .begin_ro_txn()
            .map_err(|error| StateError::StateReadError(error.to_string()))
    }

    /// Returns a V1 contract with Sierra if V1 contract is found, or a V0 contract without Sierra
    /// if a V1 contract is not found, or an `Error` otherwise.
    fn get_compiled_classes_from_storage(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<CompiledClasses> {
        assert!(
            self.class_reader.is_none(),
            "Should not enter this function if the class reader is set"
        );
        if self.is_declared(class_hash)? {
            // Cairo 1.
            let (casm_compiled_class, sierra) = self.read_casm_and_sierra(class_hash)?;
            let sierra_version = SierraVersion::extract_from_program(&sierra.sierra_program)?;
            return Ok(CompiledClasses::V1(
                CompiledClassV1::try_from((casm_compiled_class, sierra_version))?,
                Arc::new(sierra),
            ));
        }

        // Possibly Cairo 0.
        let v0_compiled_class = self.read_deprecated_casm(class_hash)?;
        match v0_compiled_class {
            Some(starknet_api_contract_class) => {
                Ok(CompiledClasses::V0(CompiledClassV0::try_from(starknet_api_contract_class)?))
            }
            None => Err(StateError::UndeclaredClassHash(class_hash)),
        }
    }

    fn get_compiled_classes_from_class_reader(
        &self,
        class_hash: ClassHash,
        class_reader: &ClassReader,
    ) -> StateResult<CompiledClasses> {
        let contract_class = class_reader.read_executable(class_hash)?;
        match contract_class {
            ContractClass::V0(starknet_api_contract_class) => {
                Ok(CompiledClasses::V0(CompiledClassV0::try_from(starknet_api_contract_class)?))
            }
            ContractClass::V1((casm_contract_class, sierra_version)) => {
                if !self.is_declared(class_hash)? {
                    return Err(StateError::UndeclaredClassHash(class_hash));
                }
                let sierra = class_reader.read_sierra(class_hash)?;
                let extracted_sierra_version =
                    SierraVersion::extract_from_program(&sierra.sierra_program)?;
                if extracted_sierra_version != sierra_version {
                    let message = format!(
                        "Sierra version mismatch. Expected: {sierra_version}, Actual: \
                         {extracted_sierra_version}"
                    );
                    let error = StateError::CasmAndSierraMismatch { class_hash, message };
                    error!("Error in getting compiled classes: {error:?}");
                    return Err(error);
                }
                Ok(CompiledClasses::V1(
                    CompiledClassV1::try_from((casm_contract_class, sierra_version))?,
                    Arc::new(sierra),
                ))
            }
        }
    }

    fn read_casm_and_sierra(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<(CasmContractClass, SierraContractClass)> {
        assert!(
            self.class_reader.is_none(),
            "Should not enter this function if the class reader is set"
        );
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
        assert!(
            self.class_reader.is_none(),
            "Should not enter this function if the class reader is set"
        );
        let state_number = StateNumber(self.latest_block);
        let option_casm = self
            .reader()?
            .get_state_reader()
            .and_then(|sr| sr.get_deprecated_class_definition_at(state_number, &class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?;

        Ok(option_casm)
    }

    /// Returns the compiled class hash v2 for the given class hash.
    /// If class reader is not set, it will read the compiled class hash v2 directly from the
    /// storage.
    fn read_compiled_class_hash_v2(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<Option<CompiledClassHash>> {
        let Some(class_reader) = &self.class_reader else {
            // Class reader is not set. Try to read directly from storage.
            let compiled_class_hash_v2 =
                self.reader()?
                    .get_executable_class_hash_v2(&class_hash)
                    .map_err(|err| StateError::StateReadError(err.to_string()))?;
            return Ok(compiled_class_hash_v2);
        };

        class_reader.read_compiled_class_hash_v2(class_hash)
    }
}

// Currently unused - will soon replace the same `impl` for `PapyrusStateReader`.
impl StateReader for ApolloReader {
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
        self.get_compiled_classes(class_hash).map(|class| class.to_runnable())
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        let state_number = StateNumber(self.latest_block);
        match self
            .reader()?
            .get_state_reader()
            .and_then(|sr| sr.get_compiled_class_hash_at(state_number, &class_hash))
        {
            Ok(Some(compiled_class_hash)) => Ok(compiled_class_hash),
            Ok(None) => Ok(CompiledClassHash::default()),
            Err(err) => Err(StateError::StateReadError(err.to_string())),
        }
    }

    fn get_compiled_class_hash_v2(
        &self,
        class_hash: ClassHash,
        _compiled_class: &RunnableCompiledClass,
    ) -> StateResult<CompiledClassHash> {
        self.read_compiled_class_hash_v2(class_hash)?
            .ok_or(StateError::MissingCompiledClassHashV2(class_hash))
    }
}

impl FetchCompiledClasses for ApolloReader {
    fn get_compiled_classes(&self, class_hash: ClassHash) -> StateResult<CompiledClasses> {
        match &self.class_reader {
            Some(class_reader) => {
                self.get_compiled_classes_from_class_reader(class_hash, class_reader)
            }
            None => self.get_compiled_classes_from_storage(class_hash),
        }
    }

    fn is_declared(&self, class_hash: ClassHash) -> StateResult<bool> {
        let state_number = self.latest_block;
        let class_declaration_block_number = self
            .reader()?
            .get_state_reader()
            .and_then(|sr| sr.get_class_definition_block_number(&class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?;
        Ok(matches!(class_declaration_block_number, Some(block_number)
            if block_number <= state_number))
    }
}
