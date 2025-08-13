use apollo_batcher_types::communication::BatcherRequestWrapper;
use apollo_class_manager_types::ClassManagerRequestWrapper;
use apollo_compile_to_casm_types::SierraCompilerRequestWrapper;
use apollo_gateway_types::communication::GatewayRequestWrapper;
use apollo_infra::component_definitions::ComponentCommunication;
<<<<<<< HEAD
use apollo_l1_endpoint_monitor::communication::L1EndpointMonitorRequestAndResponseSender;
use apollo_l1_gas_price::communication::L1GasPriceRequestAndResponseSender;
use apollo_l1_provider::communication::L1ProviderRequestAndResponseSender;
use apollo_mempool_p2p_types::communication::MempoolP2pPropagatorRequestAndResponseSender;
use apollo_mempool_types::communication::MempoolRequestAndResponseSender;
use apollo_signature_manager_types::SignatureManagerRequestAndResponseSender;
use apollo_state_sync_types::communication::StateSyncRequestAndResponseSender;
||||||| 38f03e1d0
use apollo_l1_endpoint_monitor::communication::L1EndpointMonitorRequestAndResponseSender;
use apollo_l1_gas_price::communication::L1GasPriceRequestAndResponseSender;
use apollo_l1_provider::communication::L1ProviderRequestAndResponseSender;
use apollo_mempool_p2p_types::communication::MempoolP2pPropagatorRequestAndResponseSender;
use apollo_mempool_types::communication::MempoolRequestAndResponseSender;
use apollo_state_sync_types::communication::StateSyncRequestAndResponseSender;
=======
use apollo_l1_endpoint_monitor::communication::L1EndpointMonitorRequestWrapper;
use apollo_l1_gas_price::communication::L1GasPriceRequestWrapper;
use apollo_l1_provider::communication::L1ProviderRequestWrapper;
use apollo_mempool_p2p_types::communication::MempoolP2pPropagatorRequestWrapper;
use apollo_mempool_types::communication::MempoolRequestWrapper;
use apollo_state_sync_types::communication::StateSyncRequestWrapper;
>>>>>>> origin/main-v0.14.0
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tracing::info;

use crate::config::node_config::SequencerNodeConfig;

pub struct SequencerNodeCommunication {
<<<<<<< HEAD
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
||||||| 38f03e1d0
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
    state_sync_channel: ComponentCommunication<StateSyncRequestAndResponseSender>,
=======
    batcher_channel: ComponentCommunication<BatcherRequestWrapper>,
    class_manager_channel: ComponentCommunication<ClassManagerRequestWrapper>,
    gateway_channel: ComponentCommunication<GatewayRequestWrapper>,
    l1_endpoint_monitor_channel: ComponentCommunication<L1EndpointMonitorRequestWrapper>,
    l1_provider_channel: ComponentCommunication<L1ProviderRequestWrapper>,
    l1_gas_price_channel: ComponentCommunication<L1GasPriceRequestWrapper>,
    mempool_channel: ComponentCommunication<MempoolRequestWrapper>,
    mempool_p2p_propagator_channel: ComponentCommunication<MempoolP2pPropagatorRequestWrapper>,
    sierra_compiler_channel: ComponentCommunication<SierraCompilerRequestWrapper>,
    state_sync_channel: ComponentCommunication<StateSyncRequestWrapper>,
>>>>>>> origin/main-v0.14.0
}

impl SequencerNodeCommunication {
    pub fn take_batcher_tx(&mut self) -> Sender<BatcherRequestWrapper> {
        self.batcher_channel.take_tx()
    }

    pub fn take_batcher_rx(&mut self) -> Receiver<BatcherRequestWrapper> {
        self.batcher_channel.take_rx()
    }

    pub fn take_class_manager_tx(&mut self) -> Sender<ClassManagerRequestWrapper> {
        self.class_manager_channel.take_tx()
    }

    pub fn take_class_manager_rx(&mut self) -> Receiver<ClassManagerRequestWrapper> {
        self.class_manager_channel.take_rx()
    }

    pub fn take_gateway_tx(&mut self) -> Sender<GatewayRequestWrapper> {
        self.gateway_channel.take_tx()
    }

    pub fn take_gateway_rx(&mut self) -> Receiver<GatewayRequestWrapper> {
        self.gateway_channel.take_rx()
    }

    pub fn take_l1_endpoint_monitor_tx(&mut self) -> Sender<L1EndpointMonitorRequestWrapper> {
        self.l1_endpoint_monitor_channel.take_tx()
    }

    pub fn take_l1_endpoint_monitor_rx(&mut self) -> Receiver<L1EndpointMonitorRequestWrapper> {
        self.l1_endpoint_monitor_channel.take_rx()
    }

    pub fn take_l1_provider_tx(&mut self) -> Sender<L1ProviderRequestWrapper> {
        self.l1_provider_channel.take_tx()
    }

    pub fn take_l1_provider_rx(&mut self) -> Receiver<L1ProviderRequestWrapper> {
        self.l1_provider_channel.take_rx()
    }

    pub fn take_l1_gas_price_tx(&mut self) -> Sender<L1GasPriceRequestWrapper> {
        self.l1_gas_price_channel.take_tx()
    }
    pub fn take_l1_gas_price_rx(&mut self) -> Receiver<L1GasPriceRequestWrapper> {
        self.l1_gas_price_channel.take_rx()
    }

    pub fn take_mempool_p2p_propagator_tx(&mut self) -> Sender<MempoolP2pPropagatorRequestWrapper> {
        self.mempool_p2p_propagator_channel.take_tx()
    }
    pub fn take_mempool_p2p_propagator_rx(
        &mut self,
    ) -> Receiver<MempoolP2pPropagatorRequestWrapper> {
        self.mempool_p2p_propagator_channel.take_rx()
    }

    pub fn take_mempool_tx(&mut self) -> Sender<MempoolRequestWrapper> {
        self.mempool_channel.take_tx()
    }

    pub fn take_mempool_rx(&mut self) -> Receiver<MempoolRequestWrapper> {
        self.mempool_channel.take_rx()
    }

    pub fn take_sierra_compiler_tx(&mut self) -> Sender<SierraCompilerRequestWrapper> {
        self.sierra_compiler_channel.take_tx()
    }

    pub fn take_sierra_compiler_rx(&mut self) -> Receiver<SierraCompilerRequestWrapper> {
        self.sierra_compiler_channel.take_rx()
    }

<<<<<<< HEAD
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
||||||| 38f03e1d0
    pub fn take_state_sync_tx(&mut self) -> Sender<StateSyncRequestAndResponseSender> {
=======
    pub fn take_state_sync_tx(&mut self) -> Sender<StateSyncRequestWrapper> {
>>>>>>> origin/main-v0.14.0
        self.state_sync_channel.take_tx()
    }

    pub fn take_state_sync_rx(&mut self) -> Receiver<StateSyncRequestWrapper> {
        self.state_sync_channel.take_rx()
    }
}

pub fn create_node_channels(config: &SequencerNodeConfig) -> SequencerNodeCommunication {
    info!("Creating node channels.");
    let (tx_batcher, rx_batcher) = channel::<BatcherRequestWrapper>(
        config.components.batcher.local_server_config.channel_capacity,
    );

    let (tx_class_manager, rx_class_manager) = channel::<ClassManagerRequestWrapper>(
        config.components.class_manager.local_server_config.channel_capacity,
    );

    let (tx_gateway, rx_gateway) = channel::<GatewayRequestWrapper>(
        config.components.gateway.local_server_config.channel_capacity,
    );

    let (tx_l1_endpoint_monitor, rx_l1_endpoint_monitor) = channel::<L1EndpointMonitorRequestWrapper>(
        config.components.l1_endpoint_monitor.local_server_config.channel_capacity,
    );

    let (tx_l1_provider, rx_l1_provider) = channel::<L1ProviderRequestWrapper>(
        config.components.l1_provider.local_server_config.channel_capacity,
    );

    let (tx_l1_gas_price, rx_l1_gas_price) = channel::<L1GasPriceRequestWrapper>(
        config.components.l1_gas_price_provider.local_server_config.channel_capacity,
    );

    let (tx_mempool, rx_mempool) = channel::<MempoolRequestWrapper>(
        config.components.mempool.local_server_config.channel_capacity,
    );

    let (tx_mempool_p2p_propagator, rx_mempool_p2p_propagator) =
        channel::<MempoolP2pPropagatorRequestWrapper>(
            config.components.mempool_p2p.local_server_config.channel_capacity,
        );

    let (tx_sierra_compiler, rx_sierra_compiler) = channel::<SierraCompilerRequestWrapper>(
        config.components.sierra_compiler.local_server_config.channel_capacity,
    );

<<<<<<< HEAD
    let (tx_signature_manager, rx_signature_manager) =
        channel::<SignatureManagerRequestAndResponseSender>(
            config.components.state_sync.local_server_config.channel_capacity,
        );

    let (tx_state_sync, rx_state_sync) = channel::<StateSyncRequestAndResponseSender>(
||||||| 38f03e1d0
    let (tx_state_sync, rx_state_sync) = channel::<StateSyncRequestAndResponseSender>(
=======
    let (tx_state_sync, rx_state_sync) = channel::<StateSyncRequestWrapper>(
>>>>>>> origin/main-v0.14.0
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
