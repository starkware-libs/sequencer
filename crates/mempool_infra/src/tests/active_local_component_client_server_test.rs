use std::future::pending;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use starknet_sequencer_infra::component_client::{ClientError, ClientResult, LocalComponentClient};
use starknet_sequencer_infra::component_definitions::{
    ComponentRequestAndResponseSender,
    ComponentRequestHandler,
    ComponentStarter,
};
use starknet_sequencer_infra::component_server::{
    ComponentServerStarter,
    LocalActiveComponentServer,
    WrapperServer,
};
use starknet_sequencer_infra::errors::ComponentError;
use tokio::sync::mpsc::{channel, Sender};
use tokio::sync::{Barrier, Mutex};
use tokio::task;

#[derive(Debug, Clone)]
struct ComponentC {
    counter: Arc<Mutex<usize>>,
    max_iterations: usize,
    barrier: Arc<Barrier>,
}

impl ComponentC {
    pub fn new(init_counter_value: usize, max_iterations: usize, barrier: Arc<Barrier>) -> Self {
        Self { counter: Arc::new(Mutex::new(init_counter_value)), max_iterations, barrier }
    }

    pub async fn c_get_counter(&self) -> usize {
        *self.counter.lock().await
    }

    pub async fn c_increment_counter(&self) {
        *self.counter.lock().await += 1;
    }
}

#[async_trait]
impl ComponentStarter for ComponentC {
    async fn start(&mut self) -> Result<(), ComponentError> {
        for _ in 0..self.max_iterations {
            self.c_increment_counter().await;
        }
        let val = self.c_get_counter().await;
        assert!(val >= self.max_iterations);
        self.barrier.wait().await;

        // Mimicking real start function that should not return.
        let () = pending().await;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ComponentCRequest {
    CIncCounter,
    CGetCounter,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ComponentCResponse {
    CIncCounter,
    CGetCounter(usize),
}

#[async_trait]
trait ComponentCClientTrait: Send + Sync {
    async fn c_inc_counter(&self) -> ClientResult<()>;
    async fn c_get_counter(&self) -> ClientResult<usize>;
}

struct ComponentD {
    c: Box<dyn ComponentCClientTrait>,
    max_iterations: usize,
    barrier: Arc<Barrier>,
}

impl ComponentD {
    pub fn new(
        c: Box<dyn ComponentCClientTrait>,
        max_iterations: usize,
        barrier: Arc<Barrier>,
    ) -> Self {
        Self { c, max_iterations, barrier }
    }

    pub async fn d_increment_counter(&self) {
        self.c.c_inc_counter().await.unwrap()
    }

    pub async fn d_get_counter(&self) -> usize {
        self.c.c_get_counter().await.unwrap()
    }
}

#[async_trait]
impl ComponentStarter for ComponentD {
    async fn start(&mut self) -> Result<(), ComponentError> {
        for _ in 0..self.max_iterations {
            self.d_increment_counter().await;
        }
        let val = self.d_get_counter().await;
        assert!(val >= self.max_iterations);
        self.barrier.wait().await;

        // Mimicking real start function that should not return.
        let () = pending().await;
        Ok(())
    }
}

#[async_trait]
impl ComponentCClientTrait for LocalComponentClient<ComponentCRequest, ComponentCResponse> {
    async fn c_inc_counter(&self) -> ClientResult<()> {
        let res = self.send(ComponentCRequest::CIncCounter).await;
        match res {
            ComponentCResponse::CIncCounter => Ok(()),
            _ => Err(ClientError::UnexpectedResponse("Unexpected Responce".to_string())),
        }
    }

    async fn c_get_counter(&self) -> ClientResult<usize> {
        let res = self.send(ComponentCRequest::CGetCounter).await;
        match res {
            ComponentCResponse::CGetCounter(counter) => Ok(counter),
            _ => Err(ClientError::UnexpectedResponse("Unexpected Responce".to_string())),
        }
    }
}

#[async_trait]
impl ComponentRequestHandler<ComponentCRequest, ComponentCResponse> for ComponentC {
    async fn handle_request(&mut self, request: ComponentCRequest) -> ComponentCResponse {
        match request {
            ComponentCRequest::CGetCounter => {
                ComponentCResponse::CGetCounter(self.c_get_counter().await)
            }
            ComponentCRequest::CIncCounter => {
                self.c_increment_counter().await;
                ComponentCResponse::CIncCounter
            }
        }
    }
}

async fn wait_and_verify_response(
    tx_c: Sender<ComponentRequestAndResponseSender<ComponentCRequest, ComponentCResponse>>,
    expected_counter_value: usize,
    barrier: Arc<Barrier>,
) {
    let c_client = LocalComponentClient::new(tx_c);

    barrier.wait().await;
    assert_eq!(c_client.c_get_counter().await.unwrap(), expected_counter_value);
}

#[tokio::test]
async fn test_setup_c_d() {
    let init_counter_value: usize = 0;
    let max_iterations: usize = 1024;
    let expected_counter_value = max_iterations * 2;

    let (tx_c, rx_c) =
        channel::<ComponentRequestAndResponseSender<ComponentCRequest, ComponentCResponse>>(32);

    let c_client = LocalComponentClient::new(tx_c.clone());

    let barrier = Arc::new(Barrier::new(3));
    let component_c = ComponentC::new(init_counter_value, max_iterations, barrier.clone());
    let component_d = ComponentD::new(Box::new(c_client), max_iterations, barrier.clone());

    let mut component_c_server = LocalActiveComponentServer::new(component_c, rx_c);
    let mut component_d_server = WrapperServer::new(component_d);

    task::spawn(async move {
        let _ = component_c_server.start().await;
    });

    task::spawn(async move {
        let _ = component_d_server.start().await;
    });

    // Wait for the components to finish incrementing of the ComponentC::counter and verify it.
    wait_and_verify_response(tx_c.clone(), expected_counter_value, barrier).await;
}
