use starknet_api::core::Nonce;
use starknet_mempool_types::communication::MempoolResult;
use starknet_mempool_types::errors::MempoolError;

pub fn try_increment_nonce(nonce: Nonce) -> MempoolResult<Nonce> {
    nonce.try_increment().map_err(|_| MempoolError::NonceTooLarge(nonce))
}
