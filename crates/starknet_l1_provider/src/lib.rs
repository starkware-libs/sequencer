use indexmap::{IndexMap, IndexSet};
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_l1_provider_types::errors::L1ProviderError;
use starknet_l1_provider_types::l1_provider_types::{L1ProviderResult, ProviderState};

#[cfg(test)]
#[path = "l1_provider_tests.rs"]
pub mod l1_provider_tests;

// TODO: optimistic proposer support, will add later to keep things simple, but the design here
// is compatible with it.
#[derive(Debug, Default)]
pub struct L1Provider {
    tx_manager: TransactionManager,
    // TODO(Gilad): consider transitioning to a generic phantom state once the infra is stabilized
    // and we see how well it handles consuming the L1Provider when moving between states.
    state: ProviderState,
}

impl L1Provider {
    pub async fn new(_config: L1ProviderConfig) -> L1ProviderResult<Self> {
        todo!(
            "init crawler to start next crawl from ~1 hour ago, this can have l1 errors when \
             finding the latest block on L1 to 'subtract' 1 hour from."
        );
    }

    /// Retrieves up to `n_txs` transactions that have yet to be proposed or accepted on L2.
    pub fn get_txs(&mut self, n_txs: usize) -> L1ProviderResult<Vec<L1HandlerTransaction>> {
        match self.state {
            ProviderState::Propose => Ok(self.tx_manager.get_txs(n_txs)),
            ProviderState::Pending => Err(L1ProviderError::GetTransactionsInPendingState),
            ProviderState::Validate => Err(L1ProviderError::GetTransactionConsensusBug),
        }
    }

    pub fn validate(&self, _tx: &L1HandlerTransaction) -> L1ProviderResult<bool> {
        todo!(
            "Check that tx is unconsumed and not present in L2. Error if in Propose state, NOP if \
             in pending state (likely due to a crash and losing one validator for the block's \
             duration node isn't serious)."
        )
    }

    // TODO: when deciding on consensus, if possible, have commit_block also tell the node if it's
    // about to [optimistically-]propose or validate the next block.
    pub fn commit_block(&mut self, _commited_txs: &[TransactionHash]) {
        todo!(
            "Purges txs from internal buffers, if was proposer clear staging buffer,
            reset state to Pending until we get proposing/validating notice from consensus."
        )
    }

    // TODO: pending formal consensus API, guessing the API here to keep things moving.
    // TODO: consider adding block number, it isn't strictly necessary, but will help debugging.
    pub fn validation_start(&mut self) -> L1ProviderResult<()> {
        todo!("Sets internal state as validate, returns error if state is Pending.")
    }

    pub fn proposal_start(&mut self) -> L1ProviderResult<()> {
        self.state = self.state.transition_to_propose()?;
        Ok(())
    }

    /// Simple recovery from L1 and L2 reorgs by reseting the service, which rewinds L1 and L2
    /// information.
    pub fn handle_reorg(&mut self) -> L1ProviderResult<()> {
        self.reset()
    }

    // TODO: this will likely change during integration with infra team.
    pub async fn start(&self) {
        todo!(
            "Create a process that wakes up every config.poll_interval seconds and updates
        internal L1 and L2 buffers according to collected L1 events and recent blocks created on
        L2."
        )
    }

    fn reset(&mut self) -> L1ProviderResult<()> {
        todo!(
            "resets internal buffers and rewinds the internal crawler _pointer_ back for ~1 \
             hour,so that the main loop will start collecting from that time gracefully. May hit \
             base layer errors."
        );
    }
}

#[derive(Debug, Default)]
struct TransactionManager {
    txs: IndexMap<TransactionHash, L1HandlerTransaction>,
    proposed_txs: IndexSet<TransactionHash>,
    _on_l2_awaiting_l1_consumption: IndexSet<TransactionHash>,
}

impl TransactionManager {
    pub fn get_txs(&mut self, n_txs: usize) -> Vec<L1HandlerTransaction> {
        let (tx_hashes, txs): (Vec<_>, Vec<_>) = self
            .txs
            .iter()
            .skip(self.proposed_txs.len()) // Transactions are proposed FIFO.
            .take(n_txs)
            .map(|(&hash, tx)| (hash, tx.clone()))
            .unzip();

        self.proposed_txs.extend(tx_hashes);
        txs
    }

    pub fn _add_unconsumed_l1_not_in_l2_block_tx(&mut self, _tx: L1HandlerTransaction) {
        todo!(
            "Check if tx is in L2, if it isn't on L2 add it to the txs buffer, otherwise print
             debug and do nothing."
        )
    }

    pub fn _mark_tx_included_on_l2(&mut self, _tx_hash: &TransactionHash) {
        todo!("Adds the tx hash to l2 buffer; remove tx from the txs storage if it's there.")
    }
}

#[derive(Debug)]
pub struct L1ProviderConfig;
