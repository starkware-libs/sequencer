use std::collections::HashMap;

use starknet_api::core::ContractAddress;
use starknet_api::internal_transaction::InternalTransaction;
use starknet_api::transaction::TransactionHash;

use crate::errors::MempoolError;

pub type MempoolResult<T> = Result<T, MempoolError>;

pub struct Mempool;

impl Mempool {
    /// Retrieves up to `n_txs` transactions with the highest priority from the mempool.
    /// Transactions are guaranteed to be unique across calls until `commit_block` is invoked.
    // TODO: the last part about commit_block is incorrect if we delete txs in get_txs and then push
    // back.
    pub fn get_txs(_n_txs: u8) -> MempoolResult<Vec<InternalTransaction>> {
        todo!();
    }

    /// Adds a new transaction to the mempool.
    /// TODO: support fee escalation and transactions with future nonces.
    pub fn add_tx(
        &mut self,
        _tx: InternalTransaction,
        _account_state: AccountState,
    ) -> MempoolResult<()> {
        todo!();
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
}

pub struct AccountState;
