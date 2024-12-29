use rstest::rstest;
use starknet_api::block::BlockNumber;

use super::{CendeAmbassador, RECORDER_WRITE_BLOB_PATH};
use crate::cende::{BlobParameters, CendeConfig, CendeContext};

#[rstest]
#[case::success(RECORDER_WRITE_BLOB_PATH, 200, 10, Some(10), 1, true)]
#[case::none_prev_block(RECORDER_WRITE_BLOB_PATH, 200, 10, None, 0, false)]
#[case::mismatch_heihgt_prev_block(RECORDER_WRITE_BLOB_PATH, 200, 10, Some(7), 0, false)]
#[case::send_recorder_fail("different_recorder_path", 200, 10, Some(10), 0, false)]
#[case::recorder_return_error(RECORDER_WRITE_BLOB_PATH, 500, 10, Some(10), 1, false)]
#[tokio::test]
async fn write_prev_height_blob(
    #[case] path: &str,
    #[case] status_code: usize,
    #[case] written_height: u64,
    #[case] prev_block: Option<u64>,
    #[case] expected_calls: usize,
    #[case] result: bool,
) {
    let mut server = mockito::Server::new_async().await;
    let url = server.url();
    let mock = server.mock("POST", path).with_status(status_code).create();

    let cende_ambassador = CendeAmbassador::new(CendeConfig { recorder_url: url.parse().unwrap() });

    if let Some(prev_block) = prev_block {
        cende_ambassador
            .prepare_blob_for_next_height(BlobParameters { height: BlockNumber(prev_block) })
            .await;
    }

    let reciver = cende_ambassador.write_prev_height_blob(BlockNumber(written_height));

    assert_eq!(reciver.await.unwrap(), result);
    mock.expect(expected_calls);
}

#[tokio::test]
async fn prepare_blob_for_next_height() {
    let cende_ambassador =
        CendeAmbassador::new(CendeConfig { recorder_url: "http://parsable_url".parse().unwrap() });

    const WRITTEN_HEIGHT: BlockNumber = BlockNumber(1701);

    cende_ambassador.prepare_blob_for_next_height(BlobParameters { height: WRITTEN_HEIGHT }).await;

    assert_eq!(
        cende_ambassador.prev_height_blob.lock().await.as_ref().unwrap().block_number,
        WRITTEN_HEIGHT
    );
}
