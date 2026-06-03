use std::collections::HashMap;

use axum::extract::Query;
use axum::response::Response;
use axum::Extension;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::hash::StarkHash;

use crate::errors::FeederGatewayError;
use crate::objects::FeederGatewaySignature;
use crate::reader::{AppState, FgResult};
use crate::serialization::fg_json;

#[cfg(test)]
#[path = "handlers_test.rs"]
mod handlers_test;

/// `GET /feeder_gateway/get_contract_addresses` — returns the configured well-known contract
/// addresses in the legacy Python feeder gateway JSON shape.
pub(crate) async fn get_contract_addresses(Extension(state): Extension<AppState>) -> Response {
    fg_json(&state.config.contract_addresses)
}

/// `GET /feeder_gateway/get_public_key` — returns the configured sequencer public key as a bare
/// felt, matching the Python feeder gateway (verified against the live service).
pub(crate) async fn get_public_key(Extension(state): Extension<AppState>) -> Response {
    fg_json(&state.config.sequencer_public_key)
}

/// `GET /feeder_gateway/get_signature?blockNumber=<n>` — returns the block hash and `[r, s]` block
/// signature (verified against the live service; the parameter is `blockNumber` for this endpoint).
pub(crate) async fn get_signature(
    Extension(state): Extension<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, FeederGatewayError> {
    let block_number = parse_block_number(&params)?;
    let (block_hash, signature) = state.reader.block_signature(block_number).await?;
    Ok(fg_json(&FeederGatewaySignature { block_hash, signature: [signature.0.r, signature.0.s] }))
}

/// `GET /feeder_gateway/get_block_hash_by_id?blockId=<n>` — returns the block hash of the given
/// block. The query parameter is named `blockId` to match the Python feeder gateway (verified
/// against the live service).
pub(crate) async fn get_block_hash_by_id(
    Extension(state): Extension<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, FeederGatewayError> {
    let block_number = parse_block_id(&params)?;
    let block_hash = state.reader.block_hash(block_number).await.map_err(|error| match error {
        // Verified live: a block id beyond the chain head is MALFORMED_REQUEST ("Block ID should
        // be in the range [0, <head+1>); got: <id>."), NOT BLOCK_NOT_FOUND. A synced-storage miss
        // is exactly that out-of-range condition (headers are stored contiguously from genesis).
        FeederGatewayError::BlockNotFound => {
            FeederGatewayError::MalformedRequest(format!("block id out of range: {block_number}"))
        }
        other => other,
    })?;
    Ok(fg_json(&block_hash))
}

/// `GET /feeder_gateway/get_block_id_by_hash?blockHash=0x..` — returns the bare block number of
/// the block with the given hash, or the legacy BLOCK_NOT_FOUND envelope if no synced block has it
/// (verified against the live service: the parameter is `blockHash` and the response is an
/// unquoted number).
pub(crate) async fn get_block_id_by_hash(
    Extension(state): Extension<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, FeederGatewayError> {
    let block_hash = parse_block_hash(&params)?;
    let block_number = state
        .reader
        .block_number_by_hash(block_hash)
        .await?
        .ok_or(FeederGatewayError::BlockNotFound)?;
    Ok(fg_json(&block_number))
}

/// Parses the required `blockId` query parameter as a block number. Treats the request as
/// adversarial: a missing or non-`u64` value yields a `MalformedRequest` (400) rather than a panic,
/// and `u64` parsing caps the value inherently.
///
/// Only the numeric form exists: the live feeder gateway parses `blockId` as a JSON integer and
/// rejects `latest`/`pending`/hash/`null` forms as MALFORMED_REQUEST (verified live; an earlier
/// plan note claiming otherwise was wrong).
fn parse_block_id(params: &HashMap<String, String>) -> FgResult<BlockNumber> {
    let raw = params
        .get("blockId")
        .ok_or_else(|| FeederGatewayError::MalformedRequest("missing blockId".to_string()))?;
    let block_number = raw
        .parse::<u64>()
        .map_err(|_| FeederGatewayError::MalformedRequest(format!("invalid blockId: {raw}")))?;
    Ok(BlockNumber(block_number))
}

/// Parses the required `blockHash` query parameter as a block hash. Treats the request as
/// adversarial: a missing value or a non-felt yields a `MalformedRequest` (400) rather than a
/// panic. The lowercase `0x` prefix is required, matching the live feeder gateway (it rejects
/// `0X`-prefixed and bare-hex forms).
fn parse_block_hash(params: &HashMap<String, String>) -> FgResult<BlockHash> {
    let raw = params
        .get("blockHash")
        .ok_or_else(|| FeederGatewayError::MalformedRequest("missing blockHash".to_string()))?;
    if !raw.starts_with("0x") {
        return Err(FeederGatewayError::MalformedRequest(format!("invalid blockHash: {raw}")));
    }
    let felt_value = StarkHash::from_hex(raw)
        .map_err(|_| FeederGatewayError::MalformedRequest(format!("invalid blockHash: {raw}")))?;
    Ok(BlockHash(felt_value))
}

/// Parses the required `blockNumber` query parameter as a block number (never panics on bad input;
/// `u64` parsing caps the value). Some legacy endpoints use `blockNumber` rather than `blockId`.
fn parse_block_number(params: &HashMap<String, String>) -> FgResult<BlockNumber> {
    let raw = params
        .get("blockNumber")
        .ok_or_else(|| FeederGatewayError::MalformedRequest("missing blockNumber".to_string()))?;
    let block_number = raw
        .parse::<u64>()
        .map_err(|_| FeederGatewayError::MalformedRequest(format!("invalid blockNumber: {raw}")))?;
    Ok(BlockNumber(block_number))
}
