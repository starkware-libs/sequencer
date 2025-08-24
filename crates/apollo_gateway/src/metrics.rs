use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
    GATEWAY_LOCAL_MSGS_PROCESSED,
    GATEWAY_LOCAL_MSGS_RECEIVED,
    GATEWAY_LOCAL_QUEUE_DEPTH,
    GATEWAY_REMOTE_CLIENT_SEND_ATTEMPTS,
    GATEWAY_REMOTE_MSGS_PROCESSED,
    GATEWAY_REMOTE_MSGS_RECEIVED,
    GATEWAY_REMOTE_NUMBER_OF_CONNECTIONS,
    GATEWAY_REMOTE_VALID_MSGS_RECEIVED,
};
#[cfg(test)]
use apollo_metrics::metrics::LabeledMetricCounter;
use apollo_metrics::{define_metrics, generate_permutation_labels};
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use starknet_api::rpc_transaction::{RpcTransaction, RpcTransactionLabelValue};
use strum::{EnumVariantNames, VariantNames};
use strum_macros::IntoStaticStr;

use crate::communication::GATEWAY_REQUEST_LABELS;

pub const LABEL_NAME_TX_TYPE: &str = "tx_type";
pub const LABEL_NAME_SOURCE: &str = "source";

generate_permutation_labels! {
    TRANSACTION_TYPE_AND_SOURCE_LABELS,
    (LABEL_NAME_TX_TYPE, RpcTransactionLabelValue),
    (LABEL_NAME_SOURCE, SourceLabelValue),
}

define_metrics!(
    Gateway => {
        LabeledMetricCounter { GATEWAY_TRANSACTIONS_RECEIVED, "gateway_transactions_received", "Counter of transactions received", init = 0 , labels = TRANSACTION_TYPE_AND_SOURCE_LABELS},
        LabeledMetricCounter { GATEWAY_TRANSACTIONS_FAILED, "gateway_transactions_failed", "Counter of failed transactions", init = 0 , labels = TRANSACTION_TYPE_AND_SOURCE_LABELS},
        LabeledMetricCounter { GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL, "gateway_transactions_sent_to_mempool", "Counter of transactions sent to the mempool", init = 0 , labels = TRANSACTION_TYPE_AND_SOURCE_LABELS},
        MetricHistogram { GATEWAY_ADD_TX_LATENCY, "gateway_add_tx_latency", "Latency of gateway add_tx function in secs" },
        MetricHistogram { GATEWAY_VALIDATE_TX_LATENCY, "gateway_validate_tx_latency", "Latency of gateway validate function in secs" },
    },
    Infra => {
        LabeledMetricHistogram {
            GATEWAY_LABELED_PROCESSING_TIMES_SECS,
            "gateway_labeled_processing_times_secs",
            "Request processing times of the gateway, per label (secs)",
            labels = GATEWAY_REQUEST_LABELS
        },
        LabeledMetricHistogram {
            GATEWAY_LABELED_QUEUEING_TIMES_SECS,
            "gateway_labeled_queueing_times_secs",
            "Request queueing times of the gateway, per label (secs)",
            labels = GATEWAY_REQUEST_LABELS
        },
        LabeledMetricHistogram {
            GATEWAY_LABELED_LOCAL_RESPONSE_TIMES_SECS,
            "gateway_labeled_local_response_times_secs",
            "Request local response times of the gateway, per label (secs)",
            labels = GATEWAY_REQUEST_LABELS
        },
        LabeledMetricHistogram {
            GATEWAY_LABELED_REMOTE_RESPONSE_TIMES_SECS,
            "gateway_labeled_remote_response_times_secs",
            "Request remote response times of the gateway, per label (secs)",
            labels = GATEWAY_REQUEST_LABELS
        },
        LabeledMetricHistogram {
            GATEWAY_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
            "gateway_labeled_remote_client_communication_failure_times_secs",
            "Request communication failure times of the gateway, per label (secs)",
            labels = GATEWAY_REQUEST_LABELS
        },
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
        GATEWAY_TRANSACTIONS_RECEIVED.increment(1, &self.label());
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
                GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL.increment(1, &self.label())
            }
            TransactionStatus::Failed => GATEWAY_TRANSACTIONS_FAILED.increment(1, &self.label()),
        }
    }
}

pub(crate) fn register_metrics() {
    GATEWAY_TRANSACTIONS_RECEIVED.register();
    GATEWAY_TRANSACTIONS_FAILED.register();
    GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL.register();
    GATEWAY_ADD_TX_LATENCY.register();
    GATEWAY_VALIDATE_TX_LATENCY.register();
}

pub const GATEWAY_INFRA_METRICS: InfraMetrics = InfraMetrics::new(
    LocalClientMetrics::new(&GATEWAY_LABELED_LOCAL_RESPONSE_TIMES_SECS),
    RemoteClientMetrics::new(
        &GATEWAY_REMOTE_CLIENT_SEND_ATTEMPTS,
        &GATEWAY_LABELED_REMOTE_RESPONSE_TIMES_SECS,
        &GATEWAY_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
    ),
    LocalServerMetrics::new(
        &GATEWAY_LOCAL_MSGS_RECEIVED,
        &GATEWAY_LOCAL_MSGS_PROCESSED,
        &GATEWAY_LOCAL_QUEUE_DEPTH,
        &GATEWAY_LABELED_PROCESSING_TIMES_SECS,
        &GATEWAY_LABELED_QUEUEING_TIMES_SECS,
    ),
    RemoteServerMetrics::new(
        &GATEWAY_REMOTE_MSGS_RECEIVED,
        &GATEWAY_REMOTE_VALID_MSGS_RECEIVED,
        &GATEWAY_REMOTE_MSGS_PROCESSED,
        &GATEWAY_REMOTE_NUMBER_OF_CONNECTIONS,
    ),
);
