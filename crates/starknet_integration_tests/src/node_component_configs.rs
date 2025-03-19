use std::net::{Ipv4Addr, SocketAddr};

use starknet_infra_utils::test_utils::AvailablePortsGenerator;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::config::component_execution_config::{
    ActiveComponentExecutionConfig,
    ReactiveComponentExecutionConfig,
};
use starknet_sequencer_node::deployment::{
    get_batcher_config,
    get_class_manager_config,
    get_consensus_manager_config,
    get_consolidated_config,
    get_gateway_config,
    get_http_server_config,
    get_l1_provider_config,
    get_mempool_config,
    get_sierra_compiler_config,
    get_state_sync_config,
    DistributedNodeServiceConfigPair,
};

/// Holds the component configs for a set of sequencers, composing a single sequencer node.
pub struct NodeComponentConfigs {
    component_configs: Vec<ComponentConfig>,
    batcher_index: usize,
    http_server_index: usize,
    state_sync_index: usize,
}

impl NodeComponentConfigs {
    fn new(
        component_configs: Vec<ComponentConfig>,
        batcher_index: usize,
        http_server_index: usize,
        state_sync_index: usize,
    ) -> Self {
        Self { component_configs, batcher_index, http_server_index, state_sync_index }
    }

    pub fn len(&self) -> usize {
        self.component_configs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.component_configs.is_empty()
    }

    pub fn get_batcher_index(&self) -> usize {
        self.batcher_index
    }

    pub fn get_http_server_index(&self) -> usize {
        self.http_server_index
    }

    pub fn get_state_sync_index(&self) -> usize {
        self.state_sync_index
    }
}

impl IntoIterator for NodeComponentConfigs {
    type Item = ComponentConfig;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.component_configs.into_iter()
    }
}

/// Generates configurations for a specified number of distributed sequencer nodes,
/// each consisting of an HTTP component configuration and a non-HTTP component configuration.
/// returns a vector of vectors, where each inner vector contains the two configurations.
pub fn create_distributed_node_configs(
    available_ports_generator: &mut AvailablePortsGenerator,
    distributed_sequencers_num: usize,
) -> Vec<NodeComponentConfigs> {
    std::iter::repeat_with(|| {
        let mut available_ports = available_ports_generator
            .next()
            .expect("Failed to get an AvailablePorts instance for distributed node configs");
        let gateway_socket = available_ports.get_next_local_host_socket();
        let mempool_socket = available_ports.get_next_local_host_socket();
        let mempool_p2p_socket = available_ports.get_next_local_host_socket();
        let state_sync_socket = available_ports.get_next_local_host_socket();
        let class_manager_socket = available_ports.get_next_local_host_socket();

        NodeComponentConfigs::new(
            vec![
                get_http_container_config(
                    gateway_socket,
                    mempool_socket,
                    mempool_p2p_socket,
                    state_sync_socket,
                    class_manager_socket,
                ),
                get_non_http_container_config(
                    gateway_socket,
                    mempool_socket,
                    mempool_p2p_socket,
                    state_sync_socket,
                    class_manager_socket,
                ),
            ],
            // TODO(noamsp): remove these hardcoded values and get the indexes from a mapping.
            // batcher is in executable index 1.
            1,
            // http server is in executable index 0.
            0,
            // state sync is in executable index 1.
            1,
        )
    })
    .take(distributed_sequencers_num)
    .collect()
}

pub fn create_consolidated_sequencer_configs(
    num_of_consolidated_nodes: usize,
) -> Vec<NodeComponentConfigs> {
    // Both batcher, http server and state sync are in executable index 0.
    std::iter::repeat_with(|| NodeComponentConfigs::new(vec![get_consolidated_config()], 0, 0, 0))
        .take(num_of_consolidated_nodes)
        .collect()
}

// TODO(Nadin/Tsabary): create this as a deployment fn.
// TODO(Nadin/Tsabary): find a better name for this function.
fn get_http_container_config(
    gateway_socket: SocketAddr,
    mempool_socket: SocketAddr,
    mempool_p2p_socket: SocketAddr,
    state_sync_socket: SocketAddr,
    class_manager_socket: SocketAddr,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.http_server = ActiveComponentExecutionConfig::default();
    config.gateway = ReactiveComponentExecutionConfig::local_with_remote_enabled(
        Ipv4Addr::LOCALHOST.to_string(),
        gateway_socket.ip(),
        gateway_socket.port(),
    );
    config.mempool = ReactiveComponentExecutionConfig::local_with_remote_enabled(
        Ipv4Addr::LOCALHOST.to_string(),
        mempool_socket.ip(),
        mempool_socket.port(),
    );
    config.mempool_p2p = ReactiveComponentExecutionConfig::local_with_remote_enabled(
        Ipv4Addr::LOCALHOST.to_string(),
        mempool_p2p_socket.ip(),
        mempool_p2p_socket.port(),
    );
    config.state_sync = ReactiveComponentExecutionConfig::remote(
        Ipv4Addr::LOCALHOST.to_string(),
        state_sync_socket.ip(),
        state_sync_socket.port(),
    );
    config.class_manager = ReactiveComponentExecutionConfig::local_with_remote_enabled(
        Ipv4Addr::LOCALHOST.to_string(),
        class_manager_socket.ip(),
        class_manager_socket.port(),
    );
    config.sierra_compiler = ReactiveComponentExecutionConfig::local_with_remote_disabled();
    config.monitoring_endpoint = ActiveComponentExecutionConfig::default();
    config
}

fn get_non_http_container_config(
    gateway_socket: SocketAddr,
    mempool_socket: SocketAddr,
    mempool_p2p_socket: SocketAddr,
    state_sync_socket: SocketAddr,
    class_manager_socket: SocketAddr,
) -> ComponentConfig {
    ComponentConfig {
        http_server: ActiveComponentExecutionConfig::disabled(),
        monitoring_endpoint: Default::default(),
        gateway: ReactiveComponentExecutionConfig::remote(
            Ipv4Addr::LOCALHOST.to_string(),
            gateway_socket.ip(),
            gateway_socket.port(),
        ),
        mempool: ReactiveComponentExecutionConfig::remote(
            Ipv4Addr::LOCALHOST.to_string(),
            mempool_socket.ip(),
            mempool_socket.port(),
        ),
        mempool_p2p: ReactiveComponentExecutionConfig::remote(
            Ipv4Addr::LOCALHOST.to_string(),
            mempool_p2p_socket.ip(),
            mempool_p2p_socket.port(),
        ),
        state_sync: ReactiveComponentExecutionConfig::local_with_remote_enabled(
            Ipv4Addr::LOCALHOST.to_string(),
            state_sync_socket.ip(),
            state_sync_socket.port(),
        ),
        class_manager: ReactiveComponentExecutionConfig::remote(
            Ipv4Addr::LOCALHOST.to_string(),
            class_manager_socket.ip(),
            class_manager_socket.port(),
        ),
        ..ComponentConfig::default()
    }
}

// TODO(alonl): use enums to represent the different types of units distributions.
pub fn create_nodes_deployment_units_configs(
    available_ports_generator: &mut AvailablePortsGenerator,
    distributed_sequencers_num: usize,
) -> Vec<NodeComponentConfigs> {
    std::iter::repeat_with(|| {
        let mut available_ports = available_ports_generator
            .next()
            .expect("Failed to get an AvailablePorts instance for distributed node configs");
        let batcher_socket = available_ports.get_next_local_host_socket();
        let class_manager_socket = available_ports.get_next_local_host_socket();
        let gateway_socket = available_ports.get_next_local_host_socket();
        let mempool_socket = available_ports.get_next_local_host_socket();
        let sierra_compiler_socket = available_ports.get_next_local_host_socket();
        let state_sync_socket = available_ports.get_next_local_host_socket();
        let l1_provider_socket = available_ports.get_next_local_host_socket();

        let batcher_remote_config_pair = DistributedNodeServiceConfigPair::new(
            Ipv4Addr::LOCALHOST.to_string(),
            batcher_socket.ip(),
            batcher_socket.port(),
        );
        let class_manager_remote_config_pair = DistributedNodeServiceConfigPair::new(
            Ipv4Addr::LOCALHOST.to_string(),
            class_manager_socket.ip(),
            class_manager_socket.port(),
        );
        let gateway_remote_config_pair = DistributedNodeServiceConfigPair::new(
            Ipv4Addr::LOCALHOST.to_string(),
            gateway_socket.ip(),
            gateway_socket.port(),
        );
        let mempool_remote_config_pair = DistributedNodeServiceConfigPair::new(
            Ipv4Addr::LOCALHOST.to_string(),
            mempool_socket.ip(),
            mempool_socket.port(),
        );
        let sierra_compiler_remote_config_pair = DistributedNodeServiceConfigPair::new(
            Ipv4Addr::LOCALHOST.to_string(),
            sierra_compiler_socket.ip(),
            sierra_compiler_socket.port(),
        );
        let state_sync_remote_config_pair = DistributedNodeServiceConfigPair::new(
            Ipv4Addr::LOCALHOST.to_string(),
            state_sync_socket.ip(),
            state_sync_socket.port(),
        );
        let l1_provider_remote_config_pair = DistributedNodeServiceConfigPair::new(
            Ipv4Addr::LOCALHOST.to_string(),
            l1_provider_socket.ip(),
            l1_provider_socket.port(),
        );

        NodeComponentConfigs::new(
            vec![
                get_batcher_config(
                    batcher_remote_config_pair.local(),
                    class_manager_remote_config_pair.remote(),
                    l1_provider_remote_config_pair.remote(),
                    mempool_remote_config_pair.remote(),
                ),
                get_class_manager_config(
                    class_manager_remote_config_pair.local(),
                    sierra_compiler_remote_config_pair.remote(),
                ),
                get_gateway_config(
                    gateway_remote_config_pair.local(),
                    class_manager_remote_config_pair.remote(),
                    mempool_remote_config_pair.remote(),
                    state_sync_remote_config_pair.remote(),
                ),
                get_mempool_config(
                    mempool_remote_config_pair.local(),
                    class_manager_remote_config_pair.remote(),
                    gateway_remote_config_pair.remote(),
                ),
                get_sierra_compiler_config(sierra_compiler_remote_config_pair.local()),
                get_state_sync_config(
                    state_sync_remote_config_pair.local(),
                    class_manager_remote_config_pair.remote(),
                ),
                get_http_server_config(gateway_remote_config_pair.remote()),
                get_consensus_manager_config(
                    batcher_remote_config_pair.remote(),
                    class_manager_remote_config_pair.remote(),
                    state_sync_remote_config_pair.remote(),
                ),
                get_l1_provider_config(
                    l1_provider_remote_config_pair.local(),
                    state_sync_remote_config_pair.remote(),
                ),
            ],
            // TODO(noamsp): remove these hardcoded values and get the indexes from a mapping.
            // batcher is in executable index 0.
            0,
            // http server is in executable index 6.
            6,
            // state sync is in executable index 5.
            5,
        )
    })
    .take(distributed_sequencers_num)
    .collect()
}
