use apollo_state_sync_types::communication::StateSyncClientResult;
use async_trait::async_trait;
use blockifier::state::state_reader_and_contract_manager::FetchCompiledClasses;
#[cfg(test)]
use mockall::automock;

use crate::gateway_fixed_block_state_reader::GatewayFixedBlockStateReader;
#[cfg_attr(test, automock)]
#[async_trait]
pub trait StateReaderFactory: Send + Sync {
    async fn get_blockifier_state_reader_and_gateway_fixed_block_from_latest_block(
        &self,
    ) -> StateSyncClientResult<(
        Box<dyn GatewayStateReaderWithCompiledClasses>,
        Box<dyn GatewayFixedBlockStateReader>,
    )>;
}

// TODO(Arni): Delete this trait, once we replace `dyn GatewayStateReaderWithCompiledClasses` with
// generics.
pub trait GatewayStateReaderWithCompiledClasses: FetchCompiledClasses + Send + Sync {}
