use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use starknet_mempool_infra::component_client::ClientResult;
use starknet_mempool_infra::starters::DefaultComponentStarter;
use starknet_types_core::felt::Felt;

pub(crate) type ValueA = Felt;
pub(crate) type ValueB = Felt;

pub(crate) type ResultA = ClientResult<ValueA>;
pub(crate) type ResultB = ClientResult<ValueB>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ComponentARequest {
    AGetValue,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ComponentAResponse {
    AGetValue(ValueA),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ComponentBRequest {
    BGetValue,
    BSetValue(ValueB),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ComponentBResponse {
    BGetValue(ValueB),
    BSetValue,
}

#[async_trait]
pub(crate) trait ComponentAClientTrait: Send + Sync {
    async fn a_get_value(&self) -> ResultA;
}

#[async_trait]
pub(crate) trait ComponentBClientTrait: Send + Sync {
    async fn b_get_value(&self) -> ResultB;
    async fn b_set_value(&self, value: ValueB) -> ClientResult<()>;
}

pub(crate) struct ComponentA {
    b: Box<dyn ComponentBClientTrait>,
}

impl ComponentA {
    pub fn new(b: Box<dyn ComponentBClientTrait>) -> Self {
        Self { b }
    }

    pub async fn a_get_value(&self) -> ValueA {
        self.b.b_get_value().await.unwrap()
    }
}

impl DefaultComponentStarter for ComponentA {}

pub(crate) struct ComponentB {
    value: ValueB,
    _a: Box<dyn ComponentAClientTrait>,
}

impl ComponentB {
    pub fn new(value: ValueB, a: Box<dyn ComponentAClientTrait>) -> Self {
        Self { value, _a: a }
    }

    pub fn b_get_value(&self) -> ValueB {
        self.value
    }

    pub fn b_set_value(&mut self, value: ValueB) {
        self.value = value;
    }
}

impl DefaultComponentStarter for ComponentB {}

pub(crate) async fn test_a_b_functionality(
    a_client: impl ComponentAClientTrait,
    b_client: impl ComponentBClientTrait,
    expected_value: ValueA,
) {
    // Check the setup value in component B through client A.
    assert_eq!(a_client.a_get_value().await.unwrap(), expected_value);

    let new_expected_value: ValueA = expected_value + 1;
    // Check that setting a new value to component B succeeds.
    assert!(b_client.b_set_value(new_expected_value).await.is_ok());
    // Check the new value in component B through client A.
    assert_eq!(a_client.a_get_value().await.unwrap(), new_expected_value);
}
