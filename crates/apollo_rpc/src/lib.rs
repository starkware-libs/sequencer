// config compiler to support coverage_attribute feature when running coverage in nightly mode
// within this crate
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

mod api;
mod middleware;
mod pending;
mod rpc_metrics;
#[cfg(test)]
mod rpc_test;
mod syncing_state;
#[cfg(test)]
mod test_utils;
mod v0_8;
mod version_config;

use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use apollo_class_manager_types::SharedClassManagerClient;
use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::validators::validate_ascii;
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_rpc_execution::ExecutionConfig;
use apollo_starknet_client::reader::PendingData;
use apollo_starknet_client::writer::StarknetGatewayClient;
use apollo_starknet_client::RetryConfig;
use apollo_storage::base_layer::BaseLayerStorageReader;
use apollo_storage::body::events::EventIndex;
use apollo_storage::db::TransactionKind;
use apollo_storage::state::StateStorageReader;
use apollo_storage::{StorageReader, StorageScope, StorageTxn};
use jsonrpsee::core::RpcResult;
use jsonrpsee::server::{ServerBuilder, ServerHandle};
use jsonrpsee::types::error::ErrorCode::InternalError;
use jsonrpsee::types::error::INTERNAL_ERROR_MSG;
use jsonrpsee::types::ErrorObjectOwned;
pub use latest::error;
use papyrus_common::pending_classes::PendingClasses;
use rpc_metrics::MetricLogger;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHashAndNumber, BlockNumber, BlockStatus};
use starknet_api::core::ChainId;
use tokio::sync::RwLock;
use tracing::{debug, error, info, instrument};
// Aliasing the latest version of the RPC.
use v0_8 as latest;
pub use v0_8::api::CompiledContractClass;
use validator::Validate;

use crate::api::get_methods_from_supported_apis;
use crate::middleware::proxy_rpc_request;
use crate::syncing_state::get_last_synced_block;
pub use crate::v0_8::transaction::{
    InvokeTransaction as InvokeTransactionRPC0_8,
    InvokeTransactionV1 as InvokeTransactionV1RPC0_8,
    TransactionVersion1 as TransactionVersion1RPC0_8,
};
pub use crate::v0_8::write_api_result::AddInvokeOkResult as AddInvokeOkResultRPC0_8;

// TODO(Shahak): Consider adding genesis hash to the config to support chains that have
// different genesis hash.
// TODO(DanB): Consider moving to a more general place.
const GENESIS_HASH: &str = "0x0";

/// Maximum size of a supported transaction body - 10MB.
pub const SERVER_MAX_BODY_SIZE: u32 = 10 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Validate)]
pub struct RpcConfig {
    #[validate(custom = "validate_ascii")]
    pub chain_id: ChainId,
    pub ip: IpAddr,
    pub port: u16,
    pub max_events_chunk_size: usize,
    pub max_events_keys: usize,
    // TODO(lev,shahak): remove once we remove papyrus.
    pub collect_metrics: bool,
    pub starknet_url: String,
    pub apollo_gateway_retry_config: RetryConfig,
    pub execution_config: ExecutionConfig,
}

impl Default for RpcConfig {
    fn default() -> Self {
        RpcConfig {
            chain_id: ChainId::Mainnet,
            ip: "0.0.0.0".parse().unwrap(),
            port: 8090,
            max_events_chunk_size: 1000,
            max_events_keys: 100,
            collect_metrics: false,
            starknet_url: String::from("https://alpha-mainnet.starknet.io/"),
            apollo_gateway_retry_config: RetryConfig {
                retry_base_millis: 50,
                retry_max_delay_millis: 1000,
                max_retries: 5,
            },
            execution_config: ExecutionConfig::default(),
        }
    }
}

impl SerializeConfig for RpcConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut self_params_dump = BTreeMap::from_iter([
            ser_param(
                "chain_id",
                &self.chain_id,
                "The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "ip",
                &self.ip.to_string(), "The JSON RPC server ip.",
                ParamPrivacyInput::Public
            ),
            ser_param(
                "port",
                &self.port,
                "The JSON RPC server port.",
                ParamPrivacyInput::Public
            ),
            ser_param(
                "max_events_chunk_size",
                &self.max_events_chunk_size,
                "Maximum chunk size supported by the node in get_events requests.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_events_keys",
                &self.max_events_keys,
                "Maximum number of keys supported by the node in get_events requests.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "collect_metrics",
                &self.collect_metrics,
                "If true, collect metrics for the rpc.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "starknet_url",
                &self.starknet_url,
                "URL for communicating with Starknet in write_api methods.",
                ParamPrivacyInput::Public,
            ),
        ]);

        self_params_dump
            .append(&mut prepend_sub_config_name(self.execution_config.dump(), "execution_config"));
        let mut retry_config_dump = prepend_sub_config_name(
            self.apollo_gateway_retry_config.dump(),
            "apollo_gateway_retry_config",
        );
        for param in retry_config_dump.values_mut() {
            param.description = format!(
                "For communicating with Starknet gateway, {}{}",
                param.description[0..1].to_lowercase(),
                &param.description[1..]
            );
        }
        self_params_dump.append(&mut retry_config_dump);
        self_params_dump
    }
}

fn internal_server_error(err: impl std::fmt::Display) -> ErrorObjectOwned {
    error!("{}: {}", INTERNAL_ERROR_MSG, err);
    ErrorObjectOwned::owned(InternalError.code(), INTERNAL_ERROR_MSG, None::<()>)
}

fn internal_server_error_with_msg(err: impl std::fmt::Display) -> ErrorObjectOwned {
    error!("{}: {}", INTERNAL_ERROR_MSG, err);
    ErrorObjectOwned::owned(InternalError.code(), err.to_string(), None::<()>)
}

fn verify_storage_scope(storage_reader: &StorageReader) -> RpcResult<()> {
    match storage_reader.get_scope() {
        StorageScope::StateOnly => {
            Err(internal_server_error_with_msg("Unsupported method in state-only scope."))
        }
        StorageScope::FullArchive => Ok(()),
    }
}

/// Get the latest block that we've downloaded and that we've downloaded its state diff.
fn get_latest_block_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
) -> Result<Option<BlockNumber>, ErrorObjectOwned> {
    Ok(txn.get_state_marker().map_err(internal_server_error)?.prev())
}

fn get_block_status<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<BlockStatus, ErrorObjectOwned> {
    let base_layer_tip = txn.get_base_layer_block_marker().map_err(internal_server_error)?;
    let status = if block_number < base_layer_tip {
        BlockStatus::AcceptedOnL1
    } else {
        BlockStatus::AcceptedOnL2
    };

    Ok(status)
}

#[derive(Clone, Debug, PartialEq)]
struct ContinuationTokenAsStruct(EventIndex);

#[instrument(skip(storage_reader, class_manager_client), level = "debug", err)]
pub async fn run_server(
    config: &RpcConfig,
    shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    pending_data: Arc<RwLock<PendingData>>,
    pending_classes: Arc<RwLock<PendingClasses>>,
    storage_reader: StorageReader,
    node_version: &'static str,
    class_manager_client: Option<SharedClassManagerClient>,
) -> anyhow::Result<(SocketAddr, ServerHandle)> {
    debug!("Started get_last_synced_block");
    let starting_block = get_last_synced_block(storage_reader.clone())?;
    debug!("Starting JSON-RPC.");
    let methods = get_methods_from_supported_apis(
        &config.chain_id,
        config.execution_config,
        storage_reader,
        config.max_events_chunk_size,
        config.max_events_keys,
        starting_block,
        shared_highest_block,
        pending_data,
        pending_classes,
        Arc::new(StarknetGatewayClient::new(
            &config.starknet_url,
            node_version,
            config.apollo_gateway_retry_config,
        )?),
        class_manager_client,
    );
    let addr;
    let handle;
    let server_builder = ServerBuilder::default()
        .max_request_body_size(SERVER_MAX_BODY_SIZE)
        .set_middleware(tower::ServiceBuilder::new().filter_async(proxy_rpc_request));

    let server_address = SocketAddr::new(config.ip, config.port);

    if config.collect_metrics {
        let server =
            server_builder.set_logger(MetricLogger::new(&methods)).build(&server_address).await?;
        addr = server.local_addr()?;
        handle = server.start(methods);
    } else {
        let server = server_builder.build(&server_address).await?;
        addr = server.local_addr()?;
        handle = server.start(methods);
    }
    info!(local_address = %addr, "JSON-RPC is running.");
    Ok((addr, handle))
}
