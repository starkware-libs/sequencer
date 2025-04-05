use async_trait::async_trait;

#[async_trait]
pub trait ComponentServerStarter {
    async fn start(&mut self);
}
