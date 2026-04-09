// TODO(Aviv): Delete this mod and use utils from type-rs
mod blake_utils;
pub mod communication;
#[cfg(feature = "gsm")]
pub mod gsm_key_store;
pub mod config;
pub mod metrics;
pub mod signature_manager;

use apollo_infra::component_definitions::ComponentStarter;
use apollo_network_types::network_types::PeerId;
use apollo_signature_manager_types::SignatureManagerResult;
use async_trait::async_trait;
use starknet_api::block::BlockHash;
use starknet_api::crypto::utils::{Challenge, RawSignature};

use crate::config::{KeySourceConfig, SignatureManagerConfig};
use crate::signature_manager::{GenericSignatureManager, LocalKeyStore};

#[derive(Clone, Debug)]
pub enum SignatureManager {
    Local(GenericSignatureManager<LocalKeyStore>),
}

impl SignatureManager {
    pub async fn sign_identification(
        &self,
        peer_id: PeerId,
        challenge: Challenge,
    ) -> SignatureManagerResult<RawSignature> {
        match self {
            Self::Local(sm) => sm.sign_identification(peer_id, challenge).await,
        }
    }

    pub async fn sign_precommit_vote(
        &self,
        block_hash: BlockHash,
    ) -> SignatureManagerResult<RawSignature> {
        match self {
            Self::Local(sm) => sm.sign_precommit_vote(block_hash).await,
        }
    }
}

/// Creates a `SignatureManager` configured according to `config`.
pub async fn create_signature_manager(config: &SignatureManagerConfig) -> SignatureManager {
    match &config.key_source {
        KeySourceConfig::Testing => {
            SignatureManager::Local(GenericSignatureManager::new(LocalKeyStore::new_for_testing()))
        }
        KeySourceConfig::GoogleSecretManager { .. } => {
            panic!(
                "KeySourceConfig::GoogleSecretManager requires the 'gsm' feature flag to be \
                 enabled when building apollo_signature_manager"
            )
        }
    }
}

#[async_trait]
impl ComponentStarter for SignatureManager {}
