use std::fmt::Debug;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use rstest::rstest;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::channel;
use tokio::task;
use tracing::debug;

use super::AVAILABLE_PORTS;
use crate::component_client::{ClientResult, LocalComponentClient, RemoteComponentClient};
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

type TestValue = u64;
type TestResult = ClientResult<TestValue>;

#[derive(Serialize, Deserialize, Debug)]
enum ConcurrentComponentRequest {
    GetValue(String, TestValue),
}

#[derive(Serialize, Deserialize, Debug)]
enum ConcurrentComponentResponse {
    GetValue(TestValue),
}

type LocalConcurrentComponentClient =
    LocalComponentClient<ConcurrentComponentRequest, ConcurrentComponentResponse>;
type RemoteConcurrentComponentClient =
    RemoteComponentClient<ConcurrentComponentRequest, ConcurrentComponentResponse>;

#[async_trait]
trait ConcurrentComponentClientTrait: Send + Sync {
    async fn get_value(&self, name: String, value: TestValue) -> TestResult;
    fn clone_box(&self) -> Box<dyn ConcurrentComponentClientTrait>;
}

#[derive(Clone)]
struct ConcurrentComponent {
    a: Arc<AtomicU64>,
    b: Arc<AtomicU64>,
}

async fn perform_atomic_operations(a: &AtomicU64, b: &AtomicU64, value: u64) -> u64 {
    while b.load(Ordering::Relaxed) != 0 {
        task::yield_now().await;
    }
    b.store(value, Ordering::Relaxed);
    while a.load(Ordering::Relaxed) == 0 {
        task::yield_now().await;
    }
    let a_value = a.load(Ordering::Relaxed);
    a.store(0, Ordering::Relaxed);
    a_value
}

impl ConcurrentComponent {
    pub fn new() -> Self {
        Self { a: Arc::new(AtomicU64::new(0)), b: Arc::new(AtomicU64::new(0)) }
    }

    pub async fn get_value(&self, name: String, value: TestValue) -> TestValue {
        debug!("[in] get_value: name: {}, value: {}", name, value);
        let res = if name == "a" {
            perform_atomic_operations(&self.a, &self.b, value).await
        } else {
            perform_atomic_operations(&self.b, &self.a, value).await
        };
        debug!("[out] get_value: name: {}, value: {}", name, res);
        res
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
            ConcurrentComponentRequest::GetValue(name, value) => {
                ConcurrentComponentResponse::GetValue(self.get_value(name, value).await)
            }
        }
    }
}

#[async_trait]
impl ConcurrentComponentClientTrait for LocalConcurrentComponentClient {
    async fn get_value(&self, name: String, value: TestValue) -> TestResult {
        match self.send(ConcurrentComponentRequest::GetValue(name, value)).await? {
            ConcurrentComponentResponse::GetValue(value) => Ok(value),
        }
    }

    fn clone_box(&self) -> Box<dyn ConcurrentComponentClientTrait> {
        Box::new(self.clone())
    }
}

#[async_trait]
impl ConcurrentComponentClientTrait for RemoteConcurrentComponentClient {
    async fn get_value(&self, name: String, value: TestValue) -> TestResult {
        match self.send(ConcurrentComponentRequest::GetValue(name, value)).await? {
            ConcurrentComponentResponse::GetValue(value) => Ok(value),
        }
    }

    fn clone_box(&self) -> Box<dyn ConcurrentComponentClientTrait> {
        Box::new(self.clone())
    }
}

async fn setup_concurrent_test(remote: bool) -> Box<dyn ConcurrentComponentClientTrait> {
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

        let mut concurrent_remote_server = RemoteComponentServer::new(local_client.clone(), socket);
        task::spawn(async move {
            let _ = concurrent_remote_server.start().await;
        });
        return Box::new(RemoteConcurrentComponentClient::new(config, socket));
    }

    Box::new(local_client)
}

async fn test_server(
    client: Box<dyn ConcurrentComponentClientTrait>,
    name: String,
    send_values: Vec<TestValue>,
    expected_values: Vec<TestValue>,
) -> Result<(), String> {
    for (send_value, expected_value) in send_values.iter().zip(expected_values.iter()) {
        let value =
            client.get_value(name.clone(), *send_value).await.map_err(|_e| "Cant read a value")?;
        if value != *expected_value {
            return Err(format!("[{name}]: Expected value: {}, got: {}", *expected_value, value));
        }
    }
    Ok(())
}

#[rstest]
#[case::local_concurrent_server(false)]
#[case::remote_server_concurrency(true)]
#[tokio::test]
async fn test_concurrency(#[case] remote: bool) {
    let client_1 = setup_concurrent_test(remote).await;
    let client_2 = client_1.clone_box();

    let set_test_1: Vec<TestValue> = (1..=10).collect();
    let set_test_1_cloned = set_test_1.clone();
    let set_test_2: Vec<TestValue> = (90..=99).collect();
    let set_test_2_cloned = set_test_2.clone();

    let test_thread_1_handle =
        task::spawn(
            async move { test_server(client_1, "a".to_string(), set_test_1, set_test_2).await },
        );

    let test_thread_2_handle = task::spawn(async move {
        test_server(client_2, "b".to_string(), set_test_2_cloned, set_test_1_cloned).await
    });

    let res = tokio::try_join!(test_thread_1_handle, test_thread_2_handle).unwrap();

    assert!(res.0.is_ok());
    assert!(res.1.is_ok());
}
