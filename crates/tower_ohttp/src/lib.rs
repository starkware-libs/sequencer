//! Oblivious HTTP (RFC 9458) tower middleware.
//!
//! This crate provides a framework-agnostic `OhttpLayer`/`OhttpService` tower
//! middleware that decapsulates incoming `message/ohttp-req` requests, forwards
//! them to the inner service, and encapsulates the response as `message/ohttp-res`.
//! Non-OHTTP requests are forwarded to the inner service with their original
//! streaming body — the layer does not buffer, inspect, or otherwise touch them.
//! A `GET /ohttp-keys` request returns the HPKE key configuration.
//!
//! The layer wraps any `Service<Request<B>, Response = Response<B>>` where `B` is
//! the framework's native body type (same on both sides). For OHTTP requests it
//! buffers the encrypted envelope into `Full<Bytes>`, decapsulates, and rebuilds
//! the inner request — converting its body to `B` via a `body_builder` closure
//! (`Fn(Full<Bytes>) -> B`). The same closure converts the `Full<Bytes>` responses
//! the layer constructs itself (OHTTP-encrypted, error, `/ohttp-keys`) into `B`,
//! and the layer's `Error` is `S::Error`, so the inner service's error type flows
//! through unchanged.
//!
//! Consumers pass `HttpBody::new` for jsonrpsee, `axum::body::Body::new` for axum,
//! or `std::convert::identity` when the inner service already uses `Full<Bytes>`.

pub mod bhttp_codec;
pub mod errors;
pub mod gateway;
pub mod layer;

/// Client-side OHTTP primitives for tests. Enabled by the `testing` feature
/// so consumers can depend on them from `[dev-dependencies]` without pulling
/// the symbols into release builds.
#[cfg(any(test, feature = "testing"))]
pub mod test_utils;

pub use errors::OhttpError;
pub use gateway::OhttpGateway;
pub use layer::{OhttpLayer, OhttpService};

pub(crate) const OHTTP_REQUEST_CONTENT_TYPE: &str = "message/ohttp-req";
pub(crate) const OHTTP_RESPONSE_CONTENT_TYPE: &str = "message/ohttp-res";
pub(crate) const OHTTP_KEYS_PATH: &str = "/ohttp-keys";
