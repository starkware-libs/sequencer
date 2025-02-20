use std::convert::TryInto;
use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use metrics::set_default_local_recorder;
use metrics_exporter_prometheus::PrometheusBuilder;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::channel;
use tokio::sync::Semaphore;
use tokio::task;

use crate::component_client::{ClientResult, LocalComponentClient};
use crate::component_definitions::{
    ComponentClient,
    ComponentRequestAndResponseSender,
    ComponentRequestHandler,
    ComponentStarter,
};
use crate::component_server::{ComponentServerStarter, LocalComponentServer};
use crate::tests::TEST_LOCAL_SERVER_METRICS;

type TestResult = ClientResult<()>;

#[derive(Serialize, Deserialize, Debug)]
enum TestComponentRequest {
    PerformTest,
}

#[derive(Serialize, Deserialize, Debug)]
enum TestComponentResponse {
    PerformTest,
}

type LocalTestComponentClient = LocalComponentClient<TestComponentRequest, TestComponentResponse>;

#[async_trait]
trait TestComponentClientTrait: Send + Sync {
    async fn perform_test(&self) -> TestResult;
}

#[derive(Clone)]
struct TestComponent {
    test_sem: Arc<Semaphore>,
}

impl TestComponent {
    pub fn new(test_sem: Arc<Semaphore>) -> Self {
        Self { test_sem }
    }

    pub async fn perform_test(&self) {
        let _ = self.test_sem.acquire().await.unwrap();
    }
}

impl ComponentStarter for TestComponent {}

#[async_trait]
impl ComponentRequestHandler<TestComponentRequest, TestComponentResponse> for TestComponent {
    async fn handle_request(&mut self, request: TestComponentRequest) -> TestComponentResponse {
        match request {
            TestComponentRequest::PerformTest => {
                self.perform_test().await;
                TestComponentResponse::PerformTest
            }
        }
    }
}

#[async_trait]
impl<ComponentClientType> TestComponentClientTrait for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<TestComponentRequest, TestComponentResponse>,
{
    async fn perform_test(&self) -> TestResult {
        match self.send(TestComponentRequest::PerformTest).await? {
            TestComponentResponse::PerformTest => Ok(()),
        }
    }
}

async fn setup_local_server_test() -> (Arc<Semaphore>, LocalTestComponentClient) {
    let test_sem = Arc::new(Semaphore::new(0));
    let component = TestComponent::new(test_sem.clone());

    let (tx_a, rx_a) = channel::<
        ComponentRequestAndResponseSender<TestComponentRequest, TestComponentResponse>,
    >(32);

    let local_client = LocalTestComponentClient::new(tx_a);

    let max_concurrency = 1;
    let mut local_server =
        LocalComponentServer::new(component, rx_a, max_concurrency, TEST_LOCAL_SERVER_METRICS);
    task::spawn(async move {
        let _ = local_server.start().await;
    });

    (test_sem, local_client)
}

fn usize_to_u64(value: usize) -> u64 {
    value.try_into().expect("Conversion failed")
}

fn assert_server_metrics(
    metrics_as_string: &str,
    expected_received_msgs: usize,
    expected_processed_msgs: usize,
    expected_queue_depth: usize,
) {
    let received_msgs = TEST_LOCAL_SERVER_METRICS.get_received_value(metrics_as_string);
    let processed_msgs = TEST_LOCAL_SERVER_METRICS.get_processed_value(metrics_as_string);
    let queue_depth = TEST_LOCAL_SERVER_METRICS.get_queue_depth_value(metrics_as_string);

    assert_eq!(
        received_msgs,
        Some(usize_to_u64(expected_received_msgs)),
        "unexpected value for receives_msgs_started counter, expected {} got {:?}",
        expected_received_msgs,
        received_msgs,
    );
    assert_eq!(
        processed_msgs,
        Some(usize_to_u64(expected_processed_msgs)),
        "unexpected value for processed_msgs counter, expected {} got {:?}",
        expected_processed_msgs,
        processed_msgs,
    );
    assert_eq!(
        queue_depth,
        Some(expected_queue_depth),
        "unexpected value for queue_depth, expected {} got {:?}",
        expected_queue_depth,
        queue_depth,
    );
}

#[tokio::test]
async fn only_metrics_counters_for_local_server() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);

    let (test_sem, client) = setup_local_server_test().await;

    let number_of_iterations = 10;
    test_sem.add_permits(number_of_iterations);
    for _ in 0..number_of_iterations {
        client.perform_test().await.unwrap();
    }

    let metrics_as_string = recorder.handle().render();
    assert_server_metrics(
        metrics_as_string.as_str(),
        number_of_iterations,
        number_of_iterations,
        0,
    );
}
