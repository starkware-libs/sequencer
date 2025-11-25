use std::sync::Arc;

use apollo_class_manager_types::MockClassManagerClient;
use metrics_exporter_prometheus::PrometheusBuilder;
use reqwest::StatusCode;
use rstest::rstest;
use starknet_api::block::{BlockInfo, BlockNumber};
use url::Url;

use super::{CendeAmbassador, RECORDER_WRITE_BLOB_PATH};
use crate::cende::{
    BlobParameters,
    CendeConfig,
    CendeContext,
    GetLatestBlobResponse,
    RECORDER_GET_LATEST_BLOB_PATH,
};
use crate::metrics::{
    register_metrics,
    CendeWritePrevHeightFailureReason,
    CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER,
    CENDE_WRITE_BLOB_FAILURE,
    CENDE_WRITE_BLOB_SUCCESS,
    LABEL_CENDE_FAILURE_REASON,
};

const HEIGHT_TO_WRITE: BlockNumber = BlockNumber(10);

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
    failure_communication_error: usize,
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

    fn skip_write_height() -> Self {
        Self { failure_skip_write_height: 1, ..Default::default() }
    }

    fn communication_error() -> Self {
        Self { failure_communication_error: 1, ..Default::default() }
    }

    fn verify_metrics(&self, metrics: &str) {
        CENDE_WRITE_BLOB_FAILURE.assert_eq(
            metrics,
            self.failure_skip_write_height,
            &[(
                LABEL_CENDE_FAILURE_REASON,
                CendeWritePrevHeightFailureReason::SkipWriteHeight.into(),
            )],
        );
        CENDE_WRITE_BLOB_FAILURE.assert_eq(
            metrics,
            self.failure_no_prev_blob,
            &[(
                LABEL_CENDE_FAILURE_REASON,
                CendeWritePrevHeightFailureReason::BlobNotAvailable.into(),
            )],
        );
        CENDE_WRITE_BLOB_FAILURE.assert_eq(
            metrics,
            self.failure_block_height_mismatch,
            &[(
                LABEL_CENDE_FAILURE_REASON,
                CendeWritePrevHeightFailureReason::HeightMismatch.into(),
            )],
        );
        CENDE_WRITE_BLOB_FAILURE.assert_eq(
            metrics,
            self.failure_recorder_error,
            &[(
                LABEL_CENDE_FAILURE_REASON,
                CendeWritePrevHeightFailureReason::CendeRecorderError.into(),
            )],
        );
        CENDE_WRITE_BLOB_FAILURE.assert_eq(
            metrics,
            self.failure_communication_error,
            &[(
                LABEL_CENDE_FAILURE_REASON,
                CendeWritePrevHeightFailureReason::CommunicationError.into(),
            )],
        );
        CENDE_WRITE_BLOB_SUCCESS.assert_eq(metrics, self.success);
    }
}

#[rstest]
#[case::success(200, HEIGHT_TO_WRITE, Some(9), 1, true, ExpectedMetrics::success())]
#[case::height_zero(200, BlockNumber(0), None, 0, true, ExpectedMetrics::skip_write_height())]
#[case::prev_block_height_mismatch(
    200,
    HEIGHT_TO_WRITE,
    Some(7),
    0,
    false,
    ExpectedMetrics::height_mismatch()
)]
#[case::recorder_return_fatal_error(
    400,
    HEIGHT_TO_WRITE,
    Some(9),
    1,
    false,
    ExpectedMetrics::recorder_error()
)]
#[tokio::test]
async fn write_prev_height_blob(
    #[case] mock_status_code: usize,
    #[case] current_height: BlockNumber,
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

    let receiver = cende_ambassador.write_prev_height_blob(current_height);

    assert_eq!(receiver.await.unwrap(), expected_result);
    mock.expect(expected_calls).assert();

    expected_metrics.verify_metrics(&recorder.handle().render());
}

#[rstest]
#[case::success_after_multiple_retries(StatusCode::OK, 1, true)]
#[case::failure_after_multiple_retries(StatusCode::TOO_MANY_REQUESTS, 50, false)]
#[tokio::test]
async fn write_prev_height_blob_multiple_retries(
    #[case] final_status_code: StatusCode,
    #[case] expected_retries_max: usize,
    #[case] expected_result: bool,
) {
    let mut server = mockito::Server::new_async().await;
    let url = server.url();

    let mock_error = server
        .mock("POST", RECORDER_WRITE_BLOB_PATH)
        .with_status(StatusCode::TOO_MANY_REQUESTS.as_u16().into())
        .create();
    let mock_final = server
        .mock("POST", RECORDER_WRITE_BLOB_PATH)
        .with_status(final_status_code.as_u16().into())
        .create();

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
async fn write_prev_height_blob_no_prev_blob_and_cende_communication_error() {
    const CURRENT_HEIGHT: BlockNumber = BlockNumber(10);

    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    register_metrics();

    let cende_ambassador = CendeAmbassador::new(
        CendeConfig {
            // We do not create a mock server and we send a bogus URL. This should cause
            // request_builder.send to return an Err.
            recorder_url: Url::parse("https://bogus.url").unwrap(),
            ..Default::default()
        },
        Arc::new(MockClassManagerClient::new()),
    );

    let receiver = cende_ambassador.write_prev_height_blob(CURRENT_HEIGHT);

    assert!(!receiver.await.unwrap());

    ExpectedMetrics::communication_error().verify_metrics(&recorder.handle().render());
}

#[tokio::test]
async fn write_prev_height_blob_no_prev_blob_and_cende_recorder_error() {
    const CURRENT_HEIGHT: BlockNumber = BlockNumber(10);

    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    register_metrics();

    let mut server = mockito::Server::new_async().await;
    let url = server.url();

    let get_latest_blob_mock = server
        .mock("GET", RECORDER_GET_LATEST_BLOB_PATH)
        // Return a non retryable error.
        .with_status(StatusCode::METHOD_NOT_ALLOWED.as_u16().into())
        .create();

    let cende_ambassador = CendeAmbassador::new(
        CendeConfig { recorder_url: url.parse().unwrap(), ..Default::default() },
        Arc::new(MockClassManagerClient::new()),
    );

    let receiver = cende_ambassador.write_prev_height_blob(CURRENT_HEIGHT);

    assert!(!receiver.await.unwrap());
    get_latest_blob_mock.expect(1).assert();

    ExpectedMetrics::recorder_error().verify_metrics(&recorder.handle().render());
}

#[tokio::test]
async fn write_prev_height_blob_no_prev_blob_and_not_written_to_cende() {
    const CURRENT_HEIGHT: BlockNumber = BlockNumber(10);

    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    register_metrics();

    let mut server = mockito::Server::new_async().await;
    let url = server.url();

    let latest_blob_response = GetLatestBlobResponse {
        // The latest blob is at height 2 below current, so we don't have the previous blob.
        height: CURRENT_HEIGHT.prev().unwrap().prev().unwrap(),
        proposal_commitment: "Unused".to_string(),
    };

    let get_latest_blob_mock = server
        .mock("GET", RECORDER_GET_LATEST_BLOB_PATH)
        .with_body(serde_json::to_string(&latest_blob_response).unwrap())
        .create();

    let cende_ambassador = CendeAmbassador::new(
        CendeConfig { recorder_url: url.parse().unwrap(), ..Default::default() },
        Arc::new(MockClassManagerClient::new()),
    );

    let receiver = cende_ambassador.write_prev_height_blob(CURRENT_HEIGHT);

    assert!(!receiver.await.unwrap());
    get_latest_blob_mock.expect(1).assert();

    ExpectedMetrics::no_prev_blob().verify_metrics(&recorder.handle().render());
}

#[tokio::test]
async fn write_prev_height_blob_no_prev_blob_but_it_was_written_to_cende() {
    const CURRENT_HEIGHT: BlockNumber = BlockNumber(10);

    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    register_metrics();

    let mut server = mockito::Server::new_async().await;
    let url = server.url();

    let latest_blob_response = GetLatestBlobResponse {
        // The latest blob is at height 1 below current, so we don't have to write the previous
        // blob.
        height: CURRENT_HEIGHT.prev().unwrap(),
        proposal_commitment: "Unused".to_string(),
    };

    let get_latest_blob_mock = server
        .mock("GET", RECORDER_GET_LATEST_BLOB_PATH)
        .with_body(serde_json::to_string(&latest_blob_response).unwrap())
        .create();

    let cende_ambassador = CendeAmbassador::new(
        CendeConfig { recorder_url: url.parse().unwrap(), ..Default::default() },
        Arc::new(MockClassManagerClient::new()),
    );

    let receiver = cende_ambassador.write_prev_height_blob(CURRENT_HEIGHT);

    assert!(receiver.await.unwrap());
    get_latest_blob_mock.expect(1).assert();

    ExpectedMetrics::skip_write_height().verify_metrics(&recorder.handle().render());
}
