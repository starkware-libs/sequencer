use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader as BlockifierStateReader, StateResult};
use futures::executor::block_on;
use starknet_api::block::{BlockInfo, BlockNumber};
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_state_sync_types::communication::SharedStateSyncClient;
use starknet_types_core::felt::Felt;

use crate::state_reader::{MempoolStateReader, StateReaderFactory};

#[allow(dead_code)]
struct SyncStateReader {
    block_number: BlockNumber,
    state_sync_client: SharedStateSyncClient,
}

impl SyncStateReader {
    pub fn from_number(
        state_sync_client: SharedStateSyncClient,
        block_number: BlockNumber,
    ) -> Self {
        Self { block_number, state_sync_client }
    }
}

impl MempoolStateReader for SyncStateReader {
    fn get_block_info(&self) -> StateResult<BlockInfo> {
        todo!()
    }
}

impl BlockifierStateReader for SyncStateReader {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        let res = block_on(self.state_sync_client.get_storage_at(
            self.block_number,
            contract_address,
            key,
        ))
        .map_err(|e| StateError::StateReadError(e.to_string()))?;

        Ok(res)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        let res =
            block_on(self.state_sync_client.get_nonce_at(self.block_number, contract_address))
                .map_err(|e| StateError::StateReadError(e.to_string()))?;

        Ok(res)
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        let contract_class = block_on(
            self.state_sync_client.get_compiled_class_deprecated(self.block_number, class_hash),
        )
        .map_err(|e| StateError::StateReadError(e.to_string()))?;

        match contract_class {
            ContractClass::V1(casm_contract_class) => {
                Ok(RunnableCompiledClass::V1(casm_contract_class.try_into()?))
            }
            ContractClass::V0(deprecated_contract_class) => {
                Ok(RunnableCompiledClass::V0(deprecated_contract_class.try_into()?))
            }
        }
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        let res =
            block_on(self.state_sync_client.get_class_hash_at(self.block_number, contract_address))
                .map_err(|e| StateError::StateReadError(e.to_string()))?;

        Ok(res)
    }

    fn get_compiled_class_hash(&self, _class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        todo!()
    }
}

pub struct SyncStateReaderFactory {
    pub shared_state_sync_client: SharedStateSyncClient,
}

impl StateReaderFactory for SyncStateReaderFactory {
    // TODO(noamsp): Decide if we need this
    fn get_state_reader_from_latest_block(&self) -> Box<dyn MempoolStateReader> {
        todo!()
    }

    fn get_state_reader(&self, block_number: BlockNumber) -> Box<dyn MempoolStateReader> {
        Box::new(SyncStateReader::from_number(self.shared_state_sync_client.clone(), block_number))
    }
}
