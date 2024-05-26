use std::net::SocketAddr;

use axum::body::{Body, HttpBody};
use axum::http::{Request, StatusCode};
use hyper::client::HttpConnector;
use hyper::{Client, Response};
use starknet_api::external_transaction::ExternalTransaction;

use crate::errors::GatewayError;
use crate::starknet_api_test_utils::external_invoke_tx_to_json;

pub type GatewayResult<T> = Result<T, GatewayError>;

/// A test utility client for interacting with a gateway server.
pub struct GatewayClient {
    socket: SocketAddr,
    client: Client<HttpConnector>,
}
impl GatewayClient {
    pub fn new(socket: SocketAddr) -> Self {
        let client = Client::new();
        Self { socket, client }
    }

    pub async fn add_tx(&self, tx: ExternalTransaction) -> GatewayResult<String> {
        let tx_json = external_invoke_tx_to_json(tx);
        let request = Request::builder()
            .method("POST")
            .uri(format!("http://{}", self.socket) + "/add_tx")
            .header("content-type", "application/json")
            .body(Body::from(tx_json))?;

        // Send a POST request with the transaction data as the body
        let response: Response<Body> = self.client.request(request).await?;
        assert_eq!(response.status(), StatusCode::OK);

        let response_string =
            String::from_utf8(response.into_body().collect().await?.to_bytes().to_vec())?;
        Ok(response_string)
    }
}
