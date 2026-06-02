use axum::response::Response;
use axum::Extension;

use crate::reader::AppState;
use crate::serialization::fg_json;

#[cfg(test)]
#[path = "handlers_test.rs"]
mod handlers_test;

/// `GET /feeder_gateway/get_contract_addresses` — returns the configured well-known contract
/// addresses in the legacy Python feeder gateway JSON shape.
pub(crate) async fn get_contract_addresses(Extension(state): Extension<AppState>) -> Response {
    fg_json(&state.config.contract_addresses)
}
