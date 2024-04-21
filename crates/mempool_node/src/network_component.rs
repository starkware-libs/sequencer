use tokio::sync::mpsc::{Receiver, Sender};

pub struct NetworkComponent<S, R> {
    pub tx: Sender<S>,
    pub rx: Receiver<R>,
}

impl<S, R> NetworkComponent<S, R> {
    pub fn new(tx: Sender<S>, rx: Receiver<R>) -> Self {
        Self { tx, rx }
    }
}
