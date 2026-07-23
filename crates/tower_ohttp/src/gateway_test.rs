use super::*;

fn test_ikm() -> [u8; 32] {
    let mut ikm = [0u8; 32];
    ikm[0] = 1; // non-zero so key derivation produces a valid key
    ikm
}

#[test]
fn from_ikm_produces_valid_config() {
    let gateway = OhttpGateway::from_ikm(0, Kem::X25519Sha256, &test_ikm()).unwrap();
    assert!(!gateway.encoded_config().is_empty());
}

#[test]
fn from_hex_key_roundtrip() {
    let hex = hex::encode(test_ikm());
    let gateway = OhttpGateway::from_hex_key(&hex).unwrap();
    assert!(!gateway.encoded_config().is_empty());
}

#[test]
fn from_hex_key_rejects_short() {
    let result = OhttpGateway::from_hex_key("aabb");
    assert!(matches!(result, Err(OhttpError::InvalidKey(_))));
}

#[test]
fn from_hex_key_rejects_invalid_hex() {
    let result = OhttpGateway::from_hex_key("zzzz");
    assert!(matches!(result, Err(OhttpError::InvalidKey(_))));
}

#[test]
fn from_hex_key_error_does_not_leak_key_material() {
    let leaky_input = "0xDEADBEEFdeadbeefDEADBEEFdeadbeefDEADBEEFdeadbeefDEADBEEFdeadbeef";
    let Err(error) = OhttpGateway::from_hex_key(leaky_input) else {
        panic!("expected error for malformed input");
    };
    let rendered = format!("{error}");

    assert!(!rendered.contains("DEADBEEF"), "raw key bytes leaked: {rendered}");
    assert!(!rendered.contains("deadbeef"), "raw key bytes leaked: {rendered}");
    assert!(!rendered.contains("0x"), "input prefix leaked: {rendered}");
    assert!(
        rendered.to_lowercase().contains("hex"),
        "error should still mention 'hex' to communicate failure mode: {rendered}"
    );
}

#[test]
fn decapsulate_roundtrip() {
    let gateway = OhttpGateway::from_ikm(0, Kem::X25519Sha256, &test_ikm()).unwrap();

    let config_bytes = gateway.encoded_config();
    let client_request = ohttp::ClientRequest::from_encoded_config_list(config_bytes).unwrap();

    let plaintext_request = b"test request body";
    let (encapsulated, client_response) = client_request.encapsulate(plaintext_request).unwrap();

    let (decapsulated, server_response) = gateway.server().decapsulate(&encapsulated).unwrap();
    assert_eq!(decapsulated, plaintext_request);

    let plaintext_response = b"test response body";
    let encapsulated_response = server_response.encapsulate(plaintext_response).unwrap();

    let decapsulated_response = client_response.decapsulate(&encapsulated_response).unwrap();
    assert_eq!(decapsulated_response, plaintext_response);
}
