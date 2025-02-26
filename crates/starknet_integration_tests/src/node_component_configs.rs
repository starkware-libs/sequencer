use std::net::SocketAddr;

use starknet_infra_utils::test_utils::AvailablePorts;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::config::component_execution_config::{
    ActiveComponentExecutionConfig,
    ReactiveComponentExecutionConfig,
};

/// Holds the component configs for a set of sequencers, composing a single sequencer node.
pub struct NodeComponentConfigs {
    component_configs: Vec<ComponentConfig>,
    batcher_index: usize,
    http_server_index: usize,
}

impl NodeComponentConfigs {
    fn new(
        component_configs: Vec<ComponentConfig>,
        batcher_index: usize,
        http_server_index: usize,
    ) -> Self {
        Self { component_configs, batcher_index, http_server_index }
    }

    // pub fn into_iter(self) -> impl Iterator<Item = ComponentConfig> {
    //     self.component_configs.into_iter()
    // }

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
    available_ports: &mut AvailablePorts,
    distributed_sequencers_num: usize,
) -> Vec<NodeComponentConfigs> {
    std::iter::repeat_with(|| {
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
            // batcher is in executable index 1.
            1,
            // http server is in executable index 0.
            0,
        )
    })
    .take(distributed_sequencers_num)
    .collect()
}

pub fn create_consolidated_sequencer_configs(
    num_of_consolidated_nodes: usize,
) -> Vec<NodeComponentConfigs> {
    // Both batcher and http server are in executable index 0.
    std::iter::repeat_with(|| {
        NodeComponentConfigs::new(
            vec![ComponentConfig {
                // The L1 scraper is disabled in to avoid running an instance of L1 in the
                // 'docker-build-push' test.
                // TODO(Arni): reenable the l1 scraper.
                l1_scraper: ActiveComponentExecutionConfig::disabled(),
                ..ComponentConfig::default()
            }],
            0,
            0,
        )
    })
    .take(num_of_consolidated_nodes)
    .collect()
}

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
    let local_url = "127.0.0.1".to_string();
    config.gateway = ReactiveComponentExecutionConfig::local_with_remote_enabled(
        local_url.clone(),
        gateway_socket.ip(),
        gateway_socket.port(),
    );
    config.mempool = ReactiveComponentExecutionConfig::local_with_remote_enabled(
        local_url.clone(),
        mempool_socket.ip(),
        mempool_socket.port(),
    );
    config.mempool_p2p = ReactiveComponentExecutionConfig::local_with_remote_enabled(
        local_url.clone(),
        mempool_p2p_socket.ip(),
        mempool_p2p_socket.port(),
    );
    config.state_sync = ReactiveComponentExecutionConfig::remote(
        local_url.clone(),
        state_sync_socket.ip(),
        state_sync_socket.port(),
    );
    config.class_manager = ReactiveComponentExecutionConfig::local_with_remote_enabled(
        local_url.clone(),
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
    let local_url = "127.0.0.1".to_string();
    ComponentConfig {
        http_server: ActiveComponentExecutionConfig::disabled(),
        monitoring_endpoint: Default::default(),
        gateway: ReactiveComponentExecutionConfig::remote(
            local_url.clone(),
            gateway_socket.ip(),
            gateway_socket.port(),
        ),
        mempool: ReactiveComponentExecutionConfig::remote(
            local_url.clone(),
            mempool_socket.ip(),
            mempool_socket.port(),
        ),
        mempool_p2p: ReactiveComponentExecutionConfig::remote(
            local_url.clone(),
            mempool_p2p_socket.ip(),
            mempool_p2p_socket.port(),
        ),
        state_sync: ReactiveComponentExecutionConfig::local_with_remote_enabled(
            local_url.clone(),
            state_sync_socket.ip(),
            state_sync_socket.port(),
        ),
        class_manager: ReactiveComponentExecutionConfig::remote(
            local_url.clone(),
            class_manager_socket.ip(),
            class_manager_socket.port(),
        ),
        ..ComponentConfig::default()
    }
}
