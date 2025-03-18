use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use starknet_api::rpc_transaction::{RpcTransaction, RpcTransactionLabelValue};
use starknet_sequencer_metrics::metrics::{LabeledMetricCounter, MetricHistogram};
use starknet_sequencer_metrics::{define_metrics, generate_permutation_labels};
use strum::{EnumVariantNames, VariantNames};
use strum_macros::IntoStaticStr;

pub const LABEL_NAME_TX_TYPE: &str = "tx_type";
pub const LABEL_NAME_SOURCE: &str = "source";

generate_permutation_labels! {
    TRANSACTION_TYPE_AND_SOURCE_LABELS,
    (LABEL_NAME_TX_TYPE, RpcTransactionLabelValue),
    (LABEL_NAME_SOURCE, SourceLabelValue),
}

define_metrics!(
    Gateway => {
        LabeledMetricCounter { TRANSACTIONS_RECEIVED, "gateway_transactions_received", "Counter of transactions received", init = 0 , labels = TRANSACTION_TYPE_AND_SOURCE_LABELS},
        LabeledMetricCounter { TRANSACTIONS_FAILED, "gateway_transactions_failed", "Counter of failed transactions", init = 0 , labels = TRANSACTION_TYPE_AND_SOURCE_LABELS},
        LabeledMetricCounter { TRANSACTIONS_SENT_TO_MEMPOOL, "gateway_transactions_sent_to_mempool", "Counter of transactions sent to the mempool", init = 0 , labels = TRANSACTION_TYPE_AND_SOURCE_LABELS},
        MetricHistogram { GATEWAY_ADD_TX_LATENCY, "gateway_add_tx_latency", "Latency of gateway add_tx function in secs" },
    },
);

#[derive(Clone, Copy, Debug, IntoStaticStr, EnumVariantNames)]
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
    TRANSACTIONS_RECEIVED.register();
    TRANSACTIONS_FAILED.register();
    TRANSACTIONS_SENT_TO_MEMPOOL.register();
    GATEWAY_ADD_TX_LATENCY.register();
}
