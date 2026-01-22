#[cfg(test)]
mod rpc_metrics_test;

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use jsonrpsee::server::middleware::rpc::RpcServiceT;
use jsonrpsee::server::MethodResponse;
use jsonrpsee::types::Request;
use jsonrpsee::Methods;
use metrics::{counter, histogram};

// Name of the metrics.
pub(crate) const INCOMING_REQUEST: &str = "rpc_incoming_requests";
pub(crate) const FAILED_REQUESTS: &str = "rpc_failed_requests";
const REQUEST_LATENCY: &str = "rpc_request_latency_seconds";

// Labels for the metrics.
pub(crate) const METHOD_LABEL: &str = "method";
pub(crate) const VERSION_LABEL: &str = "version";
pub(crate) const ILLEGAL_METHOD: &str = "illegal_method";

// Register the metrics and returns a set of the method names.
fn init_metrics(methods: &Methods) -> HashSet<String> {
    let mut methods_set: HashSet<String> = HashSet::new();
    counter!(INCOMING_REQUEST, METHOD_LABEL => ILLEGAL_METHOD).absolute(0);
    counter!(FAILED_REQUESTS, METHOD_LABEL => ILLEGAL_METHOD).absolute(0);
    for method in methods.method_names() {
        methods_set.insert(method.to_string());
        let (method_name, version) = get_method_and_version(method);
        counter!(FAILED_REQUESTS, METHOD_LABEL => method_name.clone(), VERSION_LABEL => version.clone()).absolute(0);
        counter!(INCOMING_REQUEST, METHOD_LABEL => method_name.clone(), VERSION_LABEL => version.clone()).absolute(0);
        histogram!(REQUEST_LATENCY, METHOD_LABEL => method_name, VERSION_LABEL => version)
            .record(0);
    }
    methods_set
}
#[derive(Clone)]
pub(crate) struct MetricLogger {
    // A set of all the method names the node support.
    methods_set: HashSet<String>,
}

impl MetricLogger {
    pub(crate) fn new(methods: &Methods) -> Self {
        let methods_set = init_metrics(methods);
        Self { methods_set }
    }

    pub(crate) fn on_result(&self, method_name: &str, is_success: bool, started_at: Instant) {
        // To prevent creating metrics for illegal methods.
        if self.methods_set.contains(method_name) {
            let (method, version) = get_method_and_version(method_name);
            if !is_success {
                counter!(FAILED_REQUESTS, METHOD_LABEL=> method.clone(), VERSION_LABEL=> version.clone()).increment(1);
            }
            counter!(INCOMING_REQUEST, METHOD_LABEL=> method.clone(), VERSION_LABEL=> version.clone()).increment(1);
            let latency = started_at.elapsed().as_secs_f64();
            histogram!(REQUEST_LATENCY, METHOD_LABEL=> method, VERSION_LABEL=> version)
                .record(latency);
        } else {
            counter!(INCOMING_REQUEST, METHOD_LABEL => ILLEGAL_METHOD).increment(1);
            counter!(FAILED_REQUESTS, METHOD_LABEL => ILLEGAL_METHOD).increment(1);
        }
    }
}

impl<S> tower::Layer<S> for MetricLogger {
    type Service = MetricLoggerService<S>;

    fn layer(&self, service: S) -> Self::Service {
        MetricLoggerService { service, logger: self.clone() }
    }
}

/// A middleware service that logs metrics for each RPC call.
#[derive(Clone)]
pub(crate) struct MetricLoggerService<S> {
    service: S,
    logger: MetricLogger,
}

impl<'a, S> RpcServiceT<'a> for MetricLoggerService<S>
where
    S: RpcServiceT<'a> + Send + Sync + Clone + 'static,
{
    type Future = MetricResponseFuture<S::Future>;

    fn call(&self, request: Request<'a>) -> Self::Future {
        let method_name = request.method_name().to_string();
        MetricResponseFuture {
            fut: Box::pin(self.service.call(request)),
            method_name,
            logger: self.logger.clone(),
            started_at: Instant::now(),
        }
    }
}

/// Response future that records metrics when the response is ready.
pub(crate) struct MetricResponseFuture<F> {
    fut: Pin<Box<F>>,
    method_name: String,
    logger: MetricLogger,
    started_at: Instant,
}

impl<F: Future<Output = MethodResponse>> Future for MetricResponseFuture<F> {
    type Output = MethodResponse;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let res = self.fut.as_mut().poll(cx);

        if let Poll::Ready(ref response) = res {
            self.logger.on_result(&self.method_name, response.is_success(), self.started_at);
        }

        res
    }
}

// Given method_name returns (method, version).
// Example: method_name: starknet_V0_6_0_blockNumber; output: (blockNumber, V0_6_0).
fn get_method_and_version(method_name: &str) -> (String, String) {
    // The structure of method_name is in the following format: "starknet_V0_6_0_blockNumber".
    // Only method in this format will arrive to this point in the code.
    let last_underscore_index = method_name
        .rfind('_')
        .expect("method_name should be in the following format: starknet_V0_6_0_blockNumber");

    (
        method_name[last_underscore_index + 1..].to_string(),
        method_name[9..last_underscore_index].to_string(),
    )
}
