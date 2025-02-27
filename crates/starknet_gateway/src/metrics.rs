use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use starknet_api::rpc_transaction::{RpcTransaction, RpcTransactionLabelValue};
use starknet_sequencer_metrics::define_metrics;
use starknet_sequencer_metrics::metrics::{LabeledMetricCounter, MetricScope};
use strum::IntoEnumIterator;

define_metrics!(
    Gateway => {
        LabeledMetricCounter { TRANSACTIONS_RECEIVED, "gateway_transactions_received", "Counter of transactions received", 0 },
        LabeledMetricCounter { TRANSACTIONS_FAILED, "gateway_transactions_failed", "Counter of failed transactions", 0 },
        LabeledMetricCounter { TRANSACTIONS_SENT_TO_MEMPOOL, "gateway_transactions_sent_to_mempool", "Counter of transactions sent to the mempool", 0 },
    },
);

pub const LABEL_NAME_TX_TYPE: &str = "tx_type";
pub const LABEL_NAME_SOURCE: &str = "source";

#[derive(Clone, Copy, Debug, strum_macros::IntoStaticStr, strum_macros::EnumIter)]
#[strum(serialize_all = "snake_case")]
pub enum SourceLabelValue {
    Http,
    P2p,
}

enum TransactionStatus {
    SentToMempool,
    Failed,
}

pub(crate) struct GatewayMetricHandle {
    tx_type: RpcTransactionLabelValue,
    source: SourceLabelValue,
    tx_status: TransactionStatus,
}

impl GatewayMetricHandle {
    pub fn new(
        tx: &RpcTransaction,
        p2p_message_metadata: &Option<BroadcastedMessageMetadata>,
    ) -> Self {
        let tx_type = RpcTransactionLabelValue::from(tx);
        let source = match p2p_message_metadata {
            Some(_) => SourceLabelValue::P2p,
            None => SourceLabelValue::Http,
        };
        Self { tx_type, source, tx_status: TransactionStatus::Failed }
    }

    fn label(&self) -> Vec<(&'static str, &'static str)> {
        vec![(LABEL_NAME_TX_TYPE, self.tx_type.into()), (LABEL_NAME_SOURCE, self.source.into())]
    }

    pub fn count_transaction_received(&self) {
        TRANSACTIONS_RECEIVED.increment(1, &self.label());
    }

    pub fn transaction_sent_to_mempool(&mut self) {
        self.tx_status = TransactionStatus::SentToMempool;
    }

    #[cfg(test)]
    pub fn get_metric_value(&self, metric_counter: LabeledMetricCounter, metrics: &str) -> u64 {
        metric_counter.parse_numeric_metric::<u64>(metrics, &self.label()).unwrap()
    }
}

impl Drop for GatewayMetricHandle {
    fn drop(&mut self) {
        match self.tx_status {
            TransactionStatus::SentToMempool => {
                TRANSACTIONS_SENT_TO_MEMPOOL.increment(1, &self.label())
            }
            TransactionStatus::Failed => TRANSACTIONS_FAILED.increment(1, &self.label()),
        }
    }
}

pub(crate) fn register_metrics() {
    let mut label_variations: Vec<Vec<(&'static str, &'static str)>> = Vec::new();
    for tx_type in RpcTransactionLabelValue::iter() {
        for source in SourceLabelValue::iter() {
            label_variations.push(vec![
                (LABEL_NAME_TX_TYPE, tx_type.into()),
                (LABEL_NAME_SOURCE, source.into()),
            ]);
        }
    }
    TRANSACTIONS_RECEIVED.register(&label_variations);
    TRANSACTIONS_FAILED.register(&label_variations);
    TRANSACTIONS_SENT_TO_MEMPOOL.register(&label_variations);
}
