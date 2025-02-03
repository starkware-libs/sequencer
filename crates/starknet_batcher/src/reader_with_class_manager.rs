use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader, StateResult};
use futures::executor::block_on;
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_class_manager_types::SharedClassManagerClient;
use starknet_types_core::felt::Felt;

// TODO(Elin): remove once class manager is properly integrated into Papyrus reader.
pub struct ReaderWithClassManager<S: StateReader> {
    state_reader: S,
    class_manager_client: SharedClassManagerClient,
}

impl<S: StateReader> ReaderWithClassManager<S> {
    pub fn new(state_reader: S, class_manager_client: SharedClassManagerClient) -> Self {
        Self { state_reader, class_manager_client }
    }
}

impl<S: StateReader> StateReader for ReaderWithClassManager<S> {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        self.state_reader.get_storage_at(contract_address, key)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        self.state_reader.get_nonce_at(contract_address)
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        self.state_reader.get_class_hash_at(contract_address)
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        let contract_class = block_on(self.class_manager_client.get_executable(class_hash))
            .map_err(|e| StateError::StateReadError(e.to_string()))?;

        match contract_class {
            // TODO(noamsp): Remove this once class manager component is implemented.
            ContractClass::V0(ref inner) if inner == &Default::default() => {
                self.state_reader.get_compiled_class(class_hash)
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
        self.state_reader.get_compiled_class_hash(class_hash)
    }
}
