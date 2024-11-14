use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sequencer_infra::errors::ComponentError;
use starknet_state_sync_types::communication::{StateSyncRequest, StateSyncResponse};

pub struct StateSyncRunner {
    pub request_receiver: mpsc::Receiver<(StateSyncRequest, oneshot::Sender<StateSyncResponse>)>,
}

#[async_trait]
impl ComponentStarter for StateSyncRunner {
    async fn start(&mut self) -> Result<(), ComponentError> {
        unimplemented!()
    }
}
