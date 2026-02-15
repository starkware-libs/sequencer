use apollo_mempool_types::errors::MempoolError;
use apollo_mempool_types::mempool_types::MempoolResult;
use starknet_api::core::Nonce;

pub fn try_increment_nonce(nonce: Nonce) -> MempoolResult<Nonce> {
    nonce.try_increment().map_err(|_| MempoolError::NonceTooLarge(nonce))
}
