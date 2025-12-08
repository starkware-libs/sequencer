use std::net::SocketAddr;

use async_trait::async_trait;
use axum::body::{Body, Bytes, HttpBody};
use axum::http::{Request, StatusCode};
use axum::response::Response;
use pretty_assertions::assert_eq;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHeader, BlockNumber};
use tower::ServiceExt;

use crate::header::{HeaderStorageReader, HeaderStorageWriter};
use crate::storage_reader_server::{ServerConfig, StorageReaderServer, StorageReaderServerHandler};
use crate::test_utils::get_test_storage;
use crate::{StorageError, StorageReader};

const TEST_SERVER_IP: [u8; 4] = [127, 0, 0, 1];

// Test request and response types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestRequest {
    block_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestResponse {
    block_number: u64,
    found: bool,
}

// Mock handler that queries for block headers
#[derive(Clone)]
struct TestHandler;

#[async_trait]
impl StorageReaderServerHandler<TestRequest, TestResponse> for TestHandler {
    async fn handle_request(
        storage_reader: &StorageReader,
        request: TestRequest,
    ) -> Result<TestResponse, StorageError> {
        let block_number = BlockNumber(request.block_number);
        let txn = storage_reader.begin_ro_txn()?;
        let header = txn.get_block_header(block_number)?;
        Ok(TestResponse { block_number: request.block_number, found: header.is_some() })
    }
}

#[derive(Clone)]
struct ErrorHandler;

#[async_trait]
impl StorageReaderServerHandler<TestRequest, TestResponse> for ErrorHandler {
    async fn handle_request(
        _storage_reader: &StorageReader,
        _request: TestRequest,
    ) -> Result<TestResponse, StorageError> {
        Err(StorageError::DBInconsistency { msg: "Test error".to_string() })
    }
}

#[tokio::test]
async fn test_endpoint_successful_query() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();

    // Add a test header at block 0
    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap()
        .commit()
        .unwrap();

    let socket = SocketAddr::from((TEST_SERVER_IP, 8081));
    let config = ServerConfig::new(socket, true);

    let server =
        StorageReaderServer::<TestHandler, TestRequest, TestResponse>::new(reader.clone(), config);
    let app = server.app();

    // Test query for existing block
    let request = TestRequest { block_number: 0 };
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/storage/query")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response).await;
    let test_response: TestResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(test_response.block_number, 0);
    assert!(test_response.found);
}

#[tokio::test]
async fn test_endpoint_query_nonexistent_block() {
    let ((reader, _writer), _temp_dir) = get_test_storage();

    let socket = SocketAddr::from((TEST_SERVER_IP, 8082));
    let config = ServerConfig::new(socket, true);

    let server =
        StorageReaderServer::<TestHandler, TestRequest, TestResponse>::new(reader.clone(), config);
    let app = server.app();

    // Test query for non-existent block
    let request = TestRequest { block_number: 999 };
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/storage/query")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response).await;
    let test_response: TestResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(test_response.block_number, 999);
    assert!(!test_response.found);
}

#[tokio::test]
async fn test_endpoint_handler_error() {
    let ((reader, _writer), _temp_dir) = get_test_storage();

    let socket = SocketAddr::from((TEST_SERVER_IP, TEST_SERVER_BASE_PORT + 3));
    let config = ServerConfig::new(socket, true);

    let server =
        StorageReaderServer::<ErrorHandler, TestRequest, TestResponse>::new(reader.clone(), config);
    let app = server.app();

    let request = TestRequest { block_number: 0 };
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/storage/query")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = to_bytes(response).await;
    let error_message = String::from_utf8(body.to_vec()).unwrap();
    assert!(error_message.contains("Storage error"));
    assert!(error_message.contains("Test error"));
}

#[tokio::test]
async fn test_endpoint_invalid_json() {
    let ((reader, _writer), _temp_dir) = get_test_storage();

    let socket = SocketAddr::from((TEST_SERVER_IP, TEST_SERVER_BASE_PORT + 4));
    let config = ServerConfig::new(socket, true);

    let server =
        StorageReaderServer::<TestHandler, TestRequest, TestResponse>::new(reader.clone(), config);
    let app = server.app();

    // Test with invalid JSON
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/storage/query")
                .header("content-type", "application/json")
                .body(Body::from("invalid json"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return error status code
    assert!(!response.status().is_success());
}

// Helper function to convert response body to bytes
async fn to_bytes(res: Response) -> Bytes {
    res.into_body().collect().await.unwrap().to_bytes()
}
