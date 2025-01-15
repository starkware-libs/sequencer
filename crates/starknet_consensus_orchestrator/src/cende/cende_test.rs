use rstest::rstest;
use starknet_api::block::{BlockInfo, BlockNumber};

use super::{CendeAmbassador, RECORDER_WRITE_BLOB_PATH};
use crate::cende::{BlobParameters, CendeConfig, CendeContext};

const HEIGHT_TO_WRITE: u64 = 10;

impl BlobParameters {
    fn with_block_number(block_number: u64) -> Self {
        Self {
            block_info: BlockInfo { block_number: BlockNumber(block_number), ..Default::default() },
            ..Default::default()
        }
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

    let cende_ambassador = CendeAmbassador::new(CendeConfig { recorder_url: url.parse().unwrap() });

    if let Some(prev_block) = prev_block {
        cende_ambassador
            .prepare_blob_for_next_height(BlobParameters::with_block_number(prev_block))
            .await;
    }

    let receiver = cende_ambassador.write_prev_height_blob(BlockNumber(HEIGHT_TO_WRITE));

    assert_eq!(receiver.await.unwrap(), expected_result);
    mock.expect(expected_calls).assert();
}

#[tokio::test]
async fn prepare_blob_for_next_height() {
    let cende_ambassador =
        CendeAmbassador::new(CendeConfig { recorder_url: "http://parsable_url".parse().unwrap() });

    cende_ambassador
        .prepare_blob_for_next_height(BlobParameters::with_block_number(HEIGHT_TO_WRITE))
        .await;
    assert_eq!(
        cende_ambassador.prev_height_blob.lock().await.as_ref().unwrap().block_number.0,
        HEIGHT_TO_WRITE
    );
}
