use metrics_exporter_prometheus::PrometheusBuilder;
use starknet_sequencer_metrics::metric_definitions::{
    TRANSACTIONS_FAILED,
    TRANSACTIONS_RECEIVED,
    TRANSACTIONS_SENT_TO_MEMPOOL,
};
use strum::IntoEnumIterator;

use super::{register_metrics, SourceLabelValue, TxTypeLabelValue};
use crate::metrics::{LABEL_NAME_SOURCE, LABEL_NAME_TX_TYPE};

#[test]
fn test_register_metrics() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    register_metrics();
    let metrics = recorder.handle().render();
    for tx_type in TxTypeLabelValue::iter() {
        for source in SourceLabelValue::iter() {
            assert_eq!(
                TRANSACTIONS_RECEIVED
                    .parse_numeric_metric::<u64>(
                        &metrics,
                        &[(LABEL_NAME_TX_TYPE, tx_type.into()), (LABEL_NAME_SOURCE, source.into()),]
                    )
                    .unwrap(),
                0
            );
            assert_eq!(
                TRANSACTIONS_FAILED
                    .parse_numeric_metric::<u64>(
                        &metrics,
                        &[(LABEL_NAME_TX_TYPE, tx_type.into()), (LABEL_NAME_SOURCE, source.into()),]
                    )
                    .unwrap(),
                0
            );
            assert_eq!(
                TRANSACTIONS_SENT_TO_MEMPOOL
                    .parse_numeric_metric::<u64>(
                        &metrics,
                        &[(LABEL_NAME_TX_TYPE, tx_type.into()), (LABEL_NAME_SOURCE, source.into()),]
                    )
                    .unwrap(),
                0
            );
        }
    }
}
