use std::collections::HashMap;

use axum::extract::Query;
use axum::response::Response;
use axum::Extension;
use starknet_api::block::BlockNumber;

use crate::errors::FeederGatewayError;
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

/// `GET /feeder_gateway/get_block_hash_by_id?blockId=<n>` — returns the block hash of the given
/// block, or the legacy error envelope if it is not synced. The query parameter is named `blockId`
/// to match the Python feeder gateway (verified against the live service).
pub(crate) async fn get_block_hash_by_id(
    Extension(state): Extension<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, FeederGatewayError> {
    let block_number = parse_block_id(&params)?;
    let block_hash = state.reader.block_hash(block_number).await?;
    Ok(fg_json(&block_hash))
}

/// Parses the required `blockId` query parameter as a block number. Treats the request as
/// adversarial: a missing or non-`u64` value yields a `MalformedRequest` (400) rather than a panic,
/// and `u64` parsing caps the value inherently.
///
/// TODO(feeder_gateway): `blockId` also accepts `latest`/`pending`/a block hash on the Python feeder
/// gateway; only the numeric form is handled for now.
fn parse_block_id(params: &HashMap<String, String>) -> FgResult<BlockNumber> {
    let raw = params
        .get("blockId")
        .ok_or_else(|| FeederGatewayError::MalformedRequest("missing blockId".to_string()))?;
    let block_number = raw
        .parse::<u64>()
        .map_err(|_| FeederGatewayError::MalformedRequest(format!("invalid blockId: {raw}")))?;
    Ok(BlockNumber(block_number))
}
