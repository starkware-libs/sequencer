//! Conversion between Binary HTTP (RFC 9292) messages and standard `http::Request`/
//! `http::Response` values. These are the pure-data helpers used by the layer;
//! they have no tower or framework dependencies.

use bytes::Bytes;
use http::header;
use http_body_util::{BodyExt, Full};
use tower::BoxError;
use tracing::debug;

use crate::errors::OhttpError;
use crate::OHTTP_RESPONSE_CONTENT_TYPE;

/// Rebuild a standard `http::Request<Full<Bytes>>` from a parsed Binary HTTP
/// message.
///
/// Method and path are required fields per RFC 9292 §3.2 — a BHTTP message
/// missing either is rejected with `OhttpError::InvalidFormat` rather than
/// silently defaulted. All BHTTP header fields are forwarded to the inner
/// request — this includes `content-type`, `accept-encoding`, and anything
/// else the client specified inside the encrypted envelope.
pub fn rebuild_request(
    bhttp_message: &bhttp::Message,
) -> Result<http::Request<Full<Bytes>>, OhttpError> {
    let body = bhttp_message.content().to_vec();

    // Method and path are required fields of a BHTTP request per RFC 9292 §3.2.
    // Missing either means the inner message is malformed.
    let method_bytes = bhttp_message
        .control()
        .method()
        .ok_or(OhttpError::InvalidFormat("missing method in BHTTP control"))?;
    let method = http::Method::from_bytes(method_bytes)
        .map_err(|_| OhttpError::InvalidFormat("invalid method in BHTTP control"))?;

    let path_bytes = bhttp_message
        .control()
        .path()
        .ok_or(OhttpError::InvalidFormat("missing path in BHTTP control"))?;
    let path = std::str::from_utf8(path_bytes)
        .map_err(|_| OhttpError::InvalidFormat("non-utf8 path in BHTTP control"))?;

    let mut builder = http::Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_LENGTH, body.len());

    // Forward all BHTTP headers to the inner request. Content-Type is essential
    // for REST routers that dispatch on it; Accept-Encoding allows the inner
    // CompressionLayer to compress the response before OHTTP encryption.
    // Skip Content-Length — we set it from the body length above.
    for field in bhttp_message.header().fields() {
        if field.name().eq_ignore_ascii_case(b"content-length") {
            continue;
        }
        builder = builder.header(field.name(), field.value());
    }

    builder.body(Full::new(Bytes::from(body))).map_err(|error| {
        debug!("Failed to rebuild inner request: {error}");
        OhttpError::InvalidFormat("failed to rebuild inner request")
    })
}

/// Encode an `http::Response` as a Binary HTTP message, then encapsulate it
/// using the supplied OHTTP `ServerResponse`.
///
/// The outer OHTTP envelope is always 200 per RFC 9458 — the inner response's
/// real status code is preserved inside the encrypted BHTTP payload, along with
/// all response headers.
pub async fn encapsulate_response<B>(
    response: http::Response<B>,
    server_response: ohttp::ServerResponse,
) -> Result<http::Response<Full<Bytes>>, OhttpError>
where
    B: http_body::Body<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError>,
{
    let status = response.status();
    let response_headers = response.headers().clone();

    let response_body = response
        .into_body()
        .collect()
        .await
        .map_err(|_| OhttpError::Internal("failed to read inner response body"))?
        .to_bytes();

    // `http::StatusCode` is 100–999; BHTTP accepts the same range per RFC 9292 §3.2,
    // so this should never fail in practice. Propagate as an internal error rather
    // than silently coercing to 500 — silent coercion would mask a real bug in the
    // inner service (e.g. a non-standard status code) under a misleading response.
    let bhttp_status = bhttp::StatusCode::try_from(u64::from(status.as_u16())).map_err(|_| {
        debug!("Inner response status {} not representable in BHTTP", status.as_u16());
        OhttpError::Internal("inner response status not representable in BHTTP")
    })?;
    let mut bhttp_response = bhttp::Message::response(bhttp_status);
    for (name, value) in &response_headers {
        bhttp_response.put_header(name.as_str(), value.as_bytes());
    }
    bhttp_response.write_content(&response_body);

    let mut bhttp_bytes = Vec::new();
    bhttp_response.write_bhttp(bhttp::Mode::KnownLength, &mut bhttp_bytes).map_err(|error| {
        debug!("Failed to encode Binary HTTP response: {error}");
        OhttpError::Internal("failed to encode BHTTP response")
    })?;

    let encrypted = server_response.encapsulate(&bhttp_bytes).map_err(|error| {
        debug!("Failed to encapsulate OHTTP response: {error}");
        OhttpError::Internal("failed to encapsulate OHTTP response")
    })?;

    Ok(http::Response::builder()
        .status(http::StatusCode::OK)
        .header(header::CONTENT_TYPE, OHTTP_RESPONSE_CONTENT_TYPE)
        .body(Full::new(Bytes::from(encrypted)))
        .expect("response builder should not fail"))
}
