#[cfg(test)]
#[path = "central_test.rs"]
mod central_test;
mod state_update_stream;

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use apollo_central_sync_config::config::CentralSourceConfig;
use apollo_starknet_client::reader::{
    ReaderClientError,
    StarknetFeederGatewayClient,
    StarknetReader,
};
use apollo_starknet_client::ClientCreationError;
use apollo_storage::state::StateStorageReader;
use apollo_storage::{StorageError, StorageReader};
use async_stream::stream;
use async_trait::async_trait;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use futures::stream::BoxStream;
use futures_util::StreamExt;
use indexmap::IndexMap;
use lru::LruCache;
#[cfg(test)]
use mockall::automock;
use papyrus_common::pending_classes::ApiContractClass;
use starknet_api::block::{Block, BlockHash, BlockHashAndNumber, BlockNumber, BlockSignature};
use starknet_api::core::{ClassHash, CompiledClassHash, SequencerPublicKey};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::StateDiff;
use starknet_api::StarknetApiError;
use tracing::trace;

use self::state_update_stream::{StateUpdateStream, StateUpdateStreamConfig};

type CentralResult<T> = Result<T, CentralError>;

pub struct GenericCentralSource<TStarknetClient: StarknetReader + Send + Sync> {
    pub concurrent_requests: usize,
    pub apollo_starknet_client: Arc<TStarknetClient>,
    pub storage_reader: StorageReader,
    pub state_update_stream_config: StateUpdateStreamConfig,
    pub(crate) class_cache: Arc<Mutex<LruCache<ClassHash, ApiContractClass>>>,
    compiled_class_cache: Arc<Mutex<LruCache<ClassHash, CasmContractClass>>>,
}

#[derive(thiserror::Error, Debug)]
pub enum CentralError {
    #[error(transparent)]
    ClientCreation(#[from] ClientCreationError),
    #[error(transparent)]
    ClientError(#[from] Arc<ReaderClientError>),
    #[error("Could not find a state update.")]
    StateUpdateNotFound,
    #[error("Could not find a class definitions.")]
    ClassNotFound,
    #[error("Could not find a compiled class of {}.", class_hash)]
    CompiledClassNotFound { class_hash: ClassHash },
    #[error("Could not find a block with block number {}.", block_number)]
    BlockNotFound { block_number: BlockNumber },
    #[error(transparent)]
    StarknetApiError(#[from] Arc<StarknetApiError>),
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error("Wrong type of contract class")]
    BadContractClassType,
}

/// Combined result of a single feeder gateway `get_state_update` call (with `includeBlock=true`
/// and `includeSignature=true`), containing the block, signature, and state diff together.
#[derive(Debug)]
pub struct CentralStateUpdate {
    pub block_number: BlockNumber,
    pub block: Block,
    pub signature: BlockSignature,
    pub block_hash: BlockHash,
    pub state_diff: StateDiff,
    pub deployed_contract_class_definitions: IndexMap<ClassHash, DeprecatedContractClass>,
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait CentralSourceTrait {
    async fn get_latest_block(&self) -> Result<Option<BlockHashAndNumber>, CentralError>;

    /// Returns a stream of block state updates (block + signature + state diff) from a single
    /// feeder gateway `get_state_update` call per block.
    fn stream_new_block_state_updates(
        &self,
        initial_block_number: BlockNumber,
        up_to_block_number: BlockNumber,
    ) -> StateUpdatesStream<'_>;

    async fn get_block_hash(
        &self,
        block_number: BlockNumber,
    ) -> Result<Option<BlockHash>, CentralError>;

    fn stream_compiled_classes(
        &self,
        initial_block_number: BlockNumber,
        up_to_block_number: BlockNumber,
    ) -> CompiledClassesStream<'_>;

    // TODO(shahak): Remove once pending block is removed.
    async fn get_class(&self, class_hash: ClassHash) -> Result<ApiContractClass, CentralError>;

    // TODO(shahak): Remove once pending block is removed.
    async fn get_compiled_class(
        &self,
        class_hash: ClassHash,
    ) -> Result<CasmContractClass, CentralError>;

    async fn get_sequencer_pub_key(&self) -> Result<SequencerPublicKey, CentralError>;
}

pub type StateUpdatesStream<'a> = BoxStream<'a, CentralResult<CentralStateUpdate>>;
type CentralCompiledClass = (BlockNumber, ClassHash, CompiledClassHash, CasmContractClass);
pub(crate) type CompiledClassesStream<'a> = BoxStream<'a, CentralResult<CentralCompiledClass>>;

#[async_trait]
impl<TStarknetClient: StarknetReader + Send + Sync + 'static> CentralSourceTrait
    for GenericCentralSource<TStarknetClient>
{
    // Returns the block hash and the block number of the latest block from the central source.
    async fn get_latest_block(&self) -> Result<Option<BlockHashAndNumber>, CentralError> {
        Ok(self.apollo_starknet_client.latest_block_number_and_hash().await.map_err(Arc::new)?)
    }

    // Returns the current block hash of the given block number from the central source.
    async fn get_block_hash(
        &self,
        block_number: BlockNumber,
    ) -> Result<Option<BlockHash>, CentralError> {
        self.apollo_starknet_client
            .block(block_number)
            .await
            .map_err(Arc::new)?
            .map_or(Ok(None), |block| Ok(Some(block.block_hash())))
    }

    // Returns a stream of block state updates downloaded from the central source using a single
    // feeder gateway `get_state_update` call per block.
    fn stream_new_block_state_updates(
        &self,
        initial_block_number: BlockNumber,
        up_to_block_number: BlockNumber,
    ) -> StateUpdatesStream<'_> {
        StateUpdateStream::new(
            initial_block_number,
            up_to_block_number,
            self.apollo_starknet_client.clone(),
            self.storage_reader.clone(),
            self.state_update_stream_config.clone(),
            self.class_cache.clone(),
        )
        .boxed()
    }

    // Returns a stream of compiled classes downloaded from the central source.
    fn stream_compiled_classes(
        &self,
        initial_block_number: BlockNumber,
        up_to_block_number: BlockNumber,
    ) -> CompiledClassesStream<'_> {
        stream! {
            let txn = self.storage_reader.begin_ro_txn().map_err(CentralError::StorageError)?;
            // TODO(Aviv): Now the class hashes include both declared classes and migrated compiled class hashes.
            // Consider refactoring it.
            let class_hashes = initial_block_number
                .iter_up_to(up_to_block_number)
                .map(|bn| {
                    match txn.get_state_diff(bn) {
                        Err(err) => Err(CentralError::StorageError(err)),
                        // TODO(yair): Consider expecting, since the state diffs should not contain
                        // holes and we suppose to never exceed the state marker.
                        Ok(None) => Err(CentralError::StateUpdateNotFound),
                        Ok(Some(state_diff)) => Ok((bn, state_diff)),
                    }
                })
                .flat_map(|maybe_state_diff| match maybe_state_diff {
                    Ok((bn, state_diff)) => state_diff
                        .class_hash_to_compiled_class_hash
                        .into_iter()
                        .map(move |(class_hash, compiled_class_hash)| Ok((bn, class_hash, compiled_class_hash)))
                        .collect::<Vec<_>>(),
                    Err(err) => vec![Err(err)],
                }).collect::<Vec<_>>();

            // Drop the txn here, so we don't unnecessarily hold it open while awaiting below.
            drop(txn);

            let mut compiled_classes = futures_util::stream::iter(class_hashes)
                .map(|maybe_item| async move {
                    match maybe_item {
                        Ok((block_number, class_hash, compiled_class_hash)) => {
                            trace!("Downloading compiled class {:?}.", class_hash);
                            let compiled_class = self.get_compiled_class(class_hash).await?;
                            Ok((block_number, class_hash, compiled_class_hash, compiled_class))
                        },
                        Err(err) => Err(err),
                    }
                })
                .buffered(self.concurrent_requests);

            while let Some(maybe_compiled_class) = compiled_classes.next().await {
                match maybe_compiled_class {
                    Ok((block_number, class_hash, compiled_class_hash, compiled_class)) => {
                        yield Ok((block_number, class_hash, compiled_class_hash, compiled_class));
                    }
                    Err(err) => {
                        yield Err(err);
                        return;
                    }
                }
            }
        }
        .boxed()
    }

    async fn get_class(&self, class_hash: ClassHash) -> Result<ApiContractClass, CentralError> {
        // TODO(shahak): Fix code duplication with StateUpdatesStream.
        {
            let mut class_cache = self.class_cache.lock().expect("Failed to lock class cache.");
            if let Some(class) = class_cache.get(&class_hash) {
                return Ok(class.clone());
            }
        }
        let client_class =
            self.apollo_starknet_client.class_by_hash(class_hash).await.map_err(Arc::new)?;
        match client_class {
            None => Err(CentralError::ClassNotFound),
            Some(class) => {
                {
                    let mut class_cache =
                        self.class_cache.lock().expect("Failed to lock class cache.");
                    class_cache.put(class_hash, class.clone().into());
                }
                Ok(class.into())
            }
        }
    }

    async fn get_compiled_class(
        &self,
        class_hash: ClassHash,
    ) -> Result<CasmContractClass, CentralError> {
        {
            let mut compiled_class_cache =
                self.compiled_class_cache.lock().expect("Failed to lock class cache.");
            if let Some(class) = compiled_class_cache.get(&class_hash) {
                return Ok(class.clone());
            }
        }
        match self.apollo_starknet_client.compiled_class_by_hash(class_hash).await {
            Ok(Some(compiled_class)) => {
                let mut compiled_class_cache =
                    self.compiled_class_cache.lock().expect("Failed to lock class cache.");
                compiled_class_cache.put(class_hash, compiled_class.clone());
                Ok(compiled_class)
            }
            Ok(None) => Err(CentralError::CompiledClassNotFound { class_hash }),
            Err(err) => Err(CentralError::ClientError(Arc::new(err))),
        }
    }

    async fn get_sequencer_pub_key(&self) -> Result<SequencerPublicKey, CentralError> {
        Ok(self.apollo_starknet_client.sequencer_pub_key().await.map_err(Arc::new)?)
    }
}

pub type CentralSource = GenericCentralSource<StarknetFeederGatewayClient>;

impl CentralSource {
    pub fn new(
        config: CentralSourceConfig,
        node_version: &'static str,
        storage_reader: StorageReader,
    ) -> Result<CentralSource, ClientCreationError> {
        let apollo_starknet_client = StarknetFeederGatewayClient::new(
            config.starknet_url.as_ref(),
            config.http_headers,
            node_version,
            config.retry_config,
        )?;

        Ok(CentralSource {
            concurrent_requests: config.concurrent_requests,
            apollo_starknet_client: Arc::new(apollo_starknet_client),
            storage_reader,
            state_update_stream_config: StateUpdateStreamConfig {
                max_state_updates_to_download: config.max_state_updates_to_download,
                max_state_updates_to_store_in_memory: config.max_state_updates_to_store_in_memory,
                max_classes_to_download: config.max_classes_to_download,
            },
            class_cache: Arc::from(Mutex::new(LruCache::new(
                NonZeroUsize::new(config.class_cache_size)
                    .expect("class_cache_size should be a positive integer."),
            ))),
            compiled_class_cache: Arc::from(Mutex::new(LruCache::new(
                NonZeroUsize::new(config.class_cache_size)
                    .expect("class_cache_size should be a positive integer."),
            ))),
        })
    }
}
