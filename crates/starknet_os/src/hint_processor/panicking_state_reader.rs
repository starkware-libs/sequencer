use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::state_api::{StateReader, StateResult};
use starknet_types_core::felt::Felt;

/// State reader that always panics.
/// Use this as the `OsExecutionHelper`'s state reader to ensure the OS execution is "stateless".
pub struct PanickingStateReader;

impl StateReader for PanickingStateReader {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        panic!("Called get_storage_at with address {contract_address} and key {key:?}.");
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        panic!("Called get_nonce_at with address {contract_address}.");
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        panic!("Called get_class_hash_at with address {contract_address}.");
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        panic!("Called get_compiled_class with class hash {class_hash}.");
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        panic!("Called get_compiled_class_hash with class hash {class_hash}.");
    }
}
