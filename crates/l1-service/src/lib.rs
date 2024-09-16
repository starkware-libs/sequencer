// FIXME: remove this.
// ignore all unused warnings in the file
#![allow(unused)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use indexmap::IndexSet;
use papyrus_base_layer::ethereum_base_layer_contract::{
    EthereumBaseLayerConfig,
    EthereumBaseLayerContract,
};
use starknet_api::transaction::L1HandlerTransaction;
use tokio::task::JoinHandle;
use tokio::time;

const RESET_COLLECTION_DEPTH: Duration = Duration::from_secs(3600); // 1 Hour.

struct L1Service {
    pub unconsumed_messages: Rc<RefCell<IndexSet<L1HandlerTransaction>>>,
    pub l1_crawler: L1Crawler,
}

impl L1Service {
    fn new(poll_interval: Duration, l1_provider: EthereumBaseLayerContract) -> Self {
        // TODO: query L1 for last_l1_block_number_checked.
        let l1_crawler = L1Crawler { l1_provider, poll_interval, last_l1_block_number_checked: 0 };
        Self { unconsumed_messages: Rc::new(RefCell::new(IndexSet::new())), l1_crawler }
    }

    async fn run(&self) {
        self.l1_crawler.run().await;
    }

    // Greedly collect recent messages in case of crash or reorg in L1 or L2.
    fn reset(&mut self) {
        todo!()
    }

    fn contains(&self, message: &L1HandlerTransaction) -> bool {
        self.unconsumed_messages.borrow().contains(message)
    }

    fn get_tx(&self) -> Option<L1HandlerTransaction> {
        let l1_handler = self.unconsumed_messages.borrow_mut().pop();
        //  if not none move to soft-delete.

        l1_handler
    }

    fn commit_block(&mut self, commited_txs: Vec<L1HandlerTransaction>) {
        todo!("commit in two-phase DS, move layer 2 to layer 1.")
    }

    fn rollback_block(&mut self, commited_txs: Vec<L1HandlerTransaction>) {
        todo!("undo phase 2 layer and rollaback soft deletes from both layers")
    }
}

struct L1Crawler {
    pub l1_provider: EthereumBaseLayerContract,
    pub poll_interval: Duration,
    pub last_l1_block_number_checked: u64, // use BlockNumber? might be confusing since this is L1.
}

impl L1Crawler {
    async fn run(&self) {
        let mut interval = time::interval(self.poll_interval);

        loop {
            interval.tick().await;
            self.collect_l1_to_l2_messages().await;
        }
    }

    async fn collect_l1_to_l2_messages(&self) {
        // self.l1_provider.get_send_message_to_l2_messages().await;
        unimplemented!();
    }
}

#[tokio::main]
async fn main() {
    let poll_interval = Duration::from_secs(5);
    // FIXME: add actual config.
    let l1_provider_config = EthereumBaseLayerConfig::default();

    let service =
        L1Service::new(poll_interval, EthereumBaseLayerContract::new(l1_provider_config).unwrap());
    service.run().await;
}
