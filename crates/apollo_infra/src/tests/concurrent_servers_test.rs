use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use strum_macros::AsRefStr;
use tokio::sync::mpsc::channel;
use tokio::sync::Semaphore;
use tokio::task;
use tokio::time::timeout;

use crate::component_client::{
    ClientResult,
    LocalComponentClient,
    RemoteClientConfig,
    RemoteComponentClient,
};
use crate::component_definitions::{
    ComponentClient,
    ComponentRequestHandler,
    ComponentStarter,
    PrioritizedRequest,
    RequestWrapper,
};
use crate::component_server::{
    ComponentServerStarter,
    ConcurrentLocalComponentServer,
    RemoteComponentServer,
};
use crate::tests::{
    AVAILABLE_PORTS,
    TEST_LOCAL_SERVER_METRICS,
    TEST_REMOTE_CLIENT_METRICS,
    TEST_REMOTE_SERVER_METRICS,
};

type TestResult = ClientResult<()>;

#[derive(Serialize, Deserialize, Debug, AsRefStr)]
enum ConcurrentComponentRequest {
    PerformAction(TestSemaphore),
}
impl PrioritizedRequest for ConcurrentComponentRequest {}

#[derive(Serialize, Deserialize, Debug)]
enum ConcurrentComponentResponse {
    PerformAction,
}

type LocalConcurrentComponentClient =
    LocalComponentClient<ConcurrentComponentRequest, ConcurrentComponentResponse>;
type RemoteConcurrentComponentClient =
    RemoteComponentClient<ConcurrentComponentRequest, ConcurrentComponentResponse>;

#[async_trait]
trait ConcurrentComponentClientTrait: Send + Sync {
    async fn perform_action(&self, field: TestSemaphore) -> TestResult;
}

#[derive(Clone)]
struct ConcurrentComponent {
    sem_a: Arc<Semaphore>,
    sem_b: Arc<Semaphore>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
enum TestSemaphore {
    A,
    B,
}

async fn perform_semaphore_operations(sem_a: &Semaphore, sem_b: &Semaphore) {
    sem_a.add_permits(1);
    let _ = sem_b.acquire().await.unwrap();
}

impl ConcurrentComponent {
    pub fn new() -> Self {
        Self { sem_a: Arc::new(Semaphore::new(0)), sem_b: Arc::new(Semaphore::new(0)) }
    }

    pub async fn perform_test(&self, field: TestSemaphore) {
        match field {
            TestSemaphore::A => perform_semaphore_operations(&self.sem_a, &self.sem_b).await,
            TestSemaphore::B => perform_semaphore_operations(&self.sem_b, &self.sem_a).await,
        };
    }
}

impl ComponentStarter for ConcurrentComponent {}

#[async_trait]
impl ComponentRequestHandler<ConcurrentComponentRequest, ConcurrentComponentResponse>
    for ConcurrentComponent
{
    async fn handle_request(
        &mut self,
        request: ConcurrentComponentRequest,
    ) -> ConcurrentComponentResponse {
        match request {
            ConcurrentComponentRequest::PerformAction(field) => {
                self.perform_test(field).await;
                ConcurrentComponentResponse::PerformAction
            }
        }
    }
}

#[async_trait]
impl<ComponentClientType> ConcurrentComponentClientTrait for ComponentClientType
where
    ComponentClientType:
        Send + Sync + ComponentClient<ConcurrentComponentRequest, ConcurrentComponentResponse>,
{
    async fn perform_action(&self, field: TestSemaphore) -> TestResult {
        match self.send(ConcurrentComponentRequest::PerformAction(field)).await? {
            ConcurrentComponentResponse::PerformAction => Ok(()),
        }
    }
}

async fn setup_concurrent_local_test() -> LocalConcurrentComponentClient {
    let component = ConcurrentComponent::new();

    let (tx_a, rx_a) =
        channel::<RequestWrapper<ConcurrentComponentRequest, ConcurrentComponentResponse>>(32);

    let local_client = LocalConcurrentComponentClient::new(tx_a);

    let max_concurrency = 10;
    let mut concurrent_local_server = ConcurrentLocalComponentServer::new(
        component,
        rx_a,
        max_concurrency,
        &TEST_LOCAL_SERVER_METRICS,
    );
    task::spawn(async move {
        let _ = concurrent_local_server.start().await;
    });

    local_client
}

async fn setup_concurrent_remote_test() -> RemoteConcurrentComponentClient {
    let local_client = setup_concurrent_local_test().await;
    let socket = AVAILABLE_PORTS.lock().await.get_next_local_host_socket();
    let config = RemoteClientConfig::default();

    let max_concurrency = 10;
    let mut concurrent_remote_server = RemoteComponentServer::new(
        local_client.clone(),
        socket.ip(),
        socket.port(),
        max_concurrency,
        TEST_REMOTE_SERVER_METRICS,
    );
    task::spawn(async move {
        let _ = concurrent_remote_server.start().await;
    });
    RemoteConcurrentComponentClient::new(
        config,
        &socket.ip().to_string(),
        socket.port(),
        TEST_REMOTE_CLIENT_METRICS,
    )
}

async fn test_server(
    client: Box<dyn ConcurrentComponentClientTrait>,
    field: TestSemaphore,
    number_of_iterations: usize,
) {
    for _ in 0..number_of_iterations {
        client.perform_action(field.clone()).await.unwrap();
    }
}

async fn perform_concurrency_test(
    client_1: Box<dyn ConcurrentComponentClientTrait>,
    client_2: Box<dyn ConcurrentComponentClientTrait>,
) {
    let number_of_iterations = 10;
    let test_task_1_handle =
        task::spawn(
            async move { test_server(client_1, TestSemaphore::A, number_of_iterations).await },
        );

    let test_task_2_handle =
        task::spawn(
            async move { test_server(client_2, TestSemaphore::B, number_of_iterations).await },
        );

    let timeout_duration = Duration::from_millis(100);
    assert!(
        timeout(timeout_duration, async {
            tokio::try_join!(test_task_1_handle, test_task_2_handle).unwrap();
        })
        .await
        .is_ok(),
        "Test timed out"
    );
}

#[tokio::test]
async fn local_concurrent_server() {
    let client = setup_concurrent_local_test().await;

    perform_concurrency_test(Box::new(client.clone()), Box::new(client)).await;
}

#[tokio::test]
async fn remote_server_concurrency() {
    let client = setup_concurrent_remote_test().await;

    perform_concurrency_test(Box::new(client.clone()), Box::new(client)).await;
}
