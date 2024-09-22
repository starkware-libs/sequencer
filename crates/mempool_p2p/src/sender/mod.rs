use async_trait::async_trait;
use starknet_mempool_infra::component_definitions::ComponentRequestHandler;
use starknet_mempool_p2p_types::communication::{
    MempoolP2pSenderRequest,
    MempoolP2pSenderResponse,
};

pub struct MempoolP2pSender;

#[async_trait]
impl ComponentRequestHandler<MempoolP2pSenderRequest, MempoolP2pSenderResponse>
    for MempoolP2pSender
{
    async fn handle_request(
        &mut self,
        _request: MempoolP2pSenderRequest,
    ) -> MempoolP2pSenderResponse {
        unimplemented!()
    }
}
