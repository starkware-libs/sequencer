//! Locks the JSON serialization format of `Felt` and the `starknet_api` newtypes the feeder
//! gateway emits. The entire byte-parity effort assumes felts serialize as lowercase `0x` hex with
//! no leading zeros (e.g. `"0x0"`, `"0xf"`), matching the Python feeder gateway. If `starknet_api`
//! ever changes this, these tests fail loudly here rather than as a confusing wire mismatch.

use starknet_api::block::BlockHash;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkHash;
use starknet_api::state::StorageKey;
use starknet_api::transaction::TransactionHash;

#[test]
fn felt_serializes_as_lowercase_hex_without_leading_zeros() {
    assert_eq!(serde_json::to_string(&StarkHash::from(0_u128)).unwrap(), "\"0x0\"");
    assert_eq!(serde_json::to_string(&StarkHash::from(15_u128)).unwrap(), "\"0xf\"");
    assert_eq!(serde_json::to_string(&StarkHash::from(255_u128)).unwrap(), "\"0xff\"");
}

#[test]
fn felt_newtypes_delegate_to_felt_serialization() {
    let felt = StarkHash::from(0xabc_u128);
    let expected = serde_json::to_string(&felt).unwrap();
    assert_eq!(expected, "\"0xabc\"");

    assert_eq!(serde_json::to_string(&ClassHash(felt)).unwrap(), expected);
    assert_eq!(serde_json::to_string(&Nonce(felt)).unwrap(), expected);
    assert_eq!(serde_json::to_string(&BlockHash(felt)).unwrap(), expected);
    assert_eq!(serde_json::to_string(&TransactionHash(felt)).unwrap(), expected);
    assert_eq!(serde_json::to_string(&ContractAddress::from(0xabc_u128)).unwrap(), expected);
    assert_eq!(serde_json::to_string(&StorageKey::from(0xabc_u128)).unwrap(), expected);
}
