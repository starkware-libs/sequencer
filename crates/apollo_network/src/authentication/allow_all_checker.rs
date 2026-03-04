use async_trait::async_trait;
use starknet_api::crypto::utils::PublicKey;

use crate::authentication::staker_authenticator::AllowListChecker;

// TODO(noam.s): Replace with a real committee-based allow list checker.
#[derive(Clone)]
pub struct AllowAllChecker;

#[async_trait]
impl AllowListChecker for AllowAllChecker {
    async fn is_allowed(&self, _public_key: &PublicKey) -> bool {
        true
    }

    fn clone_box(&self) -> Box<dyn AllowListChecker> {
        Box::new(self.clone())
    }
}
