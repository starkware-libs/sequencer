use std::sync::{Arc, LazyLock};

use apollo_batcher_config::config::{BatcherDynamicConfig, StorageAccessFilterConfig};
use blockifier::blockifier::transaction_filter::{SharedTransactionFilter, TransactionFilter};
use blockifier::execution::call_info::CallInfo;
use blockifier::execution::contract_class::TrackedResource;
use blockifier::transaction::errors::TransactionExecutionError;
use blockifier::transaction::objects::{TransactionExecutionInfo, TransactionExecutionResult};
use blockifier::transaction::transaction_execution::Transaction;
use cairo_vm::Felt252;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::core::EntryPointSelector;
use starknet_api::transaction::constants::{FELT_TRUE, VALIDATE_RETDATA};
use tracing::warn;

#[cfg(test)]
#[path = "storage_access_filter_test.rs"]
mod storage_access_filter_test;

static IS_VALID_SIGNATURE_SNAKE: LazyLock<EntryPointSelector> =
    LazyLock::new(|| selector_from_name("is_valid_signature"));
static IS_VALID_SIGNATURE_CAMEL: LazyLock<EntryPointSelector> =
    LazyLock::new(|| selector_from_name("isValidSignature"));

/// Returns the composite post-execution filter for `dynamic_config`, or `None` if all checks are
/// disabled.
pub(crate) fn create_transaction_filter(
    dynamic_config: &BatcherDynamicConfig,
) -> Option<SharedTransactionFilter> {
    let storage_filter = create_storage_access_filter(&dynamic_config.storage_access_filter_config);
    if !dynamic_config.reject_invalid_signature_return_data {
        return storage_filter;
    }
    Some(Arc::new(SignatureReturnDataFilter { inner: storage_filter }))
}

/// Returns a filter enforcing `config`, or `None` if it blocks no storage key.
pub(crate) fn create_storage_access_filter(
    config: &StorageAccessFilterConfig,
) -> Option<SharedTransactionFilter> {
    if config.blocked_storage_keys.is_empty() {
        return None;
    }
    Some(Arc::new(StorageAccessFilter { config: config.clone() }))
}

fn is_signature_validation_selector(selector: EntryPointSelector) -> bool {
    selector == *IS_VALID_SIGNATURE_SNAKE || selector == *IS_VALID_SIGNATURE_CAMEL
}

fn is_valid_signature_retdata(retdata: &[Felt252]) -> bool {
    matches!(retdata, [])
        || matches!(retdata, [x] if *x == FELT_TRUE.into() || *x == *VALIDATE_RETDATA)
}

/// Rejects transactions in which a caller running in `CairoSteps` mode receives a non-reverting
/// `is_valid_signature` or `isValidSignature` return value other than `[]`, `[1]`, or `['VALID']`.
struct SignatureReturnDataFilter {
    inner: Option<SharedTransactionFilter>,
}

impl TransactionFilter for SignatureReturnDataFilter {
    fn check(
        &self,
        tx: &Transaction,
        tx_execution_info: &TransactionExecutionInfo,
    ) -> TransactionExecutionResult<()> {
        if let Some(inner) = &self.inner {
            inner.check(tx, tx_execution_info)?;
        }
        let invalid_call = tx_execution_info
            .non_optional_call_infos()
            .flat_map(CallInfo::iter)
            .filter(|caller| caller.tracked_resource == TrackedResource::CairoSteps)
            .flat_map(|caller| caller.inner_calls.iter().map(move |callee| (caller, callee)))
            .find(|(_, callee)| {
                !callee.execution.failed
                    && is_signature_validation_selector(callee.call.entry_point_selector)
                    && !is_valid_signature_retdata(&callee.execution.retdata.0)
            });
        if let Some((caller, callee)) = invalid_call {
            warn!(
                tx_hash = %Transaction::tx_hash(tx),
                caller_address = %caller.call.storage_address,
                callee_address = %callee.call.storage_address,
                selector = %callee.call.entry_point_selector.0,
                retdata_len = callee.execution.retdata.0.len(),
                "Rejected transaction due to invalid signature validation return data."
            );
            return Err(TransactionExecutionError::RejectedByTransactionFilter);
        }
        Ok(())
    }
}

/// Rejects a transaction whose call tree, including inner calls, reads or writes a blocked storage
/// key in any contract, unless it is sent by an exempt account.
struct StorageAccessFilter {
    config: StorageAccessFilterConfig,
}

impl TransactionFilter for StorageAccessFilter {
    fn check(
        &self,
        tx: &Transaction,
        tx_execution_info: &TransactionExecutionInfo,
    ) -> TransactionExecutionResult<()> {
        if let Transaction::Account(account_tx) = tx {
            if self.config.exempt_account_address == Some(account_tx.sender_address()) {
                return Ok(());
            }
        }
        let accessed_blocked_storage_key = tx_execution_info
            .non_optional_call_infos()
            .flat_map(CallInfo::iter)
            .map(|call_info| &call_info.storage_access_tracker.accessed_storage_keys)
            .any(|accessed_storage_keys| {
                self.config
                    .blocked_storage_keys
                    .iter()
                    .any(|key| accessed_storage_keys.contains(key))
            });
        if accessed_blocked_storage_key {
            return Err(TransactionExecutionError::RejectedByTransactionFilter);
        }
        Ok(())
    }
}
