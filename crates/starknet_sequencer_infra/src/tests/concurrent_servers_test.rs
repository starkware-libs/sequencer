use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rstest::rstest;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::channel;
use tokio::sync::Semaphore;
use tokio::task;
use tokio::time::timeout;

use crate::component_client::{Client, ClientResult, LocalComponentClient, RemoteComponentClient};
use crate::component_definitions::{
    ComponentClient,
    ComponentRequestAndResponseSender,
    ComponentRequestHandler,
    ComponentStarter,
    RemoteClientConfig,
};
use crate::component_server::{
    ComponentServerStarter,
    ConcurrentLocalComponentServer,
    RemoteComponentServer,
};
use crate::tests::AVAILABLE_PORTS;

type TestResult = ClientResult<()>;

#[derive(Serialize, Deserialize, Debug)]
enum ConcurrentComponentRequest {
    PerformAction(TestSemaphore),
}

#[derive(Serialize, Deserialize, Debug)]
enum ConcurrentComponentResponse {
    PerformAction,
}

type LocalConcurrentComponentClient =
    LocalComponentClient<ConcurrentComponentRequest, ConcurrentComponentResponse>;
type RemoteConcurrentComponentClient =
    RemoteComponentClient<ConcurrentComponentRequest, ConcurrentComponentResponse>;
type ConcurrentComponentClient = Client<ConcurrentComponentRequest, ConcurrentComponentResponse>;

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

async fn setup_concurrent_test(remote: bool) -> ConcurrentComponentClient {
    let component = ConcurrentComponent::new();

    let (tx_a, rx_a) = channel::<
        ComponentRequestAndResponseSender<ConcurrentComponentRequest, ConcurrentComponentResponse>,
    >(32);

    let local_client = LocalConcurrentComponentClient::new(tx_a);

    let mut concurrent_local_server = ConcurrentLocalComponentServer::new(component, rx_a);
    task::spawn(async move {
        let _ = concurrent_local_server.start().await;
    });

    if remote {
        let socket = AVAILABLE_PORTS.lock().await.get_next_local_host_socket();
        let config = RemoteClientConfig::default();

        let mut concurrent_remote_server =
            RemoteComponentServer::new(local_client.clone(), socket.ip(), socket.port());
        task::spawn(async move {
            let _ = concurrent_remote_server.start().await;
        });
        return ConcurrentComponentClient::new(
            None,
            Some(RemoteConcurrentComponentClient::new(
                config,
                &socket.ip().to_string(),
                socket.port(),
            )),
        );
    }

    ConcurrentComponentClient::new(Some(local_client), None)
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

async fn get_client(
    remote: bool,
) -> (Box<dyn ConcurrentComponentClientTrait>, Box<dyn ConcurrentComponentClientTrait>) {
    let clients = setup_concurrent_test(remote).await;
    match remote {
        true => (
            Box::new(clients.get_remote_client().unwrap()),
            Box::new(clients.get_remote_client().unwrap()),
        ),
        false => (
            Box::new(clients.get_local_client().unwrap()),
            Box::new(clients.get_local_client().unwrap()),
        ),
    }
}

#[rstest]
#[case::local_concurrent_server(false)]
#[case::remote_server_concurrency(true)]
#[tokio::test]
async fn test_concurrency(#[case] remote: bool) {
    let (client_1, client_2) = get_client(remote).await;

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
