use async_trait::async_trait;
use starknet_mempool_infra::component_runner::ComponentStarter;
use starknet_mempool_infra::errors::ComponentError;

pub struct MempoolP2pReceiver;

#[async_trait]
impl ComponentStarter for MempoolP2pReceiver {
    async fn start(&mut self) -> Result<(), ComponentError> {
        unimplemented!()
    }
}
