use std::collections::HashSet;
use std::mem;
use std::sync::Mutex;

use apollo_l1_provider_types::{
    Event,
    L1ProviderClient,
    L1ProviderClientResult,
    SessionState,
    ValidationStatus,
};
use async_trait::async_trait;
use itertools::Itertools;
use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use starknet_api::executable_transaction::{
    L1HandlerTransaction as ExecutableL1HandlerTransaction,
    L1HandlerTransaction,
};
use starknet_api::hash::StarkHash;
use starknet_api::test_utils::l1_handler::{executable_l1_handler_tx, L1HandlerTxArgs};
use starknet_api::transaction::TransactionHash;

use crate::bootstrapper::CommitBlockBacklog;
use crate::l1_provider::L1Provider;

pub fn l1_handler(tx_hash: usize) -> L1HandlerTransaction {
    let tx_hash = TransactionHash(StarkHash::from(tx_hash));
    executable_l1_handler_tx(L1HandlerTxArgs { tx_hash, ..Default::default() })
}

/// A fake L1 provider client that buffers all received messages, allow asserting the order in which
/// they were received, and forward them to the l1 provider (flush the messages).
#[derive(Default)]
pub struct FakeL1ProviderClient {
    // Interior mutability needed since this is modifying during client API calls, which are all
    // immutable.
    pub events_received: Mutex<Vec<Event>>,
    pub commit_blocks_received: Mutex<Vec<CommitBlockBacklog>>,
}

impl FakeL1ProviderClient {
    /// Apply all messages received to the l1 provider.
    pub async fn flush_messages(&self, l1_provider: &mut L1Provider) {
        let commit_blocks = self.commit_blocks_received.lock().unwrap().drain(..).collect_vec();
        for CommitBlockBacklog { height, committed_txs } in commit_blocks {
            l1_provider.commit_block(&committed_txs, &HashSet::new(), height).unwrap();
        }

        // TODO(gilad): flush other buffers if necessary.
    }

    #[track_caller]
    pub fn assert_add_events_received_with(&self, expected: &[Event]) {
        let events_received = mem::take(&mut *self.events_received.lock().unwrap());
        assert_eq!(events_received, expected);
    }
}

#[async_trait]
impl L1ProviderClient for FakeL1ProviderClient {
    async fn start_block(
        &self,
        _state: SessionState,
        _height: BlockNumber,
    ) -> L1ProviderClientResult<()> {
        todo!()
    }

    async fn get_txs(
        &self,
        _n_txs: usize,
        _height: BlockNumber,
    ) -> L1ProviderClientResult<Vec<ExecutableL1HandlerTransaction>> {
        todo!()
    }

    async fn add_events(&self, events: Vec<Event>) -> L1ProviderClientResult<()> {
        self.events_received.lock().unwrap().extend(events);
        Ok(())
    }

    async fn commit_block(
        &self,
        l1_handler_tx_hashes: Vec<TransactionHash>,
        _rejected_l1_handler_tx_hashes: HashSet<TransactionHash>,
        height: BlockNumber,
    ) -> L1ProviderClientResult<()> {
        self.commit_blocks_received
            .lock()
            .unwrap()
            .push(CommitBlockBacklog { height, committed_txs: l1_handler_tx_hashes });
        Ok(())
    }

    async fn validate(
        &self,
        _tx_hash: TransactionHash,
        _height: BlockNumber,
    ) -> L1ProviderClientResult<ValidationStatus> {
        todo!()
    }

    async fn initialize(&self, _events: Vec<Event>) -> L1ProviderClientResult<()> {
        todo!()
    }
}
