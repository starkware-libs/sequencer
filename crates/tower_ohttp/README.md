# tower_ohttp

Framework-agnostic tower middleware for Oblivious HTTP (RFC 9458). Wraps any
`tower::Service<http::Request<B>, Response = http::Response<B>>` — where `B`
is the framework's native body type — and transparently handles OHTTP
envelope encryption:

- `GET /ohttp-keys` → returns the HPKE key configuration.
- `POST /` with `Content-Type: message/ohttp-req` → decapsulates the outer
  envelope, rebuilds the inner HTTP request (method, path, headers, body all
  preserved from the encrypted Binary HTTP payload), forwards it to the inner
  service, and encapsulates the response as `message/ohttp-res`.
- Everything else → forwarded to the inner service untouched, with the
  original streaming body. The layer does not buffer or inspect non-OHTTP
  request bodies, and the `body_limit` does not apply to them.

The crate has no framework dependencies. For OHTTP requests the layer buffers
the encrypted envelope into `Full<Bytes>`, decapsulates it, and rebuilds the
inner request — converting its body to the inner service's body type `B` via
a `body_builder` closure the consumer supplies at construction. The same
closure converts any responses the layer constructs itself (OHTTP-encrypted
responses, error responses, the `/ohttp-keys` response). Non-OHTTP requests
bypass the closure entirely.

## Usage

Load the HPKE private key once at startup, then plug `OhttpLayer` into your
framework's tower stack. The only framework-specific piece is the
`body_builder` function — a `Fn(Full<Bytes>) -> B` where `B` is the
framework's native body type. The same closure handles both request-body
buffering and layer-owned responses (OHTTP-encrypted, error, `/ohttp-keys`).

```rust
use std::sync::Arc;
use tower_ohttp::{OhttpGateway, OhttpLayer};

// Load from `OHTTP_KEY` env var (64 hex chars = 32-byte X25519 private key).
// Use `OhttpGateway::from_ikm(...)` if you load keys from a secrets manager.
let gateway = Arc::new(OhttpGateway::from_env()?);
```

### axum

`axum::body::Body` is the same type on both sides of `Service`, and axum's
`Error = Infallible`, so the layer plugs in with zero glue:

```rust
use axum::body::Body;

let ohttp_layer = OhttpLayer::new(
    gateway,
    5 * 1024 * 1024,  // body_limit (bytes)
    3600,             // key_cache_max_age_secs
    Body::new,        // body_builder
);

let app = api_router.layer(ohttp_layer);
```

No `HandleErrorLayer`, no `MapRequestBodyLayer`, no `map_err` — `Body::new`
serves as the universal `Full<Bytes> → Body` adapter for both directions,
and the layer's `Error = S::Error = Infallible` flows through unchanged.

### jsonrpsee

```rust
use jsonrpsee::server::{HttpBody, ServerBuilder};
use tower::ServiceBuilder;

let ohttp_layer = OhttpLayer::new(
    gateway,
    5 * 1024 * 1024,
    3600,
    HttpBody::new,    // body_builder
);

// ServerBuilder::default()
//     .set_http_middleware(ServiceBuilder::new().layer(ohttp_layer))
//     .build(addr).await?;
```

Compression with OHTTP is correctness-sensitive: compressing ciphertext is
wasted work, and compress-then-encrypt (compression between the layer and
the inner service) requires a
`tower_http::map_response_body::MapResponseBodyLayer::new(HttpBody::new)`
between `CompressionLayer` and `OhttpLayer` to normalize `CompressionBody<HttpBody>`
back to `HttpBody`. See `tower_ohttp`'s integration tests for a complete
example.

## Key types

- [`OhttpGateway`](src/gateway.rs) — HPKE key state. Construct once at startup
  from `OHTTP_KEY` env var, a hex string, or raw keying material.
- [`OhttpLayer`](src/layer.rs) / [`OhttpService`](src/layer.rs) — the tower
  `Layer`/`Service` that handles OHTTP traffic. Both are generic over a
  body-builder closure `Fn(Full<Bytes>) -> ResBody`. For a stable sized type
  that implements `Copy`, pass a `fn` pointer (e.g. `HttpBody::new`).
- [`OhttpError`](src/errors.rs) — unified error type for both gateway
  initialization and per-request processing. Has `into_response()` for
  consumers that want to emit OHTTP error responses from their own handlers.
