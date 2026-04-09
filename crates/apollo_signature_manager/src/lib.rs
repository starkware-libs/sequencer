// TODO(Aviv): Delete this mod and use utils from type-rs
mod blake_utils;
pub mod communication;
pub mod config;
#[cfg(feature = "gsm")]
pub mod gsm_key_store;
pub mod metrics;
pub mod signature_manager;

use apollo_infra::component_definitions::ComponentStarter;
use apollo_network_types::network_types::PeerId;
use apollo_signature_manager_types::SignatureManagerResult;
use async_trait::async_trait;
use starknet_api::block::BlockHash;
use starknet_api::crypto::utils::{Challenge, RawSignature};

use crate::config::{KeySourceConfig, SignatureManagerConfig};
#[cfg(feature = "gsm")]
use crate::gsm_key_store::GsmKeyStore;
use crate::signature_manager::{GenericSignatureManager, LocalKeyStore};

#[derive(Clone, Debug)]
pub enum SignatureManager {
    Local(GenericSignatureManager<LocalKeyStore>),
    #[cfg(feature = "gsm")]
    Gsm(GenericSignatureManager<GsmKeyStore>),
}

impl SignatureManager {
    pub async fn sign_identification(
        &self,
        peer_id: PeerId,
        challenge: Challenge,
    ) -> SignatureManagerResult<RawSignature> {
        match self {
            Self::Local(sm) => sm.sign_identification(peer_id, challenge).await,
            #[cfg(feature = "gsm")]
            Self::Gsm(sm) => sm.sign_identification(peer_id, challenge).await,
        }
    }

    pub async fn sign_precommit_vote(
        &self,
        block_hash: BlockHash,
    ) -> SignatureManagerResult<RawSignature> {
        match self {
            Self::Local(sm) => sm.sign_precommit_vote(block_hash).await,
            #[cfg(feature = "gsm")]
            Self::Gsm(sm) => sm.sign_precommit_vote(block_hash).await,
        }
    }
}

/// Creates a `SignatureManager` configured according to `config`.
pub async fn create_signature_manager(config: &SignatureManagerConfig) -> SignatureManager {
    match &config.key_source {
        KeySourceConfig::Testing => SignatureManager::Local(GenericSignatureManager::new(
            LocalKeyStore::new_for_testing(),
        )),
        #[cfg(feature = "gsm")]
        KeySourceConfig::GoogleSecretManager { secret_name } => {
            let gsm_key_store = GsmKeyStore::new(secret_name.clone())
                .await
                .expect("Failed to initialise GsmKeyStore");
            SignatureManager::Gsm(GenericSignatureManager::new(gsm_key_store))
        }
        #[cfg(not(feature = "gsm"))]
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
