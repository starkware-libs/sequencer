#[cfg(test)]
mod rpc_metrics_test;

use std::collections::HashSet;
use std::time::Instant;

use jsonrpsee::server::middleware::rpc::{Batch, Notification, RpcServiceT};
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

impl<S> RpcServiceT for MetricLoggerService<S>
where
    S: RpcServiceT<
            MethodResponse = MethodResponse,
            BatchResponse = MethodResponse,
            NotificationResponse = MethodResponse,
        > + Send
        + Sync
        + Clone
        + 'static,
{
    type MethodResponse = MethodResponse;
    type BatchResponse = MethodResponse;
    type NotificationResponse = MethodResponse;

    fn call<'a>(
        &self,
        request: Request<'a>,
    ) -> impl std::future::Future<Output = Self::MethodResponse> + Send + 'a {
        let method_name = request.method_name().to_string();
        let logger = self.logger.clone();
        let service = self.service.clone();
        let started_at = Instant::now();

        async move {
            let response = service.call(request).await;
            logger.on_result(&method_name, response.is_success(), started_at);
            response
        }
    }

    fn batch<'a>(
        &self,
        batch: Batch<'a>,
    ) -> impl std::future::Future<Output = Self::BatchResponse> + Send + 'a {
        let service = self.service.clone();
        async move { service.batch(batch).await }
    }

    fn notification<'a>(
        &self,
        n: Notification<'a>,
    ) -> impl std::future::Future<Output = Self::NotificationResponse> + Send + 'a {
        let service = self.service.clone();
        async move { service.notification(n).await }
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
