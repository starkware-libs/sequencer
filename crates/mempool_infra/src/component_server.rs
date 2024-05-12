use async_trait::async_trait;
use tokio::sync::mpsc::{Receiver, Sender};

#[async_trait]
pub trait ComponentMessageExecutor<M, R> {
    async fn execute(&self, message: M) -> R;
}

pub struct MessageAndResponseSender<M, R>
where
    M: Send + Sync,
    R: Send + Sync,
{
    pub message: M,
    pub tx: Sender<R>,
}

pub struct ComponentServer<C, M, R>
where
    C: ComponentMessageExecutor<M, R>,
    M: Send + Sync,
    R: Send + Sync,
{
    component: C,
    rx: Receiver<MessageAndResponseSender<M, R>>,
}

impl<C, M, R> ComponentServer<C, M, R>
where
    C: ComponentMessageExecutor<M, R>,
    M: Send + Sync,
    R: Send + Sync,
{
    pub fn new(component: C, rx: Receiver<MessageAndResponseSender<M, R>>) -> Self {
        Self { component, rx }
    }

    pub async fn start(&mut self) {
        while let Some(message_and_res_tx) = self.rx.recv().await {
            let message = message_and_res_tx.message;
            let tx = message_and_res_tx.tx;

            let res = self.component.execute(message).await;

            tx.send(res).await.expect("Response connection should be open.");
        }
    }
}
