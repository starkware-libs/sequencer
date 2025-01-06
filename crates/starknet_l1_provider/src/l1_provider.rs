use starknet_api::block::BlockNumber;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_l1_provider_types::errors::L1ProviderError;
use starknet_l1_provider_types::{L1ProviderResult, ValidationStatus};
use starknet_sequencer_infra::component_definitions::ComponentStarter;

use crate::transaction_manager::TransactionManager;
use crate::{L1ProviderConfig, ProviderState};

// TODO: optimistic proposer support, will add later to keep things simple, but the design here
// is compatible with it.
#[derive(Debug, Default)]
pub struct L1Provider {
    pub tx_manager: TransactionManager,
    // TODO(Gilad): consider transitioning to a generic phantom state once the infra is stabilized
    // and we see how well it handles consuming the L1Provider when moving between states.
    pub state: ProviderState,
    pub current_height: BlockNumber,
}

impl L1Provider {
    pub fn new(_config: L1ProviderConfig) -> L1ProviderResult<Self> {
        todo!("Init crawler in uninitialized_state from config, to initialize call `reset`.");
    }

    pub fn start_block(
        &mut self,
        height: BlockNumber,
        state: ProviderState,
    ) -> L1ProviderResult<()> {
        self.validate_height(height)?;
        self.state = self.state.try_into_new_state(state)?;
        self.tx_manager.start_block();
        Ok(())
    }

    /// Retrieves up to `n_txs` transactions that have yet to be proposed or accepted on L2.
    pub fn get_txs(
        &mut self,
        n_txs: usize,
        height: BlockNumber,
    ) -> L1ProviderResult<Vec<L1HandlerTransaction>> {
        // Reenable once `commit_block` is implemented so that height can be updated.
        let _disabled = self.validate_height(height);

        match self.state {
            ProviderState::Propose => Ok(self.tx_manager.get_txs(n_txs)),
            ProviderState::Pending => Err(L1ProviderError::GetTransactionsInPendingState),
            ProviderState::Validate => Err(L1ProviderError::GetTransactionConsensusBug),
            ProviderState::Uninitialized => panic!("Uninitialized L1 provider"),
        }
    }

    /// Returns true if and only if the given transaction is both not included in an L2 block, and
    /// unconsumed on L1.
    pub fn validate(
        &mut self,
        tx_hash: TransactionHash,
        height: BlockNumber,
    ) -> L1ProviderResult<ValidationStatus> {
        self.validate_height(height)?;
        match self.state {
            ProviderState::Validate => Ok(self.tx_manager.validate_tx(tx_hash)),
            ProviderState::Propose => Err(L1ProviderError::ValidateTransactionConsensusBug),
            ProviderState::Pending => Err(L1ProviderError::ValidateInPendingState),
            ProviderState::Uninitialized => panic!("Uninitialized L1 provider"),
        }
    }

    // TODO: when deciding on consensus, if possible, have commit_block also tell the node if it's
    // about to [optimistically-]propose or validate the next block.
    pub fn commit_block(&mut self, committed_txs: &[TransactionHash], height: BlockNumber) {
        if self.validate_height(height).is_ok() {
            self.tx_manager.apply_commit_block_txs(committed_txs);
            self.current_height = self
                .tx_manager
                .apply_backlogged_commit_blocks(self.current_height.unchecked_next());
        } else {
            self.tx_manager.add_commit_block_to_backlog(committed_txs, height);
        }

        self.state = self
            .state
            .try_into_new_state(ProviderState::Pending)
            .expect("Always possible to transition to Pending.");
    }

    fn validate_height(&mut self, height: BlockNumber) -> L1ProviderResult<()> {
        let next_height = self.current_height.unchecked_next();
        if height != next_height {
            return Err(L1ProviderError::UnexpectedHeight { expected: next_height, got: height });
        }
        Ok(())
    }
}

impl ComponentStarter for L1Provider {}

pub fn create_l1_provider(_config: L1ProviderConfig) -> L1Provider {
    L1Provider { state: ProviderState::Propose, ..Default::default() }
}
