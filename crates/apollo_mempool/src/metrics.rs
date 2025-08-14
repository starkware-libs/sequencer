use apollo_metrics::{define_metrics, generate_permutation_labels};
use starknet_api::rpc_transaction::{
    InternalRpcTransactionLabelValue,
    InternalRpcTransactionWithoutTxHash,
};
use strum::{EnumVariantNames, VariantNames};
use strum_macros::{EnumIter, IntoStaticStr};

define_metrics!(
    Mempool => {
        MetricCounter { MEMPOOL_TRANSACTIONS_COMMITTED, "mempool_txs_committed", "The number of transactions that were committed to block", init = 0 },
        MetricCounter { MEMPOOL_EVICTIONS_COUNT, "mempool_evictions_count", "The number of transactions evicted due to capacity", init = 0 },
        LabeledMetricCounter { MEMPOOL_TRANSACTIONS_RECEIVED, "mempool_transactions_received", "Counter of transactions received by the mempool", init = 0, labels = INTERNAL_RPC_TRANSACTION_LABELS },
        LabeledMetricCounter { MEMPOOL_TRANSACTIONS_DROPPED, "mempool_transactions_dropped", "Counter of transactions dropped from the mempool", init = 0, labels = DROP_REASON_LABELS },
        MetricGauge { MEMPOOL_POOL_SIZE, "mempool_pool_size", "The number of the transactions in the mempool's transaction pool" },
        MetricGauge { MEMPOOL_PRIORITY_QUEUE_SIZE, "mempool_priority_queue_size", "The size of the mempool's priority queue" },
        MetricGauge { MEMPOOL_PENDING_QUEUE_SIZE, "mempool_pending_queue_size", "The size of the mempool's pending queue" },
        MetricGauge { MEMPOOL_GET_TXS_SIZE, "mempool_get_txs_size", "The number of transactions returned in the last get_txs() api call" },
        MetricGauge { MEMPOOL_DELAYED_DECLARES_SIZE, "mempool_delayed_declare_size", "The number of declare transactions that are being delayed" },
        MetricGauge { MEMPOOL_TOTAL_SIZE_BYTES, "mempool_total_size_bytes", "The total size in bytes of the transactions in the mempool"},
        MetricHistogram { TRANSACTION_TIME_SPENT_IN_MEMPOOL, "mempool_transaction_time_spent", "The time (seconds) a transaction spent in the mempool before removal (any reason - commit, reject, TTL expiry, fee escalation, or eviction)" },
        MetricHistogram { TRANSACTION_TIME_SPENT_UNTIL_COMMITTED, "mempool_transaction_time_spent_until_committed", "The time (seconds) a transaction spent in the mempool until it was committed" },
    },
);

pub const LABEL_NAME_TX_TYPE: &str = "tx_type";
pub const LABEL_NAME_DROP_REASON: &str = "drop_reason";

generate_permutation_labels! {
    INTERNAL_RPC_TRANSACTION_LABELS,
    (LABEL_NAME_TX_TYPE, InternalRpcTransactionLabelValue),
}

generate_permutation_labels! {
    DROP_REASON_LABELS,
    (LABEL_NAME_DROP_REASON, DropReason),
}

enum TransactionStatus {
    AddedToMempool,
    Dropped,
}

#[derive(IntoStaticStr, EnumIter, EnumVariantNames)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum DropReason {
    FailedAddTxChecks,
    Expired,
    Rejected,
}

pub(crate) struct MempoolMetricHandle {
    tx_type: InternalRpcTransactionLabelValue,
    tx_status: TransactionStatus,
}

impl MempoolMetricHandle {
    pub fn new(tx: &InternalRpcTransactionWithoutTxHash) -> Self {
        let tx_type = InternalRpcTransactionLabelValue::from(tx);
        Self { tx_type, tx_status: TransactionStatus::Dropped }
    }

    fn label(&self) -> Vec<(&'static str, &'static str)> {
        vec![(LABEL_NAME_TX_TYPE, self.tx_type.into())]
    }

    pub fn count_transaction_received(&self) {
        MEMPOOL_TRANSACTIONS_RECEIVED.increment(1, &self.label());
    }

    pub fn transaction_inserted(&mut self) {
        self.tx_status = TransactionStatus::AddedToMempool;
    }
}

impl Drop for MempoolMetricHandle {
    fn drop(&mut self) {
        match self.tx_status {
            TransactionStatus::Dropped => MEMPOOL_TRANSACTIONS_DROPPED
                .increment(1, &[(LABEL_NAME_DROP_REASON, DropReason::FailedAddTxChecks.into())]),
            TransactionStatus::AddedToMempool => {}
        }
    }
}

pub(crate) fn metric_count_expired_txs(n_txs: usize) {
    MEMPOOL_TRANSACTIONS_DROPPED.increment(
        n_txs.try_into().expect("The number of expired_txs should fit u64"),
        &[(LABEL_NAME_DROP_REASON, DropReason::Expired.into())],
    );
}

pub(crate) fn metric_count_rejected_txs(n_txs: usize) {
    MEMPOOL_TRANSACTIONS_DROPPED.increment(
        n_txs.try_into().expect("The number of rejected_txs should fit u64"),
        &[(LABEL_NAME_DROP_REASON, DropReason::Rejected.into())],
    );
}

pub(crate) fn metric_count_committed_txs(committed_txs: usize) {
    MEMPOOL_TRANSACTIONS_COMMITTED
        .increment(committed_txs.try_into().expect("The number of committed_txs should fit u64"));
}

pub(crate) fn metric_set_get_txs_size(size: usize) {
    MEMPOOL_GET_TXS_SIZE.set_lossy(size);
}

pub(crate) fn register_metrics() {
    // Register Counters.
    MEMPOOL_TRANSACTIONS_COMMITTED.register();
    MEMPOOL_TRANSACTIONS_RECEIVED.register();
    MEMPOOL_TRANSACTIONS_DROPPED.register();
    MEMPOOL_EVICTIONS_COUNT.register();
    // Register Gauges.
    MEMPOOL_POOL_SIZE.register();
    MEMPOOL_PRIORITY_QUEUE_SIZE.register();
    MEMPOOL_PENDING_QUEUE_SIZE.register();
    MEMPOOL_GET_TXS_SIZE.register();
    MEMPOOL_DELAYED_DECLARES_SIZE.register();
    MEMPOOL_TOTAL_SIZE_BYTES.register();
    // Register Histograms.
    TRANSACTION_TIME_SPENT_IN_MEMPOOL.register();
    TRANSACTION_TIME_SPENT_UNTIL_COMMITTED.register();
}
