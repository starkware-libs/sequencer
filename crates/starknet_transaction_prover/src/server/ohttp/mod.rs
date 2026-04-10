//! Oblivious HTTP (OHTTP, RFC 9458) support for the proving service.
//!
//! When enabled, provides application-layer encryption of HTTP requests
//! and responses using HPKE. The `OhttpLayer` is a Tower middleware
//! that transparently decapsulates incoming `message/ohttp-req` requests
//! and encapsulates responses as `message/ohttp-res`, leaving the
//! JSON-RPC handlers completely unchanged.

pub(crate) mod errors;
pub mod gateway;
pub mod layer;

pub(crate) const OHTTP_REQUEST_CONTENT_TYPE: &str = "message/ohttp-req";
pub(crate) const OHTTP_RESPONSE_CONTENT_TYPE: &str = "message/ohttp-res";
pub(crate) const OHTTP_KEYS_PATH: &str = "/ohttp-keys";
