use std::future::pending;
use std::pin::Pin;

use futures::{Future, FutureExt};
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerContract;
use starknet_batcher::communication::{LocalBatcherServer, RemoteBatcherServer};
use starknet_consensus_manager::communication::ConsensusManagerServer;
use starknet_gateway::communication::{LocalGatewayServer, RemoteGatewayServer};
use starknet_http_server::communication::HttpServer;
use starknet_l1_provider::communication::{
    L1ScraperServer,
    LocalL1ProviderServer,
    RemoteL1ProviderServer,
};
use starknet_mempool::communication::{LocalMempoolServer, RemoteMempoolServer};
use starknet_mempool_p2p::propagator::{
    LocalMempoolP2pPropagatorServer,
    RemoteMempoolP2pPropagatorServer,
};
use starknet_mempool_p2p::runner::MempoolP2pRunnerServer;
use starknet_monitoring_endpoint::communication::MonitoringEndpointServer;
use starknet_sequencer_infra::component_server::{
    ComponentServerStarter,
    LocalComponentServer,
    RemoteComponentServer,
    WrapperServer,
};
use starknet_sequencer_infra::errors::ComponentServerError;
use starknet_state_sync::runner::StateSyncRunnerServer;
use starknet_state_sync::{LocalStateSyncServer, RemoteStateSyncServer};
use tokio::task::{JoinError, JoinSet};
use tracing::error;

use crate::clients::SequencerNodeClients;
use crate::communication::SequencerNodeCommunication;
use crate::components::SequencerNodeComponents;
use crate::config::component_execution_config::{
    ActiveComponentExecutionMode,
    ReactiveComponentExecutionMode,
};
use crate::config::node_config::SequencerNodeConfig;

// Component servers that can run locally.
struct LocalServers {
    pub(crate) batcher: Option<Box<LocalBatcherServer>>,
    pub(crate) gateway: Option<Box<LocalGatewayServer>>,
    pub(crate) l1_provider: Option<Box<LocalL1ProviderServer>>,
    pub(crate) mempool: Option<Box<LocalMempoolServer>>,
    pub(crate) mempool_p2p_propagator: Option<Box<LocalMempoolP2pPropagatorServer>>,
    pub(crate) state_sync: Option<Box<LocalStateSyncServer>>,
}

// Component servers that wrap a component without a server.
struct WrapperServers {
    pub(crate) consensus_manager: Option<Box<ConsensusManagerServer>>,
    pub(crate) http_server: Option<Box<HttpServer>>,
    pub(crate) l1_scraper_server: Option<Box<L1ScraperServer<EthereumBaseLayerContract>>>,
    pub(crate) monitoring_endpoint: Option<Box<MonitoringEndpointServer>>,
    pub(crate) mempool_p2p_runner: Option<Box<MempoolP2pRunnerServer>>,
    pub(crate) state_sync_runner: Option<Box<StateSyncRunnerServer>>,
}

// Component servers that can run remotely.
// TODO(Nadin): Remove pub from the struct and update the fields to be pub(crate).
pub struct RemoteServers {
    pub batcher: Option<Box<RemoteBatcherServer>>,
    pub gateway: Option<Box<RemoteGatewayServer>>,
    pub l1_provider: Option<Box<RemoteL1ProviderServer>>,
    pub mempool: Option<Box<RemoteMempoolServer>>,
    pub mempool_p2p_propagator: Option<Box<RemoteMempoolP2pPropagatorServer>>,
    pub state_sync: Option<Box<RemoteStateSyncServer>>,
}

pub struct SequencerNodeServers {
    local_servers: LocalServers,
    remote_servers: RemoteServers,
    wrapper_servers: WrapperServers,
}

/// A macro for creating a remote component server based on the component's execution mode.
/// Returns a remote server if the component is configured with Remote execution mode; otherwise,
/// returns None.
///
/// # Arguments
///
/// * `$execution_mode` - Component execution mode reference.
/// * `$local_client_getter` - Local client getter function, used for the remote server
///   initialization if needed.
/// * `$config` - Remote server configuration.
///
/// # Returns
///
/// An `Option<Box<RemoteComponentServer<LocalClientType, RequestType, ResponseType>>>` containing
/// the remote server if the execution mode is Remote, or None if the execution mode is Disabled,
/// LocalExecutionWithRemoteEnabled, or LocalExecutionWithRemoteDisabled.
///
/// # Example
///
/// ```rust,ignore
/// let batcher_remote_server = create_remote_server!(
///     &config.components.batcher.execution_mode,
///     || {clients.get_gateway_local_client()},
///     config.socket
/// );
/// match batcher_remote_server {
///     Some(server) => println!("Remote server created: {:?}", server),
///     None => println!("Remote server not created because the execution mode is not remote."),
/// }
/// ```
#[macro_export]
macro_rules! create_remote_server {
    ($execution_mode:expr, $local_client_getter:expr, $socket:expr) => {
        match *$execution_mode {
            ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
                let local_client = $local_client_getter()
                    .expect("Local client should be set for inbound remote connections.");

                Some(Box::new(RemoteComponentServer::new(local_client, $socket.clone())))
            }
            ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
            | ReactiveComponentExecutionMode::Remote
            | ReactiveComponentExecutionMode::Disabled => None,
        }
    };
}

/// A macro for creating a component server, determined by the component's execution mode. Returns a
/// local server if the component is run locally, otherwise None.
///
/// # Arguments
///
/// * $execution_mode - A reference to the component's execution mode, i.e., type
///   &ReactiveComponentExecutionMode.
/// * $component - The component that will be taken to initialize the server if the execution mode
///   is enabled(LocalExecutionWithRemoteDisabled / LocalExecutionWithRemoteEnabled).
/// * $Receiver - receiver side for the server.
///
/// # Returns
///
/// An Option<Box<LocalComponentServer<ComponentType, RequestType, ResponseType>>> containing the
/// server if the execution mode is enabled(LocalExecutionWithRemoteDisabled /
/// LocalExecutionWithRemoteEnabled), or None if the execution mode is Disabled.
///
/// # Example
///
/// ```rust,ignore
/// let batcher_server = create_local_server!(
///     &config.components.batcher.execution_mode,
///     components.batcher,
///     communication.take_batcher_rx()
/// );
/// match batcher_server {
///     Some(server) => println!("Server created: {:?}", server),
///     None => println!("Server not created because the execution mode is disabled."),
/// }
/// ```
macro_rules! create_local_server {
    ($execution_mode:expr, $component:expr, $receiver:expr) => {
        match *$execution_mode {
            ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
            | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
                Some(Box::new(LocalComponentServer::new(
                    $component
                        .take()
                        .expect(concat!(stringify!($component), " is not initialized.")),
                    $receiver,
                )))
            }
            ReactiveComponentExecutionMode::Disabled | ReactiveComponentExecutionMode::Remote => {
                None
            }
        }
    };
}

/// A macro for creating a WrapperServer, determined by the component's execution mode. Returns a
/// wrapper server if the component is run locally, otherwise None.
///
/// # Arguments
///
/// * $execution_mode - A reference to the component's execution mode, i.e., type
///   &ReactiveComponentExecutionMode.
/// * $component - The component that will be taken to initialize the server if the execution mode
///   is enabled(LocalExecutionWithRemoteDisabled / LocalExecutionWithRemoteEnabled).
///
/// # Returns
///
/// An `Option<Box<WrapperServer<ComponentType>>>` containing the server if the execution mode is
/// enabled(LocalExecutionWithRemoteDisabled / LocalExecutionWithRemoteEnabled), or `None` if the
/// execution mode is `Disabled`.
///
/// # Example
///
/// ```rust, ignore
/// // Assuming ReactiveComponentExecutionMode and components are defined, and WrapperServer
/// // has a new method that accepts a component.
/// let consensus_manager_server = create_wrapper_server!(
///     &config.components.consensus_manager.execution_mode,
///     components.consensus_manager
/// );
///
/// match consensus_manager_server {
///     Some(server) => println!("Server created: {:?}", server),
///     None => println!("Server not created because the execution mode is disabled."),
/// }
/// ```
macro_rules! create_wrapper_server {
    ($execution_mode:expr, $component:expr) => {
        match *$execution_mode {
            ActiveComponentExecutionMode::Enabled => Some(Box::new(WrapperServer::new(
                $component.take().expect(concat!(stringify!($component), " is not initialized.")),
            ))),
            ActiveComponentExecutionMode::Disabled => None,
        }
    };
}

fn create_local_servers(
    config: &SequencerNodeConfig,
    communication: &mut SequencerNodeCommunication,
    components: &mut SequencerNodeComponents,
) -> LocalServers {
    let batcher_server = create_local_server!(
        &config.components.batcher.execution_mode,
        components.batcher,
        communication.take_batcher_rx()
    );
    let gateway_server = create_local_server!(
        &config.components.gateway.execution_mode,
        components.gateway,
        communication.take_gateway_rx()
    );
    let l1_provider_server = create_local_server!(
        &config.components.l1_provider.execution_mode,
        components.l1_provider,
        communication.take_l1_provider_rx()
    );
    let mempool_server = create_local_server!(
        &config.components.mempool.execution_mode,
        components.mempool,
        communication.take_mempool_rx()
    );
    let mempool_p2p_propagator_server = create_local_server!(
        &config.components.mempool_p2p.execution_mode,
        components.mempool_p2p_propagator,
        communication.take_mempool_p2p_propagator_rx()
    );
    let state_sync_server = create_local_server!(
        &config.components.state_sync.execution_mode,
        components.state_sync,
        communication.take_state_sync_rx()
    );

    LocalServers {
        batcher: batcher_server,
        gateway: gateway_server,
        l1_provider: l1_provider_server,
        mempool: mempool_server,
        mempool_p2p_propagator: mempool_p2p_propagator_server,
        state_sync: state_sync_server,
    }
}

async fn create_servers<ReturnType: Send + 'static>(
    labeled_futures: Vec<(impl Future<Output = ReturnType> + Send + 'static, String)>,
) -> JoinSet<(ReturnType, String)> {
    let mut tasks = JoinSet::new();

    for (future, label) in labeled_futures {
        tasks.spawn(async move {
            let res = future.await;
            (res, label)
        });
    }

    tasks
}

impl LocalServers {
    async fn run(self) -> JoinSet<(Result<(), ComponentServerError>, String)> {
        create_servers(vec![
            server_future_and_label(self.batcher, "Local Batcher"),
            server_future_and_label(self.gateway, "Local Gateway"),
            server_future_and_label(self.l1_provider, "Local L1 Provider"),
            server_future_and_label(self.mempool, "Local Mempool"),
            server_future_and_label(self.mempool_p2p_propagator, "Local Mempool P2p Propagator"),
            server_future_and_label(self.state_sync, "Local State Sync"),
        ])
        .await
    }
}

pub fn create_remote_servers(
    config: &SequencerNodeConfig,
    clients: &SequencerNodeClients,
) -> RemoteServers {
    let batcher_server = create_remote_server!(
        &config.components.batcher.execution_mode,
        || { clients.get_batcher_local_client() },
        config.components.batcher.socket
    );

    let gateway_server = create_remote_server!(
        &config.components.gateway.execution_mode,
        || { clients.get_gateway_local_client() },
        config.components.gateway.socket
    );

    let l1_provider_server = create_remote_server!(
        &config.components.l1_provider.execution_mode,
        || { clients.get_l1_provider_local_client() },
        config.components.l1_provider.socket
    );

    let mempool_server = create_remote_server!(
        &config.components.mempool.execution_mode,
        || { clients.get_mempool_local_client() },
        config.components.mempool.socket
    );

    let mempool_p2p_propagator_server = create_remote_server!(
        &config.components.mempool_p2p.execution_mode,
        || { clients.get_mempool_p2p_propagator_local_client() },
        config.components.mempool_p2p.socket
    );

    let state_sync_server = create_remote_server!(
        &config.components.state_sync.execution_mode,
        || { clients.get_state_sync_local_client() },
        config.components.state_sync.socket
    );

    RemoteServers {
        batcher: batcher_server,
        gateway: gateway_server,
        l1_provider: l1_provider_server,
        mempool: mempool_server,
        mempool_p2p_propagator: mempool_p2p_propagator_server,
        state_sync: state_sync_server,
    }
}

impl RemoteServers {
    async fn run(self) -> JoinSet<(Result<(), ComponentServerError>, String)> {
        create_servers(vec![
            server_future_and_label(self.batcher, "Remote Batcher"),
            server_future_and_label(self.gateway, "Remote Gateway"),
            server_future_and_label(self.l1_provider, "Remote L1 Provider"),
            server_future_and_label(self.mempool, "Remote Mempool"),
            server_future_and_label(self.mempool_p2p_propagator, "Remote Mempool P2p Propagator"),
            server_future_and_label(self.state_sync, "Remote State Sync"),
        ])
        .await
    }
}

fn create_wrapper_servers(
    config: &SequencerNodeConfig,
    components: &mut SequencerNodeComponents,
) -> WrapperServers {
    let consensus_manager_server = create_wrapper_server!(
        &config.components.consensus_manager.execution_mode,
        components.consensus_manager
    );

    let http_server = create_wrapper_server!(
        &config.components.http_server.execution_mode,
        components.http_server
    );

    let l1_scraper_server =
        create_wrapper_server!(&config.components.l1_scraper.execution_mode, components.l1_scraper);

    let monitoring_endpoint_server = create_wrapper_server!(
        &config.components.monitoring_endpoint.execution_mode,
        components.monitoring_endpoint
    );

    let mempool_p2p_runner_server = create_wrapper_server!(
        &config.components.mempool_p2p.execution_mode.clone().into(),
        components.mempool_p2p_runner
    );
    let state_sync_runner_server = create_wrapper_server!(
        &config.components.state_sync.execution_mode.clone().into(),
        components.state_sync_runner
    );

    WrapperServers {
        consensus_manager: consensus_manager_server,
        http_server,
        l1_scraper_server,
        monitoring_endpoint: monitoring_endpoint_server,
        mempool_p2p_runner: mempool_p2p_runner_server,
        state_sync_runner: state_sync_runner_server,
    }
}

impl WrapperServers {
    async fn run(self) -> JoinSet<(Result<(), ComponentServerError>, String)> {
        create_servers(vec![
            server_future_and_label(self.consensus_manager, "Consensus Manager"),
            server_future_and_label(self.http_server, "Http"),
            server_future_and_label(self.l1_scraper_server, "L1 Scraper"),
            server_future_and_label(self.monitoring_endpoint, "Monitoring Endpoint"),
            server_future_and_label(self.mempool_p2p_runner, "Mempool P2p Runner"),
            server_future_and_label(self.state_sync_runner, "State Sync Runner"),
        ])
        .await
    }
}

pub fn create_node_servers(
    config: &SequencerNodeConfig,
    communication: &mut SequencerNodeCommunication,
    components: SequencerNodeComponents,
    clients: &SequencerNodeClients,
) -> SequencerNodeServers {
    let mut components = components;
    let local_servers = create_local_servers(config, communication, &mut components);
    let remote_servers = create_remote_servers(config, clients);
    let wrapper_servers = create_wrapper_servers(config, &mut components);

    SequencerNodeServers { local_servers, remote_servers, wrapper_servers }
}

type JoinSetResult<T> = Option<Result<T, JoinError>>;

fn get_server_error(
    result: JoinSetResult<(Result<(), ComponentServerError>, String)>,
) -> anyhow::Result<()> {
    if let Some(result) = result {
        match result {
            Ok((res, label)) => {
                error!("{} Server stoped", label);
                Ok(res?)
            }
            Err(e) => {
                error!("Error while waiting for the first task: {:?}", e);
                Err(e.into())
            }
        }
    } else {
        Ok(())
    }
}

pub async fn run_component_servers(servers: SequencerNodeServers) -> anyhow::Result<()> {
    let mut local_servers = servers.local_servers.run().await;
    let mut remote_servers = servers.remote_servers.run().await;
    let mut wrapper_servers = servers.wrapper_servers.run().await;

    // TODO (Lev/Itay): Consider using JoinSet instead of tokio::select!.
    let (result, servers_type) = tokio::select! {
        res = local_servers.join_next() => {
            (res, "Local")
        }
        res = remote_servers.join_next() => {
            (res, "Remote")
        }
        res = wrapper_servers.join_next() => {
            (res, "Wrapper")
        }
    };

    let result = get_server_error(result);
    error!("{} Servers ended unexpectedly.", servers_type);

    local_servers.abort_all();
    remote_servers.abort_all();
    wrapper_servers.abort_all();

    result
}

type ComponentServerFuture = Pin<Box<dyn Future<Output = Result<(), ComponentServerError>> + Send>>;

fn get_server_future(
    server: Option<Box<impl ComponentServerStarter + Send + 'static>>,
) -> ComponentServerFuture {
    match server {
        Some(mut server) => async move { server.start().await }.boxed(),
        None => pending().boxed(),
    }
}

pub fn server_future_and_label(
    server: Option<Box<impl ComponentServerStarter + Send + 'static>>,
    label: &str,
) -> (ComponentServerFuture, String) {
    (get_server_future(server), label.to_string())
}
