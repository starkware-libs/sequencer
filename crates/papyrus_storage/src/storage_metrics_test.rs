use metrics_exporter_prometheus::PrometheusBuilder;
use papyrus_test_utils::prometheus_is_contained;
use prometheus_parse::Value::{Counter, Gauge};

use super::update_storage_metrics;
use crate::test_utils::get_test_storage;

#[test]
fn update_storage_metrics_test() {
    let ((reader, _writer), _temp_dir) = get_test_storage();
    let handle = PrometheusBuilder::new().install_recorder().unwrap();

    assert!(prometheus_is_contained(handle.render(), "storage_free_pages_number", &[]).is_none());
    assert!(prometheus_is_contained(handle.render(), "storage_last_page_number", &[]).is_none());
    assert!(
        prometheus_is_contained(handle.render(), "storage_last_transaction_index", &[]).is_none()
    );

    update_storage_metrics(&reader).unwrap();

    let Gauge(free_pages) =
        prometheus_is_contained(handle.render(), "storage_free_pages_number", &[]).unwrap()
    else {
        panic!("storage_free_pages_number is not a Gauge")
    };
    // TODO(dvir): add an upper limit when the bug in the binding freelist function will be fixed.
    assert!(0f64 < free_pages);

    let Counter(last_page) =
        prometheus_is_contained(handle.render(), "storage_last_page_number", &[]).unwrap()
    else {
        panic!("storage_last_page_number is not a Counter")
    };
    assert!(0f64 < last_page);
    assert!(last_page < 1000f64);

    let Counter(last_transaction) =
        prometheus_is_contained(handle.render(), "storage_last_transaction_index", &[]).unwrap()
    else {
        panic!("storage_last_transaction_index is not a Counter")
    };
    assert!(0f64 < last_transaction);
    assert!(last_transaction < 100f64);
}
