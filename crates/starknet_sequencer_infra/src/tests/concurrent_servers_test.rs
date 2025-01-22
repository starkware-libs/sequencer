use std::fmt::Debug;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::channel;
use tokio::task;
use tracing::debug;

use crate::component_client::{ClientResult, LocalComponentClient};
use crate::component_definitions::{
    ComponentClient,
    ComponentRequestAndResponseSender,
    ComponentRequestHandler,
    ComponentStarter,
};
use crate::component_server::{ComponentServerStarter, ConcurrentLocalComponentServer};

type TestValue = u64;
type TestResult = ClientResult<TestValue>;

#[derive(Serialize, Deserialize, Debug)]
enum ConcurrentComponentRequest {
    GetValue(TestField, TestValue),
}

#[derive(Serialize, Deserialize, Debug)]
enum ConcurrentComponentResponse {
    GetValue(TestValue),
}

type LocalConcurrentComponentClient =
    LocalComponentClient<ConcurrentComponentRequest, ConcurrentComponentResponse>;

#[async_trait]
trait ConcurrentComponentClientTrait: Send + Sync {
    async fn get_value(&self, field: TestField, value: TestValue) -> TestResult;
}

#[derive(Clone)]
struct ConcurrentComponent {
    a: Arc<AtomicU64>,
    b: Arc<AtomicU64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
enum TestField {
    A,
    B,
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

    pub async fn get_value(&self, field: TestField, value: TestValue) -> TestValue {
        debug!("[in] get_value: field: {:?}, value: {}", field, value);
        let res = match field {
            TestField::A => perform_atomic_operations(&self.a, &self.b, value).await,
            TestField::B => perform_atomic_operations(&self.b, &self.a, value).await,
        };
        debug!("[out] get_value: field: {:?}, value: {}", field, res);
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
impl ConcurrentComponentClientTrait
    for LocalComponentClient<ConcurrentComponentRequest, ConcurrentComponentResponse>
{
    async fn get_value(&self, field: TestField, value: TestValue) -> TestResult {
        match self.send(ConcurrentComponentRequest::GetValue(field, value)).await? {
            ConcurrentComponentResponse::GetValue(value) => Ok(value),
        }
    }
}

async fn setup_concurrent_test() -> LocalConcurrentComponentClient {
    let component = ConcurrentComponent::new();

    let (tx_a, rx_a) = channel::<
        ComponentRequestAndResponseSender<ConcurrentComponentRequest, ConcurrentComponentResponse>,
    >(32);

    let local_client = LocalConcurrentComponentClient::new(tx_a);

    let mut concurrent_local_server = ConcurrentLocalComponentServer::new(component, rx_a);
    task::spawn(async move {
        let _ = concurrent_local_server.start().await;
    });

    local_client
}

async fn test_server(
    client: impl ConcurrentComponentClientTrait,
    field: TestField,
    send_and_expect_values: Vec<(TestValue, TestValue)>,
) {
    for (send_value, expected_value) in send_and_expect_values {
        let value = client.get_value(field.clone(), send_value).await.unwrap();
        if value != expected_value {
            panic!("[{:?}]: Expected value: {}, got: {}", field, expected_value, value);
        }
    }
}

#[tokio::test]
async fn test_local_concurrent_server() {
    let client_1 = setup_concurrent_test().await;
    let client_2 = client_1.clone();

    let send_and_expect_values_thread_1: Vec<(TestValue, TestValue)> =
        (1..=10).zip(90..=99).collect();
    let send_and_expect_values_thread_2: Vec<(TestValue, TestValue)> =
        (90..=99).zip(1..=10).collect();

    let test_thread_1_handle = task::spawn(async move {
        test_server(client_1, TestField::A, send_and_expect_values_thread_1).await
    });

    let test_thread_2_handle = task::spawn(async move {
        test_server(client_2, TestField::B, send_and_expect_values_thread_2).await
    });

    tokio::try_join!(test_thread_1_handle, test_thread_2_handle).unwrap();
}
