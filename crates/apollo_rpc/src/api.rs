use std::sync::Arc;

use apollo_class_manager_types::SharedClassManagerClient;
use apollo_rpc_execution::ExecutionConfig;
use apollo_starknet_client::reader::PendingData;
use apollo_storage::StorageReader;
use jsonrpsee::{Methods, RpcModule};
use papyrus_common::pending_classes::PendingClasses;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_api::core::{ChainId, ContractAddress, EntryPointSelector};
use starknet_api::transaction::fields::Calldata;
use tokio::sync::RwLock;

use crate::v0_8::api::api_impl::JsonRpcServerImpl as JsonRpcServerV0_8Impl;
use crate::version_config;

#[derive(Copy, Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Tag {
    /// The most recent fully constructed block
    #[serde(rename = "latest")]
    Latest,
    /// Currently constructed block
    #[serde(rename = "pending")]
    Pending,
}

#[derive(Copy, Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BlockHashOrNumber {
    #[serde(rename = "block_hash")]
    Hash(BlockHash),
    #[serde(rename = "block_number")]
    Number(BlockNumber),
}

#[derive(Copy, Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum BlockId {
    HashOrNumber(BlockHashOrNumber),
    Tag(Tag),
}

/// The parameters of a call.
#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq)]
pub struct CallRequest {
    pub contract_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub calldata: Calldata,
}

/// Returns a `Methods` object with all the methods from the supported APIs.
/// Whenever adding a new API version we need to add the new version mapping here.
#[allow(clippy::too_many_arguments)]
pub fn get_methods_from_supported_apis(
    chain_id: &ChainId,
    execution_config: ExecutionConfig,
    storage_reader: StorageReader,
    max_events_chunk_size: usize,
    max_events_keys: usize,
    starting_block: BlockHashAndNumber,
    shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    pending_data: Arc<RwLock<PendingData>>,
    pending_classes: Arc<RwLock<PendingClasses>>,
    class_manager_client: Option<SharedClassManagerClient>,
) -> Methods {
    let mut methods: Methods = Methods::new();
    let server_gen = JsonRpcServerImplGenerator {
        chain_id: chain_id.clone(),
        execution_config,
        storage_reader,
        max_events_chunk_size,
        max_events_keys,
        starting_block,
        shared_highest_block,
        pending_data,
        pending_classes,
        class_manager_client,
    };
    version_config::VERSION_CONFIG
        .iter()
        .filter_map(|version_config| {
            let (version, version_state) = version_config;
            match version_state {
                version_config::VersionState::Deprecated => None,
                version_config::VersionState::Supported => {
                    let methods = match *version {
                        version_config::VERSION_0_8 => {
                            server_gen.clone().generator::<JsonRpcServerV0_8Impl>()
                        }
                        // TODO(yair): remove this once the version is an enum instead of a string.
                        _ => unreachable!("Unrecognized RPC spec version: {}", version),
                    };
                    Some(methods)
                }
            }
        })
        .fold(&mut methods, |methods, new_methods| {
            let _res = methods.merge(new_methods);
            methods
        });
    methods
}

pub trait JsonRpcServerTrait: Sized {
    #[allow(clippy::too_many_arguments)]
    fn new(
        chain_id: ChainId,
        execution_config: ExecutionConfig,
        storage_reader: StorageReader,
        max_events_chunk_size: usize,
        max_events_keys: usize,
        starting_block: BlockHashAndNumber,
        shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
        pending_data: Arc<RwLock<PendingData>>,
        pending_classes: Arc<RwLock<PendingClasses>>,
        class_manager_client: Option<SharedClassManagerClient>,
    ) -> Self;

    fn into_rpc_module(self) -> RpcModule<Self>;
}

#[derive(Clone)]
struct JsonRpcServerImplGenerator {
    chain_id: ChainId,
    execution_config: ExecutionConfig,
    storage_reader: StorageReader,
    max_events_chunk_size: usize,
    max_events_keys: usize,
    starting_block: BlockHashAndNumber,
    shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    pending_data: Arc<RwLock<PendingData>>,
    pending_classes: Arc<RwLock<PendingClasses>>,
    class_manager_client: Option<SharedClassManagerClient>,
}

type JsonRpcServerImplParams = (
    ChainId,
    ExecutionConfig,
    StorageReader,
    usize,
    usize,
    BlockHashAndNumber,
    Arc<RwLock<Option<BlockHashAndNumber>>>,
    Arc<RwLock<PendingData>>,
    Arc<RwLock<PendingClasses>>,
    Option<SharedClassManagerClient>,
);

impl JsonRpcServerImplGenerator {
    fn get_params(self) -> JsonRpcServerImplParams {
        (
            self.chain_id,
            self.execution_config,
            self.storage_reader,
            self.max_events_chunk_size,
            self.max_events_keys,
            self.starting_block,
            self.shared_highest_block,
            self.pending_data,
            self.pending_classes,
            self.class_manager_client,
        )
    }

    fn generator<T>(self) -> Methods
    where
        T: JsonRpcServerTrait,
    {
        let (
            chain_id,
            fee_contract_address,
            storage_reader,
            max_events_chunk_size,
            max_events_keys,
            starting_block,
            shared_highest_block,
            pending_data,
            pending_classes,
            class_manager_client,
        ) = self.get_params();
        Into::<Methods>::into(
            T::new(
                chain_id,
                fee_contract_address,
                storage_reader,
                max_events_chunk_size,
                max_events_keys,
                starting_block,
                shared_highest_block,
                pending_data,
                pending_classes,
                class_manager_client,
            )
            .into_rpc_module(),
        )
    }
}
