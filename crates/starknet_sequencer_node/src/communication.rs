use starknet_batcher_types::communication::BatcherRequestAndResponseSender;
use starknet_class_manager_types::ClassManagerRequestAndResponseSender;
use starknet_gateway_types::communication::GatewayRequestAndResponseSender;
use starknet_l1_gas_price::communication::L1GasPriceRequestAndResponseSender;
use starknet_l1_provider::communication::L1ProviderRequestAndResponseSender;
use starknet_mempool_p2p_types::communication::MempoolP2pPropagatorRequestAndResponseSender;
use starknet_mempool_types::communication::MempoolRequestAndResponseSender;
use starknet_sequencer_infra::component_definitions::ComponentCommunication;
use starknet_sierra_multicompile_types::SierraCompilerRequestAndResponseSender;
use starknet_state_sync_types::communication::StateSyncRequestAndResponseSender;
use tokio::sync::mpsc::{channel, Receiver, Sender};

pub struct SequencerNodeCommunication {
    batcher_channel: ComponentCommunication<BatcherRequestAndResponseSender>,
    class_manager_channel: ComponentCommunication<ClassManagerRequestAndResponseSender>,
    gateway_channel: ComponentCommunication<GatewayRequestAndResponseSender>,
    l1_provider_channel: ComponentCommunication<L1ProviderRequestAndResponseSender>,
    l1_gas_price_channel: ComponentCommunication<L1GasPriceRequestAndResponseSender>,
    mempool_channel: ComponentCommunication<MempoolRequestAndResponseSender>,
    mempool_p2p_propagator_channel:
        ComponentCommunication<MempoolP2pPropagatorRequestAndResponseSender>,
    sierra_compiler_channel: ComponentCommunication<SierraCompilerRequestAndResponseSender>,
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

    pub fn take_state_sync_tx(&mut self) -> Sender<StateSyncRequestAndResponseSender> {
        self.state_sync_channel.take_tx()
    }

    pub fn take_state_sync_rx(&mut self) -> Receiver<StateSyncRequestAndResponseSender> {
        self.state_sync_channel.take_rx()
    }
}

pub fn create_node_channels() -> SequencerNodeCommunication {
    const DEFAULT_INVOCATIONS_QUEUE_SIZE: usize = 32;
    let (tx_batcher, rx_batcher) =
        channel::<BatcherRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    let (tx_class_manager, rx_class_manager) =
        channel::<ClassManagerRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    let (tx_gateway, rx_gateway) =
        channel::<GatewayRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    let (tx_l1_provider, rx_l1_provider) =
        channel::<L1ProviderRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    let (tx_l1_gas_price, rx_l1_gas_price) =
        channel::<L1GasPriceRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    let (tx_mempool, rx_mempool) =
        channel::<MempoolRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    let (tx_mempool_p2p_propagator, rx_mempool_p2p_propagator) =
        channel::<MempoolP2pPropagatorRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    let (tx_sierra_compiler, rx_sierra_compiler) =
        channel::<SierraCompilerRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    let (tx_state_sync, rx_state_sync) =
        channel::<StateSyncRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    SequencerNodeCommunication {
        batcher_channel: ComponentCommunication::new(Some(tx_batcher), Some(rx_batcher)),
        class_manager_channel: ComponentCommunication::new(
            Some(tx_class_manager),
            Some(rx_class_manager),
        ),
        gateway_channel: ComponentCommunication::new(Some(tx_gateway), Some(rx_gateway)),
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
        state_sync_channel: ComponentCommunication::new(Some(tx_state_sync), Some(rx_state_sync)),
    }
}
