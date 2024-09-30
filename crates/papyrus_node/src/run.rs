#[cfg(test)]
#[path = "run_test.rs"]
mod run_test;

use std::future::pending;
use std::process::exit;
use std::sync::Arc;
use std::time::Duration;

use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use papyrus_common::metrics::COLLECT_PROFILING_METRICS;
use papyrus_common::pending_classes::PendingClasses;
use papyrus_common::BlockHashAndNumber;
use papyrus_config::presentation::get_config_presentation;
use papyrus_config::validators::config_validate;
use papyrus_consensus::config::ConsensusConfig;
use papyrus_consensus_orchestrator::papyrus_consensus_context::PapyrusConsensusContext;
use papyrus_monitoring_gateway::MonitoringServer;
use papyrus_network::gossipsub_impl::Topic;
use papyrus_network::network_manager::NetworkManager;
use papyrus_network::{network_manager, NetworkConfig};
use papyrus_p2p_sync::client::{P2PSyncClient, P2PSyncClientChannels};
use papyrus_p2p_sync::server::{P2PSyncServer, P2PSyncServerChannels};
use papyrus_p2p_sync::{Protocol, BUFFER_SIZE};
#[cfg(feature = "rpc")]
use papyrus_rpc::run_server;
use papyrus_storage::{open_storage, update_storage_metrics, StorageReader, StorageWriter};
use papyrus_sync::sources::base_layer::{BaseLayerSourceError, EthereumBaseLayerSource};
use papyrus_sync::sources::central::{CentralError, CentralSource, CentralSourceConfig};
use papyrus_sync::sources::pending::PendingSource;
use papyrus_sync::{StateSync, SyncConfig};
use starknet_api::block::BlockHash;
use starknet_api::felt;
use starknet_client::reader::objects::pending_data::{PendingBlock, PendingBlockOrDeprecated};
use starknet_client::reader::PendingData;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::metadata::LevelFilter;
use tracing::{debug, debug_span, error, info, warn, Instrument};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

use crate::config::NodeConfig;
use crate::version::VERSION_FULL;

// TODO(yair): Add to config.
const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;

// TODO(shahak): Consider adding genesis hash to the config to support chains that have
// different genesis hash.
// TODO: Consider moving to a more general place.
const GENESIS_HASH: &str = "0x0";

// TODO(dvir): add this to config.
// Duration between updates to the storage metrics (those in the collect_storage_metrics function).
const STORAGE_METRICS_UPDATE_INTERVAL: Duration = Duration::from_secs(10);

pub struct PapyrusResources {
    pub storage_reader: StorageReader,
    pub storage_writer: StorageWriter,
    pub maybe_network_manager: Option<NetworkManager>,
    pub local_peer_id: String,
    pub shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    pub pending_data: Arc<RwLock<PendingData>>,
    pub pending_classes: Arc<RwLock<PendingClasses>>,
}

/// Struct which allows configuring how the node will run.
/// - If left `None`, the task will be spawn with its default (prod) configuration.
/// - If set to Some, that variant of the task will be run, and the default ignored.
///     - If you want to disable a task set it to `Some(tokio::spawn(pending()))`.
#[derive(Default)]
pub struct PapyrusTaskHandles {
    pub storage_metrics_handle: Option<JoinHandle<anyhow::Result<()>>>,
    pub rpc_server_handle: Option<JoinHandle<anyhow::Result<()>>>,
    pub sync_client_handle: Option<JoinHandle<anyhow::Result<()>>>,
    pub monitoring_server_handle: Option<JoinHandle<anyhow::Result<()>>>,
    pub p2p_sync_server_handle: Option<JoinHandle<anyhow::Result<()>>>,
    pub consensus_handle: Option<JoinHandle<anyhow::Result<()>>>,
    pub network_handle: Option<JoinHandle<anyhow::Result<()>>>,
}

impl PapyrusResources {
    pub fn new(config: &NodeConfig) -> anyhow::Result<Self> {
        let (storage_reader, storage_writer) = open_storage(config.storage.clone())?;
        let (maybe_network_manager, local_peer_id) = build_network_manager(config.network.clone())?;
        let shared_highest_block = Arc::new(RwLock::new(None));
        let pending_data = Arc::new(RwLock::new(PendingData {
            // The pending data might change later to DeprecatedPendingBlock, depending on the
            // response from the feeder gateway.
            block: PendingBlockOrDeprecated::Current(PendingBlock {
                parent_block_hash: BlockHash(felt!(GENESIS_HASH)),
                ..Default::default()
            }),
            ..Default::default()
        }));
        let pending_classes = Arc::new(RwLock::new(PendingClasses::default()));
        Ok(Self {
            storage_reader,
            storage_writer,
            maybe_network_manager,
            local_peer_id,
            shared_highest_block,
            pending_data,
            pending_classes,
        })
    }
}

fn build_network_manager(
    network_config: Option<NetworkConfig>,
) -> anyhow::Result<(Option<NetworkManager>, String)> {
    let Some(network_config) = network_config else {
        return Ok((None, "".to_string()));
    };
    let network_manager = network_manager::NetworkManager::new(
        network_config.clone(),
        Some(VERSION_FULL.to_string()),
    );
    let local_peer_id = network_manager.get_local_peer_id();

    Ok((Some(network_manager), local_peer_id))
}

#[cfg(feature = "rpc")]
async fn spawn_rpc_server(
    config: &NodeConfig,
    shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    pending_data: Arc<RwLock<PendingData>>,
    pending_classes: Arc<RwLock<PendingClasses>>,
    storage_reader: StorageReader,
) -> anyhow::Result<JoinHandle<anyhow::Result<()>>> {
    let (_, server_handle) = run_server(
        &config.rpc,
        shared_highest_block,
        pending_data,
        pending_classes,
        storage_reader,
        VERSION_FULL,
    )
    .await?;
    Ok(tokio::spawn(async move {
        server_handle.stopped().await;
        Ok(())
    }))
}

#[cfg(not(feature = "rpc"))]
async fn spawn_rpc_server(
    _config: &NodeConfig,
    _shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    _pending_data: Arc<RwLock<PendingData>>,
    _pending_classes: Arc<RwLock<PendingClasses>>,
    _storage_reader: StorageReader,
) -> anyhow::Result<JoinHandle<anyhow::Result<()>>> {
    Ok(tokio::spawn(pending()))
}

fn spawn_monitoring_server(
    storage_reader: StorageReader,
    local_peer_id: String,
    config: &NodeConfig,
) -> anyhow::Result<JoinHandle<anyhow::Result<()>>> {
    let monitoring_server = MonitoringServer::new(
        config.monitoring_gateway.clone(),
        get_config_presentation(config, true)?,
        get_config_presentation(config, false)?,
        storage_reader,
        VERSION_FULL,
        local_peer_id,
    )?;
    Ok(tokio::spawn(async move { Ok(monitoring_server.run_server().await?) }))
}

fn spawn_consensus(
    config: Option<&ConsensusConfig>,
    storage_reader: StorageReader,
    network_manager: Option<&mut NetworkManager>,
) -> anyhow::Result<JoinHandle<anyhow::Result<()>>> {
    let (Some(config), Some(network_manager)) = (config, network_manager) else {
        info!("Consensus is disabled.");
        return Ok(tokio::spawn(pending()));
    };
    let config = config.clone();
    debug!("Consensus configuration: {config:?}");

    let network_channels = network_manager
        .register_broadcast_topic(Topic::new(config.network_topic.clone()), BUFFER_SIZE)?;
    let context = PapyrusConsensusContext::new(
        storage_reader.clone(),
        network_channels.broadcast_topic_client.clone(),
        config.num_validators,
        None,
    );
    Ok(tokio::spawn(async move {
        Ok(papyrus_consensus::run_consensus(
            context,
            config.start_height,
            config.validator_id,
            config.consensus_delay,
            config.timeouts.clone(),
            network_channels.into(),
            futures::stream::pending(),
        )
        .await?)
    }))
}

async fn run_sync(
    configs: (SyncConfig, CentralSourceConfig, EthereumBaseLayerConfig),
    shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    pending_data: Arc<RwLock<PendingData>>,
    pending_classes: Arc<RwLock<PendingClasses>>,
    storage: (StorageReader, StorageWriter),
) -> anyhow::Result<()> {
    let (sync_config, central_config, base_layer_config) = configs;
    let (storage_reader, storage_writer) = storage;
    let central_source =
        CentralSource::new(central_config.clone(), VERSION_FULL, storage_reader.clone())
            .map_err(CentralError::ClientCreation)?;
    let pending_source =
        PendingSource::new(central_config, VERSION_FULL).map_err(CentralError::ClientCreation)?;
    let base_layer_source = EthereumBaseLayerSource::new(base_layer_config)
        .map_err(|e| BaseLayerSourceError::BaseLayerSourceCreationError(e.to_string()))?;
    let mut sync = StateSync::new(
        sync_config,
        shared_highest_block,
        pending_data,
        pending_classes,
        central_source,
        pending_source,
        base_layer_source,
        storage_reader.clone(),
        storage_writer,
    );
    Ok(sync.run().await?)
}

async fn spawn_sync_client(
    maybe_network_manager: Option<&mut NetworkManager>,
    storage_reader: StorageReader,
    storage_writer: StorageWriter,
    config: &NodeConfig,
    shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    pending_data: Arc<RwLock<PendingData>>,
    pending_classes: Arc<RwLock<PendingClasses>>,
) -> JoinHandle<anyhow::Result<()>> {
    match (config.sync, config.p2p_sync) {
        (Some(_), Some(_)) => {
            panic!("One of --sync.#is_none or --p2p_sync.#is_none must be turned on");
        }
        (None, None) => tokio::spawn(pending()),
        (Some(sync_config), None) => {
            let configs = (sync_config, config.central.clone(), config.base_layer.clone());
            let storage = (storage_reader.clone(), storage_writer);
            tokio::spawn(run_sync(
                configs,
                shared_highest_block,
                pending_data,
                pending_classes,
                storage,
            ))
        }
        (None, Some(p2p_sync_client_config)) => {
            let network_manager = maybe_network_manager
                .expect("If p2p sync is enabled, network needs to be enabled too");
            let header_client_sender = network_manager
                .register_sqmr_protocol_client(Protocol::SignedBlockHeader.into(), BUFFER_SIZE);
            let state_diff_client_sender = network_manager
                .register_sqmr_protocol_client(Protocol::StateDiff.into(), BUFFER_SIZE);
            let transaction_client_sender = network_manager
                .register_sqmr_protocol_client(Protocol::Transaction.into(), BUFFER_SIZE);
            let class_client_sender =
                network_manager.register_sqmr_protocol_client(Protocol::Class.into(), BUFFER_SIZE);
            let p2p_sync_client_channels = P2PSyncClientChannels::new(
                header_client_sender,
                state_diff_client_sender,
                transaction_client_sender,
                class_client_sender,
            );
            let p2p_sync = P2PSyncClient::new(
                p2p_sync_client_config,
                storage_reader,
                storage_writer,
                p2p_sync_client_channels,
            );
            tokio::spawn(async move { Ok(p2p_sync.run().await?) })
        }
    }
}

fn spawn_p2p_sync_server(
    network_manager: Option<&mut NetworkManager>,
    storage_reader: StorageReader,
) -> JoinHandle<anyhow::Result<()>> {
    let Some(network_manager) = network_manager else {
        info!("P2P Sync is disabled.");
        return tokio::spawn(pending());
    };

    let header_server_receiver = network_manager
        .register_sqmr_protocol_server(Protocol::SignedBlockHeader.into(), BUFFER_SIZE);
    let state_diff_server_receiver =
        network_manager.register_sqmr_protocol_server(Protocol::StateDiff.into(), BUFFER_SIZE);
    let transaction_server_receiver =
        network_manager.register_sqmr_protocol_server(Protocol::Transaction.into(), BUFFER_SIZE);
    let class_server_receiver =
        network_manager.register_sqmr_protocol_server(Protocol::Class.into(), BUFFER_SIZE);
    let event_server_receiver =
        network_manager.register_sqmr_protocol_server(Protocol::Event.into(), BUFFER_SIZE);

    let p2p_sync_server_channels = P2PSyncServerChannels::new(
        header_server_receiver,
        state_diff_server_receiver,
        transaction_server_receiver,
        class_server_receiver,
        event_server_receiver,
    );

    let p2p_sync_server = P2PSyncServer::new(storage_reader.clone(), p2p_sync_server_channels);
    tokio::spawn(async move {
        p2p_sync_server.run().await;
        Ok(())
    })
}

async fn run_threads(
    config: NodeConfig,
    mut resources: PapyrusResources,
    tasks: PapyrusTaskHandles,
) -> anyhow::Result<()> {
    let consensus_handle = if let Some(handle) = tasks.consensus_handle {
        handle
    } else {
        spawn_consensus(
            config.consensus.as_ref(),
            resources.storage_reader.clone(),
            resources.maybe_network_manager.as_mut(),
        )?
    };

    let storage_metrics_handle = if let Some(handle) = tasks.storage_metrics_handle {
        handle
    } else {
        spawn_storage_metrics_collector(
            config.monitoring_gateway.collect_metrics,
            resources.storage_reader.clone(),
            STORAGE_METRICS_UPDATE_INTERVAL,
        )
    };
    // Monitoring server.
    let monitoring_server_handle = if let Some(handle) = tasks.monitoring_server_handle {
        handle
    } else {
        spawn_monitoring_server(
            resources.storage_reader.clone(),
            resources.local_peer_id.clone(),
            &config,
        )?
    };

    // JSON-RPC server.
    let rpc_server_handle = if let Some(handle) = tasks.rpc_server_handle {
        handle
    } else {
        spawn_rpc_server(
            &config,
            resources.shared_highest_block.clone(),
            resources.pending_data.clone(),
            resources.pending_classes.clone(),
            resources.storage_reader.clone(),
        )
        .await?
    };

    // P2P Sync Server task.
    let p2p_sync_server_handle = if let Some(handle) = tasks.p2p_sync_server_handle {
        handle
    } else {
        spawn_p2p_sync_server(
            resources.maybe_network_manager.as_mut(),
            resources.storage_reader.clone(),
        )
    };

    // Sync task.
    let sync_client_handle = if let Some(handle) = tasks.sync_client_handle {
        handle
    } else {
        spawn_sync_client(
            resources.maybe_network_manager.as_mut(),
            resources.storage_reader,
            resources.storage_writer,
            &config,
            resources.shared_highest_block,
            resources.pending_data,
            resources.pending_classes,
        )
        .await
    };

    // Created last since it consumes the network manager.
    let network_handle = if let Some(handle) = tasks.network_handle {
        handle
    } else {
        match resources.maybe_network_manager {
            Some(manager) => tokio::spawn(async move { Ok(manager.run().await?) }),
            None => tokio::spawn(pending()),
        }
    };
    tokio::select! {
        res = storage_metrics_handle => {
            error!("collecting storage metrics stopped.");
            res??
        }
        res = rpc_server_handle => {
            error!("RPC server stopped.");
            res??
        }
        res = monitoring_server_handle => {
            error!("Monitoring server stopped.");
            res??
        }
        res = sync_client_handle => {
            error!("Sync stopped.");
            res??
        }
        res = p2p_sync_server_handle => {
            error!("P2P Sync server stopped");
            res??
        }
        res = network_handle => {
            error!("Network stopped.");
            res??
        }
        res = consensus_handle => {
            error!("Consensus stopped.");
            res??
        }
    };
    error!("Task ended with unexpected Ok.");
    Ok(())
}

// TODO(yair): add dynamic level filtering.
// TODO(dan): filter out logs from dependencies (happens when RUST_LOG=DEBUG)
// TODO(yair): define and implement configurable filtering.
fn configure_tracing() {
    let fmt_layer = fmt::layer().compact().with_target(false);
    let level_filter_layer =
        EnvFilter::builder().with_default_directive(DEFAULT_LEVEL.into()).from_env_lossy();

    // This sets a single subscriber to all of the threads. We may want to implement different
    // subscriber for some threads and use set_global_default instead of init.
    tracing_subscriber::registry().with(fmt_layer).with(level_filter_layer).init();
}

fn spawn_storage_metrics_collector(
    collect_metrics: bool,
    storage_reader: StorageReader,
    interval: Duration,
) -> JoinHandle<anyhow::Result<()>> {
    if !collect_metrics {
        return tokio::spawn(pending());
    }

    tokio::spawn(
        async move {
            loop {
                if let Err(error) = update_storage_metrics(&storage_reader) {
                    warn!("Failed to update storage metrics: {error}");
                }
                tokio::time::sleep(interval).await;
            }
        }
        .instrument(debug_span!("collect_storage_metrics")),
    )
}

pub async fn run(
    config: NodeConfig,
    resources: PapyrusResources,
    tasks: PapyrusTaskHandles,
) -> anyhow::Result<()> {
    configure_tracing();

    if let Err(errors) = config_validate(&config) {
        error!("{}", errors);
        exit(1);
    }

    COLLECT_PROFILING_METRICS
        .set(config.collect_profiling_metrics)
        .expect("This should be the first and only time we set this value.");

    info!("Booting up.");
    run_threads(config, resources, tasks).await
}
