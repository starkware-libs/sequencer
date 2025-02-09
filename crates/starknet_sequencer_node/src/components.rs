use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerContract;
use papyrus_base_layer::BaseLayerContract;
use starknet_batcher::batcher::{create_batcher, Batcher};
use starknet_class_manager::class_manager::create_class_manager;
use starknet_class_manager::ClassManager;
use starknet_consensus_manager::consensus_manager::ConsensusManager;
use starknet_gateway::gateway::{create_gateway, Gateway};
use starknet_http_server::http_server::{create_http_server, HttpServer};
use starknet_l1_gas_price::l1_gas_price_provider::L1GasPriceProvider;
use starknet_l1_gas_price::l1_gas_price_scraper::L1GasPriceScraper;
use starknet_l1_provider::event_identifiers_to_track;
use starknet_l1_provider::l1_provider::{create_l1_provider, L1Provider};
use starknet_l1_provider::l1_scraper::L1Scraper;
use starknet_mempool::communication::{create_mempool, MempoolCommunicationWrapper};
use starknet_mempool_p2p::create_p2p_propagator_and_runner;
use starknet_mempool_p2p::propagator::MempoolP2pPropagator;
use starknet_mempool_p2p::runner::MempoolP2pRunner;
use starknet_monitoring_endpoint::monitoring_endpoint::{
    create_monitoring_endpoint,
    MonitoringEndpoint,
};
use starknet_sierra_multicompile::{create_sierra_compiler, SierraCompiler};
use starknet_state_sync::runner::StateSyncRunner;
use starknet_state_sync::{create_state_sync_and_runner, StateSync};
use tracing::warn;

use crate::clients::SequencerNodeClients;
use crate::config::component_execution_config::{
    ActiveComponentExecutionMode,
    ReactiveComponentExecutionMode,
};
use crate::config::node_config::SequencerNodeConfig;
use crate::version::VERSION_FULL;

pub struct SequencerNodeComponents {
    pub batcher: Option<Batcher>,
    pub class_manager: Option<ClassManager>,
    pub consensus_manager: Option<ConsensusManager>,
    pub gateway: Option<Gateway>,
    pub http_server: Option<HttpServer>,
    pub l1_scraper: Option<L1Scraper<EthereumBaseLayerContract>>,
    pub l1_provider: Option<L1Provider>,
    pub l1_gas_price_scraper: Option<L1GasPriceScraper<EthereumBaseLayerContract>>,
    pub l1_gas_price_provider: Option<L1GasPriceProvider>,
    pub mempool: Option<MempoolCommunicationWrapper>,
    pub monitoring_endpoint: Option<MonitoringEndpoint>,
    pub mempool_p2p_propagator: Option<MempoolP2pPropagator>,
    pub mempool_p2p_runner: Option<MempoolP2pRunner>,
    pub sierra_compiler: Option<SierraCompiler>,
    pub state_sync: Option<StateSync>,
    pub state_sync_runner: Option<StateSyncRunner>,
}

pub async fn create_node_components(
    config: &SequencerNodeConfig,
    clients: &SequencerNodeClients,
) -> SequencerNodeComponents {
    let batcher = match config.components.batcher.execution_mode {
        ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            let mempool_client =
                clients.get_mempool_shared_client().expect("Mempool Client should be available");
            let l1_provider_client = clients
                .get_l1_provider_shared_client()
                .expect("L1 Provider Client should be available");
            // TODO(guyn): Should also create a gas price shared client and give it to batcher?
            let class_manager_client = clients
                .get_class_manager_shared_client()
                .expect("Class Manager Client should be available");
            Some(create_batcher(
                config.batcher_config.clone(),
                mempool_client,
                l1_provider_client,
                class_manager_client,
            ))
        }
        ReactiveComponentExecutionMode::Disabled | ReactiveComponentExecutionMode::Remote => None,
    };

    let class_manager = match config.components.class_manager.execution_mode {
        ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            let compiler_shared_client = clients
                .get_sierra_compiler_shared_client()
                .expect("Sierra Compiler Client should be available");
            Some(create_class_manager(config.class_manager_config.clone(), compiler_shared_client))
        }
        ReactiveComponentExecutionMode::Disabled | ReactiveComponentExecutionMode::Remote => None,
    };

    let consensus_manager = match config.components.consensus_manager.execution_mode {
        ActiveComponentExecutionMode::Enabled => {
            let batcher_client =
                clients.get_batcher_shared_client().expect("Batcher Client should be available");
            let state_sync_client = clients
                .get_state_sync_shared_client()
                .expect("State Sync Client should be available");
            let class_manager_client = clients
                .get_class_manager_shared_client()
                .expect("Class Manager Client should be available");
            Some(ConsensusManager::new(
                config.consensus_manager_config.clone(),
                batcher_client,
                state_sync_client,
                class_manager_client,
            ))
        }
        ActiveComponentExecutionMode::Disabled => None,
    };
    let gateway = match config.components.gateway.execution_mode {
        ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            let mempool_client =
                clients.get_mempool_shared_client().expect("Mempool Client should be available");
            let state_sync_client = clients
                .get_state_sync_shared_client()
                .expect("State Sync Client should be available");
            let class_manager_client = clients
                .get_class_manager_shared_client()
                .expect("Class Manager Client should be available");
            Some(create_gateway(
                config.gateway_config.clone(),
                state_sync_client,
                mempool_client,
                class_manager_client,
            ))
        }
        ReactiveComponentExecutionMode::Disabled | ReactiveComponentExecutionMode::Remote => None,
    };
    let http_server = match config.components.http_server.execution_mode {
        ActiveComponentExecutionMode::Enabled => {
            let gateway_client =
                clients.get_gateway_shared_client().expect("Gateway Client should be available");

            Some(create_http_server(config.http_server_config.clone(), gateway_client))
        }
        ActiveComponentExecutionMode::Disabled => None,
    };

    let (mempool_p2p_propagator, mempool_p2p_runner) =
        match config.components.mempool_p2p.execution_mode {
            ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
            | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
                let gateway_client = clients
                    .get_gateway_shared_client()
                    .expect("Gateway Client should be available");
                let class_manager_client = clients
                    .get_class_manager_shared_client()
                    .expect("Class Manager Client should be available");
                let mempool_p2p_propagator_client = clients
                    .get_mempool_p2p_propagator_shared_client()
                    .expect("Mempool P2p Propagator Client should be available");
                let (mempool_p2p_propagator, mempool_p2p_runner) = create_p2p_propagator_and_runner(
                    config.mempool_p2p_config.clone(),
                    gateway_client,
                    class_manager_client,
                    mempool_p2p_propagator_client,
                );
                (Some(mempool_p2p_propagator), Some(mempool_p2p_runner))
            }
            ReactiveComponentExecutionMode::Disabled | ReactiveComponentExecutionMode::Remote => {
                (None, None)
            }
        };

    let mempool = match config.components.mempool.execution_mode {
        ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            let mempool_p2p_propagator_client = clients
                .get_mempool_p2p_propagator_shared_client()
                .expect("Propagator Client should be available");
            let mempool =
                create_mempool(config.mempool_config.clone(), mempool_p2p_propagator_client);
            Some(mempool)
        }
        ReactiveComponentExecutionMode::Disabled | ReactiveComponentExecutionMode::Remote => None,
    };

    let monitoring_endpoint = match config.components.monitoring_endpoint.execution_mode {
        ActiveComponentExecutionMode::Enabled => Some(create_monitoring_endpoint(
            config.monitoring_endpoint_config.clone(),
            VERSION_FULL,
        )),
        ActiveComponentExecutionMode::Disabled => None,
    };

    let (state_sync, state_sync_runner) = match config.components.state_sync.execution_mode {
        ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            let class_manager_client = clients
                .get_class_manager_shared_client()
                .expect("Class Manager Client should be available");
            let (state_sync, state_sync_runner) = create_state_sync_and_runner(
                config.state_sync_config.clone(),
                class_manager_client,
            );
            (Some(state_sync), Some(state_sync_runner))
        }
        ReactiveComponentExecutionMode::Disabled | ReactiveComponentExecutionMode::Remote => {
            (None, None)
        }
    };

    let l1_scraper = match config.components.l1_scraper.execution_mode {
        ActiveComponentExecutionMode::Enabled => {
            let l1_provider_client = clients.get_l1_provider_shared_client().unwrap();
            let l1_scraper_config = config.l1_scraper_config.clone();
            let base_layer = EthereumBaseLayerContract::new(config.base_layer_config.clone());
            Some(
                L1Scraper::new(
                    l1_scraper_config,
                    l1_provider_client,
                    base_layer,
                    event_identifiers_to_track(),
                )
                .await
                .unwrap(),
            )
        }
        ActiveComponentExecutionMode::Disabled => None,
    };

    // Must be initialized after the l1 scraper, since the provider's (L2) startup height is derived
    // from the scraper's (L1) startup height (unless the former is overridden via the config).
    let l1_provider = match config.components.l1_provider.execution_mode {
        ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            let provider_startup_height = match &l1_scraper {
                Some(l1_scraper) => {
                    let (base_layer, l1_scraper_start_l1_height) =
                        (l1_scraper.base_layer.clone(), l1_scraper.last_l1_block_processed.number);
                    base_layer
                        .get_proved_block_at(l1_scraper_start_l1_height)
                        .await
                        .map(|block| block.number)
                        // This will likely only fail on tests, or on nodes that want to reexecute from
                        // genesis. The former should override the height, or setup Anvil accordingly, and
                        // the latter should use the correct L1 height.
                        .inspect_err(|err|{
                            warn!("Error while attempting to get the L2 block at the L1 height the
                            scraper was initialized on. This is either due to running a test with
                            faulty Anvil state, or if the scraper was initialized too far back.
                            Attempting to use provider startup height override (read its docstring
                            before using!).\n {err}")})
                        .ok().or(config.l1_provider_config.provider_startup_height_override)
                }
                None => {
                    warn!(
                        "L1 Scraper is disabled, attempting to use provider startup height \
                         override in order to find L1 Provider startup height (read its docstring \
                         before using!)"
                    );
                    config.l1_provider_config.provider_startup_height_override
                }
            };
            let provider_startup_height = provider_startup_height.expect(
                "Cannot determine L1 Provider startup height. Either enable the L1 Scraper on \
                 height that is at least as old as L2 genesis, or override provider startup \
                 height (read its docstring before using)",
            );

            Some(create_l1_provider(
                config.l1_provider_config,
                clients.get_l1_provider_shared_client().unwrap(),
                clients.get_state_sync_shared_client().unwrap(),
                provider_startup_height,
            ))
        }
        ReactiveComponentExecutionMode::Disabled | ReactiveComponentExecutionMode::Remote => None,
    };

    let l1_gas_price_provider = match config.components.l1_gas_price_provider.execution_mode {
        ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            Some(L1GasPriceProvider::new(config.l1_gas_price_provider_config.clone()))
        }
        ReactiveComponentExecutionMode::Disabled | ReactiveComponentExecutionMode::Remote => None,
    };
    let l1_gas_price_scraper = match config.components.l1_gas_price_scraper.execution_mode {
        ActiveComponentExecutionMode::Enabled => {
            let l1_gas_price_client = clients.get_l1_gas_price_shared_client().unwrap();
            let l1_gas_price_scraper_config = config.l1_gas_price_scraper_config.clone();
            let base_layer = EthereumBaseLayerContract::new(config.base_layer_config.clone());

            Some(L1GasPriceScraper::new(
                l1_gas_price_scraper_config,
                l1_gas_price_client,
                base_layer,
            ))
        }
        ActiveComponentExecutionMode::Disabled => None,
    };

    let sierra_compiler = match config.components.sierra_compiler.execution_mode {
        ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            Some(create_sierra_compiler(config.compiler_config.clone()))
        }
        ReactiveComponentExecutionMode::Disabled | ReactiveComponentExecutionMode::Remote => None,
    };

    SequencerNodeComponents {
        batcher,
        class_manager,
        consensus_manager,
        gateway,
        http_server,
        l1_scraper,
        l1_provider,
        l1_gas_price_scraper,
        l1_gas_price_provider,
        mempool,
        monitoring_endpoint,
        mempool_p2p_propagator,
        mempool_p2p_runner,
        sierra_compiler,
        state_sync,
        state_sync_runner,
    }
}
