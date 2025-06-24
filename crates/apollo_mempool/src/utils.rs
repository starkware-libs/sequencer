use apollo_mempool_types::communication::MempoolResult;
use apollo_mempool_types::errors::MempoolError;
use starknet_api::core::Nonce;

pub fn try_increment_nonce(nonce: Nonce) -> MempoolResult<Nonce> {
    nonce.try_increment().map_err(|_| MempoolError::NonceTooLarge(nonce))
}
