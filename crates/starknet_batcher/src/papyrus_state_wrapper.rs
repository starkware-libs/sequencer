use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader, StateResult};
use futures::executor::block_on;
use papyrus_state_reader::papyrus_state::PapyrusReader;
use papyrus_storage::StorageReader;
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_class_manager_types::SharedClassManagerClient;
use starknet_types_core::felt::Felt;
pub struct PapyrusReaderWithClassManager {
    papyrus_reader: PapyrusReader,
    class_manager_client: SharedClassManagerClient,
}

impl PapyrusReaderWithClassManager {
    pub fn new(
        storage_reader: StorageReader,
        latest_block: BlockNumber,
        contract_class_manager: ContractClassManager,
        class_manager_client: SharedClassManagerClient,
    ) -> Self {
        Self {
            papyrus_reader: PapyrusReader::new(
                storage_reader,
                latest_block,
                contract_class_manager,
            ),
            class_manager_client,
        }
    }
}

impl StateReader for PapyrusReaderWithClassManager {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        self.papyrus_reader.get_storage_at(contract_address, key)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        self.papyrus_reader.get_nonce_at(contract_address)
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        self.papyrus_reader.get_class_hash_at(contract_address)
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        let contract_class = block_on(self.class_manager_client.get_executable(class_hash))
            .map_err(|e| StateError::StateReadError(e.to_string()))?;

        match contract_class {
            // TODO(noamsp): Remove this once class manager component is implemented.
            ContractClass::V0(ref inner) if inner == &Default::default() => {
                self.papyrus_reader.get_compiled_class(class_hash)
            }
            ContractClass::V1(casm_contract_class) => {
                Ok(RunnableCompiledClass::V1(casm_contract_class.try_into()?))
            }
            ContractClass::V0(deprecated_contract_class) => {
                Ok(RunnableCompiledClass::V0(deprecated_contract_class.try_into()?))
            }
        }
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        self.papyrus_reader.get_compiled_class_hash(class_hash)
    }
}
