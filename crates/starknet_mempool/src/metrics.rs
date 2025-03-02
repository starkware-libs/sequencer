use starknet_api::rpc_transaction::{
    InternalRpcTransactionLabelValue,
    InternalRpcTransactionWithoutTxHash,
};
use starknet_sequencer_metrics::define_metrics;
use starknet_sequencer_metrics::metrics::{
    LabeledMetricCounter,
    MetricCounter,
    MetricGauge,
    MetricScope,
};
use strum::IntoEnumIterator;

define_metrics!(
    Mempool => {
        MetricCounter { MEMPOOL_TRANSACTIONS_COMMITTED, "mempool_txs_committed", "The number of transactions that were committed to block", init = 0 },
        LabeledMetricCounter { MEMPOOL_TRANSACTIONS_RECEIVED, "mempool_transactions_received", "Counter of transactions received by the mempool", init = 0 },
        LabeledMetricCounter { MEMPOOL_TRANSACTIONS_DROPPED, "mempool_transactions_dropped", "Counter of transactions dropped from the mempool", init = 0 },
        MetricGauge { MEMPOOL_POOL_SIZE, "mempool_pool_size", "The size of the mempool's transaction pool" },
        MetricGauge { MEMPOOL_PRIORITY_QUEUE_SIZE, "mempool_priority_queue_size", "The size of the mempool's priority queue" },
        MetricGauge { MEMPOOL_PENDING_QUEUE_SIZE, "mempool_pending_queue_size", "The size of the mempool's pending queue" },
        MetricGauge { MEMPOOL_GET_TXS_SIZE, "mempool_get_txs_size", "The number of transactions returned in the last get_txs() api call" },
        MetricGauge { TRANSACTION_TIME_SPENT_IN_MEMPOOL, "mempool_transaction_time_spent_in_mempool", "The time spent by a transaction in the mempool" },
    },
);

pub(crate) const LABEL_NAME_TX_TYPE: &str = "tx_type";
pub(crate) const LABEL_NAME_DROP_REASON: &str = "drop_reason";
enum TransactionStatus {
    AddedToMempool,
    Dropped,
}

#[derive(strum_macros::IntoStaticStr, strum_macros::EnumIter)]
#[strum(serialize_all = "snake_case")]
pub enum DropReason {
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

#[allow(clippy::as_conversions)] // FIXME: use int metrics so `as f64` may be removed.
pub(crate) fn metric_set_get_txs_size(size: usize) {
    MEMPOOL_GET_TXS_SIZE.set(size as f64);
}

pub struct MempoolStateMetrics {
    pub pool_size: usize,
    pub priority_queue_size: usize,
    pub pending_queue_size: usize,
}

#[allow(clippy::as_conversions)] // FIXME: use int metrics so `as f64` may be removed.
pub(crate) fn update_mempool_state_metrics(state_metrics: MempoolStateMetrics) {
    MEMPOOL_POOL_SIZE.set(state_metrics.pool_size as f64);
    MEMPOOL_PRIORITY_QUEUE_SIZE.set(state_metrics.priority_queue_size as f64);
    MEMPOOL_PENDING_QUEUE_SIZE.set(state_metrics.pending_queue_size as f64);
}

pub(crate) fn register_metrics() {
    // Register Counters.
    MEMPOOL_TRANSACTIONS_COMMITTED.register();

    // Register LabeledCounters
    let mut tx_type_label_variations: Vec<Vec<(&'static str, &'static str)>> = Vec::new();
    for tx_type in InternalRpcTransactionLabelValue::iter() {
        tx_type_label_variations.push(vec![(LABEL_NAME_TX_TYPE, tx_type.into())]);
    }
    MEMPOOL_TRANSACTIONS_RECEIVED.register(&tx_type_label_variations);

    let mut drop_reason_label_variations: Vec<Vec<(&'static str, &'static str)>> = Vec::new();
    for drop_reason in DropReason::iter() {
        drop_reason_label_variations.push(vec![(LABEL_NAME_DROP_REASON, drop_reason.into())]);
    }
    MEMPOOL_TRANSACTIONS_DROPPED.register(&drop_reason_label_variations);

    // Register Gauges.
    MEMPOOL_POOL_SIZE.register();
    MEMPOOL_PRIORITY_QUEUE_SIZE.register();
    MEMPOOL_PENDING_QUEUE_SIZE.register();
    MEMPOOL_GET_TXS_SIZE.register();
    TRANSACTION_TIME_SPENT_IN_MEMPOOL.register();
}
