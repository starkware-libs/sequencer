use async_trait::async_trait;
use starknet_mempool_infra::component_runner::{ComponentStartError, ComponentStarter};

pub struct MempoolP2PReceiver;

#[async_trait]
impl ComponentStarter for MempoolP2PReceiver {
    async fn start(&mut self) -> Result<(), ComponentStartError> {
        unimplemented!()
    }
}
