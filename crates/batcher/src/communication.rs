use std::net::IpAddr;

use async_trait::async_trait;
use starknet_batcher_types::communication::{
    BatcherRequest,
    BatcherRequestAndResponseSender,
    BatcherResponse,
};
use starknet_mempool_infra::component_definitions::ComponentRequestHandler;
use starknet_mempool_infra::component_runner::ComponentStarter;
use starknet_mempool_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use tokio::sync::mpsc::Receiver;

use crate::batcher::Batcher;

pub type BatcherServer = LocalComponentServer<Batcher, BatcherRequest, BatcherResponse>;

pub type RemoteBatcherServer = RemoteComponentServer<Batcher, BatcherRequest, BatcherResponse>;

pub fn create_batcher_server(
    batcher: Batcher,
    rx_batcher: Receiver<BatcherRequestAndResponseSender>,
) -> BatcherServer {
    LocalComponentServer::new(batcher, rx_batcher)
}

pub fn create_remote_batcher_server(
    batcher: Batcher,
    ip_address: IpAddr,
    port: u16,
) -> RemoteBatcherServer {
    RemoteComponentServer::new(batcher, ip_address, port)
}

#[async_trait]
impl ComponentRequestHandler<BatcherRequest, BatcherResponse> for Batcher {
    async fn handle_request(&mut self, request: BatcherRequest) -> BatcherResponse {
        match request {
            BatcherRequest::PlaceholderBatcherRequest(_batcher_input) => {
                // TODO(Tsabary/Yael/Dafna): Invoke a function that returns
                // BatcherResult<PlaceholderReturnType>, and return
                // the BatcherResponse::PlaceholderBatcherResponse accordingly.
                unimplemented!()
            }
        }
    }
}

#[async_trait]
impl ComponentStarter for Batcher {}
