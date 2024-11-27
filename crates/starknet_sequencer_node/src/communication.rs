use starknet_batcher_types::communication::BatcherRequestAndResponseSender;
use starknet_gateway_types::communication::GatewayRequestAndResponseSender;
use starknet_mempool_p2p_types::communication::MempoolP2pPropagatorRequestAndResponseSender;
use starknet_mempool_types::communication::MempoolRequestAndResponseSender;
use starknet_sequencer_infra::component_definitions::ComponentCommunication;
use starknet_state_sync_types::communication::StateSyncRequestAndResponseSender;
use tokio::sync::mpsc::{channel, Receiver, Sender};

pub struct SequencerNodeCommunication {
    batcher_channel: ComponentCommunication<BatcherRequestAndResponseSender>,
    gateway_channel: ComponentCommunication<GatewayRequestAndResponseSender>,
    mempool_channel: ComponentCommunication<MempoolRequestAndResponseSender>,
    mempool_p2p_propagator_channel:
        ComponentCommunication<MempoolP2pPropagatorRequestAndResponseSender>,
    state_sync_channel: ComponentCommunication<StateSyncRequestAndResponseSender>,
}

impl SequencerNodeCommunication {
    pub fn take_batcher_tx(&mut self) -> Sender<BatcherRequestAndResponseSender> {
        self.batcher_channel.take_tx()
    }

    pub fn take_batcher_rx(&mut self) -> Receiver<BatcherRequestAndResponseSender> {
        self.batcher_channel.take_rx()
    }

    pub fn take_gateway_tx(&mut self) -> Sender<GatewayRequestAndResponseSender> {
        self.gateway_channel.take_tx()
    }

    pub fn take_gateway_rx(&mut self) -> Receiver<GatewayRequestAndResponseSender> {
        self.gateway_channel.take_rx()
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

    let (tx_gateway, rx_gateway) =
        channel::<GatewayRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    let (tx_mempool, rx_mempool) =
        channel::<MempoolRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    let (tx_mempool_p2p_propagator, rx_mempool_p2p_propagator) =
        channel::<MempoolP2pPropagatorRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    let (tx_state_sync, rx_state_sync) =
        channel::<StateSyncRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    SequencerNodeCommunication {
        batcher_channel: ComponentCommunication::new(Some(tx_batcher), Some(rx_batcher)),
        gateway_channel: ComponentCommunication::new(Some(tx_gateway), Some(rx_gateway)),
        mempool_channel: ComponentCommunication::new(Some(tx_mempool), Some(rx_mempool)),
        mempool_p2p_propagator_channel: ComponentCommunication::new(
            Some(tx_mempool_p2p_propagator),
            Some(rx_mempool_p2p_propagator),
        ),
        state_sync_channel: ComponentCommunication::new(Some(tx_state_sync), Some(rx_state_sync)),
    }
}
