use apollo_batcher_types::communication::BatcherRequestAndResponseSender;
use apollo_class_manager_types::ClassManagerRequestAndResponseSender;
use apollo_compile_to_casm_types::SierraCompilerRequestAndResponseSender;
use apollo_gateway_types::communication::GatewayRequestAndResponseSender;
use apollo_infra::component_definitions::ComponentCommunication;
use apollo_l1_endpoint_monitor::communication::L1EndpointMonitorRequestAndResponseSender;
use apollo_l1_gas_price::communication::L1GasPriceRequestAndResponseSender;
use apollo_l1_provider::communication::L1ProviderRequestAndResponseSender;
use apollo_mempool_p2p_types::communication::MempoolP2pPropagatorRequestAndResponseSender;
use apollo_mempool_types::communication::MempoolRequestAndResponseSender;
use apollo_signature_manager_types::SignatureManagerRequestAndResponseSender;
use apollo_state_sync_types::communication::StateSyncRequestAndResponseSender;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tracing::info;

use crate::config::node_config::SequencerNodeConfig;

pub struct SequencerNodeCommunication {
    batcher_channel: ComponentCommunication<BatcherRequestAndResponseSender>,
    class_manager_channel: ComponentCommunication<ClassManagerRequestAndResponseSender>,
    gateway_channel: ComponentCommunication<GatewayRequestAndResponseSender>,
    l1_endpoint_monitor_channel: ComponentCommunication<L1EndpointMonitorRequestAndResponseSender>,
    l1_provider_channel: ComponentCommunication<L1ProviderRequestAndResponseSender>,
    l1_gas_price_channel: ComponentCommunication<L1GasPriceRequestAndResponseSender>,
    mempool_channel: ComponentCommunication<MempoolRequestAndResponseSender>,
    mempool_p2p_propagator_channel:
        ComponentCommunication<MempoolP2pPropagatorRequestAndResponseSender>,
    sierra_compiler_channel: ComponentCommunication<SierraCompilerRequestAndResponseSender>,
    signature_manager_channel: ComponentCommunication<SignatureManagerRequestAndResponseSender>,
    state_sync_channel: ComponentCommunication<StateSyncRequestAndResponseSender>,
}

impl SequencerNodeCommunication {
    pub fn take_batcher_tx(&mut self) -> Sender<BatcherRequestAndResponseSender> {
        self.batcher_channel.take_tx()
    }

    pub fn take_batcher_rx(&mut self) -> Receiver<BatcherRequestAndResponseSender> {
        self.batcher_channel.take_rx()
    }

    pub fn take_class_manager_tx(&mut self) -> Sender<ClassManagerRequestAndResponseSender> {
        self.class_manager_channel.take_tx()
    }

    pub fn take_class_manager_rx(&mut self) -> Receiver<ClassManagerRequestAndResponseSender> {
        self.class_manager_channel.take_rx()
    }

    pub fn take_gateway_tx(&mut self) -> Sender<GatewayRequestAndResponseSender> {
        self.gateway_channel.take_tx()
    }

    pub fn take_gateway_rx(&mut self) -> Receiver<GatewayRequestAndResponseSender> {
        self.gateway_channel.take_rx()
    }

    pub fn take_l1_endpoint_monitor_tx(
        &mut self,
    ) -> Sender<L1EndpointMonitorRequestAndResponseSender> {
        self.l1_endpoint_monitor_channel.take_tx()
    }

    pub fn take_l1_endpoint_monitor_rx(
        &mut self,
    ) -> Receiver<L1EndpointMonitorRequestAndResponseSender> {
        self.l1_endpoint_monitor_channel.take_rx()
    }

    pub fn take_l1_provider_tx(&mut self) -> Sender<L1ProviderRequestAndResponseSender> {
        self.l1_provider_channel.take_tx()
    }

    pub fn take_l1_provider_rx(&mut self) -> Receiver<L1ProviderRequestAndResponseSender> {
        self.l1_provider_channel.take_rx()
    }

    pub fn take_l1_gas_price_tx(&mut self) -> Sender<L1GasPriceRequestAndResponseSender> {
        self.l1_gas_price_channel.take_tx()
    }
    pub fn take_l1_gas_price_rx(&mut self) -> Receiver<L1GasPriceRequestAndResponseSender> {
        self.l1_gas_price_channel.take_rx()
    }

    pub fn take_mempool_p2p_propagator_tx(
        &mut self,
    ) -> Sender<MempoolP2pPropagatorRequestAndResponseSender> {
        self.mempool_p2p_propagator_channel.take_tx()
    }
    pub fn take_mempool_p2p_propagator_rx(
        &mut self,
    ) -> Receiver<MempoolP2pPropagatorRequestAndResponseSender> {
        self.mempool_p2p_propagator_channel.take_rx()
    }

    pub fn take_mempool_tx(&mut self) -> Sender<MempoolRequestAndResponseSender> {
        self.mempool_channel.take_tx()
    }

    pub fn take_mempool_rx(&mut self) -> Receiver<MempoolRequestAndResponseSender> {
        self.mempool_channel.take_rx()
    }

    pub fn take_sierra_compiler_tx(&mut self) -> Sender<SierraCompilerRequestAndResponseSender> {
        self.sierra_compiler_channel.take_tx()
    }

    pub fn take_sierra_compiler_rx(&mut self) -> Receiver<SierraCompilerRequestAndResponseSender> {
        self.sierra_compiler_channel.take_rx()
    }

    pub fn take_signature_manager_tx(
        &mut self,
    ) -> Sender<SignatureManagerRequestAndResponseSender> {
        self.signature_manager_channel.take_tx()
    }

    pub fn take_signature_manager_rx(
        &mut self,
    ) -> Receiver<SignatureManagerRequestAndResponseSender> {
        self.signature_manager_channel.take_rx()
    }

    pub fn take_state_sync_tx(&mut self) -> Sender<StateSyncRequestAndResponseSender> {
        self.state_sync_channel.take_tx()
    }

    pub fn take_state_sync_rx(&mut self) -> Receiver<StateSyncRequestAndResponseSender> {
        self.state_sync_channel.take_rx()
    }
}

pub fn create_node_channels(config: &SequencerNodeConfig) -> SequencerNodeCommunication {
    info!("Creating node channels.");
    let (tx_batcher, rx_batcher) = channel::<BatcherRequestAndResponseSender>(
        config.components.batcher.local_server_config.channel_capacity,
    );

    let (tx_class_manager, rx_class_manager) = channel::<ClassManagerRequestAndResponseSender>(
        config.components.class_manager.local_server_config.channel_capacity,
    );

    let (tx_gateway, rx_gateway) = channel::<GatewayRequestAndResponseSender>(
        config.components.gateway.local_server_config.channel_capacity,
    );

    let (tx_l1_endpoint_monitor, rx_l1_endpoint_monitor) =
        channel::<L1EndpointMonitorRequestAndResponseSender>(
            config.components.l1_endpoint_monitor.local_server_config.channel_capacity,
        );

    let (tx_l1_provider, rx_l1_provider) = channel::<L1ProviderRequestAndResponseSender>(
        config.components.l1_provider.local_server_config.channel_capacity,
    );

    let (tx_l1_gas_price, rx_l1_gas_price) = channel::<L1GasPriceRequestAndResponseSender>(
        config.components.l1_gas_price_provider.local_server_config.channel_capacity,
    );

    let (tx_mempool, rx_mempool) = channel::<MempoolRequestAndResponseSender>(
        config.components.mempool.local_server_config.channel_capacity,
    );

    let (tx_mempool_p2p_propagator, rx_mempool_p2p_propagator) =
        channel::<MempoolP2pPropagatorRequestAndResponseSender>(
            config.components.mempool_p2p.local_server_config.channel_capacity,
        );

    let (tx_sierra_compiler, rx_sierra_compiler) = channel::<SierraCompilerRequestAndResponseSender>(
        config.components.sierra_compiler.local_server_config.channel_capacity,
    );

    let (tx_signature_manager, rx_signature_manager) =
        channel::<SignatureManagerRequestAndResponseSender>(
            config.components.state_sync.local_server_config.channel_capacity,
        );

    let (tx_state_sync, rx_state_sync) = channel::<StateSyncRequestAndResponseSender>(
        config.components.state_sync.local_server_config.channel_capacity,
    );

    SequencerNodeCommunication {
        batcher_channel: ComponentCommunication::new(Some(tx_batcher), Some(rx_batcher)),
        class_manager_channel: ComponentCommunication::new(
            Some(tx_class_manager),
            Some(rx_class_manager),
        ),
        gateway_channel: ComponentCommunication::new(Some(tx_gateway), Some(rx_gateway)),
        l1_endpoint_monitor_channel: ComponentCommunication::new(
            Some(tx_l1_endpoint_monitor),
            Some(rx_l1_endpoint_monitor),
        ),
        l1_provider_channel: ComponentCommunication::new(
            Some(tx_l1_provider),
            Some(rx_l1_provider),
        ),
        l1_gas_price_channel: ComponentCommunication::new(
            Some(tx_l1_gas_price),
            Some(rx_l1_gas_price),
        ),
        mempool_channel: ComponentCommunication::new(Some(tx_mempool), Some(rx_mempool)),
        mempool_p2p_propagator_channel: ComponentCommunication::new(
            Some(tx_mempool_p2p_propagator),
            Some(rx_mempool_p2p_propagator),
        ),
        sierra_compiler_channel: ComponentCommunication::new(
            Some(tx_sierra_compiler),
            Some(rx_sierra_compiler),
        ),
        signature_manager_channel: ComponentCommunication::new(
            Some(tx_signature_manager),
            Some(rx_signature_manager),
        ),
        state_sync_channel: ComponentCommunication::new(Some(tx_state_sync), Some(rx_state_sync)),
    }
}
