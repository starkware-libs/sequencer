pub mod config;
pub mod runner;

use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use futures::SinkExt;
use starknet_sequencer_infra::component_definitions::ComponentRequestHandler;
use starknet_state_sync_types::communication::{StateSyncRequest, StateSyncResponse};
use starknet_state_sync_types::errors::StateSyncError;

use crate::config::StateSyncConfig;
use crate::runner::StateSyncRunner;

// TODO(shahak): consider adding to config
const BUFFER_SIZE: usize = 100000;

pub fn create_state_sync_and_runner(config: StateSyncConfig) -> (StateSync, StateSyncRunner) {
    let (request_sender, request_receiver) = mpsc::channel(BUFFER_SIZE);
    (StateSync { request_sender }, StateSyncRunner::new(config, request_receiver))
}

pub struct StateSync {
    pub request_sender: mpsc::Sender<(StateSyncRequest, oneshot::Sender<StateSyncResponse>)>,
}

// TODO(shahak): Have StateSyncRunner call StateSync instead of the opposite once we stop supporting
// papyrus executable and can move the storage into StateSync.
#[async_trait]
impl ComponentRequestHandler<StateSyncRequest, StateSyncResponse> for StateSync {
    async fn handle_request(&mut self, request: StateSyncRequest) -> StateSyncResponse {
        let (response_sender, response_receiver) = oneshot::channel();
        if self.request_sender.send((request, response_sender)).await.is_err() {
            return StateSyncResponse::GetBlock(Err(StateSyncError::RunnerCommunicationError));
        }
        response_receiver.await.unwrap_or_else(|_| {
            StateSyncResponse::GetBlock(Err(StateSyncError::RunnerCommunicationError))
        })
    }
}
