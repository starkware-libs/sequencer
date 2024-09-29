use async_trait::async_trait;

#[async_trait]
pub trait Startable<StartError> {
    async fn start(&mut self) -> Result<(), StartError>;
}
