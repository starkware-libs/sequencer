use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use strum::EnumVariantNames;
use strum_macros::{AsRefStr, EnumDiscriminants, EnumIter, IntoStaticStr};
use tokio::sync::mpsc::channel;
use tokio::task;

use crate::component_client::LocalComponentClient;
use crate::component_definitions::{
    ComponentClient,
    ComponentRequestHandler,
    ComponentStarter,
    PrioritizedRequest,
    RequestPriority,
    RequestWrapper,
};
use crate::component_server::{ComponentServerStarter, LocalComponentServer, LocalServerConfig};
use crate::tests::TEST_LOCAL_SERVER_METRICS;
use crate::{impl_debug_for_infra_requests_and_responses, impl_labeled_request};

#[async_trait]
impl ComponentRequestHandler<PriorityTestRequest, PriorityTestResponse> for PriorityTestComponent {
    async fn handle_request(&mut self, request: PriorityTestRequest) -> PriorityTestResponse {
        match request {
            PriorityTestRequest::Get => PriorityTestResponse::Get(self.get_values()),
            PriorityTestRequest::HighPriorityAdd(value) => {
                self.add_value(value);
                PriorityTestResponse::HighPriorityAdd(value)
            }
            PriorityTestRequest::NormalPriorityAdd(value) => {
                self.add_value(value);
                PriorityTestResponse::NormalPriorityAdd(value)
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, AsRefStr, EnumDiscriminants)]
#[strum_discriminants(
    name(PriorityTestRequestLabelValue),
    derive(IntoStaticStr, EnumIter, EnumVariantNames),
    strum(serialize_all = "snake_case")
)]
pub enum PriorityTestRequest {
    HighPriorityAdd(usize),
    NormalPriorityAdd(usize),
    Get,
}
impl_debug_for_infra_requests_and_responses!(PriorityTestRequest);
impl_labeled_request!(PriorityTestRequest, PriorityTestRequestLabelValue);

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum PriorityTestResponse {
    HighPriorityAdd(usize),
    NormalPriorityAdd(usize),
    Get(Vec<usize>),
}

pub struct PriorityTestComponent {
    values: Vec<usize>,
}
impl ComponentStarter for PriorityTestComponent {}

impl PriorityTestComponent {
    pub fn new() -> Self {
        Self { values: Vec::new() }
    }

    pub fn add_value(&mut self, value: usize) {
        self.values.push(value);
    }

    pub fn get_values(&self) -> Vec<usize> {
        self.values.clone()
    }
}

impl PrioritizedRequest for PriorityTestRequest {
    fn priority(&self) -> RequestPriority {
        match self {
            PriorityTestRequest::Get => RequestPriority::Normal,
            PriorityTestRequest::HighPriorityAdd(_) => RequestPriority::High,
            PriorityTestRequest::NormalPriorityAdd(_) => RequestPriority::Normal,
        }
    }
}

#[tokio::test]
async fn request_prioritization() {
    const NUMBER_OF_MESSAGES: usize = 10;

    // Create the channel, a client, a component, and a server.
    let (tx, rx) = channel::<RequestWrapper<PriorityTestRequest, PriorityTestResponse>>(32);
    let client = LocalComponentClient::new(tx);
    let component = PriorityTestComponent::new();
    let local_server_config = LocalServerConfig::default();
    let mut component_server =
        LocalComponentServer::new(component, &local_server_config, rx, &TEST_LOCAL_SERVER_METRICS);

    // Send requests with different priorities before starting the server, creating a backlog of
    // mixed priority requests.
    for i in 1..=NUMBER_OF_MESSAGES {
        let client = client.clone();
        task::spawn(async move {
            let request = if i % 2 == 0 {
                PriorityTestRequest::HighPriorityAdd(i)
            } else {
                PriorityTestRequest::NormalPriorityAdd(i)
            };
            let expected_response = if i % 2 == 0 {
                PriorityTestResponse::HighPriorityAdd(i)
            } else {
                PriorityTestResponse::NormalPriorityAdd(i)
            };
            let response = client.send(request).await.unwrap();
            assert_eq!(
                response, expected_response,
                "Response mismatch for request {i}, got {response:?}, expected \
                 {expected_response:?}"
            );
        });
    }
    // Ensure all send tasks are triggered before starting the server.
    task::yield_now().await;

    // Start the server to process, and specifically, to sort the requests according to their
    // prioritization.
    task::spawn(async move {
        let _ = component_server.start().await;
    });

    // Ensure the server has started running.
    task::yield_now().await;

    // Obtain added values from the server.
    let values = match client.send(PriorityTestRequest::Get).await.unwrap() {
        PriorityTestResponse::Get(values) => values,
        other => panic!("Unexpected response: {:?}", other),
    };

    let expected_values: Vec<usize> = (2..=NUMBER_OF_MESSAGES)
        .step_by(2) // evens: 2,4,6,8,10
        .chain((1..NUMBER_OF_MESSAGES).step_by(2)) // odds: 1,3,5,7,9
        .collect();

    assert_eq!(
        values, expected_values,
        "Values mismatch, got {values:?}, expected {expected_values:?}"
    );
}
