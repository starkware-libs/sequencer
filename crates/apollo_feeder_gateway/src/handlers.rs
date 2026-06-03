use std::collections::HashMap;

use axum::extract::Query;
use axum::response::Response;
use axum::Extension;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::hash::StarkHash;

use crate::errors::FeederGatewayError;
use crate::legacy_params::{parse_legacy_json_scalar, LegacyJsonScalar};
use crate::objects::{FeederGatewayContractAddressesResponse, FeederGatewaySignature};
use crate::reader::{AppState, FgResult};
use crate::serialization::fg_json;

#[cfg(test)]
#[path = "handlers_test.rs"]
mod handlers_test;

/// `GET /feeder_gateway/get_contract_addresses` — returns the configured well-known contract
/// addresses in the legacy Python feeder gateway JSON shape (ordered EIP-55 L1 contracts followed
/// by the two L2 fee-token felts).
pub(crate) async fn get_contract_addresses(Extension(state): Extension<AppState>) -> Response {
    fg_json(&FeederGatewayContractAddressesResponse(&state.config.contract_addresses))
}

/// `GET /feeder_gateway/get_public_key` — returns the configured sequencer public key as a bare
/// felt, matching the Python feeder gateway (verified against the live service).
pub(crate) async fn get_public_key(Extension(state): Extension<AppState>) -> Response {
    fg_json(&state.config.sequencer_public_key)
}

/// `GET /feeder_gateway/get_signature?blockNumber=<n>` — returns the block hash and `[r, s]` block
/// signature. Live semantics (verified 2026-06-03): a missing or `null` blockNumber serves the
/// LATEST synced block's signature; the value must be a non-negative JSON integer (booleans count,
/// Python-style; floats are rejected); integers beyond u64 are BLOCK_NOT_FOUND.
pub(crate) async fn get_signature(
    Extension(state): Extension<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, FeederGatewayError> {
    let block_number = match parse_block_number(&params)? {
        Some(block_number) => block_number,
        None => latest_block_number(&state).await?,
    };
    let (block_hash, signature) = state.reader.block_signature(block_number).await?;
    Ok(fg_json(&FeederGatewaySignature { block_hash, signature: [signature.0.r, signature.0.s] }))
}

/// The latest synced block number, for the endpoints whose missing/`null` block argument means
/// "latest". An empty chain maps to the block-0 not-found envelope.
async fn latest_block_number(state: &AppState) -> FgResult<BlockNumber> {
    state
        .reader
        .latest_block_header()
        .await?
        .map(|header| header.block_header_without_hash.block_number)
        .ok_or(FeederGatewayError::BlockNotFound(BlockNumber(0)))
}

/// `GET /feeder_gateway/get_block_hash_by_id?blockId=<n>` — returns the block hash of the given
/// block. The query parameter is named `blockId` to match the Python feeder gateway (verified
/// against the live service).
pub(crate) async fn get_block_hash_by_id(
    Extension(state): Extension<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, FeederGatewayError> {
    let (block_number, python_int_echo) = match parse_block_id(&params)? {
        CoercedBlockId::InRange { block_number, python_int_echo } => {
            (block_number, python_int_echo)
        }
        CoercedBlockId::OutOfRange { python_int_echo } => {
            return Err(block_id_out_of_range(&state, &python_int_echo).await);
        }
    };
    match state.reader.block_hash(block_number).await {
        Ok(block_hash) => Ok(fg_json(&block_hash)),
        // Verified live: a block id beyond the chain head is MALFORMED_REQUEST with the range
        // message, NOT BLOCK_NOT_FOUND; a synced-storage miss is exactly that out-of-range
        // condition (headers are stored contiguously from genesis).
        Err(FeederGatewayError::BlockNotFound(_)) => {
            Err(block_id_out_of_range(&state, &python_int_echo).await)
        }
        Err(other) => Err(other),
    }
}

/// Builds the live out-of-range blockId message; the bound is one past OUR latest synced block.
/// The live service's bound is its block-ID assignment counter, which runs AHEAD of the latest
/// finalized block (observed live: bound 10421102 vs latest 10415834) — instance state a
/// re-serving node cannot replicate, so the FORMAT is parity while the bound reflects this node's
/// view.
async fn block_id_out_of_range(state: &AppState, python_int_echo: &str) -> FeederGatewayError {
    let block_id_bound = match state.reader.latest_block_header().await {
        Ok(Some(header)) => header.block_header_without_hash.block_number.0 + 1,
        _ => 0,
    };
    FeederGatewayError::MalformedRequest(format!(
        "Block ID should be in the range [0, {block_id_bound}); got: {python_int_echo}."
    ))
}

/// `GET /feeder_gateway/get_block_id_by_hash?blockHash=0x..` — returns the bare block number of
/// the block with the given hash, or the legacy BLOCK_NOT_FOUND envelope if no synced block has it
/// (verified against the live service: the parameter is `blockHash` and the response is an
/// unquoted number).
pub(crate) async fn get_block_id_by_hash(
    Extension(state): Extension<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, FeederGatewayError> {
    let (block_hash, raw_block_hash) = parse_block_hash(&params)?;
    let block_number = state
        .reader
        .block_number_by_hash(block_hash)
        .await?
        // The live message echoes the request's raw hash string verbatim.
        .ok_or_else(|| FeederGatewayError::BlockHashNotFound(raw_block_hash.to_string()))?;
    Ok(fg_json(&block_number))
}

/// A `blockId` value after Python's `int(json.loads(value))` coercion: a candidate block number,
/// or a value that can never be a block (negative or beyond u64). `python_int_echo` is the
/// coerced integer as Python would print it in the out-of-range message.
enum CoercedBlockId {
    InRange { block_number: BlockNumber, python_int_echo: String },
    OutOfRange { python_int_echo: String },
}

/// Parses the required `blockId` query parameter with the live semantics (verified 2026-06-03):
/// Python's `int(json.loads(value))`, so floats truncate (1.5 is block 1), booleans coerce (true
/// is block 1), `null` fails `int(None)` with the echoed TypeError, a missing field echoes the
/// KeyError, and non-JSON forms (`latest`/`pending`/hashes) echo the `json.loads` error. Treats
/// the request as adversarial: every malformed form is a 400, never a panic.
fn parse_block_id(params: &HashMap<String, String>) -> FgResult<CoercedBlockId> {
    let raw = params
        .get("blockId")
        .ok_or_else(|| FeederGatewayError::MalformedRequest("'blockId'".to_string()))?;
    match parse_legacy_json_scalar(raw).map_err(FeederGatewayError::MalformedRequest)? {
        LegacyJsonScalar::Null => Err(FeederGatewayError::MalformedRequest(
            "int() argument must be a string, a bytes-like object or a number, not 'NoneType'"
                .to_string(),
        )),
        LegacyJsonScalar::Bool(boolean_value) => Ok(CoercedBlockId::InRange {
            block_number: BlockNumber(u64::from(boolean_value)),
            python_int_echo: u64::from(boolean_value).to_string(),
        }),
        LegacyJsonScalar::Float { truncated, .. } => {
            let python_int_echo = truncated.to_string();
            match u64::try_from(truncated) {
                Ok(block_number) => Ok(CoercedBlockId::InRange {
                    block_number: BlockNumber(block_number),
                    python_int_echo,
                }),
                Err(_) => Ok(CoercedBlockId::OutOfRange { python_int_echo }),
            }
        }
        LegacyJsonScalar::Int { value: Some(block_number), python_repr, .. } => {
            Ok(CoercedBlockId::InRange {
                block_number: BlockNumber(block_number),
                python_int_echo: python_repr,
            })
        }
        LegacyJsonScalar::Int { python_repr, .. } => {
            Ok(CoercedBlockId::OutOfRange { python_int_echo: python_repr })
        }
    }
}

/// Parses the required `blockHash` query parameter, returning the parsed hash and the raw string
/// (live error messages echo the raw form verbatim). Treats the request as adversarial: a missing
/// value or a non-felt yields a `MalformedRequest` (400) rather than a panic. The lowercase `0x`
/// prefix is required, matching the live feeder gateway (it rejects `0X`-prefixed and bare-hex
/// forms).
fn parse_block_hash(params: &HashMap<String, String>) -> FgResult<(BlockHash, &str)> {
    // Both message texts replicate the live service verbatim (verified 2026-06-03), including
    // rejecting `null` and `0X`-prefixed forms with the same "should be a hexadecimal" message.
    let raw = params.get("blockHash").ok_or_else(|| {
        FeederGatewayError::MalformedRequest("Block hash must be given.".to_string())
    })?;
    let malformed_block_hash = || {
        FeederGatewayError::MalformedRequest(format!(
            "Block hash should be a hexadecimal string starting with 0x, or 'null'; got: {raw}."
        ))
    };
    if !raw.starts_with("0x") {
        return Err(malformed_block_hash());
    }
    let felt_value = StarkHash::from_hex(raw).map_err(|_| malformed_block_hash())?;
    Ok((BlockHash(felt_value), raw))
}

/// Parses the optional `blockNumber` query parameter with the live get_signature semantics
/// (verified 2026-06-03): missing or `null` means latest (`None`); the value must be a
/// non-negative JSON integer, where booleans count as 0/1 (Python's bool is an int subclass) but
/// floats are rejected (NOT coerced like `blockId`); integers beyond u64 are BLOCK_NOT_FOUND
/// since no such block can exist. Never panics on bad input.
fn parse_block_number(params: &HashMap<String, String>) -> FgResult<Option<BlockNumber>> {
    let Some(raw) = params.get("blockNumber") else {
        return Ok(None);
    };
    let non_negative_integer_required = |python_typed_echo: String| {
        FeederGatewayError::MalformedRequest(format!(
            // The missing space after ';' replicates the live message exactly.
            "Field blockNumber must be a non-negative integer, or 'null';got: {python_typed_echo}."
        ))
    };
    match parse_legacy_json_scalar(raw).map_err(FeederGatewayError::MalformedRequest)? {
        LegacyJsonScalar::Null => Ok(None),
        LegacyJsonScalar::Bool(boolean_value) => Ok(Some(BlockNumber(u64::from(boolean_value)))),
        LegacyJsonScalar::Int { value: Some(block_number), .. } => {
            Ok(Some(BlockNumber(block_number)))
        }
        LegacyJsonScalar::Int { negative: true, python_repr, .. } => {
            Err(non_negative_integer_required(format!("int({python_repr})")))
        }
        LegacyJsonScalar::Int { python_repr, .. } => {
            Err(FeederGatewayError::OversizedBlockNumberNotFound(python_repr))
        }
        LegacyJsonScalar::Float { python_repr, .. } => {
            Err(non_negative_integer_required(format!("float({python_repr})")))
        }
    }
}
