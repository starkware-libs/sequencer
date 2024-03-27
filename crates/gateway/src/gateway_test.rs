use crate::gateway::handle_request;
use hyper::{Body, Request};

#[tokio::test]
async fn test_invalid_request() {
    // Create a sample GET request for an invalid path
    let request = Request::get("/some_invalid_path")
        .body(Body::empty())
        .unwrap();
    let response = handle_request(request).await.unwrap();

    assert_eq!(response.status(), 404);
    assert_eq!(
        String::from_utf8_lossy(&hyper::body::to_bytes(response.into_body()).await.unwrap()),
        "Not found."
    );
}
