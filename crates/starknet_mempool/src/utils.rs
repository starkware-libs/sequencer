use std::time::Instant;

use starknet_api::core::Nonce;
use starknet_mempool_types::communication::MempoolResult;
use starknet_mempool_types::errors::MempoolError;

pub fn try_increment_nonce(nonce: Nonce) -> MempoolResult<Nonce> {
    nonce.try_increment().map_err(|_| MempoolError::NonceTooLarge(nonce))
}

// TODO(dafna, 01/03/2025): Move to a common utils crate.
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

pub struct InstantClock;

impl Clock for InstantClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}
