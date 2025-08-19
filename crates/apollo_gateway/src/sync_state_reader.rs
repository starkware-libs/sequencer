use apollo_class_manager_types::SharedClassManagerClient;
use apollo_state_sync_types::communication::{
    SharedStateSyncClient,
    StateSyncClientError,
    StateSyncClientResult,
};
use apollo_state_sync_types::errors::StateSyncError;
use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader as BlockifierStateReader, StateResult};
use futures::executor::block_on;
use starknet_api::block::{BlockInfo, BlockNumber, GasPriceVector, GasPrices};
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::state_reader::{MempoolStateReader, StateReaderFactory};

pub(crate) struct SyncStateReader {
    block_number: BlockNumber,
    state_sync_client: SharedStateSyncClient,
    class_manager_client: SharedClassManagerClient,
    runtime: tokio::runtime::Handle,
}

impl SyncStateReader {
    pub fn from_number(
        state_sync_client: SharedStateSyncClient,
        class_manager_client: SharedClassManagerClient,
        block_number: BlockNumber,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        Self { block_number, state_sync_client, class_manager_client, runtime }
    }
}

impl MempoolStateReader for SyncStateReader {
    fn get_block_info(&self) -> StateResult<BlockInfo> {
        let block = block_on(self.state_sync_client.get_block(self.block_number))
            .map_err(|e| StateError::StateReadError(e.to_string()))?;

        let block_header = block.block_header_without_hash;
        let block_info = BlockInfo {
            block_number: block_header.block_number,
            block_timestamp: block_header.timestamp,
            sequencer_address: block_header.sequencer.0,
            gas_prices: GasPrices {
                eth_gas_prices: GasPriceVector {
                    l1_gas_price: block_header.l1_gas_price.price_in_wei.try_into()?,
                    l1_data_gas_price: block_header.l1_data_gas_price.price_in_wei.try_into()?,
                    l2_gas_price: block_header.l2_gas_price.price_in_wei.try_into()?,
                },
                strk_gas_prices: GasPriceVector {
                    l1_gas_price: block_header.l1_gas_price.price_in_fri.try_into()?,
                    l1_data_gas_price: block_header.l1_data_gas_price.price_in_fri.try_into()?,
                    l2_gas_price: block_header.l2_gas_price.price_in_fri.try_into()?,
                },
            },
            use_kzg_da: match block_header.l1_da_mode {
                L1DataAvailabilityMode::Blob => true,
                L1DataAvailabilityMode::Calldata => false,
            },
        };

        Ok(block_info)
    }
}

impl BlockifierStateReader for SyncStateReader {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        let res = self.runtime.block_on(self.state_sync_client.get_storage_at(
            self.block_number,
            contract_address,
            key,
        ));

        match res {
            Ok(value) => Ok(value),
            Err(StateSyncClientError::StateSyncError(StateSyncError::ContractNotFound(_))) => {
                Ok(Felt::default())
            }
            Err(e) => Err(StateError::StateReadError(e.to_string())),
        }
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        let res = self
            .runtime
            .block_on(self.state_sync_client.get_nonce_at(self.block_number, contract_address));

        match res {
            Ok(value) => Ok(value),
            Err(StateSyncClientError::StateSyncError(StateSyncError::ContractNotFound(_))) => {
                Ok(Nonce::default())
            }
            Err(e) => Err(StateError::StateReadError(e.to_string())),
        }
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        let is_class_declared = self
            .runtime
            .block_on(self.state_sync_client.is_class_declared_at(self.block_number, class_hash))
            .map_err(|e| StateError::StateReadError(e.to_string()))?;

        if !is_class_declared {
            return Err(StateError::UndeclaredClassHash(class_hash));
        }

        let contract_class = self
            .runtime
            .block_on(self.class_manager_client.get_executable(class_hash))
            .map_err(|e| StateError::StateReadError(e.to_string()))?
            .expect(
                "Class with hash {class_hash:?} doesn't appear in class manager even though it \
                 was declared",
            );

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
        let res = self.runtime.block_on(
            self.state_sync_client.get_class_hash_at(self.block_number, contract_address),
        );

        match res {
            Ok(value) => Ok(value),
            Err(StateSyncClientError::StateSyncError(StateSyncError::ContractNotFound(_))) => {
                Ok(ClassHash::default())
            }
            Err(e) => Err(StateError::StateReadError(e.to_string())),
        }
    }

    fn get_compiled_class_hash(&self, _class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        todo!()
    }
}

pub struct SyncStateReaderFactory {
    pub shared_state_sync_client: SharedStateSyncClient,
    pub class_manager_client: SharedClassManagerClient,
    pub runtime: tokio::runtime::Handle,
}

impl StateReaderFactory for SyncStateReaderFactory {
    fn get_state_reader_from_latest_block(
        &self,
    ) -> StateSyncClientResult<Box<dyn MempoolStateReader>> {
        let latest_block_number = self
            .runtime
            .block_on(self.shared_state_sync_client.get_latest_block_number())?
            .ok_or(StateSyncClientError::StateSyncError(StateSyncError::EmptyState))?;

        Ok(Box::new(SyncStateReader::from_number(
            self.shared_state_sync_client.clone(),
            self.class_manager_client.clone(),
            latest_block_number,
            self.runtime.clone(),
        )))
    }

    fn get_state_reader(&self, block_number: BlockNumber) -> Box<dyn MempoolStateReader> {
        Box::new(SyncStateReader::from_number(
            self.shared_state_sync_client.clone(),
            self.class_manager_client.clone(),
            block_number,
            self.runtime.clone(),
        ))
    }
}
