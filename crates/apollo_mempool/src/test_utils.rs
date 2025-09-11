use std::collections::HashMap;

use apollo_mempool_types::errors::MempoolError;
use apollo_mempool_types::mempool_types::{AddTransactionArgs, CommitBlockArgs};
use apollo_metrics::metrics::HistogramValue;
use metrics_exporter_prometheus::PrometheusRecorder;
use pretty_assertions::assert_eq;
use starknet_api::rpc_transaction::{InternalRpcTransaction, RpcTransactionLabelValue};
use starknet_api::transaction::TransactionHash;
use starknet_api::{contract_address, nonce};

use crate::mempool::Mempool;
use crate::metrics::{
    DropReason,
    LABEL_NAME_DROP_REASON,
    LABEL_NAME_TX_TYPE,
    MEMPOOL_DELAYED_DECLARES_SIZE,
    MEMPOOL_GET_TXS_SIZE,
    MEMPOOL_PENDING_QUEUE_SIZE,
    MEMPOOL_POOL_SIZE,
    MEMPOOL_PRIORITY_QUEUE_SIZE,
    MEMPOOL_TOTAL_SIZE_BYTES,
    MEMPOOL_TRANSACTIONS_COMMITTED,
    MEMPOOL_TRANSACTIONS_DROPPED,
    MEMPOOL_TRANSACTIONS_RECEIVED,
    TRANSACTION_TIME_SPENT_IN_MEMPOOL,
    TRANSACTION_TIME_SPENT_UNTIL_COMMITTED,
};

/// Creates an executable invoke transaction with the given field subset (the rest receive default
/// values).
#[macro_export]
macro_rules! tx {
    (
        tx_hash: $tx_hash:expr,
        address: $address:expr,
        tx_nonce: $tx_nonce:expr,
        tip: $tip:expr,
        max_l2_gas_price: $max_l2_gas_price:expr
    ) => {{
            use starknet_api::block::GasPrice;
            use starknet_api::{invoke_tx_args, tx_hash};
            use starknet_api::test_utils::invoke::internal_invoke_tx;
            use starknet_api::transaction::fields::{
                AllResourceBounds,
                ResourceBounds,
                Tip,
                ValidResourceBounds,
            };

            let resource_bounds = ValidResourceBounds::AllResources(AllResourceBounds {
                l2_gas: ResourceBounds {
                    max_price_per_unit: GasPrice($max_l2_gas_price),
                    ..Default::default()
                },
                ..Default::default()
            });

            internal_invoke_tx(invoke_tx_args!{
                tx_hash: tx_hash!($tx_hash),
                sender_address: contract_address!($address),
                nonce: nonce!($tx_nonce),
                tip: Tip($tip),
                resource_bounds,
            })
    }};
    (tx_hash: $tx_hash:expr, address: $address:expr, tx_nonce: $tx_nonce:expr, tip: $tip:expr) => {{
        use mempool_test_utils::starknet_api_test_utils::VALID_L2_GAS_MAX_PRICE_PER_UNIT;
        tx!(
            tx_hash: $tx_hash,
            address: $address,
            tx_nonce: $tx_nonce,
            tip: $tip,
            max_l2_gas_price: VALID_L2_GAS_MAX_PRICE_PER_UNIT
        )
    }};
    (tx_hash: $tx_hash:expr, address: $address:expr, tx_nonce: $tx_nonce:expr) => {
        tx!(tx_hash: $tx_hash, address: $address, tx_nonce: $tx_nonce, tip: 0)
    };
    (tx_hash: $tx_hash:expr, address: $address:expr, tip: $tip:expr) => {
        tx!(tx_hash: $tx_hash, address: $address, tx_nonce: 0, tip: $tip)
    };
    (tx_hash: $tx_hash:expr, address: $address:expr, max_l2_gas_price: $max_l2_gas_price:expr) => {
        tx!(
            tx_hash: $tx_hash,
            address: $address,
            tx_nonce: 0,
            tip: 0,
            max_l2_gas_price: $max_l2_gas_price
        )
    };
    (tx_hash: $tx_hash:expr, tip: $tip:expr, max_l2_gas_price: $max_l2_gas_price:expr) => {
        tx!(
            tx_hash: $tx_hash,
            address: "0x0",
            tx_nonce: 0,
            tip: $tip,
            max_l2_gas_price: $max_l2_gas_price
        )
    };
    (tip: $tip:expr, max_l2_gas_price: $max_l2_gas_price:expr) => {
        tx!(tx_hash: 0, address: "0x0", tx_nonce: 0, tip: $tip, max_l2_gas_price: $max_l2_gas_price)
    };
    () => {
        tx!(tx_hash: 0, address: "0x0", tx_nonce: 0)
    };
}

/// Creates an input for `add_tx` with the given field subset (the rest receive default values).
#[macro_export]
macro_rules! add_tx_input {
    (
        tx_hash: $tx_hash:expr,
        address: $address:expr,
        tx_nonce: $tx_nonce:expr,
        account_nonce: $account_nonce:expr,
        tip: $tip:expr,
        max_l2_gas_price: $max_l2_gas_price:expr
    ) => {{
        use starknet_api::{contract_address, nonce};
        use apollo_mempool_types::mempool_types::{AccountState, AddTransactionArgs};

        let tx = $crate::tx!(
            tx_hash: $tx_hash,
            address: $address,
            tx_nonce: $tx_nonce,
            tip: $tip,
            max_l2_gas_price: $max_l2_gas_price
        );
        let address = contract_address!($address);
        let account_nonce = nonce!($account_nonce);
        let account_state = AccountState { address, nonce: account_nonce };

        AddTransactionArgs { tx, account_state }
    }};
    (
        tx_hash: $tx_hash:expr,
        address: $address:expr,
        tx_nonce: $tx_nonce:expr,
        account_nonce: $account_nonce:expr,
        tip: $tip:expr
    ) => {{
        use mempool_test_utils::starknet_api_test_utils::VALID_L2_GAS_MAX_PRICE_PER_UNIT;
        add_tx_input!(
            tx_hash: $tx_hash,
            address: $address,
            tx_nonce: $tx_nonce,
            account_nonce: $account_nonce,
            tip: $tip,
            max_l2_gas_price: VALID_L2_GAS_MAX_PRICE_PER_UNIT
        )
    }};
    (
        tx_hash: $tx_hash:expr,
        address: $address:expr,
        tx_nonce: $tx_nonce:expr,
        tip: $tip:expr,
        max_l2_gas_price: $max_l2_gas_price:expr
    ) => {
        add_tx_input!(
            tx_hash: $tx_hash,
            address: $address,
            tx_nonce: $tx_nonce,
            account_nonce: 0,
            tip: $tip,
            max_l2_gas_price: $max_l2_gas_price
        )
    };
    (tx_hash: $tx_hash:expr, address: $address:expr, tip: $tip:expr) => {
        add_tx_input!(
            tx_hash: $tx_hash,
            address: $address,
            tx_nonce: 0,
            account_nonce: 0,
            tip: $tip
        )
    };
    (
        tx_hash: $tx_hash:expr,
        address: $address:expr,
        tx_nonce: $tx_nonce:expr,
        account_nonce: $account_nonce:expr
    ) => {
        add_tx_input!(
            tx_hash: $tx_hash,
            address: $address,
            tx_nonce: $tx_nonce,
            account_nonce: $account_nonce,
            tip: 0
        )
    };
    (tx_hash: $tx_hash:expr, tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr) => {
        add_tx_input!(
            tx_hash: $tx_hash,
            address: "0x0",
            tx_nonce: $tx_nonce,
            account_nonce: $account_nonce
        )
    };
    (tx_hash: $tx_hash:expr, tx_nonce: $tx_nonce:expr) => {
        add_tx_input!(tx_hash: $tx_hash, tx_nonce: $tx_nonce, account_nonce: 0)
    };
    (
        tx_hash: $tx_hash:expr,
        address: $address:expr,
        tip: $tip:expr,
        max_l2_gas_price: $max_l2_gas_price:expr
    ) => {
        add_tx_input!(
            tx_hash: $tx_hash,
            address: $address,
            tx_nonce: 0,
            account_nonce: 0,
            tip: $tip,
            max_l2_gas_price: $max_l2_gas_price
        )
    };
    (tx_hash: $tx_hash:expr, tip: $tip:expr, max_l2_gas_price: $max_l2_gas_price:expr) => {
        add_tx_input!(
            tx_hash: $tx_hash,
            address: "0x0",
            tip: $tip,
            max_l2_gas_price: $max_l2_gas_price
        )
    };
    (tip: $tip:expr, max_l2_gas_price: $max_l2_gas_price:expr) => {
        add_tx_input!(
            tx_hash: 0,
            address: "0x0",
            tx_nonce: 0,
            tip: $tip,
            max_l2_gas_price: $max_l2_gas_price
        )
    };
    (address: $address:expr) => {
        add_tx_input!(
            tx_hash: 0,
            address: $address,
            tip: 0
        )
    };
}

#[track_caller]
pub fn add_tx(mempool: &mut Mempool, input: &AddTransactionArgs) {
    assert_eq!(mempool.add_tx(input.clone()), Ok(()));
}

#[track_caller]
pub fn add_tx_expect_error(
    mempool: &mut Mempool,
    input: &AddTransactionArgs,
    expected_error: MempoolError,
) {
    assert_eq!(mempool.add_tx(input.clone()), Err(expected_error));
}

#[track_caller]
pub fn commit_block(
    mempool: &mut Mempool,
    nonces: impl IntoIterator<Item = (&'static str, u8)>,
    rejected_tx_hashes: impl IntoIterator<Item = TransactionHash>,
) {
    let nonces = HashMap::from_iter(
        nonces.into_iter().map(|(address, nonce)| (contract_address!(address), nonce!(nonce))),
    );
    let rejected_tx_hashes = rejected_tx_hashes.into_iter().collect();
    let args = CommitBlockArgs { address_to_nonce: nonces, rejected_tx_hashes };

    mempool.commit_block(args);
}

#[track_caller]
pub fn get_txs_and_assert_expected(
    mempool: &mut Mempool,
    n_txs: usize,
    expected_txs: &[InternalRpcTransaction],
) {
    let txs = mempool.get_txs(n_txs).unwrap();
    assert_eq!(txs, expected_txs);
}

#[derive(Default)]
pub struct MempoolMetrics {
    pub txs_received_invoke: u64,
    pub txs_received_declare: u64,
    pub txs_received_deploy_account: u64,
    pub txs_committed: u64,
    pub txs_dropped_expired: u64,
    pub txs_dropped_failed_add_tx_checks: u64,
    pub txs_dropped_rejected: u64,
    pub txs_dropped_evicted: u64,
    pub pool_size: u64,
    pub priority_queue_size: u64,
    pub pending_queue_size: u64,
    pub get_txs_size: u64,
    pub delayed_declares_size: u64,
    pub total_size_in_bytes: u64,
    pub evictions_count: u64,
    pub transaction_time_spent_in_mempool: HistogramValue,
    pub transaction_time_spent_until_committed: HistogramValue,
}

impl MempoolMetrics {
    pub fn verify_metrics(&self, recorder: &PrometheusRecorder) {
        let metrics = &recorder.handle().render();
        MEMPOOL_TRANSACTIONS_RECEIVED.assert_eq(
            metrics,
            self.txs_received_invoke,
            &[(LABEL_NAME_TX_TYPE, RpcTransactionLabelValue::Invoke.into())],
        );
        MEMPOOL_TRANSACTIONS_RECEIVED.assert_eq(
            metrics,
            self.txs_received_declare,
            &[(LABEL_NAME_TX_TYPE, RpcTransactionLabelValue::Declare.into())],
        );
        MEMPOOL_TRANSACTIONS_RECEIVED.assert_eq(
            metrics,
            self.txs_received_deploy_account,
            &[(LABEL_NAME_TX_TYPE, RpcTransactionLabelValue::DeployAccount.into())],
        );
        MEMPOOL_TRANSACTIONS_COMMITTED.assert_eq(metrics, self.txs_committed);
        MEMPOOL_TRANSACTIONS_DROPPED.assert_eq(
            metrics,
            self.txs_dropped_expired,
            &[(LABEL_NAME_DROP_REASON, DropReason::Expired.into())],
        );
        MEMPOOL_TRANSACTIONS_DROPPED.assert_eq(
            metrics,
            self.txs_dropped_failed_add_tx_checks,
            &[(LABEL_NAME_DROP_REASON, DropReason::FailedAddTxChecks.into())],
        );
        MEMPOOL_TRANSACTIONS_DROPPED.assert_eq(
            metrics,
            self.txs_dropped_rejected,
            &[(LABEL_NAME_DROP_REASON, DropReason::Rejected.into())],
        );
        MEMPOOL_TRANSACTIONS_DROPPED.assert_eq(
            metrics,
            self.txs_dropped_evicted,
            &[(LABEL_NAME_DROP_REASON, DropReason::Evicted.into())],
        );
        MEMPOOL_POOL_SIZE.assert_eq(metrics, self.pool_size);
        MEMPOOL_PRIORITY_QUEUE_SIZE.assert_eq(metrics, self.priority_queue_size);
        MEMPOOL_PENDING_QUEUE_SIZE.assert_eq(metrics, self.pending_queue_size);
        MEMPOOL_GET_TXS_SIZE.assert_eq(metrics, self.get_txs_size);
        MEMPOOL_DELAYED_DECLARES_SIZE.assert_eq(metrics, self.delayed_declares_size);
        MEMPOOL_TOTAL_SIZE_BYTES.assert_eq(metrics, self.total_size_in_bytes);
        TRANSACTION_TIME_SPENT_IN_MEMPOOL
            .assert_eq(metrics, &self.transaction_time_spent_in_mempool);
        TRANSACTION_TIME_SPENT_UNTIL_COMMITTED
            .assert_eq(metrics, &self.transaction_time_spent_until_committed);
    }
}
