//! jsonrpsee RPC middleware that logs JSON-RPC traffic without params.
//!
//! The `http_request` line carries no method name, and a failed call still gets status 200. This
//! layer logs one `event="rpc_request"` line per single call and one `event="rpc_batch"` line per
//! batch. The README's "JSON-RPC log lines" section lists the fields. It never logs params, because
//! they carry transaction data.
//!
//! A line carries `request_id` only when jsonrpsee dispatches inside the `http_request` span, as it
//! does for calls over HTTP and not for calls over a WebSocket connection.

use std::future::Future;
use std::time::Instant;

use jsonrpsee::server::middleware::rpc::{Batch, Notification, RpcServiceBuilder, RpcServiceT};
use jsonrpsee::server::{MethodResponse, Methods};
use jsonrpsee::types::Request;
use tower::layer::util::{Identity, Stack};
use tower::Layer;
use tracing::info;

#[cfg(test)]
#[path = "rpc_log_test.rs"]
mod rpc_log_test;

/// The `rpc_method` value for a method name the server does not register.
const UNKNOWN_RPC_METHOD: &str = "unknown";

/// The production JSON-RPC middleware, applied by both transports. `methods` must be the set the
/// server is started with, because the layer logs a call to a method absent from it as `unknown`.
pub fn prover_rpc_middleware(methods: &Methods) -> RpcServiceBuilder<Stack<RpcLogLayer, Identity>> {
    RpcServiceBuilder::new().layer(RpcLogLayer { methods: methods.clone() })
}

/// jsonrpsee RPC [`Layer`] producing [`RpcLogService`].
#[derive(Clone)]
pub struct RpcLogLayer {
    /// The methods this server registers. Only these names reach the logs.
    methods: Methods,
}

impl<S> Layer<S> for RpcLogLayer {
    type Service = RpcLogService<S>;

    fn layer(&self, service: S) -> Self::Service {
        RpcLogService { service, methods: self.methods.clone() }
    }
}

/// Logs the calls and batches that `service` handles. See the module docs for the lines.
pub struct RpcLogService<S> {
    service: S,
    methods: Methods,
}

impl<S> RpcServiceT for RpcLogService<S>
where
    S: RpcServiceT<MethodResponse = MethodResponse>,
{
    type MethodResponse = MethodResponse;
    type NotificationResponse = S::NotificationResponse;
    type BatchResponse = S::BatchResponse;

    fn call<'a>(
        &self,
        request: Request<'a>,
    ) -> impl Future<Output = Self::MethodResponse> + Send + 'a {
        // The name comes from the registered set, so a client cannot write arbitrary text into the
        // logs.
        let rpc_method = self
            .methods
            .method_with_name(request.method_name())
            .map_or(UNKNOWN_RPC_METHOD, |(registered_name, _callback)| registered_name);
        // The clock starts before dispatch, because jsonrpsee runs synchronous methods inside
        // `call` itself.
        let start = Instant::now();
        let response_future = self.service.call(request);
        async move {
            let response = response_future.await;
            let outcome = if response.is_error() { "error" } else { "success" };
            let latency_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
            info!(
                event = "rpc_request",
                rpc_method,
                outcome,
                rpc_error_code = response.as_error_code(),
                latency_ms,
                "JSON-RPC call handled."
            );
            response
        }
    }

    /// jsonrpsee's batch handling dispatches each entry below this layer, so the calls in a batch
    /// log no lines of their own. `batch_size` counts every entry, including notifications and
    /// entries jsonrpsee cannot parse.
    fn batch<'a>(&self, batch: Batch<'a>) -> impl Future<Output = Self::BatchResponse> + Send + 'a {
        let batch_size = batch.len();
        let start = Instant::now();
        let response_future = self.service.batch(batch);
        async move {
            let response = response_future.await;
            let latency_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
            info!(event = "rpc_batch", batch_size, latency_ms, "JSON-RPC batch handled.");
            response
        }
    }

    /// Forwards without a line. jsonrpsee acknowledges a notification without running the method
    /// it names.
    fn notification<'a>(
        &self,
        notification: Notification<'a>,
    ) -> impl Future<Output = Self::NotificationResponse> + Send + 'a {
        self.service.notification(notification)
    }
}
