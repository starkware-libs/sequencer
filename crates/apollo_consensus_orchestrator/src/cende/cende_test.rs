use std::sync::Arc;

use apollo_class_manager_types::MockClassManagerClient;
use metrics_exporter_prometheus::PrometheusBuilder;
use rstest::rstest;
use starknet_api::block::{BlockInfo, BlockNumber};

use super::{CendeAmbassador, RECORDER_WRITE_BLOB_PATH};
use crate::cende::{BlobParameters, CendeConfig, CendeContext};
use crate::metrics::{
    register_metrics,
    CendeWriteFailureReason,
    CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER,
    CENDE_WRITE_BLOB_FAILURE,
    CENDE_WRITE_BLOB_SUCCESS,
    LABEL_CENDE_FAILURE_REASON,
};

const HEIGHT_TO_WRITE: BlockNumber = BlockNumber(10);
const TOO_MANY_REQUESTS_STATUS_CODE: usize = 429;

impl BlobParameters {
    fn with_block_number(block_number: BlockNumber) -> Self {
        Self { block_info: BlockInfo { block_number, ..Default::default() }, ..Default::default() }
    }
}

#[derive(Debug, Default)]
struct ExpectedMetrics {
    success: usize,
    failure_no_prev_blob: usize,
    failure_block_height_mismatch: usize,
    failure_recorder_error: usize,
    failure_skip_write_height: usize,
}

impl ExpectedMetrics {
    fn success() -> Self {
        Self { success: 1, ..Default::default() }
    }

    fn no_prev_blob() -> Self {
        Self { failure_no_prev_blob: 1, ..Default::default() }
    }

    fn height_mismatch() -> Self {
        Self { failure_block_height_mismatch: 1, ..Default::default() }
    }

    fn recorder_error() -> Self {
        Self { failure_recorder_error: 1, ..Default::default() }
    }

    fn verify_metrics(&self, metrics: &str) {
        CENDE_WRITE_BLOB_FAILURE.assert_eq(
            metrics,
            self.failure_skip_write_height,
            &[(LABEL_CENDE_FAILURE_REASON, CendeWriteFailureReason::SkipWriteHeight.into())],
        );
        CENDE_WRITE_BLOB_FAILURE.assert_eq(
            metrics,
            self.failure_no_prev_blob,
            &[(LABEL_CENDE_FAILURE_REASON, CendeWriteFailureReason::BlobNotAvailable.into())],
        );
        CENDE_WRITE_BLOB_FAILURE.assert_eq(
            metrics,
            self.failure_block_height_mismatch,
            &[(LABEL_CENDE_FAILURE_REASON, CendeWriteFailureReason::HeightMismatch.into())],
        );
        CENDE_WRITE_BLOB_FAILURE.assert_eq(
            metrics,
            self.failure_recorder_error,
            &[(LABEL_CENDE_FAILURE_REASON, CendeWriteFailureReason::CendeRecorderError.into())],
        );
        CENDE_WRITE_BLOB_SUCCESS.assert_eq(metrics, self.success);
    }
}

#[rstest]
#[case::success(200, Some(9), 1, true, ExpectedMetrics::success())]
#[case::no_prev_block(200, None, 0, false, ExpectedMetrics::no_prev_blob())]
#[case::prev_block_height_mismatch(200, Some(7), 0, false, ExpectedMetrics::height_mismatch())]
#[case::recorder_return_fatal_error(400, Some(9), 1, false, ExpectedMetrics::recorder_error())]
#[tokio::test]
async fn write_prev_height_blob(
    #[case] mock_status_code: usize,
    #[case] prev_block: Option<u64>,
    #[case] expected_calls: usize,
    #[case] expected_result: bool,
    #[case] expected_metrics: ExpectedMetrics,
) {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    register_metrics();

    let mut server = mockito::Server::new_async().await;
    let url = server.url();
    let mock = server.mock("POST", RECORDER_WRITE_BLOB_PATH).with_status(mock_status_code).create();

    let cende_ambassador = CendeAmbassador::new(
        CendeConfig { recorder_url: url.parse().unwrap(), ..Default::default() },
        Arc::new(MockClassManagerClient::new()),
    );

    if let Some(prev_block) = prev_block {
        cende_ambassador
            .prepare_blob_for_next_height(BlobParameters::with_block_number(BlockNumber(
                prev_block,
            )))
            .await
            .unwrap();
    }

    let receiver = cende_ambassador.write_prev_height_blob(HEIGHT_TO_WRITE);

    assert_eq!(receiver.await.unwrap(), expected_result);
    mock.expect(expected_calls).assert();

    expected_metrics.verify_metrics(&recorder.handle().render());
}

#[rstest]
#[case::success_after_multiple_retries(200, 1, true)]
#[case::failure_after_multiple_retries(TOO_MANY_REQUESTS_STATUS_CODE, 50, false)]
#[tokio::test]
async fn write_prev_height_blob_multiple_retries(
    #[case] final_status_code: usize,
    #[case] expected_retries_max: usize,
    #[case] expected_result: bool,
) {
    let mut server = mockito::Server::new_async().await;
    let url = server.url();

    let mock_error = server
        .mock("POST", RECORDER_WRITE_BLOB_PATH)
        .with_status(TOO_MANY_REQUESTS_STATUS_CODE)
        .create();
    let mock_final =
        server.mock("POST", RECORDER_WRITE_BLOB_PATH).with_status(final_status_code).create();

    let cende_ambassador = CendeAmbassador::new(
        CendeConfig { recorder_url: url.parse().unwrap(), ..Default::default() },
        Arc::new(MockClassManagerClient::new()),
    );

    cende_ambassador
        .prepare_blob_for_next_height(BlobParameters::with_block_number(
            HEIGHT_TO_WRITE.prev().unwrap(),
        ))
        .await
        .unwrap();

    let receiver = cende_ambassador.write_prev_height_blob(HEIGHT_TO_WRITE);

    assert_eq!(receiver.await.unwrap(), expected_result);
    mock_error.expect(1).assert();
    mock_final.expect_at_least(1).expect_at_most(expected_retries_max).assert();
}

#[tokio::test]
async fn prepare_blob_for_next_height() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    register_metrics();

    let cende_ambassador =
        CendeAmbassador::new(CendeConfig::default(), Arc::new(MockClassManagerClient::new()));

    cende_ambassador
        .prepare_blob_for_next_height(BlobParameters::with_block_number(HEIGHT_TO_WRITE))
        .await
        .unwrap();
    assert_eq!(
        cende_ambassador.prev_height_blob.lock().await.as_ref().unwrap().block_number,
        HEIGHT_TO_WRITE
    );

    CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER.assert_eq(&recorder.handle().render(), HEIGHT_TO_WRITE.0);
}

#[tokio::test]
async fn no_write_at_skipped_height() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    register_metrics();

    const SKIP_WRITE_HEIGHT: BlockNumber = HEIGHT_TO_WRITE;
    let cende_ambassador = CendeAmbassador::new(
        CendeConfig { skip_write_height: Some(SKIP_WRITE_HEIGHT), ..Default::default() },
        Arc::new(MockClassManagerClient::new()),
    );

    // Returns false since the blob is missing and the height is different than skip_write_height.
    assert!(
        !cende_ambassador.write_prev_height_blob(HEIGHT_TO_WRITE.unchecked_next()).await.unwrap()
    );

    assert!(cende_ambassador.write_prev_height_blob(HEIGHT_TO_WRITE).await.unwrap());

    // Verify metrics.
    let expected_metrics = ExpectedMetrics {
        failure_no_prev_blob: 1,
        failure_skip_write_height: 1,
        ..Default::default()
    };
    expected_metrics.verify_metrics(&recorder.handle().render());
}
