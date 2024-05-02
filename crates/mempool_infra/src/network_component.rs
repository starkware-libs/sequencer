use async_trait::async_trait;
use tokio::sync::mpsc::{error::SendError, Receiver, Sender};

#[async_trait]
pub trait CommunicationInterface<S, R> {
    async fn send(&self, message: S) -> Result<(), SendError<S>>;
    async fn recv(&mut self) -> Option<R>;
}

pub struct NetworkComponent<S, R> {
    tx: Sender<S>,
    rx: Receiver<R>,
}

impl<S, R> NetworkComponent<S, R> {
    pub fn new(tx: Sender<S>, rx: Receiver<R>) -> Self {
        Self { tx, rx }
    }
}

#[async_trait]
impl<S, R> CommunicationInterface<S, R> for NetworkComponent<S, R>
where
    S: Send + Sync,
    R: Send + Sync,
{
    async fn send(&self, message: S) -> Result<(), SendError<S>> {
        self.tx.send(message).await
    }

    async fn recv(&mut self) -> Option<R> {
        self.rx.recv().await
    }
}
