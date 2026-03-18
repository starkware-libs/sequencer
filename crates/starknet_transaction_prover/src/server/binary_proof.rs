//! Tower middleware that intercepts `POST /proof` requests and returns the proof as raw binary
//! bytes (`application/octet-stream`) instead of JSON with base64 encoding.
//!
//! The JSON-RPC `proveTransaction` endpoint returns the proof as a base64-encoded string inside
//! JSON, which adds ~33% overhead. This middleware provides an alternative endpoint that returns
//! the raw proof bytes directly, reducing response size and avoiding base64 encode/decode costs.
//!
//! The proof metadata (proof_facts, l2_to_l1_messages) is returned in response headers as JSON,
//! keeping the binary body clean.
//!
//! ## Wire format
//!
//! **Request**: Same JSON body as `proveTransaction`:
//! ```json
//! {
//!   "block_id": "latest",
//!   "transaction": { ... }
//! }
//! ```
//!
//! **Response**:
//! - `Content-Type: application/octet-stream`
//! - `X-Proof-Facts: <JSON array of hex strings>`
//! - `X-L2-To-L1-Messages: <JSON array of MessageToL1>`
//! - Body: raw proof bytes (Vec<u32> as big-endian bytes)

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use http::header::CONTENT_TYPE;
use http::{Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use tokio::sync::Semaphore;
use tower::{Layer, Service};
use tracing::warn;

use crate::proving::virtual_snos_prover::RpcVirtualSnosProver;

/// Request body for the binary proof endpoint.
#[derive(serde::Deserialize)]
struct BinaryProveRequest {
    block_id: blockifier_reexecution::state_reader::rpc_objects::BlockId,
    transaction: starknet_api::rpc_transaction::RpcTransaction,
}

/// Tower layer that wraps services with [`BinaryProofService`].
#[derive(Clone)]
pub struct BinaryProofLayer {
    prover: RpcVirtualSnosProver,
    semaphore: Arc<Semaphore>,
    max_concurrent_requests: usize,
}

impl BinaryProofLayer {
    pub fn new(
        prover: RpcVirtualSnosProver,
        max_concurrent_requests: usize,
        semaphore: Arc<Semaphore>,
    ) -> Self {
        Self { prover, semaphore, max_concurrent_requests }
    }
}

impl<S> Layer<S> for BinaryProofLayer {
    type Service = BinaryProofService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        BinaryProofService {
            inner,
            prover: self.prover.clone(),
            semaphore: self.semaphore.clone(),
            max_concurrent_requests: self.max_concurrent_requests,
        }
    }
}

/// Tower service that intercepts `POST /proof` and returns raw binary proof bytes.
/// All other requests are forwarded to the inner service (jsonrpsee).
#[derive(Clone)]
pub struct BinaryProofService<S> {
    inner: S,
    prover: RpcVirtualSnosProver,
    semaphore: Arc<Semaphore>,
    max_concurrent_requests: usize,
}

/// A boxed HTTP body type that can hold either our binary response or the inner service's response.
type BoxBody =
    http_body_util::combinators::UnsyncBoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>;

fn full_to_boxed(body: Full<Bytes>) -> BoxBody {
    body.map_err(|never| match never {}).boxed_unsync()
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for BinaryProofService<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    ReqBody: hyper::body::Body<Data = Bytes> + Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    ResBody: hyper::body::Body<Data = Bytes> + Send + 'static,
    ResBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Response = Response<BoxBody>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        // Only intercept POST /proof.
        if req.method() == http::Method::POST && req.uri().path() == "/proof" {
            let prover = self.prover.clone();
            let semaphore = self.semaphore.clone();
            let max_concurrent_requests = self.max_concurrent_requests;

            Box::pin(async move {
                handle_binary_proof(req, prover, semaphore, max_concurrent_requests).await
            })
        } else {
            let mut inner = self.inner.clone();
            Box::pin(async move {
                let response = inner.call(req).await.map_err(Into::into)?;
                Ok(response.map(|body| body.map_err(Into::into).boxed_unsync()))
            })
        }
    }
}

async fn handle_binary_proof<ReqBody>(
    req: Request<ReqBody>,
    prover: RpcVirtualSnosProver,
    semaphore: Arc<Semaphore>,
    max_concurrent_requests: usize,
) -> Result<Response<BoxBody>, Box<dyn std::error::Error + Send + Sync>>
where
    ReqBody: hyper::body::Body<Data = Bytes> + Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    // Acquire concurrency permit.
    let _permit = match semaphore.try_acquire() {
        Ok(permit) => permit,
        Err(_) => {
            warn!(max_concurrent_requests, "Rejected binary proof request: service is at capacity");
            return Ok(Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(full_to_boxed(Full::new(Bytes::from("Service at capacity"))))?);
        }
    };

    // Read and parse the request body.
    let body_bytes: Bytes = BodyExt::collect(req.into_body()).await.map_err(Into::into)?.to_bytes();
    let prove_request: BinaryProveRequest = match serde_json::from_slice(&body_bytes) {
        Ok(req) => req,
        Err(err) => {
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(full_to_boxed(Full::new(Bytes::from(format!("Invalid request: {err}")))))?);
        }
    };

    // Run the prover.
    let result =
        match prover.prove_transaction(prove_request.block_id, prove_request.transaction).await {
            Ok(result) => result,
            Err(err) => {
                warn!("binary prove_transaction failed: {:?}", err);
                return Ok(Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(
                    full_to_boxed(Full::new(Bytes::from(format!("Proving failed: {err}")))),
                )?);
            }
        };

    // Convert proof Vec<u32> to raw big-endian bytes.
    let proof_bytes: Vec<u8> = result.proof.iter().flat_map(|n| n.to_be_bytes()).collect();

    // Serialize metadata as JSON headers.
    let proof_facts_json = serde_json::to_string(&result.proof_facts)?;
    let messages_json = serde_json::to_string(&result.l2_to_l1_messages)?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/octet-stream")
        .header("X-Proof-Facts", &proof_facts_json)
        .header("X-L2-To-L1-Messages", &messages_json)
        .body(full_to_boxed(Full::new(Bytes::from(proof_bytes))))?)
}
