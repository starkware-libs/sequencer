use super::*;
use crate::test_utils::get_test_storage;
use starknet_api::block::BlockNumber;

#[test]
fn handle_marker_request() {
    let ((reader, _writer), _temp_dir) = get_test_storage();
    let handler = StorageReaderHandler::new(reader);

    let request = StorageReaderRequest::GetMarker(crate::MarkerKind::State);
    let response = handler.handle_request(request);

    match response {
        StorageReaderResponse::GetMarker(result) => {
            assert!(result.is_ok(), "Should successfully get marker");
        }
        _ => panic!("Expected GetMarker response"),
    }
}

// TODO(Dean): Add comprehensive tests for all 26 request types.

