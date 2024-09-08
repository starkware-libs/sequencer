#[cfg(test)]
#[path = "run_test.rs"]
mod run_test;

use std::future::pending;
use std::process::exit;
use std::sync::Arc;
use std::time::Duration;

use futures::stream::StreamExt;
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
use papyrus_network::network_manager::{BroadcastTopicChannels, NetworkManager};
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
use starknet_api::block::{BlockHash, BlockNumber};
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

pub struct PapyrusUtilities {
    storage_reader: StorageReader,
    storage_writer: StorageWriter,
    maybe_network_manager: Option<NetworkManager>,
    local_peer_id: String,
}

impl PapyrusUtilities {
    pub fn new(config: &NodeConfig) -> anyhow::Result<Self> {
        let (storage_reader, storage_writer) = open_storage(config.storage.clone())?;
        let (maybe_network_manager, local_peer_id) = register_to_network(config.network.clone())?;
        Ok(Self { storage_reader, storage_writer, maybe_network_manager, local_peer_id })
    }
}

pub struct PapyrusTaskHandles {
    storage_metrics_handle: Option<JoinHandle<anyhow::Result<()>>>,
    sync_client_and_rpc_server_handles:
        Option<(JoinHandle<anyhow::Result<()>>, JoinHandle<anyhow::Result<()>>)>,
    monitoring_server_handle: Option<JoinHandle<anyhow::Result<()>>>,
    p2p_sync_server_handle: Option<JoinHandle<anyhow::Result<()>>>,
    consensus_handle: Option<JoinHandle<anyhow::Result<()>>>,
    network_handle: Option<JoinHandle<anyhow::Result<()>>>,
}

impl Default for PapyrusTaskHandles {
    fn default() -> Self {
        Self {
            storage_metrics_handle: None,
            sync_client_and_rpc_server_handles: None,
            monitoring_server_handle: None,
            p2p_sync_server_handle: None,
            consensus_handle: None,
            network_handle: None,
        }
    }
}

#[cfg(feature = "rpc")]
async fn create_rpc_server_future(
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
    Ok(tokio::spawn(async move { Ok(server_handle.stopped().await) }))
}

#[cfg(not(feature = "rpc"))]
async fn create_rpc_server_future(
    _config: &NodeConfig,
    _shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    _pending_data: Arc<RwLock<PendingData>>,
    _pending_classes: Arc<RwLock<PendingClasses>>,
    _storage_reader: StorageReader,
) -> anyhow::Result<JoinHandle<anyhow::Result<()>>> {
    Ok(tokio::spawn(pending()))
}

fn build_consensus(
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
    let BroadcastTopicChannels { messages_to_broadcast_sender, broadcast_client_channels } =
        network_channels;
    // TODO(matan): connect this to an actual channel.
    if let Some(test_config) = config.test.as_ref() {
        let sync_channels = network_manager
            .register_broadcast_topic(Topic::new(test_config.sync_topic.clone()), BUFFER_SIZE)?;
        let context = PapyrusConsensusContext::new(
            storage_reader.clone(),
            messages_to_broadcast_sender,
            config.num_validators,
            Some(sync_channels.messages_to_broadcast_sender),
        );
        let sync_receiver =
            sync_channels.broadcast_client_channels.map(|(vote, _report_sender)| {
                BlockNumber(vote.expect("Sync channel should never have errors").height)
            });
        Ok(tokio::spawn(async move {
            Ok(papyrus_consensus::run_consensus(
                context,
                config.start_height,
                config.validator_id,
                config.consensus_delay,
                config.timeouts.clone(),
                broadcast_client_channels,
                sync_receiver,
            )
            .await?)
        }))
    } else {
        let context = PapyrusConsensusContext::new(
            storage_reader.clone(),
            messages_to_broadcast_sender,
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
                broadcast_client_channels,
                futures::stream::pending(),
            )
            .await?)
        }))
    }
}
fn spawn_storage_metrics_collector(
    storage_reader: StorageReader,
    update_interval: Duration,
) -> JoinHandle<anyhow::Result<()>> {
    tokio::spawn(
        async move {
            loop {
                if let Err(error) = update_storage_metrics(&storage_reader) {
                    warn!("Failed to update storage metrics: {error}");
                }
                tokio::time::sleep(update_interval).await;
            }
        }
        .instrument(debug_span!("collect_storage_metrics")),
    )
}

fn build_storage_metrics(
    collect_metrics: bool,
    storage_reader: StorageReader,
) -> JoinHandle<anyhow::Result<()>> {
    if collect_metrics {
        spawn_storage_metrics_collector(storage_reader, STORAGE_METRICS_UPDATE_INTERVAL)
    } else {
        tokio::spawn(pending())
    }
}

fn build_p2p_sync_server(
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
    tokio::spawn(async move { Ok(p2p_sync_server.run().await) })
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

fn build_monitoring_server(
    utils: &PapyrusUtilities,
    config: &NodeConfig,
) -> anyhow::Result<JoinHandle<anyhow::Result<()>>> {
    let monitoring_server = MonitoringServer::new(
        config.monitoring_gateway.clone(),
        get_config_presentation(config, true)?,
        get_config_presentation(config, false)?,
        utils.storage_reader.clone(),
        VERSION_FULL,
        utils.local_peer_id.clone(),
    )?;
    Ok(tokio::spawn(async move { Ok(monitoring_server.run_server().await?) }))
}

async fn build_sync_client_and_rpc_server(
    maybe_network_manager: Option<&mut NetworkManager>,
    storage_reader: StorageReader,
    storage_writer: StorageWriter,
    config: &NodeConfig,
) -> anyhow::Result<(JoinHandle<anyhow::Result<()>>, JoinHandle<anyhow::Result<()>>)> {
    // The sync is the only writer of the syncing state.
    let shared_highest_block = Arc::new(RwLock::new(None));
    let pending_data = Arc::new(RwLock::new(PendingData {
        // The pending data might change later to DeprecatedPendingBlock, depending on the response
        // from the feeder gateway.
        block: PendingBlockOrDeprecated::Current(PendingBlock {
            parent_block_hash: BlockHash(felt!(GENESIS_HASH)),
            ..Default::default()
        }),
        ..Default::default()
    }));
    let pending_classes = Arc::new(RwLock::new(PendingClasses::default()));

    // JSON-RPC server.
    let server_handle = create_rpc_server_future(
        config,
        shared_highest_block.clone(),
        pending_data.clone(),
        pending_classes.clone(),
        storage_reader.clone(),
    )
    .await?;

    // Sync task.
    let sync_handle = match (config.sync, config.p2p_sync) {
        (Some(_), Some(_)) => {
            panic!("One of --sync.#is_none or --p2p_sync.#is_none must be turned on");
        }
        (None, None) => tokio::spawn(pending()),
        (Some(sync_config), None) => {
            let configs = (sync_config.clone(), config.central.clone(), config.base_layer.clone());
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
    };
    Ok((sync_handle, server_handle))
}

async fn run_threads(
    config: NodeConfig,
    mut utils: PapyrusUtilities,
    tasks: PapyrusTaskHandles,
) -> anyhow::Result<()> {
    let storage_metrics_handle = tasks.storage_metrics_handle.unwrap_or_else(|| {
        build_storage_metrics(
            config.monitoring_gateway.collect_metrics,
            utils.storage_reader.clone(),
        )
    });
    let p2p_sync_server_handle = tasks.p2p_sync_server_handle.unwrap_or_else(|| {
        build_p2p_sync_server(utils.maybe_network_manager.as_mut(), utils.storage_reader.clone())
    });

    let consensus_handle = if let Some(handle) = tasks.consensus_handle {
        handle
    } else {
        build_consensus(
            config.consensus.as_ref(),
            utils.storage_reader.clone(),
            utils.maybe_network_manager.as_mut(),
        )?
    };

    let monitoring_server_handle = if let Some(handle) = tasks.monitoring_server_handle {
        handle
    } else {
        build_monitoring_server(&utils, &config)?
    };

    let (sync_client_handle, rpc_server_handle) =
        if let Some((sync_client_handle, rpc_server_handle)) =
            tasks.sync_client_and_rpc_server_handles
        {
            (sync_client_handle, rpc_server_handle)
        } else {
            build_sync_client_and_rpc_server(
                utils.maybe_network_manager.as_mut(),
                utils.storage_reader.clone(),
                utils.storage_writer,
                &config,
            )
            .await?
        };

    // Run last, since the network manager is passed to other tasks during setup.
    let network_handle =
        tasks.network_handle.unwrap_or_else(|| match utils.maybe_network_manager {
            Some(manager) => tokio::spawn(async move { Ok(manager.run().await?) }),
            None => tokio::spawn(pending()),
        });

    // Run tasks.
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
    return Ok(());
}

type NetworkRunReturn = (Option<NetworkManager>, String);

fn register_to_network(network_config: Option<NetworkConfig>) -> anyhow::Result<NetworkRunReturn> {
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

pub async fn run(
    config: NodeConfig,
    utils: PapyrusUtilities,
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
    run_threads(config, utils, tasks).await
}
