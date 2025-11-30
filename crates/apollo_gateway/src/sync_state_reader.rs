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
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{SierraContractClass, StorageKey};
use starknet_types_core::felt::Felt;
use tracing::error;

use crate::fixed_block_state_reader::{
    GatewayFixedBlockStateReader,
    GatewayFixedBlockSyncStateClient,
};
use crate::metrics::{
    GATEWAY_VALIDATE_STATEFUL_TX_STORAGE_OPERATIONS,
    GATEWAY_VALIDATE_STATEFUL_TX_STORAGE_TIME,
};
use crate::state_reader::{GatewayStateReaderWithCompiledClasses, StateReaderFactory};

/// A transaction should use a single instance of this struct rather than creating multiple ones to
/// make sure metrics are accurate.
pub(crate) struct SyncStateReader {
    block_number: BlockNumber,
    state_sync_client: SharedStateSyncClientMetricWrapper,
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
        Self {
            block_number,
            state_sync_client: SharedStateSyncClientMetricWrapper::new(state_sync_client),
            class_manager_client,
            runtime,
        }
    }

    fn read_sierra(&self, class_hash: ClassHash) -> StateResult<Option<SierraContractClass>> {
        let sierra = self
            .runtime
            .block_on(self.class_manager_client.get_sierra(class_hash))
            .map_err(|err| StateError::StateReadError(err.to_string()))?;

        Ok(sierra)
    }

    fn get_compiled_class_from_client(&self, class_hash: ClassHash) -> StateResult<ContractClass> {
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

        Ok(contract_class)
    }
}

impl FetchCompiledClasses for SyncStateReader {
    fn get_compiled_classes(&self, class_hash: ClassHash) -> StateResult<CompiledClasses> {
        let contract_class = self.get_compiled_class_from_client(class_hash)?;
        match contract_class {
            ContractClass::V1(casm_contract_class) => {
                let sierra = self.read_sierra(class_hash)?.ok_or_else(|| {
                    error!(
                        "Class hash {class_hash:?} is declared in CASM but not in Sierra. Even \
                         though it should be coupled."
                    );
                    StateError::UndeclaredClassHash(class_hash)
                })?;
                Ok(CompiledClasses::V1(
                    CompiledClassV1::try_from(casm_contract_class)?,
                    Arc::new(sierra),
                ))
            }
            ContractClass::V0(deprecated_contract_class) => {
                Ok(CompiledClasses::V0(CompiledClassV0::try_from(deprecated_contract_class)?))
            }
        }
    }

    /// Returns whether the given Cairo1 class is declared.
    fn is_declared(&self, class_hash: ClassHash) -> StateResult<bool> {
        self.runtime
            .block_on(
                self.state_sync_client.is_cairo_1_class_declared_at(self.block_number, class_hash),
            )
            .map_err(|e| StateError::StateReadError(e.to_string()))
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
        let contract_class = self.get_compiled_class_from_client(class_hash)?;

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

impl GatewayStateReaderWithCompiledClasses for SyncStateReader {}

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
    async fn is_cairo_1_class_declared_at(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> StateSyncClientResult<bool> {
        self.run_command_with_metrics(
            self.state_sync_client.is_cairo_1_class_declared_at(block_number, class_hash),
        )
        .await
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
#[async_trait]
impl StateReaderFactory for SyncStateReaderFactory {
    // TODO(guy.f): The call to `get_latest_block_number()` is not counted in the storage metrics as
    // it is done prior to the creation of SharedStateSyncClientMetricWrapper, directly via the
    // SharedStateSyncClient.
    async fn get_blockifier_state_reader_and_gateway_fixed_block_from_latest_block(
        &self,
    ) -> StateSyncClientResult<(
        Box<dyn GatewayStateReaderWithCompiledClasses>,
        Box<dyn GatewayFixedBlockStateReader>,
    )> {
        let latest_block_number = self
            .shared_state_sync_client
            .get_latest_block_number()
            .await?
            .ok_or(StateSyncClientError::StateSyncError(StateSyncError::EmptyState))?;

        let blockifier_state_reader = SyncStateReader::from_number(
            self.shared_state_sync_client.clone(),
            self.class_manager_client.clone(),
            latest_block_number,
            self.runtime.clone(),
        );
        let gateway_fixed_block_sync_state_client = GatewayFixedBlockSyncStateClient::new(
            self.shared_state_sync_client.clone(),
            latest_block_number,
        );
        Ok((Box::new(blockifier_state_reader), Box::new(gateway_fixed_block_sync_state_client)))
    }
}
