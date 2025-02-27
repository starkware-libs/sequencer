use starknet_api::rpc_transaction::{
    InternalRpcTransactionLabelValue,
    InternalRpcTransactionWithoutTxHash,
};
use starknet_sequencer_metrics::metrics::{LabeledMetricCounter, MetricCounter, MetricScope};
use starknet_sequencer_metrics::{define_metrics, generate_permutation_labels};
use strum::{EnumVariantNames, VariantNames};
use strum_macros::{EnumIter, IntoStaticStr};

define_metrics!(
    Mempool => {
        MetricCounter { MEMPOOL_TRANSACTIONS_COMMITTED, "mempool_txs_committed", "The number of transactions that were committed to block", init = 0 },
        LabeledMetricCounter { MEMPOOL_TRANSACTIONS_RECEIVED, "mempool_transactions_received", "Counter of transactions received by the mempool", init = 0, labels = INTERNALRPCTRANSACTIONLABELVALUE_LABELS },
        LabeledMetricCounter { MEMPOOL_TRANSACTIONS_DROPPED, "mempool_transactions_dropped", "Counter of transactions dropped from the mempool", init = 0, labels = DROPREASON_LABELS },
    },
);

pub(crate) const LABEL_NAME_TX_TYPE: &str = "tx_type";
pub(crate) const LABEL_NAME_DROP_REASON: &str = "drop_reason";

generate_permutation_labels! {
    (LABEL_NAME_TX_TYPE, InternalRpcTransactionLabelValue),
}

generate_permutation_labels! {
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

pub(crate) fn register_metrics() {
    MEMPOOL_TRANSACTIONS_COMMITTED.register();
    MEMPOOL_TRANSACTIONS_RECEIVED.register();
    MEMPOOL_TRANSACTIONS_DROPPED.register();
}
