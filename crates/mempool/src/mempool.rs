use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::collections::HashMap;

use anyhow::Result;
use mempool_infra::network_component::CommunicationInterface;
use starknet_api::core::ContractAddress;
use starknet_api::transaction::TransactionHash;
use starknet_mempool_types::mempool_types::{
    Account, AccountState, BatcherToMempoolChannels, BatcherToMempoolMessage,
    GatewayToMempoolMessage, MempoolInput, MempoolNetworkComponent, ThinTransaction,
};
use tokio::select;

use crate::errors::MempoolError;
use crate::priority_queue::PriorityQueue;

#[cfg(test)]
#[path = "mempool_test.rs"]
pub mod mempool_test;

pub type MempoolResult<T> = Result<T, MempoolError>;

pub struct Mempool {
    // TODO: add docstring explaining visibility and coupling of the fields.
    pub gateway_network: MempoolNetworkComponent,
    batcher_network: BatcherToMempoolChannels,
    txs_queue: PriorityQueue,
    state: HashMap<ContractAddress, AccountState>,
}

impl Mempool {
    pub fn new(
        inputs: impl IntoIterator<Item = MempoolInput>,
        gateway_network: MempoolNetworkComponent,
        batcher_network: BatcherToMempoolChannels,
    ) -> Self {
        let mut mempool = Mempool {
            txs_queue: Default::default(),
            state: Default::default(),
            gateway_network,
            batcher_network,
        };

        mempool.txs_queue = PriorityQueue::from_iter(inputs.into_iter().map(|input| {
            // Attempts to insert a key-value pair into the mempool's state. Returns `None` if the
            // key was not present, otherwise returns the old value while updating the new value.
            let prev_value = mempool.state.insert(input.account.address, input.account.state);
            // Assert that the contract address does not exist in the mempool's state to ensure that
            // there is only one transaction per contract address.
            assert!(
                prev_value.is_none(),
                "Contract address: {:?} already exists in the mempool. Can't add {:?} to the \
                 mempool.",
                input.account.address,
                input.tx
            );
            input.tx
        }));

        mempool
    }

    /// Retrieves up to `n_txs` transactions with the highest priority from the mempool.
    /// Transactions are guaranteed to be unique across calls until `commit_block` is invoked.
    // TODO: the last part about commit_block is incorrect if we delete txs in get_txs and then push
    // back. TODO: Consider renaming to `pop_txs` to be more consistent with the standard
    // library.
    pub fn get_txs(&mut self, n_txs: usize) -> MempoolResult<Vec<ThinTransaction>> {
        let txs = self.txs_queue.pop_last_chunk(n_txs);
        for tx in &txs {
            self.state.remove(&tx.contract_address);
        }

        Ok(txs)
    }

    /// Adds a new transaction to the mempool.
    /// TODO: support fee escalation and transactions with future nonces.
    /// TODO: change input type to `MempoolInput`.
    pub fn add_tx(&mut self, tx: ThinTransaction, account: Account) -> MempoolResult<()> {
        match self.state.entry(account.address) {
            Occupied(_) => Err(MempoolError::DuplicateTransaction { tx_hash: tx.tx_hash }),
            Vacant(entry) => {
                entry.insert(account.state);
                self.txs_queue.push(tx);
                Ok(())
            }
        }
    }

    /// Update the mempool's internal state according to the committed block's transactions.
    /// This method also updates internal state (resolves nonce gaps, updates account balances).
    // TODO: the part about resolving nonce gaps is incorrect if we delete txs in get_txs and then
    // push back.
    pub fn commit_block(
        &mut self,
        _block_number: u64,
        _txs_in_block: &[TransactionHash],
        _state_changes: HashMap<ContractAddress, AccountState>,
    ) -> MempoolResult<()> {
        todo!()
    }

    /// Listens asynchronously for network messages and processes them.
    pub async fn run(&mut self) -> Result<()> {
        loop {
            select! {
                optional_gateway_message = self.gateway_network.recv() => {
                    match optional_gateway_message {
                        Some(message) => {
                            self.process_gateway_message(message)?;
                        },
                        // Channel was closed; exit.
                        None => break,
                    }
                }
                optional_batcher_message = self.batcher_network.rx.recv() => {
                    match optional_batcher_message {
                        Some(message) => {
                            self.process_batcher_message(message).await?;
                        },
                        // Channel was closed; exit.
                        None => break,
                    }
                }

            }
        }
        Ok(())
    }

    fn process_gateway_message(&mut self, message: GatewayToMempoolMessage) -> Result<()> {
        match message {
            GatewayToMempoolMessage::AddTransaction(mempool_input) => {
                self.add_tx(mempool_input.tx, mempool_input.account)?;
                Ok(())
            }
        }
    }

    async fn process_batcher_message(&mut self, message: BatcherToMempoolMessage) -> Result<()> {
        match message {
            BatcherToMempoolMessage::GetTransactions(n_txs) => {
                let txs = self.get_txs(n_txs)?;
                self.batcher_network.tx.send(txs).await?;
                Ok(())
            }
        }
    }
}
