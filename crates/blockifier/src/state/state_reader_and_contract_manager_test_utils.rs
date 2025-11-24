use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::execution::contract_class::RunnableCompiledClass;
use crate::state::global_cache::CompiledClasses;
use crate::state::state_api::{StateReader, StateResult};
use crate::state::state_reader_and_contract_manager::FetchCompiledClasses as FetchCompiledClassesTrait;

mockall::mock! {
    pub FetchCompiledClasses {}

    impl StateReader for FetchCompiledClasses {
        fn get_storage_at(
            &self,
            contract_address: ContractAddress,
            key: StorageKey,
        ) -> StateResult<Felt>;

        fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce>;

        fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash>;

        fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass>;

        fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash>;
    }

    // Implement FetchCompiledClasses methods
    impl FetchCompiledClassesTrait for FetchCompiledClasses {
        fn get_compiled_classes(&self, class_hash: ClassHash) -> StateResult<CompiledClasses>;
        fn is_declared(&self, class_hash: ClassHash) -> StateResult<bool>;
    }
}
