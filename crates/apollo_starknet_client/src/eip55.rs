//! EIP-55 mixed-case checksum formatting for L1 addresses. The live Python feeder gateway serves
//! L1 addresses in this form (get_contract_addresses values and receipt message addresses;
//! verified 2026-06-03), while `EthAddress`'s own serde emits plain lowercase hex.

#[cfg(test)]
#[path = "eip55_test.rs"]
mod eip55_test;

use serde::Serializer;
use sha3::{Digest, Keccak256};
use starknet_api::core::EthAddress;

/// Formats an L1 address with the EIP-55 mixed-case checksum, e.g.
/// `0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4`.
///
/// Per EIP-55: keccak256 the 40-character lowercase hex form (without the `0x` prefix) and
/// uppercase every hex letter whose corresponding keccak nibble is >= 8.
pub fn eip55_checksum_address(address: &EthAddress) -> String {
    let lowercase_hex = format!("{:x}", address.0);
    let keccak_digest = Keccak256::digest(lowercase_hex.as_bytes());
    let mut checksummed_address = String::with_capacity(2 + lowercase_hex.len());
    checksummed_address.push_str("0x");
    for (nibble_index, hex_character) in lowercase_hex.chars().enumerate() {
        let keccak_byte = keccak_digest[nibble_index / 2];
        let keccak_nibble =
            if nibble_index % 2 == 0 { keccak_byte >> 4 } else { keccak_byte & 0x0f };
        if keccak_nibble >= 8 {
            checksummed_address.push(hex_character.to_ascii_uppercase());
        } else {
            checksummed_address.push(hex_character);
        }
    }
    checksummed_address
}

/// Serde helper serializing an [`EthAddress`] field in EIP-55 form for feeder gateway wire
/// parity. Deserialization is unaffected (hex parsing is case-insensitive).
pub fn serialize_eip55<S: Serializer>(
    address: &EthAddress,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&eip55_checksum_address(address))
}
