use std::cmp::min;
use std::convert::TryInto;
use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use metrics::set_default_local_recorder;
use metrics_exporter_prometheus::PrometheusBuilder;
use serde::{Deserialize, Serialize};
use strum_macros::AsRefStr;
use tokio::sync::mpsc::{channel, Receiver};
use tokio::sync::Semaphore;
use tokio::task::{self, JoinSet};

use crate::component_client::{
    ClientResult,
    LocalComponentClient,
    RemoteClientConfig,
    RemoteComponentClient,
};
use crate::component_definitions::{
    ComponentClient,
    ComponentRequestAndResponseSender,
    ComponentRequestHandler,
    ComponentStarter,
    PrioritizedRequest,
};
use crate::component_server::{
    ComponentServerStarter,
    ConcurrentLocalComponentServer,
    LocalComponentServer,
    RemoteComponentServer,
};
use crate::tests::{
    AVAILABLE_PORTS,
    TEST_LOCAL_SERVER_METRICS,
    TEST_REMOTE_CLIENT_METRICS,
    TEST_REMOTE_SERVER_METRICS,
};

type TestResult = ClientResult<()>;

const NUMBER_OF_ITERATIONS: usize = 10;

#[derive(Serialize, Deserialize, Debug, AsRefStr)]
enum TestComponentRequest {
    PerformTest,
}

impl PrioritizedRequest for TestComponentRequest {}

#[derive(Serialize, Deserialize, Debug)]
enum TestComponentResponse {
    PerformTest,
}

type LocalTestComponentClient = LocalComponentClient<TestComponentRequest, TestComponentResponse>;
type RemoteTestComponentClient = RemoteComponentClient<TestComponentRequest, TestComponentResponse>;

type TestReceiver =
    Receiver<ComponentRequestAndResponseSender<TestComponentRequest, TestComponentResponse>>;

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

    pub async fn reduce_permit(&self) {
        self.test_sem.acquire().await.unwrap().forget();
    }
}

impl ComponentStarter for TestComponent {}

#[async_trait]
impl ComponentRequestHandler<TestComponentRequest, TestComponentResponse> for TestComponent {
    async fn handle_request(&mut self, request: TestComponentRequest) -> TestComponentResponse {
        match request {
            TestComponentRequest::PerformTest => {
                self.reduce_permit().await;
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

struct BasicSetup {
    component: TestComponent,
    local_client: LocalTestComponentClient,
    rx: TestReceiver,
    test_sem: Arc<Semaphore>,
}

fn basic_test_setup() -> BasicSetup {
    let test_sem = Arc::new(Semaphore::new(0));
    let component = TestComponent::new(test_sem.clone());

    let (tx, rx) = channel::<
        ComponentRequestAndResponseSender<TestComponentRequest, TestComponentResponse>,
    >(32);

    let local_client = LocalTestComponentClient::new(tx);

    BasicSetup { component, local_client, rx, test_sem }
}

async fn setup_local_server_test() -> (Arc<Semaphore>, LocalTestComponentClient) {
    let BasicSetup { component, local_client, rx, test_sem } = basic_test_setup();

    let mut local_server = LocalComponentServer::new(component, rx, &TEST_LOCAL_SERVER_METRICS);
    task::spawn(async move {
        let _ = local_server.start().await;
    });

    (test_sem, local_client)
}

async fn setup_concurrent_local_server_test(
    max_concurrency: usize,
) -> (Arc<Semaphore>, LocalTestComponentClient) {
    let BasicSetup { component, local_client, rx, test_sem } = basic_test_setup();

    let mut concurrent_local_server = ConcurrentLocalComponentServer::new(
        component,
        rx,
        max_concurrency,
        TEST_LOCAL_SERVER_METRICS,
    );
    task::spawn(async move {
        let _ = concurrent_local_server.start().await;
    });

    (test_sem, local_client)
}

async fn setup_remote_server_test(
    max_concurrency: usize,
) -> (Arc<Semaphore>, RemoteTestComponentClient) {
    let (test_sem, local_client) = setup_local_server_test().await;
    let socket = AVAILABLE_PORTS.lock().await.get_next_local_host_socket();
    let config = RemoteClientConfig::default();

    let mut remote_server = RemoteComponentServer::new(
        local_client.clone(),
        socket.ip(),
        socket.port(),
        max_concurrency,
        TEST_REMOTE_SERVER_METRICS,
    );
    task::spawn(async move {
        let _ = remote_server.start().await;
    });
    let remote_client = RemoteTestComponentClient::new(
        config,
        &socket.ip().to_string(),
        socket.port(),
        TEST_REMOTE_CLIENT_METRICS,
    );

    (test_sem, remote_client)
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
        usize_to_u64(expected_received_msgs),
        "unexpected value for receives_msgs_started counter, expected {} got {:?}",
        expected_received_msgs,
        received_msgs,
    );
    assert_eq!(
        processed_msgs,
        usize_to_u64(expected_processed_msgs),
        "unexpected value for processed_msgs counter, expected {} got {:?}",
        expected_processed_msgs,
        processed_msgs,
    );
    assert_eq!(
        queue_depth, expected_queue_depth,
        "unexpected value for queue_depth, expected {} got {:?}",
        expected_queue_depth, queue_depth,
    );
}

fn assert_remote_server_metrics(
    metrics_as_string: &str,
    expected_total_received_msgs: usize,
    expected_valid_received_msgs: usize,
    expected_processed_msgs: usize,
) {
    let total_received_msgs =
        TEST_REMOTE_SERVER_METRICS.get_total_received_value(metrics_as_string);
    let valid_received_msgs =
        TEST_REMOTE_SERVER_METRICS.get_valid_received_value(metrics_as_string);
    let processed_msgs = TEST_REMOTE_SERVER_METRICS.get_processed_value(metrics_as_string);

    assert_eq!(
        total_received_msgs,
        usize_to_u64(expected_total_received_msgs),
        "unexpected value for total_receives_msgs_started counter, expected {} got {:?}",
        expected_total_received_msgs,
        total_received_msgs,
    );
    assert_eq!(
        valid_received_msgs,
        usize_to_u64(expected_valid_received_msgs),
        "unexpected value for valid_receives_msgs_started counter, expected {} got {:?}",
        expected_total_received_msgs,
        valid_received_msgs,
    );
    assert_eq!(
        processed_msgs,
        usize_to_u64(expected_processed_msgs),
        "unexpected value for processed_msgs counter, expected {} got {:?}",
        expected_processed_msgs,
        processed_msgs,
    );
}

#[tokio::test]
async fn only_metrics_counters_for_local_server() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);

    let (test_sem, client) = setup_local_server_test().await;

    // At the beginning all metrics counters are zero.
    let metrics_as_string = recorder.handle().render();
    assert_server_metrics(metrics_as_string.as_str(), 0, 0, 0);

    // In order to process a message the test component tries to acquire a permit from the
    // test semaphore. Current test is checking that all metrics counters actually count so we
    // need to provide enough permits for all messages to be processed.
    test_sem.add_permits(NUMBER_OF_ITERATIONS);
    for i in 0..NUMBER_OF_ITERATIONS {
        client.perform_test().await.unwrap();

        // Every time the request is sent and the response is received the metrics counters should
        // be increased by one.
        let metrics_as_string = recorder.handle().render();
        assert_server_metrics(metrics_as_string.as_str(), i + 1, i + 1, 0);
    }
}

// TODO(Tsabary): rewrite this test to verify all queue depths.
#[tokio::test]
async fn all_metrics_for_local_server() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);

    let (test_sem, client) = setup_local_server_test().await;

    // In order to test not only message counters but the queue depth too, first we will send all
    // the messages by spawning multiple clients and by that filling the channel queue.
    for _ in 0..NUMBER_OF_ITERATIONS {
        let multi_client = client.clone();
        task::spawn(async move {
            multi_client.perform_test().await.unwrap();
        });
    }
    task::yield_now().await;

    // Add permits one by one and check that all metrics are adjusted accordingly: all messages
    // should be received, the queue should be empty (depth 0), and  the number of processed
    // messages should be equal to the number of permits added.
    for i in 0..NUMBER_OF_ITERATIONS + 1 {
        let metrics_as_string = recorder.handle().render();
        assert_server_metrics(metrics_as_string.as_str(), NUMBER_OF_ITERATIONS, i, 0);
        test_sem.add_permits(1);
        task::yield_now().await;
    }
}

#[tokio::test]
async fn only_metrics_counters_for_concurrent_server() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);

    let max_concurrency = NUMBER_OF_ITERATIONS;
    let (test_sem, client) = setup_concurrent_local_server_test(max_concurrency).await;

    // Current test is checking that all metrics counters can actually count in parallel.
    // So first we send all the messages.
    let mut tasks = JoinSet::new();
    for _ in 0..NUMBER_OF_ITERATIONS {
        let multi_client = client.clone();
        tasks.spawn(async move {
            multi_client.perform_test().await.unwrap();
        });
    }
    task::yield_now().await;

    // By now all messages should be received but not processed.
    let metrics_as_string = recorder.handle().render();
    assert_server_metrics(metrics_as_string.as_str(), NUMBER_OF_ITERATIONS, 0, 0);

    // Now we provide all permits and wait for all messages to be processed.
    test_sem.add_permits(NUMBER_OF_ITERATIONS);
    tasks.join_all().await;

    // Finally all messages processed and queue is empty.
    let metrics_as_string = recorder.handle().render();
    assert_server_metrics(
        metrics_as_string.as_str(),
        NUMBER_OF_ITERATIONS,
        NUMBER_OF_ITERATIONS,
        0,
    );
}

#[tokio::test]
async fn all_metrics_for_concurrent_server() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);

    let max_concurrency = NUMBER_OF_ITERATIONS / 2;
    let (test_sem, client) = setup_concurrent_local_server_test(max_concurrency).await;

    // Current test is checking not only message counters but the queue depth too.
    // So first we send all the messages.
    for _ in 0..NUMBER_OF_ITERATIONS {
        let multi_client = client.clone();
        task::spawn(async move {
            multi_client.perform_test().await.unwrap();
        });
    }
    task::yield_now().await;

    for i in 0..NUMBER_OF_ITERATIONS {
        // After sending i permits, we should have 'max_concurrency + i + 1' received messages,
        // up to a maximum of NUMBER_OF_ITERATIONS.
        let expected_received_msgs = min(max_concurrency + 1 + i, NUMBER_OF_ITERATIONS);

        // The queue depth should be: 'NUMBER_OF_ITERATIONS - number of received messages'.
        let expected_queue_depth = NUMBER_OF_ITERATIONS - expected_received_msgs;

        let metrics_as_string = recorder.handle().render();
        assert_server_metrics(
            metrics_as_string.as_str(),
            expected_received_msgs,
            i,
            expected_queue_depth,
        );
        test_sem.add_permits(1);
        task::yield_now().await;
    }
}

#[tokio::test]
async fn metrics_counters_for_remote_server() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);

    let max_concurrency = NUMBER_OF_ITERATIONS;
    let (test_sem, remote_client) = setup_remote_server_test(max_concurrency).await;

    // At the beginning all metrics counters are zero.
    let metrics_as_string = recorder.handle().render();
    assert_server_metrics(metrics_as_string.as_str(), 0, 0, 0);

    // In order to process a message the test component tries to acquire a permit from the
    // test semaphore. Current test is checking that all metrics counters actually count so we
    // need to provide enough permits for all messages to be processed.
    test_sem.add_permits(NUMBER_OF_ITERATIONS);
    for i in 0..NUMBER_OF_ITERATIONS {
        remote_client.perform_test().await.unwrap();

        // Every time the request is sent and the response is received the metrics counters should
        // be increased by one.
        let metrics_as_string = recorder.handle().render();
        assert_remote_server_metrics(metrics_as_string.as_str(), i + 1, i + 1, i + 1);
    }
}
