use reqwest::StatusCode;
use serde_json::{json, Value};

use super::FakeStarknetServer;
use crate::fake_starknet_server::{BLOCK_NOT_FOUND_JSON, BLOCK_SIGNATURE_JSON};

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
    let feeder_json1 = make_feeder_block(BLOCK_NUMBER1);
    server.state.lock().unwrap().seed_block(
        BLOCK_NUMBER1,
        &format!("0x{BLOCK_NUMBER1:x}"),
        feeder_json1.clone(),
    );

    let client = reqwest::Client::new();
    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_block?blockNumber={BLOCK_NUMBER1}")),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), feeder_json1);

    let feeder_json2 = make_feeder_block(BLOCK_NUMBER2);
    server.state.lock().unwrap().seed_block(
        BLOCK_NUMBER2,
        &format!("0x{BLOCK_NUMBER2:x}"),
        feeder_json2.clone(),
    );

    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_block?blockNumber={BLOCK_NUMBER2}")),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), feeder_json2);

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
        state.seed_block(
            BLOCK_NUMBER3,
            &format!("0x{BLOCK_NUMBER3:x}"),
            make_feeder_block(BLOCK_NUMBER3),
        );
        state.seed_block(
            BLOCK_NUMBER2,
            &format!("0x{BLOCK_NUMBER2:x}"),
            make_feeder_block(BLOCK_NUMBER2),
        );
        state.seed_block(
            BLOCK_NUMBER1,
            &format!("0x{BLOCK_NUMBER1:x}"),
            make_feeder_block(BLOCK_NUMBER1),
        );
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
    server.state.lock().unwrap().seed_block(
        BLOCK_NUMBER,
        &format!("0x{BLOCK_NUMBER:x}"),
        make_feeder_block(BLOCK_NUMBER),
    );

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
    let feeder_json = make_feeder_block(BLOCK_NUMBER);
    server.state.lock().unwrap().seed_block(
        BLOCK_NUMBER,
        &format!("0x{BLOCK_NUMBER:x}"),
        feeder_json.clone(),
    );

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
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), feeder_json);
}

#[tokio::test]
async fn get_block_without_hash_returns_block_not_found() {
    const BLOCK_NUMBER: u64 = 5;
    let server = FakeStarknetServer::new().await;
    // Seed feeder content but no block hash.
    server.state.lock().unwrap().blocks.entry(BLOCK_NUMBER).or_default().feeder_json =
        Some(make_feeder_block(BLOCK_NUMBER));

    let client = reqwest::Client::new();
    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_block?blockNumber={BLOCK_NUMBER}")),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, BLOCK_NOT_FOUND_JSON);
}

#[tokio::test]
async fn get_block_without_feeder_content_returns_block_not_found() {
    const BLOCK_NUMBER: u64 = 5;
    let server = FakeStarknetServer::new().await;
    // Set block hash but no feeder content.
    server.state.lock().unwrap().blocks.entry(BLOCK_NUMBER).or_default().block_hash =
        Some(format!("0x{BLOCK_NUMBER:x}"));

    let client = reqwest::Client::new();
    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_block?blockNumber={BLOCK_NUMBER}")),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, BLOCK_NOT_FOUND_JSON);
}

#[tokio::test]
async fn get_state_update_returns_stored_state_update() {
    const BLOCK_NUMBER: u64 = 2;
    const BAD_BLOCK_NUMBER: u64 = 99;

    let server = FakeStarknetServer::new().await;
    let state_update = json!({"state_diff": {}});
    {
        let mut state = server.state.lock().unwrap();
        let block = state.blocks.entry(BLOCK_NUMBER).or_default();
        block.block_hash = Some(format!("0x{BLOCK_NUMBER:x}"));
        block.state_update = Some(state_update.clone());
    }

    let client = reqwest::Client::new();
    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_state_update?blockNumber={BLOCK_NUMBER}")),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), state_update);

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
async fn get_state_update_without_block_hash_returns_block_not_found() {
    const BLOCK_NUMBER: u64 = 2;
    let server = FakeStarknetServer::new().await;
    // Seed state_update but no block hash.
    server.state.lock().unwrap().blocks.entry(BLOCK_NUMBER).or_default().state_update =
        Some(json!({"state_diff": {}}));

    let client = reqwest::Client::new();
    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_state_update?blockNumber={BLOCK_NUMBER}")),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, BLOCK_NOT_FOUND_JSON);
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
async fn get_signature_returns_constant() {
    const BLOCK_NUMBER: u64 = 6;
    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();
    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_signature?blockNumber={BLOCK_NUMBER}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, BLOCK_SIGNATURE_JSON);
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
async fn write_blob_records_block_without_hash() {
    const BLOCK_NUMBER: u64 = 10;
    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();

    let status =
        post_json(&client, &url(&server, "/cende_recorder/write_blob"), &make_blob(BLOCK_NUMBER))
            .await;

    assert_eq!(status, StatusCode::OK);
    // The block exists in state but has no confirmed hash yet.
    let state = server.state.lock().unwrap();
    let block = state.blocks.get(&BLOCK_NUMBER).expect("block should exist after write_blob");
    assert!(block.block_hash.is_none());
    assert!(block.feeder_json.is_none());
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
    assert!(server.state.lock().unwrap().blocks.is_empty());
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
async fn get_latest_received_block_returns_max_block_with_hash() {
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
        // Set block hashes directly, simulating what recent_block_hashes would do.
        state.blocks.entry(BLOCK_NUMBER1).or_default().block_hash =
            Some(format!("0x{BLOCK_NUMBER1:x}"));
        state.blocks.entry(BLOCK_NUMBER2).or_default().block_hash =
            Some(format!("0x{BLOCK_NUMBER2:x}"));
        state.blocks.entry(BLOCK_NUMBER3).or_default().block_hash =
            Some(format!("0x{BLOCK_NUMBER3:x}"));
    }

    let client = reqwest::Client::new();
    let (status, body) =
        get(&client, &url(&server, "/cende_recorder/get_latest_received_block")).await;

    assert_eq!(status, StatusCode::OK);
    let response: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(response["block_number"], BLOCK_NUMBER3);
}

#[tokio::test]
async fn get_latest_received_block_ignores_blocks_without_hash() {
    const BLOCK_WITH_HASH: u64 = 3;
    const BLOCK_WITHOUT_HASH: u64 = 5;
    const {
        assert!(BLOCK_WITH_HASH < BLOCK_WITHOUT_HASH);
    }

    let server = FakeStarknetServer::new().await;
    {
        let mut state = server.state.lock().unwrap();
        state.blocks.entry(BLOCK_WITH_HASH).or_default().block_hash =
            Some(format!("0x{BLOCK_WITH_HASH:x}"));
        // BLOCK_WITHOUT_HASH exists but has no confirmed hash (as if only its blob was posted).
        state.blocks.entry(BLOCK_WITHOUT_HASH).or_default();
    }

    let client = reqwest::Client::new();
    let (status, body) =
        get(&client, &url(&server, "/cende_recorder/get_latest_received_block")).await;

    assert_eq!(status, StatusCode::OK);
    // Only BLOCK_WITH_HASH counts.
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap()["block_number"], BLOCK_WITH_HASH);
}

#[tokio::test]
async fn write_blob_with_recent_block_hashes_fills_in_hashes() {
    const BLOB_BLOCK_NUMBER: u64 = 10;
    const RECENT_BLOCK_NUMBER: u64 = 9;

    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();

    let blob = make_blob_with_recent_hashes(
        BLOB_BLOCK_NUMBER,
        &[(RECENT_BLOCK_NUMBER, format!("0x{RECENT_BLOCK_NUMBER:x}"))],
    );
    let status = post_json(&client, &url(&server, "/cende_recorder/write_blob"), &blob).await;
    assert_eq!(status, StatusCode::OK);

    let state = server.state.lock().unwrap();
    // The blob's own block has no hash yet.
    assert!(state.blocks[&BLOB_BLOCK_NUMBER].block_hash.is_none());
    // The referenced older block has its hash filled in.
    assert_eq!(
        state.blocks[&RECENT_BLOCK_NUMBER].block_hash,
        Some(format!("0x{RECENT_BLOCK_NUMBER:x}"))
    );
}

#[tokio::test]
async fn write_blob_with_recent_hashes_updates_get_latest_received_block() {
    const BLOB_BLOCK_NUMBER: u64 = 8;
    const PREV_BLOCK_NUMBER: u64 = BLOB_BLOCK_NUMBER - 1;

    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();

    let blob = make_blob_with_recent_hashes(
        BLOB_BLOCK_NUMBER,
        &[(PREV_BLOCK_NUMBER, format!("0x{PREV_BLOCK_NUMBER:x}"))],
    );
    let write_status = post_json(&client, &url(&server, "/cende_recorder/write_blob"), &blob).await;
    assert_eq!(write_status, StatusCode::OK);

    // Only PREV_BLOCK_NUMBER has a confirmed hash; BLOB_BLOCK_NUMBER does not.
    let (status, body) =
        get(&client, &url(&server, "/cende_recorder/get_latest_received_block")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap()["block_number"], PREV_BLOCK_NUMBER);
}

#[tokio::test]
async fn write_blob_without_recent_hashes_does_not_update_get_latest_received_block() {
    const BLOCK_NUMBER: u64 = 8;
    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();

    // Blob with no recent_block_hashes: the block has no confirmed hash.
    let write_status =
        post_json(&client, &url(&server, "/cende_recorder/write_blob"), &make_blob(BLOCK_NUMBER))
            .await;
    assert_eq!(write_status, StatusCode::OK);

    let (status, body) =
        get(&client, &url(&server, "/cende_recorder/get_latest_received_block")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(serde_json::from_str::<Value>(&body).unwrap()["block_number"].is_null());
}

#[tokio::test]
async fn write_blob_without_feeder_content_is_not_served_by_feeder() {
    const BLOCK_NUMBER: u64 = 8;
    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();

    // Blob with recent_block_hashes gives BLOCK_NUMBER a hash, but no feeder content.
    let blob = make_blob_with_recent_hashes(
        BLOCK_NUMBER + 1,
        &[(BLOCK_NUMBER, format!("0x{BLOCK_NUMBER:x}"))],
    );
    let write_status = post_json(&client, &url(&server, "/cende_recorder/write_blob"), &blob).await;
    assert_eq!(write_status, StatusCode::OK);

    // Hash is known but feeder content is absent: feeder must not serve the block.
    let (status, _) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_block?blockNumber={BLOCK_NUMBER}")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn hash_set_before_feeder_content_merges_correctly() {
    const BLOCK_NUMBER: u64 = 5;
    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();

    // Step 1: hash arrives via a later blob's recent_block_hashes.
    let later_blob = make_blob_with_recent_hashes(
        BLOCK_NUMBER + 1,
        &[(BLOCK_NUMBER, format!("0x{BLOCK_NUMBER:x}"))],
    );
    post_json(&client, &url(&server, "/cende_recorder/write_blob"), &later_blob).await;

    // Step 2: feeder content seeded directly.
    let feeder_json = make_feeder_block(BLOCK_NUMBER);
    server.state.lock().unwrap().blocks.entry(BLOCK_NUMBER).or_default().feeder_json =
        Some(feeder_json.clone());

    // Both pieces are present: the block is now served by the feeder.
    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_block?blockNumber={BLOCK_NUMBER}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), feeder_json);
}

#[tokio::test]
async fn feeder_content_set_before_hash_merges_correctly() {
    const BLOCK_NUMBER: u64 = 5;
    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();

    // Step 1: feeder content seeded first.
    let feeder_json = make_feeder_block(BLOCK_NUMBER);
    server.state.lock().unwrap().blocks.entry(BLOCK_NUMBER).or_default().feeder_json =
        Some(feeder_json.clone());

    // Block is not yet served (no hash).
    let (status, _) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_block?blockNumber={BLOCK_NUMBER}")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Step 2: hash arrives via a later blob's recent_block_hashes.
    let later_blob = make_blob_with_recent_hashes(
        BLOCK_NUMBER + 1,
        &[(BLOCK_NUMBER, format!("0x{BLOCK_NUMBER:x}"))],
    );
    post_json(&client, &url(&server, "/cende_recorder/write_blob"), &later_blob).await;

    // Both pieces are now present: the block is served.
    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_block?blockNumber={BLOCK_NUMBER}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), feeder_json);
}

// State update derivation from blob state_diff

#[tokio::test]
async fn write_blob_derives_state_update_from_central_state_diff() {
    const BLOCK_NUMBER: u64 = 5;
    const NEXT_BLOCK_NUMBER: u64 = BLOCK_NUMBER + 1;
    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();

    // Blob for BLOCK_NUMBER carries a CentralStateDiff with both L1 and L2 DA-mode entries.
    let blob = json!({
        "block_number": BLOCK_NUMBER,
        "state_diff": {
            "address_to_class_hash": {"0x1": "0x10"},
            "nonces": {
                "L1": {"0x2": "0x20"},
                "L2": {"0x22": "0x220"}
            },
            "storage_updates": {
                "L1": {"0x3": {"0x30": "0x300"}},
                "L2": {"0x33": {"0x330": "0x3300"}}
            },
            "class_hash_to_compiled_class_hash": {"0x4": "0x40"},
            "block_info": {}
        }
    });
    post_json(&client, &url(&server, "/cende_recorder/write_blob"), &blob).await;

    // A later blob confirms BLOCK_NUMBER's hash via recent_block_hashes.
    let later_blob = make_blob_with_recent_hashes(
        NEXT_BLOCK_NUMBER,
        &[(BLOCK_NUMBER, format!("0x{BLOCK_NUMBER:x}"))],
    );
    post_json(&client, &url(&server, "/cende_recorder/write_blob"), &later_blob).await;

    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_state_update?blockNumber={BLOCK_NUMBER}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let state_update: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(state_update["block_hash"], format!("0x{BLOCK_NUMBER:x}"));
    let state_diff = &state_update["state_diff"];
    assert_eq!(state_diff["deployed_contracts"], json!([{"address": "0x1", "class_hash": "0x10"}]));
    // Nonces from both L1 and L2 are merged into a flat map.
    assert_eq!(state_diff["nonces"], json!({"0x2": "0x20", "0x22": "0x220"}));
    // Storage diffs from both L1 and L2 are merged into a flat map.
    assert_eq!(
        state_diff["storage_diffs"],
        json!({"0x3": [{"key": "0x30", "value": "0x300"}], "0x33": [{"key": "0x330", "value": "0x3300"}]})
    );
    assert_eq!(
        state_diff["declared_classes"],
        json!([{"class_hash": "0x4", "compiled_class_hash": "0x40"}])
    );
    assert_eq!(state_diff["replaced_classes"], json!([]));
    assert_eq!(state_diff["old_declared_contracts"], json!([]));
}

#[tokio::test]
async fn write_blob_derives_feeder_json_from_block_info() {
    const BLOCK_NUMBER: u64 = 5;
    const NEXT_BLOCK_NUMBER: u64 = BLOCK_NUMBER + 1;
    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();

    // Blob for BLOCK_NUMBER carries a full block_info and fee_market_info.
    // recent_block_hashes carries the parent (BLOCK_NUMBER - 1) hash.
    let blob = json!({
        "block_number": BLOCK_NUMBER,
        "state_diff": {
            "address_to_class_hash": {},
            "nonces": {},
            "storage_updates": {},
            "class_hash_to_compiled_class_hash": {},
            "block_info": {
                "block_timestamp": 1700000000u64,
                "use_kzg_da": true,
                "sequencer_address": "0xabcd",
                "starknet_version": "0.14.0",
                "l1_gas_price": {"price_in_wei": "0x1", "price_in_fri": "0x2"},
                "l1_data_gas_price": {"price_in_wei": "0x3", "price_in_fri": "0x4"},
                "l2_gas_price": {"price_in_wei": "0x5", "price_in_fri": "0x6"}
            }
        },
        "fee_market_info": {
            "l2_gas_consumed": "0x1234",
            "next_l2_gas_price": "0x5678"
        },
        "recent_block_hashes": [
            {"block_number": BLOCK_NUMBER - 1, "block_hash": "0xparent"}
        ]
    });
    post_json(&client, &url(&server, "/cende_recorder/write_blob"), &blob).await;

    // Hash not yet confirmed: get_block returns 404.
    let (status, _) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_block?blockNumber={BLOCK_NUMBER}")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // A later blob confirms BLOCK_NUMBER's hash via recent_block_hashes.
    let later_blob = make_blob_with_recent_hashes(
        NEXT_BLOCK_NUMBER,
        &[(BLOCK_NUMBER, format!("0x{BLOCK_NUMBER:x}"))],
    );
    post_json(&client, &url(&server, "/cende_recorder/write_blob"), &later_blob).await;

    // Now get_block should serve the derived feeder JSON.
    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_block?blockNumber={BLOCK_NUMBER}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let block: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(block["block_number"], BLOCK_NUMBER);
    assert_eq!(block["block_hash"], format!("0x{BLOCK_NUMBER:x}"));
    assert_eq!(block["parent_block_hash"], "0xparent");
    assert_eq!(block["timestamp"], 1700000000u64);
    assert_eq!(block["l1_da_mode"], "BLOB");
    assert_eq!(block["sequencer_address"], "0xabcd");
    assert_eq!(block["starknet_version"], "0.14.0");
    assert_eq!(block["l1_gas_price"], json!({"price_in_wei": "0x1", "price_in_fri": "0x2"}));
    assert_eq!(block["l1_data_gas_price"], json!({"price_in_wei": "0x3", "price_in_fri": "0x4"}));
    assert_eq!(block["l2_gas_price"], json!({"price_in_wei": "0x5", "price_in_fri": "0x6"}));
    assert_eq!(block["l2_gas_consumed"], "0x1234");
    assert_eq!(block["next_l2_gas_price"], "0x5678");
    assert_eq!(block["status"], "ACCEPTED_ON_L2");
}

#[tokio::test]
async fn state_update_not_served_until_hash_confirmed() {
    const BLOCK_NUMBER: u64 = 5;
    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();

    // Blob posted: state_diff known but hash not yet confirmed.
    let blob = json!({
        "block_number": BLOCK_NUMBER,
        "state_diff": {
            "address_to_class_hash": {},
            "nonces": {},
            "storage_updates": {},
            "class_hash_to_compiled_class_hash": {},
            "block_info": {}
        }
    });
    post_json(&client, &url(&server, "/cende_recorder/write_blob"), &blob).await;

    let (status, _) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_state_update?blockNumber={BLOCK_NUMBER}")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn hash_confirmed_before_state_diff_patches_correctly() {
    const BLOCK_NUMBER: u64 = 5;
    const NEXT_BLOCK_NUMBER: u64 = BLOCK_NUMBER + 1;
    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();

    // Hash arrives first via a later blob.
    let later_blob = make_blob_with_recent_hashes(
        NEXT_BLOCK_NUMBER,
        &[(BLOCK_NUMBER, format!("0x{BLOCK_NUMBER:x}"))],
    );
    post_json(&client, &url(&server, "/cende_recorder/write_blob"), &later_blob).await;

    // State diff arrives in the blob for BLOCK_NUMBER itself.
    let blob = json!({
        "block_number": BLOCK_NUMBER,
        "state_diff": {
            "address_to_class_hash": {"0x1": "0x10"},
            "nonces": {},
            "storage_updates": {},
            "class_hash_to_compiled_class_hash": {},
            "block_info": {}
        }
    });
    post_json(&client, &url(&server, "/cende_recorder/write_blob"), &blob).await;

    let (status, body) = get(
        &client,
        &url(&server, &format!("/feeder_gateway/get_state_update?blockNumber={BLOCK_NUMBER}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let state_update: Value = serde_json::from_str(&body).unwrap();
    // Hash was patched in at derivation time.
    assert_eq!(state_update["block_hash"], format!("0x{BLOCK_NUMBER:x}"));
    assert_eq!(
        state_update["state_diff"]["deployed_contracts"],
        json!([{"address": "0x1", "class_hash": "0x10"}])
    );
}

// write_pre_confirmed_block tests

#[tokio::test]
async fn write_pre_confirmed_block_records_block_number() {
    const BLOCK_NUMBER: u64 = 11;
    let server = FakeStarknetServer::new().await;
    let client = reqwest::Client::new();

    let status = post_json(
        &client,
        &url(&server, "/cende_recorder/write_pre_confirmed_block"),
        &make_blob(BLOCK_NUMBER),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(server.state.lock().unwrap().pre_confirmed_block_numbers.contains(&BLOCK_NUMBER));
    // Does not affect the block store.
    assert!(server.state.lock().unwrap().blocks.is_empty());
}

#[tokio::test]
async fn write_pre_confirmed_block_failure_mode_returns_500_and_does_not_store() {
    const BLOCK_NUMBER: u64 = 4;
    let server = FakeStarknetServer::new().await;
    server.state.lock().unwrap().write_pre_confirmed_block_should_succeed = false;
    let client = reqwest::Client::new();

    let status = post_json(
        &client,
        &url(&server, "/cende_recorder/write_pre_confirmed_block"),
        &make_blob(BLOCK_NUMBER),
    )
    .await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(server.state.lock().unwrap().pre_confirmed_block_numbers.is_empty());
}

// Helpers

/// Minimal blob payload with only a block number, no recent_block_hashes.
fn make_blob(block_number: u64) -> Value {
    json!({ "block_number": block_number })
}

/// Blob payload carrying recent_block_hashes entries, as a real `AerospikeBlob` would.
fn make_blob_with_recent_hashes(block_number: u64, recent: &[(u64, String)]) -> Value {
    json!({
        "block_number": block_number,
        "recent_block_hashes": recent.iter().map(|(n, h)| json!({"block_number": n, "block_hash": h})).collect::<Vec<_>>(),
    })
}

/// Minimal feeder-format block JSON with the fields required by feeder gateway handlers.
fn make_feeder_block(block_number: u64) -> Value {
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
