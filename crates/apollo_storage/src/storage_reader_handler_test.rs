use super::*;
use crate::test_utils::get_test_storage;

#[test]
fn handle_marker_request() {
    let ((reader, _writer), _temp_dir) = get_test_storage();
    let handler = StorageReaderHandler::new(reader);

    let request = StorageReaderRequest::GetMarker(crate::MarkerKind::State);
    let response = handler.handle_request(request).expect("Should successfully handle request");

    match response {
        StorageReaderResponse::GetMarker(block_number) => {
            // Default marker should be BlockNumber(0)
            assert_eq!(block_number, starknet_api::block::BlockNumber(0));
        }
        _ => panic!("Expected GetMarker response"),
    }
}

// TODO(Dean): Add comprehensive tests for all 26 request types.
