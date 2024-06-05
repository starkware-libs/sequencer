use async_trait::async_trait;

type ValueA = u32;
type ValueB = u8;

#[async_trait]
pub(crate) trait ComponentATrait: Send + Sync {
    async fn a_get_value(&self) -> ValueA;
}

#[async_trait]
pub(crate) trait ComponentBTrait: Send + Sync {
    async fn b_get_value(&self) -> ValueB;
}
