//! Feeder gateway response wire structs (serialized via `to_python_json` to match the legacy
//! Python feeder gateway byte-for-byte).

use apollo_feeder_gateway_config::config::FeederGatewayContractAddresses;
use apollo_starknet_client::eip55::eip55_checksum_address;
use serde::ser::SerializeMap;
use serde::Serialize;
use starknet_api::block::BlockHash;
use starknet_api::hash::StarkHash;

/// The `get_signature` response: the block hash and the `[r, s]` block signature.
#[derive(Debug, Serialize)]
pub(crate) struct FeederGatewaySignature {
    pub block_hash: BlockHash,
    pub signature: [StarkHash; 2],
}

/// The `get_contract_addresses` response: the configured L1 contracts in their configured order,
/// EIP-55 checksummed, followed by the two L2 fee-token address felts (matching the live Python
/// feeder gateway; the set and order are network-specific).
pub(crate) struct FeederGatewayContractAddressesResponse<'a>(
    pub &'a FeederGatewayContractAddresses,
);

impl Serialize for FeederGatewayContractAddressesResponse<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        for (contract_name, address) in &self.0.l1_contract_addresses {
            map.serialize_entry(contract_name, &eip55_checksum_address(address))?;
        }
        map.serialize_entry("strk_l2_token_address", &self.0.strk_l2_token_address)?;
        map.serialize_entry("eth_l2_token_address", &self.0.eth_l2_token_address)?;
        map.end()
    }
}
