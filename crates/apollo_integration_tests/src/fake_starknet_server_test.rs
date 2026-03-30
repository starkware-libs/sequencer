use reqwest::StatusCode;
use serde_json::{json, Value};

use super::FakeStarknetServer;
use crate::fake_starknet_server::BLOCK_NOT_FOUND_JSON;

// Feeder tests

#[tokio::test]
async fn feeder_is_alive() {
    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();
    let (status, body) = get(&client, &url(&server, "/feeder_gateway/is_alive")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "FeederGateway is alive!");
}

#[tokio::test]
async fn get_block_by_number() {
    const BLOCK_NUMBER1: u64 = 7;
    const BLOCK_NUMBER2: u64 = 10;
    const BAD_BLOCK_NUMBER: u64 = 99;
    const {
        assert!(BLOCK_NUMBER1 < BLOCK_NUMBER2);
    }

    let server = FakeStarknetServer::new().await;
    let blob = make_blob(BLOCK_NUMBER1);
    server.state.lock().unwrap().blocks.insert(BLOCK_NUMBER1, blob.clone());

    let client = reqwest::Client::new();
    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_block?blockNumber={BLOCK_NUMBER1}")),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), blob);

    let blob = make_blob(BLOCK_NUMBER2);
    server.state.lock().unwrap().blocks.insert(BLOCK_NUMBER2, blob.clone());

    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_block?blockNumber={BLOCK_NUMBER2}")),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), blob);

    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_block?blockNumber={BAD_BLOCK_NUMBER}")),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, BLOCK_NOT_FOUND_JSON);
}

#[tokio::test]
async fn get_latest_block_returns_highest_block_number() {
    const BLOCK_NUMBER1: u64 = 3;
    const BLOCK_NUMBER2: u64 = 5;
    const BLOCK_NUMBER3: u64 = 7;
    const {
        assert!(BLOCK_NUMBER1 < BLOCK_NUMBER2);
    }
    const {
        assert!(BLOCK_NUMBER2 < BLOCK_NUMBER3);
    }

    let server = FakeStarknetServer::new().await;
    {
        let mut state = server.state.lock().unwrap();
        state.blocks.insert(BLOCK_NUMBER3, make_blob(BLOCK_NUMBER3));
        state.blocks.insert(BLOCK_NUMBER2, make_blob(BLOCK_NUMBER2));
        state.blocks.insert(BLOCK_NUMBER1, make_blob(BLOCK_NUMBER1));
    }

    let client = reqwest::Client::new();
    let (status, body) =
        get(&client, &url(&server, "/feeder_gateway/get_block?blockNumber=latest")).await;

    assert_eq!(status, StatusCode::OK);
    let returned: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(returned["block_number"], BLOCK_NUMBER3);
}

#[tokio::test]
async fn get_latest_block_empty_store() {
    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();
    let (status, _) =
        get(&client, &url(&server, "/feeder_gateway/get_block?blockNumber=latest")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_block_header_only_true_returns_subset() {
    const BLOCK_NUMBER: u64 = 16;
    let server = FakeStarknetServer::new().await;
    server.state.lock().unwrap().blocks.insert(BLOCK_NUMBER, make_blob(BLOCK_NUMBER));

    let client = reqwest::Client::new();
    let (status, body) = get(
        &client,
        &url(
            &server,
            &format!("/feeder_gateway/get_block?blockNumber={BLOCK_NUMBER}&headerOnly=true"),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let returned: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(returned["block_number"], BLOCK_NUMBER);
    assert_eq!(returned["block_hash"], format!("0x{BLOCK_NUMBER:x}"));
    // No extra fields beyond block_number and block_hash.
    assert_eq!(returned.as_object().unwrap().len(), 2);
}

#[tokio::test]
async fn get_block_header_only_false_returns_full_block() {
    const BLOCK_NUMBER: u64 = 16;
    let server = FakeStarknetServer::new().await;
    let blob = make_blob(BLOCK_NUMBER);
    server.state.lock().unwrap().blocks.insert(BLOCK_NUMBER, blob.clone());

    let client = reqwest::Client::new();
    let (status, body) = get(
        &client,
        &url(
            &server,
            &format!("/feeder_gateway/get_block?blockNumber={BLOCK_NUMBER}&headerOnly=false"),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), blob);
}

#[tokio::test]
async fn get_state_update_returns_stored_state_update() {
    const BLOCK_NUMBER: u64 = 2;
    const BAD_BLOCK_NUMBER: u64 = 99;

    let server = FakeStarknetServer::new().await;
    let blob = make_blob(BLOCK_NUMBER);
    server.state.lock().unwrap().state_updates.insert(BLOCK_NUMBER, blob.clone());

    let client = reqwest::Client::new();
    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_state_update?blockNumber={BLOCK_NUMBER}")),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), blob);

    // Getting the bad block number should return a BLOCK_NOT_FOUND error.
    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_state_update?blockNumber={BAD_BLOCK_NUMBER}")),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let error: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(error["code"], "StarknetErrorCode.BLOCK_NOT_FOUND");
}

#[tokio::test]
async fn get_state_update_pending_returns_pending_data() {
    let server = FakeStarknetServer::new().await;
    let pending = json!({"pending": true});
    server.state.lock().unwrap().pending_data_json = Some(pending.clone());

    let client = reqwest::Client::new();
    let (status, body) =
        get(&client, &url(&server, "/feeder_gateway/get_state_update?blockNumber=pending")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, pending.to_string());
}

#[tokio::test]
async fn get_state_update_invalid_block_number_returns_block_not_found() {
    let server = FakeStarknetServer::new().await;

    let client = reqwest::Client::new();
    let (status, body) =
        get(&client, &url(&server, "/feeder_gateway/get_state_update?blockNumber=garbage")).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, BLOCK_NOT_FOUND_JSON);
}

#[tokio::test]
async fn get_state_update_pending_not_configured_returns_block_not_found() {
    let server = FakeStarknetServer::new().await;
    // pending_data_json is None by default.

    let client = reqwest::Client::new();
    let (status, body) =
        get(&client, &url(&server, "/feeder_gateway/get_state_update?blockNumber=pending")).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, BLOCK_NOT_FOUND_JSON);
}

#[tokio::test]
async fn get_signature_returns_stored_signature() {
    const BLOCK_NUMBER: u64 = 6;
    const BAD_BLOCK_NUMBER: u64 = 99;

    let server = FakeStarknetServer::new().await;
    let blob = make_blob(BLOCK_NUMBER);
    server.state.lock().unwrap().block_signatures.insert(BLOCK_NUMBER, blob.clone());

    let client = reqwest::Client::new();
    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_signature?blockNumber={BLOCK_NUMBER}")),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), blob);

    // Getting the bad block number should return a BLOCK_NOT_FOUND error.
    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_signature?blockNumber={BAD_BLOCK_NUMBER}")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let error: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(error["code"], "StarknetErrorCode.BLOCK_NOT_FOUND");
}

#[tokio::test]
async fn get_class_by_hash_returns_class() {
    const CLASS_HASH: &str = "0xabc";
    const BAD_CLASS_HASH: &str = "0xdeadbeef";

    let server = FakeStarknetServer::new().await;
    let class = json!({"type": "CAIRO_1"});
    server.state.lock().unwrap().classes_json.insert(CLASS_HASH.to_string(), class.clone());

    let client = reqwest::Client::new();
    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_class_by_hash?classHash={CLASS_HASH}")),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), class);

    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_class_by_hash?classHash={BAD_CLASS_HASH}")),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let error: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(error["code"], "StarknetErrorCode.UNDECLARED_CLASS");
}

#[tokio::test]
async fn get_compiled_class_returns_compiled_class() {
    const CLASS_HASH: &str = "0xdef";
    const BAD_CLASS_HASH: &str = "0xunknown";

    let server = FakeStarknetServer::new().await;
    let compiled = json!({"bytecode": [1, 2, 3]});
    server
        .state
        .lock()
        .unwrap()
        .compiled_classes_json
        .insert(CLASS_HASH.to_string(), compiled.clone());

    let client = reqwest::Client::new();
    let (status, body) = get(
        &client,
        &url(
            &server,
            &format!("/feeder_gateway/get_compiled_class_by_class_hash?classHash={CLASS_HASH}"),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), compiled);

    // Getting the bad class hash should return a UNDECLARED_CLASS error.
    let (status, body) = get(
        &client,
        &url(
            &server,
            &format!("/feeder_gateway/get_compiled_class_by_class_hash?classHash={BAD_CLASS_HASH}"),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let error: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(error["code"], "StarknetErrorCode.UNDECLARED_CLASS");
}

#[tokio::test]
async fn get_public_key() {
    const PUBLIC_KEY: &str = "0x123456";
    let server = FakeStarknetServer::new().await;
    let pub_key = json!(PUBLIC_KEY);
    server.state.lock().unwrap().sequencer_pub_key_json = Some(pub_key.clone());

    let client = reqwest::Client::new();
    let (status, body) = get(&client, &url(&server, "/feeder_gateway/get_public_key")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), pub_key);
}

#[tokio::test]
async fn get_public_key_not_configured() {
    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();
    let (status, _) = get(&client, &url(&server, "/feeder_gateway/get_public_key")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

// Cende tests

#[tokio::test]
async fn write_blob_records_block_number() {
    const BLOCK_NUMBER: u64 = 10;
    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();

    let status =
        post_json(&client, &url(&server, "/cende_recorder/write_blob"), &make_blob(BLOCK_NUMBER))
            .await;

    assert_eq!(status, StatusCode::OK);
    assert!(server.state.lock().unwrap().cende_block_numbers.contains(&BLOCK_NUMBER));
    // Cende writes do not populate the feeder block store.
    assert!(server.state.lock().unwrap().blocks.is_empty());
}

#[tokio::test]
async fn write_blob_failure_mode_returns_500_and_does_not_store() {
    const BLOCK_NUMBER: u64 = 3;
    let server = FakeStarknetServer::new().await;
    server.state.lock().unwrap().write_blob_should_succeed = false;
    let client = reqwest::Client::new();

    let status =
        post_json(&client, &url(&server, "/cende_recorder/write_blob"), &make_blob(BLOCK_NUMBER))
            .await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(server.state.lock().unwrap().cende_block_numbers.is_empty());
}

#[tokio::test]
async fn get_latest_received_block_empty_store() {
    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();
    let (status, body) =
        get(&client, &url(&server, "/cende_recorder/get_latest_received_block")).await;

    assert_eq!(status, StatusCode::OK);
    let response: Value = serde_json::from_str(&body).unwrap();
    assert!(response["block_number"].is_null());
}

#[tokio::test]
async fn get_latest_received_block_returns_max_block_number() {
    const BLOCK_NUMBER1: u64 = 1;
    const BLOCK_NUMBER2: u64 = 3;
    const BLOCK_NUMBER3: u64 = 5;
    const {
        assert!(BLOCK_NUMBER1 < BLOCK_NUMBER2);
    }
    const {
        assert!(BLOCK_NUMBER2 < BLOCK_NUMBER3);
    }

    let server = FakeStarknetServer::new().await;
    {
        let mut state = server.state.lock().unwrap();
        state.cende_block_numbers.insert(BLOCK_NUMBER3);
        state.cende_block_numbers.insert(BLOCK_NUMBER2);
        state.cende_block_numbers.insert(BLOCK_NUMBER1);
    }

    let client = reqwest::Client::new();
    let (status, body) =
        get(&client, &url(&server, "/cende_recorder/get_latest_received_block")).await;

    assert_eq!(status, StatusCode::OK);
    let response: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(response["block_number"], BLOCK_NUMBER3);
}

// Cende and feeder store independence.

#[tokio::test]
async fn write_blob_updates_get_latest_received_block() {
    const BLOCK_NUMBER: u64 = 8;
    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();

    let write_status =
        post_json(&client, &url(&server, "/cende_recorder/write_blob"), &make_blob(BLOCK_NUMBER))
            .await;
    assert_eq!(write_status, StatusCode::OK);

    let (status, body) =
        get(&client, &url(&server, "/cende_recorder/get_latest_received_block")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap()["block_number"], BLOCK_NUMBER);
}

#[tokio::test]
async fn write_blob_does_not_populate_feeder_block_store() {
    const BLOCK_NUMBER: u64 = 8;
    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();

    let write_status =
        post_json(&client, &url(&server, "/cende_recorder/write_blob"), &make_blob(BLOCK_NUMBER))
            .await;
    assert_eq!(write_status, StatusCode::OK);

    // The feeder store is independent: the block is not visible via the feeder endpoint.
    let (status, _) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_block?blockNumber={BLOCK_NUMBER}")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// Helpers

fn make_blob(block_number: u64) -> Value {
    json!({ "block_number": block_number, "block_hash": format!("0x{block_number:x}") })
}

/// Returns `{server_url}{path}`, stripping the trailing slash that `Url` adds so paths
/// with a leading slash don't produce double-slash URLs.
fn url(server: &FakeStarknetServer, path: &str) -> String {
    format!("{}{path}", server.url.as_str().trim_end_matches('/'))
}

async fn get(client: &reqwest::Client, url: &str) -> (StatusCode, String) {
    let response = client.get(url).send().await.unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    (status, body)
}

async fn post_json(client: &reqwest::Client, url: &str, body: &Value) -> StatusCode {
    client.post(url).json(body).send().await.unwrap().status()
}
