use std::net::IpAddr;

use async_trait::async_trait;
use starknet_batcher_types::communication::{
    BatcherRequest,
    BatcherRequestAndResponseSender,
    BatcherResponse,
};
use starknet_mempool_infra::component_definitions::ComponentRequestHandler;
use starknet_mempool_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use tokio::sync::mpsc::Receiver;

use crate::batcher::Batcher;

pub type LocalBatcherServer = LocalComponentServer<Batcher, BatcherRequest, BatcherResponse>;
pub type RemoteBatcherServer = RemoteComponentServer<Batcher, BatcherRequest, BatcherResponse>;

pub fn create_local_batcher_server(
    batcher: Batcher,
    rx_batcher: Receiver<BatcherRequestAndResponseSender>,
) -> LocalBatcherServer {
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
            BatcherRequest::BatcherFnOne(_batcher_input) => {
                // TODO(Tsabary/Yael/Dafna): Invoke a function that returns a
                // BatcherResult<BatcherFnOneReturnValue>, and return
                // the BatcherResponse::BatcherFnOneInput accordingly.
                unimplemented!()
            }
            BatcherRequest::BatcherFnTwo(_batcher_input) => {
                // TODO(Tsabary/Yael/Dafna): Invoke a function that returns a
                // BatcherResult<BatcherFnTwoReturnValue>, and return
                // the BatcherResponse::BatcherFnTwoInput accordingly.
                unimplemented!()
            }
        }
    }
}
