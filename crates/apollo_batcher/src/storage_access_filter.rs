use std::sync::Arc;

use apollo_batcher_config::config::StorageAccessFilterConfig;
use blockifier::blockifier::transaction_filter::{SharedTransactionFilter, TransactionFilter};
use blockifier::execution::call_info::CallInfo;
use blockifier::transaction::errors::TransactionExecutionError;
use blockifier::transaction::objects::{TransactionExecutionInfo, TransactionExecutionResult};
use blockifier::transaction::transaction_execution::Transaction;

#[cfg(test)]
#[path = "storage_access_filter_test.rs"]
mod storage_access_filter_test;

/// Returns a filter enforcing `config`, or `None` if it blocks no storage key.
pub(crate) fn create_storage_access_filter(
    config: &StorageAccessFilterConfig,
) -> Option<SharedTransactionFilter> {
    if config.blocked_storage_keys.is_empty() {
        return None;
    }
    Some(Arc::new(StorageAccessFilter(config.clone())))
}

/// Rejects a transaction whose call tree, including inner calls, reads or writes a blocked storage
/// key in any contract.
struct StorageAccessFilter(StorageAccessFilterConfig);

impl TransactionFilter for StorageAccessFilter {
    fn check(
        &self,
        _tx: &Transaction,
        tx_execution_info: &TransactionExecutionInfo,
    ) -> TransactionExecutionResult<()> {
        let accessed_blocked_storage_key = tx_execution_info
            .non_optional_call_infos()
            .flat_map(CallInfo::iter)
            .flat_map(|call_info| &call_info.storage_access_tracker.accessed_storage_keys)
            .any(|storage_key| self.0.blocked_storage_keys.contains(storage_key));
        if accessed_blocked_storage_key {
            return Err(TransactionExecutionError::RejectedByTransactionFilter {
                message: self.0.error_message.clone(),
            });
        }
        Ok(())
    }
}
