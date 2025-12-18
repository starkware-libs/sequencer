use super::*;

#[test]
fn request_description() {
    let request = StorageReaderRequest::GetStateDiffLocation(BlockNumber(0));
    let description: &'static str = (&request).into();
    assert_eq!(description, "get_state_diff_location");
}

#[test]
fn serialization_deserialization() {
    // TODO(Dean): Add comprehensive serialization/deserialization tests for all request/response
    // types
}
