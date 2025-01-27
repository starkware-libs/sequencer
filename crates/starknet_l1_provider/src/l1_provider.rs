use std::cmp::Ordering::{Equal, Greater, Less};

use starknet_api::block::BlockNumber;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_l1_provider_types::errors::L1ProviderError;
use starknet_l1_provider_types::{Event, L1ProviderResult, SessionState, ValidationStatus};
use starknet_sequencer_infra::component_definitions::ComponentStarter;

use crate::transaction_manager::TransactionManager;
use crate::{L1ProviderConfig, ProviderState};
// TODO(Gilad): optimistic proposer support, will add later to keep things simple, but the design
// here is compatible with it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct L1Provider {
    pub current_height: BlockNumber,
    pub(crate) tx_manager: TransactionManager,
    // TODO(Gilad): consider transitioning to a generic phantom state once the infra is stabilized
    // and we see how well it handles consuming the L1Provider when moving between states.
    pub(crate) state: ProviderState,
}

impl L1Provider {
    pub fn new(_config: L1ProviderConfig) -> L1ProviderResult<Self> {
        todo!("Init crawler in uninitialized_state from config, to initialize call `reset`.");
    }

    pub fn start_block(
        &mut self,
        height: BlockNumber,
        state: SessionState,
    ) -> L1ProviderResult<()> {
        self.validate_height(height)?;
        self.state = state.into();
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
            ProviderState::Pending | ProviderState::Bootstrap(_) => {
                Err(L1ProviderError::OutOfSessionGetTransactions)
            }
            ProviderState::Validate => Err(L1ProviderError::GetTransactionConsensusBug),
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
            ProviderState::Pending | ProviderState::Bootstrap(_) => {
                Err(L1ProviderError::OutOfSessionValidate)
            }
        }
    }

    // TODO(Gilad): when deciding on consensus, if possible, have commit_block also tell the node if
    // it's about to [optimistically-]propose or validate the next block.
    pub fn commit_block(
        &mut self,
        committed_txs: &[TransactionHash],
        height: BlockNumber,
    ) -> L1ProviderResult<()> {
        if self.state.is_bootstrapping() {
            // Once bootstrap completes it will transition to Pending state by itself.
            return self.bootstrap(committed_txs, height);
        }

        self.validate_height(height)?;
        self.apply_commit_block(committed_txs);

        self.state = self.state.transition_to_pending();
        Ok(())
    }

    /// Try to apply commit_block backlog, and if all caught up, drop bootstrapping state.
    fn bootstrap(
        &mut self,
        committed_txs: &[TransactionHash],
        height: BlockNumber,
    ) -> L1ProviderResult<()> {
        let other = &self.current_height;
        match height.cmp(other) {
            Less => Err(L1ProviderError::UnexpectedHeight { expected: *other, got: height })?,
            Equal => self.apply_commit_block(committed_txs),
            Greater => {
                self.state
                    .get_bootstrapper()
                    .expect("This method should only be called when bootstrapping.")
                    .add_commit_block_to_backlog(committed_txs, height);
                // No need to check the backlog or bootstrap completion, since those are only
                // applicable if we just increased the provider's height, like in the `Equal` case.
                return Ok(());
            }
        };

        let bootstrapper = self
            .state
            .get_bootstrapper()
            .expect("This method should only be called when bootstrapping.");

        // If caught up, apply the backlog, drop the Bootstrapper and transition to Pending.
        if bootstrapper.is_caught_up(self.current_height) {
            let backlog = std::mem::take(&mut bootstrapper.commit_block_backlog);
            assert!(
                backlog.is_empty()
                    || bootstrapper.catch_up_height == backlog.first().unwrap().height
                        && backlog
                            .windows(2)
                            .all(|height| height[1].height == height[0].height.unchecked_next()),
                "Backlog must have sequential heights starting sequentially after \
                 catch_up_height: {}, backlog: {:?}",
                bootstrapper.catch_up_height,
                backlog.iter().map(|commit_block| commit_block.height).collect::<Vec<_>>()
            );
            for commit_block in backlog {
                self.apply_commit_block(&commit_block.committed_txs);
            }

            // Drops bootstrapper and all of its assets.
            self.state = ProviderState::Pending;
        }

        Ok(())
    }

    pub fn process_l1_events(&mut self, events: Vec<Event>) -> L1ProviderResult<()> {
        for event in events {
            match event {
                Event::L1HandlerTransaction(l1_handler_tx) => {
                    // TODO(Gilad): can we ignore this silently?
                    let _is_known_or_committed = self.tx_manager.add_tx(l1_handler_tx);
                }
                _ => todo!(),
            }
        }
        Ok(())
    }

    /// Simple recovery from L1 and L2 reorgs by reseting the service, which rewinds L1 and L2
    /// information.
    pub async fn handle_reorg(&mut self) -> L1ProviderResult<()> {
        self.reset().await
    }

    pub async fn reset(&mut self) -> L1ProviderResult<()> {
        todo!(
            "resets internal buffers and rewinds the internal crawler _pointer_ back for ~1 \
             hour,so that the main loop will start collecting from that time gracefully. May hit \
             base layer errors when finding the latest block on l1 to 'subtract' 1 hour from. \
             Then, transition to Pending."
        );
    }

    fn validate_height(&mut self, height: BlockNumber) -> L1ProviderResult<()> {
        if height != self.current_height {
            return Err(L1ProviderError::UnexpectedHeight {
                expected: self.current_height,
                got: height,
            });
        }
        Ok(())
    }

    fn apply_commit_block(&mut self, committed_txs: &[TransactionHash]) {
        self.tx_manager.commit_txs(committed_txs);
        self.current_height = self.current_height.unchecked_next();
    }
}

impl ComponentStarter for L1Provider {}

pub fn create_l1_provider(_config: L1ProviderConfig) -> L1Provider {
    L1Provider {
        state: ProviderState::Pending,
        current_height: BlockNumber(1),
        ..Default::default()
    }
}
