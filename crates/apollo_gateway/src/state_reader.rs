use apollo_state_sync_types::communication::StateSyncClientResult;
use async_trait::async_trait;
use blockifier::state::state_reader_and_contract_manager::FetchCompiledClasses;
<<<<<<< HEAD
||||||| 2787ec6b2d
#[cfg(test)]
use mockall::automock;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;
=======
#[cfg(test)]
use mockall::automock;
>>>>>>> origin/main-v0.14.1-committer

use crate::gateway_fixed_block_state_reader::GatewayFixedBlockStateReader;
#[async_trait]
pub trait StateReaderFactory: Send + Sync {
    type TGatewayStateReaderWithCompiledClasses: GatewayStateReaderWithCompiledClasses + 'static;
    type TGatewayFixedBlockStateReader: GatewayFixedBlockStateReader + 'static;

    async fn get_blockifier_state_reader_and_gateway_fixed_block_from_latest_block(
        &self,
    ) -> StateSyncClientResult<(
        Self::TGatewayStateReaderWithCompiledClasses,
        Self::TGatewayFixedBlockStateReader,
    )>;
}

// TODO(Arni): Delete this trait, once we replace `dyn GatewayStateReaderWithCompiledClasses` with
// generics.
pub trait GatewayStateReaderWithCompiledClasses: FetchCompiledClasses + Send + Sync {}
