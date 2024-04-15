use std::sync::Arc;

use crate::mempool::{AddTransactionCallType, AddTransactionReturnType, Mempool, MempoolTrait};
use async_trait::async_trait;

use tokio::sync::mpsc::{channel, Sender};
use tokio::task;

enum ProxyFunc {
    AddTransaction(AddTransactionCallType),
}

enum ProxyRetValue {
    AddTransaction(AddTransactionReturnType),
}

#[derive(Clone)]
pub struct MempoolProxy {
    tx_call: Sender<(ProxyFunc, Sender<ProxyRetValue>)>,
}

impl Default for MempoolProxy {
    fn default() -> Self {
        let (tx_call, mut rx_call) = channel::<(ProxyFunc, Sender<ProxyRetValue>)>(32);

        task::spawn(async move {
            let mempool = Arc::new(Mempool::default());
            while let Some(call) = rx_call.recv().await {
                match call {
                    (ProxyFunc::AddTransaction(tx), tx_response) => {
                        let mempool = mempool.clone();
                        task::spawn(async move {
                            let ret_value = mempool.add_transaction(tx).await;
                            tx_response
                                .send(ProxyRetValue::AddTransaction(ret_value))
                                .await
                                .expect("Receiver should be listening.");
                        });
                    }
                }
            }
        });

        Self { tx_call }
    }
}

#[async_trait]
impl MempoolTrait for MempoolProxy {
    async fn add_transaction(&self, tx: AddTransactionCallType) -> AddTransactionReturnType {
        let (tx_response, mut rx_response) = channel(32);
        self.tx_call
            .send((ProxyFunc::AddTransaction(tx), tx_response))
            .await
            .expect("Receiver should be listening.");

        match rx_response
            .recv()
            .await
            .expect("Sender should be responding.")
        {
            ProxyRetValue::AddTransaction(ret_value) => ret_value,
        }
    }
}
