use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use mockito::{Matcher, Server};
use starknet_api::invoke_tx_args;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::test_utils::invoke::rpc_invoke_tx;
use starknet_api::transaction::fields::{AllResourceBounds, ValidResourceBounds};
use url::Url;

use super::{BlockingCheckClient, BlockingCheckResult};

fn test_transaction() -> RpcTransaction {
    rpc_invoke_tx(invoke_tx_args!(
        resource_bounds: ValidResourceBounds::AllResources(AllResourceBounds::default())
    ))
}

fn client_for_server(server: &Server) -> BlockingCheckClient {
    let url = Url::parse(&server.url()).unwrap();
    BlockingCheckClient::new(url, 0, true)
}

#[tokio::test]
async fn test_success_response_returns_allowed() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", "/")
        .match_body(Matcher::PartialJsonString(
            r#"{"method":"starknet_checkTransaction"}"#.to_string(),
        ))
        .with_status(200)
        .with_body(r#"{"jsonrpc":"2.0","result":{},"id":1}"#)
        .create_async()
        .await;

    let client = client_for_server(&server);
    let result = client.check_transaction(&BlockId::Latest, &test_transaction()).await;

    assert_eq!(result, BlockingCheckResult::Allowed);
    mock.assert_async().await;
}

#[tokio::test]
async fn test_error_10000_returns_blocked() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", "/")
        .with_status(200)
        .with_body(r#"{"jsonrpc":"2.0","error":{"code":10000,"message":"Blocked"},"id":1}"#)
        .create_async()
        .await;

    let client = client_for_server(&server);
    let result = client.check_transaction(&BlockId::Latest, &test_transaction()).await;

    assert_eq!(result, BlockingCheckResult::Blocked);
    mock.assert_async().await;
}

#[tokio::test]
async fn test_other_error_code_returns_inconclusive() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", "/")
        .with_status(200)
        .with_body(r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"Internal error"},"id":1}"#)
        .create_async()
        .await;

    let client = client_for_server(&server);
    let result = client.check_transaction(&BlockId::Latest, &test_transaction()).await;

    assert_eq!(result, BlockingCheckResult::Inconclusive);
    mock.assert_async().await;
}

#[tokio::test]
async fn test_network_error_returns_inconclusive() {
    // Point at a URL that will refuse connections.
    let url = Url::parse("http://127.0.0.1:1").unwrap();
    let client = BlockingCheckClient::new(url, 0, true);

    let result = client.check_transaction(&BlockId::Latest, &test_transaction()).await;

    assert_eq!(result, BlockingCheckResult::Inconclusive);
}

#[tokio::test]
async fn test_malformed_response_returns_inconclusive() {
    let mut server = Server::new_async().await;
    let mock = server.mock("POST", "/").with_status(200).with_body("not json").create_async().await;

    let client = client_for_server(&server);
    let result = client.check_transaction(&BlockId::Latest, &test_transaction()).await;

    assert_eq!(result, BlockingCheckResult::Inconclusive);
    mock.assert_async().await;
}
