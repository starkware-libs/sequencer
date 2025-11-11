use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use apollo_class_manager_types::SharedClassManagerClient;
use apollo_metrics::metrics::LossyIntoF64;
use apollo_state_sync_types::communication::{
    SharedStateSyncClient,
    StateSyncClient,
    StateSyncClientError,
    StateSyncClientResult,
};
use apollo_state_sync_types::errors::StateSyncError;
use apollo_state_sync_types::state_sync_types::SyncBlock;
use async_trait::async_trait;
use blockifier::execution::contract_class::{
    CompiledClassV0,
    CompiledClassV1,
    RunnableCompiledClass,
};
use blockifier::state::errors::StateError;
use blockifier::state::global_cache::CompiledClasses;
use blockifier::state::state_api::{StateReader as BlockifierStateReader, StateResult};
use blockifier::state::state_reader_and_contract_manager::FetchCompiledClasses;
use futures::executor::block_on;
use starknet_api::block::{BlockHash, BlockInfo, BlockNumber, GasPriceVector, GasPrices};
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::state::{SierraContractClass, StorageKey};
use starknet_types_core::felt::Felt;
use tracing::error;

use crate::metrics::{
    GATEWAY_VALIDATE_STATEFUL_TX_STORAGE_OPERATIONS,
    GATEWAY_VALIDATE_STATEFUL_TX_STORAGE_TIME,
};
use crate::state_reader::{MempoolStateReader, StateReaderFactory};

// TODO(Arni): Remove this class. The similar class `ClassReader` in `apollo_state_reader` is
// different enough from this one so be careful when refactoring.
pub struct ClassReader {
    pub reader: SharedClassManagerClient,
    // Used to invoke async functions from sync reader code.
    pub runtime: tokio::runtime::Handle,
}

impl ClassReader {
    fn read_executable(&self, class_hash: ClassHash) -> StateResult<Option<ContractClass>> {
        let casm = self
            .runtime
            .block_on(self.reader.get_executable(class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?;

        Ok(casm)
    }

    fn read_sierra(&self, class_hash: ClassHash) -> StateResult<Option<SierraContractClass>> {
        let sierra = self
            .runtime
            .block_on(self.reader.get_sierra(class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?;

        Ok(sierra)
    }
}

/// A transaction should use a single instance of this struct rather than creating multiple ones to
/// make sure metrics are accurate.
pub(crate) struct SyncStateReader {
    block_number: BlockNumber,
    state_sync_client: SharedStateSyncClientMetricWrapper,
    class_reader: ClassReader,
    runtime: tokio::runtime::Handle,
}

impl SyncStateReader {
    pub fn from_number(
        state_sync_client: SharedStateSyncClient,
        class_manager_client: SharedClassManagerClient,
        block_number: BlockNumber,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        let class_reader = ClassReader { reader: class_manager_client, runtime: runtime.clone() };
        Self {
            block_number,
            state_sync_client: SharedStateSyncClientMetricWrapper::new(state_sync_client),
            class_reader,
            runtime,
        }
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

impl FetchCompiledClasses for SyncStateReader {
    fn get_compiled_classes(&self, class_hash: ClassHash) -> StateResult<CompiledClasses> {
        let compiled_class = self
            .class_reader
            .read_executable(class_hash)?
            .ok_or(StateError::UndeclaredClassHash(class_hash))?;
        match compiled_class {
            ContractClass::V1((casm, _sierra_version)) => {
                let sierra = self.class_reader.read_sierra(class_hash)?.ok_or_else(|| {
                    error!(
                        "Class hash {class_hash:?} is declared in CASM but not in Sierra. Even \
                         though it should be coupled."
                    );
                    StateError::UndeclaredClassHash(class_hash)
                })?;
                let sierra_version = SierraVersion::extract_from_program(&sierra.sierra_program)?;
                assert_eq!(sierra_version, _sierra_version);
                Ok(CompiledClasses::V1(
                    CompiledClassV1::try_from((casm, sierra_version))?,
                    Arc::new(sierra),
                ))
            }
            ContractClass::V0(deprecated_contract_class) => {
                Ok(CompiledClasses::V0(CompiledClassV0::try_from(deprecated_contract_class)?))
            }
        }
    }

    /// Returns whether the given Cairo1 class is declared.
    fn is_declared(&self, _class_hash: ClassHash) -> StateResult<bool> {
        unimplemented!();

        // let compiled_class = match self.class_reader.read_executable(_class_hash)? {
        //     Some(compiled_class) => compiled_class,
        //     None => return Ok(false),
        // };

        // match compiled_class {
        //     ContractClass::V1(_) => {
        //         return Ok(true);
        //     }
        //     ContractClass::V0(_) => {
        //         return Ok(false);
        //     }
        // }
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
            .block_on(self.class_reader.reader.get_executable(class_hash))
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

struct SharedStateSyncClientMetricWrapper {
    state_sync_client: SharedStateSyncClient,
    num_storage_operations: AtomicU64,
    total_time_storage_operations_micros: AtomicU64,
}

impl SharedStateSyncClientMetricWrapper {
    fn new(state_sync_client: SharedStateSyncClient) -> Self {
        Self {
            state_sync_client,
            num_storage_operations: AtomicU64::new(0),
            total_time_storage_operations_micros: AtomicU64::new(0),
        }
    }
}

impl SharedStateSyncClientMetricWrapper {
    async fn run_command_with_metrics<T>(&self, command: impl Future<Output = T>) -> T {
        let start = Instant::now();
        let result = command.await;
        self.total_time_storage_operations_micros.fetch_add(
            start
                .elapsed()
                .as_micros()
                .try_into()
                .expect("Storage time as micros does not fit in u64 (over 550,000 years?!)"),
            Ordering::Relaxed,
        );
        self.num_storage_operations.fetch_add(1, Ordering::Relaxed);
        result
    }
}

#[async_trait]
impl StateSyncClient for SharedStateSyncClientMetricWrapper {
    async fn get_block(&self, block_number: BlockNumber) -> StateSyncClientResult<SyncBlock> {
        self.run_command_with_metrics(self.state_sync_client.get_block(block_number)).await
    }
    async fn get_block_hash(&self, block_number: BlockNumber) -> StateSyncClientResult<BlockHash> {
        self.run_command_with_metrics(self.state_sync_client.get_block_hash(block_number)).await
    }
    async fn add_new_block(&self, sync_block: SyncBlock) -> StateSyncClientResult<()> {
        self.run_command_with_metrics(self.state_sync_client.add_new_block(sync_block)).await
    }
    async fn get_storage_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
        storage_key: StorageKey,
    ) -> StateSyncClientResult<Felt> {
        self.run_command_with_metrics(self.state_sync_client.get_storage_at(
            block_number,
            contract_address,
            storage_key,
        ))
        .await
    }
    async fn get_nonce_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> StateSyncClientResult<Nonce> {
        self.run_command_with_metrics(
            self.state_sync_client.get_nonce_at(block_number, contract_address),
        )
        .await
    }
    async fn get_class_hash_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> StateSyncClientResult<ClassHash> {
        self.run_command_with_metrics(
            self.state_sync_client.get_class_hash_at(block_number, contract_address),
        )
        .await
    }
    async fn get_latest_block_number(&self) -> StateSyncClientResult<Option<BlockNumber>> {
        self.run_command_with_metrics(self.state_sync_client.get_latest_block_number()).await
    }
    async fn is_class_declared_at(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> StateSyncClientResult<bool> {
        self.run_command_with_metrics(
            self.state_sync_client.is_class_declared_at(block_number, class_hash),
        )
        .await
    }
}

impl Drop for SharedStateSyncClientMetricWrapper {
    fn drop(&mut self) {
        GATEWAY_VALIDATE_STATEFUL_TX_STORAGE_OPERATIONS
            .record(self.num_storage_operations.load(Ordering::Relaxed).into_f64());

        GATEWAY_VALIDATE_STATEFUL_TX_STORAGE_TIME.record(
            self.total_time_storage_operations_micros.load(Ordering::Relaxed).into_f64()
                / 1_000_000.0, // Histogram buckets are best fit in seconds
        );
    }
}

pub(crate) struct SyncStateReaderFactory {
    pub shared_state_sync_client: SharedStateSyncClient,
    pub class_manager_client: SharedClassManagerClient,
    pub runtime: tokio::runtime::Handle,
}

/// Use any of these factory methods only once per transaction to make sure metrics are accurate.
impl StateReaderFactory for SyncStateReaderFactory {
    fn get_state_reader_from_latest_block(
        &self,
    ) -> StateSyncClientResult<Box<dyn MempoolStateReader>> {
        let latest_block_number = self
            .runtime
            // TODO(guy.f): Do we want to count this as well?
            .block_on(self.shared_state_sync_client.get_latest_block_number())?
            .ok_or(StateSyncClientError::StateSyncError(StateSyncError::EmptyState))?;

        Ok(Box::new(SyncStateReader::from_number(
            self.shared_state_sync_client.clone(),
            self.class_manager_client.clone(),
            latest_block_number,
            self.runtime.clone(),
        )))
    }
}
