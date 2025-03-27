use std::sync::Arc;

use apollo_class_manager_types::MockClassManagerClient;
use rstest::rstest;
use starknet_api::block::{BlockInfo, BlockNumber};

use super::{CendeAmbassador, RECORDER_WRITE_BLOB_PATH};
use crate::cende::{BlobParameters, CendeConfig, CendeContext};

const HEIGHT_TO_WRITE: BlockNumber = BlockNumber(10);

impl BlobParameters {
    fn with_block_number(block_number: BlockNumber) -> Self {
        Self { block_info: BlockInfo { block_number, ..Default::default() }, ..Default::default() }
    }
}

#[rstest]
#[case::success(200, Some(9), 1, true)]
#[case::no_prev_block(200, None, 0, false)]
#[case::prev_block_height_mismatch(200, Some(7), 0, false)]
#[case::recorder_return_error(500, Some(9), 1, false)]
#[tokio::test]
async fn write_prev_height_blob(
    #[case] mock_status_code: usize,
    #[case] prev_block: Option<u64>,
    #[case] expected_calls: usize,
    #[case] expected_result: bool,
) {
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
}

#[tokio::test]
async fn prepare_blob_for_next_height() {
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
}

#[tokio::test]
async fn no_write_at_skipped_height() {
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
}
