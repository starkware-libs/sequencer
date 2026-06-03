//! Feeder gateway response wire structs (serialized via `to_python_json` to match the legacy
//! Python feeder gateway byte-for-byte).

use serde::Serialize;
use starknet_api::block::BlockHash;
use starknet_api::hash::StarkHash;

/// The `get_signature` response: the block hash and the `[r, s]` block signature.
#[derive(Debug, Serialize)]
pub(crate) struct FeederGatewaySignature {
    pub block_hash: BlockHash,
    pub signature: [StarkHash; 2],
}
