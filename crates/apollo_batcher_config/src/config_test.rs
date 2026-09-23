use std::collections::BTreeSet;

use apollo_config::dumping::SerializeConfig;
use apollo_config::SerializedContent;
use serde_json::json;
use starknet_api::state::StorageKey;

use crate::config::StorageAccessFilterConfig;

fn deserialize(blocked_storage_keys: &str) -> Result<StorageAccessFilterConfig, serde_json::Error> {
    serde_json::from_value(json!({
        "blocked_storage_keys": blocked_storage_keys,
        "error_message": "Blocked.",
    }))
}

#[test]
fn test_blocked_storage_keys_deserialization() {
    assert_eq!(deserialize("").unwrap().blocked_storage_keys, BTreeSet::new());
    assert_eq!(
        deserialize(
            "0x1,0x10,0xAB,0x0000000000000000000000000000000000000000000000000000000000000010"
        )
        .unwrap()
        .blocked_storage_keys,
        BTreeSet::from([
            StorageKey::from(0x1_u8),
            StorageKey::from(0x10_u8),
            StorageKey::from(0xab_u8)
        ])
    );
    // Not a number.
    assert!(deserialize("0x1,not_a_key").is_err());
    // Out of the storage key range.
    assert!(
        deserialize("0x800000000000000000000000000000000000000000000000000000000000000").is_err()
    );
}

#[test]
fn test_blocked_storage_keys_dump_round_trip() {
    let config = deserialize("0x10,0x1").unwrap();
    let SerializedContent::DefaultValue(dumped_keys) =
        &config.dump()["blocked_storage_keys"].content
    else {
        panic!("Blocked storage keys should be dumped as a default value.");
    };
    assert_eq!(deserialize(dumped_keys.as_str().unwrap()).unwrap(), config);
}
