use serde_json::json;
use starknet_api::core::EthAddress;
use starknet_api::hash::StarkHash;

use crate::config::FeederGatewayContractAddresses;

/// The ordered L1 contract list round-trips through its space-separated `Name:0xaddress` config
/// string form, preserving the configured order.
#[test]
fn l1_contract_addresses_round_trip_preserves_order() {
    let contract_addresses = FeederGatewayContractAddresses {
        l1_contract_addresses: vec![
            (
                "GpsStatementVerifier".to_string(),
                EthAddress::try_from(
                    StarkHash::from_hex("0xf294781d719d2f4169ce54469c28908e6fa752c1").unwrap(),
                )
                .unwrap(),
            ),
            (
                "Starknet".to_string(),
                EthAddress::try_from(
                    StarkHash::from_hex("0xe2bb56ee936fd6433dc0f6e7e3b8365c906aa057").unwrap(),
                )
                .unwrap(),
            ),
        ],
        ..Default::default()
    };

    let serialized = serde_json::to_value(&contract_addresses).unwrap();
    assert_eq!(
        serialized["l1_contract_addresses"],
        json!(
            "GpsStatementVerifier:0xf294781d719d2f4169ce54469c28908e6fa752c1 \
             Starknet:0xe2bb56ee936fd6433dc0f6e7e3b8365c906aa057"
        )
    );

    let round_tripped: FeederGatewayContractAddresses = serde_json::from_value(serialized).unwrap();
    assert_eq!(round_tripped, contract_addresses);
}

#[test]
fn empty_l1_contract_addresses_round_trip() {
    let contract_addresses = FeederGatewayContractAddresses::default();
    let serialized = serde_json::to_value(&contract_addresses).unwrap();
    assert_eq!(serialized["l1_contract_addresses"], json!(""));

    let round_tripped: FeederGatewayContractAddresses = serde_json::from_value(serialized).unwrap();
    assert_eq!(round_tripped, contract_addresses);
}

#[test]
fn malformed_l1_contract_addresses_pair_is_rejected() {
    let result = serde_json::from_value::<FeederGatewayContractAddresses>(serde_json::json!({
        "l1_contract_addresses": "MissingColonPair",
        "strk_l2_token_address": "0x0",
        "eth_l2_token_address": "0x0",
    }));
    assert!(result.is_err());
}
