use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_sequencer_metrics::metric_definitions::{
    TRANSACTIONS_FAILED,
    TRANSACTIONS_RECEIVED,
    TRANSACTIONS_SENT_TO_MEMPOOL,
};
#[cfg(test)]
use starknet_sequencer_metrics::metrics::LabeledMetricCounter;
use strum::IntoEnumIterator;

#[cfg(test)]
#[path = "metrics_test.rs"]
pub mod metrics_test;

const LABEL_NAME_TX_TYPE: &str = "tx_type";
const LABEL_NAME_SOURCE: &str = "source";

#[derive(Clone, Copy, Debug, strum_macros::IntoStaticStr, strum_macros::EnumIter)]
enum TxTypeLabelValue {
    #[strum(serialize = "invoke")]
    Invoke,
    #[strum(serialize = "declare")]
    Declare,
    #[strum(serialize = "deploy_account")]
    DeployAccount,
}

impl From<&RpcTransaction> for TxTypeLabelValue {
    fn from(tx: &RpcTransaction) -> Self {
        match tx {
            RpcTransaction::Invoke(_) => TxTypeLabelValue::Invoke,
            RpcTransaction::Declare(_) => TxTypeLabelValue::Declare,
            RpcTransaction::DeployAccount(_) => TxTypeLabelValue::DeployAccount,
        }
    }
}

#[derive(Clone, Copy, Debug, strum_macros::IntoStaticStr, strum_macros::EnumIter)]
enum SourceLabelValue {
    #[strum(serialize = "http")]
    Http,
    #[strum(serialize = "p2p")]
    P2p,
}

pub(crate) struct GatewayMetricCounters {
    tx_type: TxTypeLabelValue,
    source: SourceLabelValue,
}

impl GatewayMetricCounters {
    pub fn new(
        tx: &RpcTransaction,
        p2p_message_metadata: &Option<BroadcastedMessageMetadata>,
    ) -> Self {
        let tx_type = TxTypeLabelValue::from(tx);
        let source = match p2p_message_metadata {
            Some(_) => SourceLabelValue::P2p,
            None => SourceLabelValue::Http,
        };
        Self { tx_type, source }
    }

    fn label(&self) -> Vec<(&'static str, &'static str)> {
        vec![(LABEL_NAME_TX_TYPE, self.tx_type.into()), (LABEL_NAME_SOURCE, self.source.into())]
    }

    pub fn count_transaction_received(&self) {
        TRANSACTIONS_RECEIVED.increment(1, &self.label());
    }

    pub fn count_transaction_failed(&self) {
        TRANSACTIONS_FAILED.increment(1, &self.label());
    }

    pub fn count_transaction_sent_to_mempool(&self) {
        TRANSACTIONS_SENT_TO_MEMPOOL.increment(1, &self.label());
    }

    #[cfg(test)]
    pub fn get_metric_value(&self, metric_counter: LabeledMetricCounter, metrics: &str) -> u64 {
        metric_counter.parse_numeric_metric::<u64>(metrics, &self.label()).unwrap()
    }
}

pub(crate) fn register_metrics() {
    let mut label_variations: Vec<Vec<(&'static str, &'static str)>> = Vec::new();
    for tx_type in TxTypeLabelValue::iter() {
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
    println!("Metrics registered");
}
