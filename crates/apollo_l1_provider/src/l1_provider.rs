use std::cmp::Ordering::{Equal, Greater, Less};
use std::sync::Arc;

use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra_utils::info_every_n_sec;
use apollo_l1_provider_types::errors::L1ProviderError;
use apollo_l1_provider_types::{
    Event,
    L1ProviderResult,
    L1ProviderSnapshot,
    ProviderState,
    SessionState,
    SharedL1ProviderClient,
    ValidationStatus,
};
use apollo_state_sync_types::communication::SharedStateSyncClient;
use apollo_time::time::{Clock, DefaultClock};
use indexmap::IndexSet;
use starknet_api::block::BlockNumber;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;
use tracing::{debug, error, info, instrument, trace, warn};

use crate::bootstrapper::Bootstrapper;
use crate::transaction_manager::TransactionManager;
use crate::L1ProviderConfig;

#[cfg(test)]
#[path = "l1_provider_tests.rs"]
pub mod l1_provider_tests;

// TODO(Gilad): optimistic proposer support, will add later to keep things simple, but the design
// here is compatible with it.
#[derive(Debug, Clone)]
pub struct L1Provider {
    pub config: L1ProviderConfig,
    /// Used for catching up at startup or after a crash.
    pub bootstrapper: Bootstrapper,
    /// Represents the L2 block height being built.
    pub current_height: BlockNumber,
    pub tx_manager: TransactionManager,
    // TODO(Gilad): consider transitioning to a generic phantom state once the infra is stabilized
    // and we see how well it handles consuming the L1Provider when moving between states.
    pub state: ProviderState,
    pub clock: Arc<dyn Clock>,
    pub start_height: Option<BlockNumber>,
}

impl L1Provider {
    pub fn new(
        config: L1ProviderConfig,
        l1_provider_client: SharedL1ProviderClient,
        state_sync_client: SharedStateSyncClient,
        clock: Option<Arc<dyn Clock>>,
    ) -> Self {
        let bootstrapper = Bootstrapper::new(
            l1_provider_client,
            state_sync_client,
            config.startup_sync_sleep_retry_interval_seconds,
        );
        Self {
            config,
            bootstrapper,
            current_height: BlockNumber(0),
            tx_manager: TransactionManager::new(
                config.new_l1_handler_cooldown_seconds,
                config.l1_handler_cancellation_timelock_seconds,
                config.l1_handler_consumption_timelock_seconds,
            ),
            state: ProviderState::Uninitialized,
            clock: clock.unwrap_or_else(|| Arc::new(DefaultClock)),
            start_height: None,
        }
    }
    // Functions Called by the scraper.

    // Start the provider, get first-scrape events, start L2 sync.
    pub async fn initialize(
        &mut self,
        historic_l2_height: BlockNumber,
        events: Vec<Event>,
    ) -> L1ProviderResult<()> {
        info!("Initializing l1 provider");
        if !self.state.is_uninitialized() {
            // FIXME: This should be return FatalError or similar, which should trigger a planned
            // restart from the infra, since this CAN happen if the scraper recovered from a crash.
            // Right now this is effectively a KILL message when called in steady state.
            panic!(
                "Called initialize while not in Uninitialized state. Restart service. Provider \
                 state: {:?}",
                self.state
            );
        };

        // The provider now goes into Pending state.
        // The current_height is set to a very old height, that doesn't include any of the events
        // sent now, or to be scraped in the future. The provider will begin bootstrapping when the
        // batcher calls commit_block with a height above the current height.
        self.start_height = Some(historic_l2_height);
        self.current_height = historic_l2_height;
        self.state = ProviderState::Pending;
        self.add_events(events)?;

        Ok(())
    }

    /// Accept new events from the scraper.
    #[instrument(skip_all, err)]
    pub fn add_events(&mut self, events: Vec<Event>) -> L1ProviderResult<()> {
        if self.state.is_bootstrapping() && !self.bootstrapper.sync_started() {
            return Err(L1ProviderError::Uninitialized);
        }

        // TODO(guyn): can we remove this "every sec" since the polling interval is rather long?
        info_every_n_sec!(1, "Adding {} l1 events", events.len());
        trace!("Adding events: {events:?}");

        for event in events {
            match event {
                Event::L1HandlerTransaction {
                    l1_handler_tx,
                    block_timestamp,
                    scrape_timestamp,
                } => {
                    let tx_hash = l1_handler_tx.tx_hash;
                    let successfully_inserted =
                        self.tx_manager.add_tx(l1_handler_tx, block_timestamp, scrape_timestamp);
                    if !successfully_inserted {
                        debug!(
                            "Unexpected L1 Handler transaction with hash: {tx_hash}, already \
                             known or committed."
                        );
                    }
                }
                Event::TransactionCancellationStarted {
                    tx_hash,
                    cancellation_request_timestamp,
                } => {
                    if !self.tx_manager.exists(tx_hash) {
                        warn!(
                            "Dropping cancellation request for old L1 handler transaction \
                             {tx_hash}: not in the provider and will never be scraped at this \
                             point."
                        );
                        continue;
                    }

                    self.tx_manager
                        .request_cancellation(tx_hash, cancellation_request_timestamp)
                        .inspect(|previous_request_timestamp| {
                            // Re-requesting a cancellation is meaningful for the L1 timelock, but
                            // for the l2 timelock we only consider the first cancellation
                            // relevant.
                            info!(
                                "Dropping duplicated cancellation request for {tx_hash} at \
                                 {cancellation_request_timestamp}, previous request block \
                                 timestamp still stands: {previous_request_timestamp}"
                            );
                        });
                }
                Event::TransactionCanceled(event_data) => {
                    // TODO(guyn): delete the transaction from the provider.
                    info!(
                        "Cancellation finalized with data: {event_data:?}. THIS DOES NOT DELETE \
                         THE TRANSACTION FROM THE PROVIDER YET."
                    );
                }
                Event::TransactionConsumed { tx_hash, timestamp: consumed_at } => {
                    if let Err(previously_consumed_at) =
                        self.tx_manager.consume_tx(tx_hash, consumed_at, self.clock.unix_now())
                    {
                        // TODO(guyn): need to check if this is really a critical bug, or if we can
                        // log and ignore.
                        panic!(
                            "Double consumption of {tx_hash} at {consumed_at}, previously \
                             consumed at {previously_consumed_at}."
                        );
                    }
                }
            }
        }
        Ok(())
    }

    // Functions Called by the batcher.

    /// Start a new block as either proposer or validator.
    #[instrument(skip(self), err)]
    pub fn start_block(
        &mut self,
        height: BlockNumber,
        state: SessionState,
    ) -> L1ProviderResult<()> {
        if self.state.is_bootstrapping() && !self.bootstrapper.sync_started() {
            return Err(L1ProviderError::Uninitialized);
        }

        self.check_height_with_error(height)?;
        info!("Starting block at height: {height}");
        self.state = state.into();
        self.tx_manager.start_block();
        Ok(())
    }

    /// Retrieves up to `n_txs` transactions that have yet to be proposed or accepted on L2.
    /// Used to make new proposals. Must be in Propose state.
    #[instrument(skip(self), err)]
    pub fn get_txs(
        &mut self,
        n_txs: usize,
        height: BlockNumber,
    ) -> L1ProviderResult<Vec<L1HandlerTransaction>> {
        if self.state.is_bootstrapping() && !self.bootstrapper.sync_started() {
            return Err(L1ProviderError::Uninitialized);
        }

        self.check_height_with_error(height)?;

        match self.state {
            ProviderState::Propose => {
                let txs = self.tx_manager.get_txs(n_txs, self.clock.unix_now());
                info!(
                    "Returned {} out of {} transactions, ready for sequencing.",
                    txs.len(),
                    n_txs
                );
                debug!(
                    "Returned L1Handler txs: {:?}",
                    txs.iter().map(|tx| tx.tx_hash).collect::<Vec<_>>()
                );
                Ok(txs)
            }
            _ => Err(L1ProviderError::UnexpectedProviderState {
                expected: ProviderState::Propose,
                found: self.state,
            }),
        }
    }

    /// Returns true if and only if the given transaction is both not included in an L2 block, and
    /// unconsumed on L1. Validator should call validate on each tx during validation.
    /// Must be in Validate state.
    #[instrument(skip(self), err)]
    pub fn validate(
        &mut self,
        tx_hash: TransactionHash,
        height: BlockNumber,
    ) -> L1ProviderResult<ValidationStatus> {
        if self.state.is_bootstrapping() && !self.bootstrapper.sync_started() {
            return Err(L1ProviderError::Uninitialized);
        }

        self.check_height_with_error(height)?;
        match self.state {
            ProviderState::Validate => {
                Ok(self.tx_manager.validate_tx(tx_hash, self.clock.unix_now()))
            }
            _ => Err(L1ProviderError::UnexpectedProviderState {
                expected: ProviderState::Validate,
                found: self.state,
            }),
        }
    }

    // TODO(Gilad): when deciding on consensus, if possible, have commit_block also tell the node if
    // it's about to [optimistically-]propose or validate the next block.
    /// Upon successfully committing a block, commit all committed/rejected transactions, unstage
    /// any remaining transactions, and put provider back in Pending state.
    #[instrument(skip(self), err)]
    pub fn commit_block(
        &mut self,
        committed_txs: IndexSet<TransactionHash>,
        rejected_txs: IndexSet<TransactionHash>,
        height: BlockNumber,
    ) -> L1ProviderResult<()> {
        info!("Committing block to L1 provider at height {}.", height);
        if self.state.is_bootstrapping() && !self.bootstrapper.sync_started() {
            return Err(L1ProviderError::Uninitialized);
        }

        // TODO(guyn): this message is misleading, it checks start_height, not current_height.
        // TODO(guyn): maybe we should indeed ignore all blocks below current_height?
        // See other todo in bootstrap().
        if self.is_historical_height(height) {
            debug!(
                "Skipping commit block for historical height: {}, current height is higher: {}",
                height, self.current_height
            );
            return Ok(());
        }

        // Reroute this block to bootstrapper, either adding it to the backlog, or applying it and
        // ending the bootstrap.
        if self.state.is_bootstrapping() {
            // Once bootstrap completes it will transition to Pending state by itself.
            return self.accept_commit_while_bootstrapping(committed_txs, height);
        }

        // If not historical height and not bootstrapping, must go into bootstrap state upon getting
        // wrong height.
        match self.check_height_with_error(height) {
            Ok(_) => {
                self.apply_commit_block(committed_txs, rejected_txs);
                self.state = self.state.transition_to_pending();
                Ok(())
            }
            Err(err) => {
                // We are returning an error -> not accepting the block with this height. In order
                // to to be able to serve future requests, we must catch up to it, and finish
                // catching up when the provider has synced this height.
                if self.state.is_uninitialized() {
                    warn!(
                        "Provider received a block height ({height}) while it is uninitialized. \
                         Cannot start bootstrapping until getting the historic_height from the \
                         scraper during the initialize call."
                    );
                } else {
                    info!(
                        "Provider received a block_height ({height}) that is higher than the \
                         current height ({}), starting bootstrapping.",
                        self.current_height
                    );
                    self.start_bootstrapping(height);
                }
                Err(err)
            }
        }
    }

    // Functions called internally.

    /// Go from current state to Bootstrap state and start the L2 sync.
    pub fn start_bootstrapping(&mut self, target_height: BlockNumber) {
        self.state = ProviderState::Bootstrap;
        self.bootstrapper.start_l2_sync(self.current_height, target_height);
    }

    /// Commit the given transactions, and increment the current height.
    fn apply_commit_block(
        &mut self,
        consumed_txs: IndexSet<TransactionHash>,
        rejected_txs: IndexSet<TransactionHash>,
    ) {
        debug!("Applying commit_block to height: {}", self.current_height);
        let (rejected_and_consumed, committed_txs): (Vec<_>, Vec<_>) =
            consumed_txs.iter().copied().partition(|tx| rejected_txs.contains(tx));
        self.tx_manager.commit_txs(&committed_txs, &rejected_and_consumed);

        self.current_height = self.current_height.unchecked_next();
    }

    /// Any commit_block call gets rerouted to this function when in bootstrap state.
    /// - If block number is higher than current height, block is backlogged.
    /// - If provider gets a block consistent with current_height, apply it and then the rest of the
    ///   backlog, then transition to Pending state.
    /// - Blocks lower than current height are checked for consistency with existing transactions.
    fn accept_commit_while_bootstrapping(
        &mut self,
        committed_txs: IndexSet<TransactionHash>,
        new_height: BlockNumber,
    ) -> L1ProviderResult<()> {
        let current_height = self.current_height;
        debug!(
            "Bootstrapper processing commit-block at height: {new_height}, current height is \
             {current_height}"
        );
        match new_height.cmp(&current_height) {
            // This is likely a bug in the batcher/sync, it should never be _behind_ the provider.
            Less => {
                // TODO(guyn): check if this is reliable: old blocks can have txs that were
                // committed then consumed and deleted. We should probably decide to always log and
                // ignore old blocks or always return an error.
                let diff_from_already_committed: Vec<_> = committed_txs
                    .iter()
                    .copied()
                    .filter(|&tx_hash| !self.tx_manager.is_committed(tx_hash))
                    .collect();

                if diff_from_already_committed.is_empty() {
                    error!(
                        "Duplicate commit block: commit block for {new_height:?} already \
                         received, and all committed transaction hashes already known to be \
                         committed."
                    );
                    return Ok(());
                } else {
                    // This is either a configuration error or a bug in the
                    // batcher/sync/bootstrapper.
                    error!(
                        "Duplicate commit block: commit block for {new_height:?} already \
                         received, with DIFFERENT transaction_hashes: \
                         {diff_from_already_committed:?}"
                    );
                    Err(L1ProviderError::UnexpectedHeight {
                        expected_height: current_height,
                        got: new_height,
                    })?
                }
            }
            // TODO(guyn): check what about rejected txs here and in the backlog?
            Equal => self.apply_commit_block(committed_txs, Default::default()),
            // We're still syncing, backlog it, it'll get applied later.
            Greater => {
                self.bootstrapper.add_commit_block_to_backlog(committed_txs, new_height);
                // No need to check the backlog or bootstrap completion, since those are only
                // applicable if we just increased the provider's height, like in the `Equal` case.
                return Ok(());
            }
        };

        // If caught up, apply the backlog and transition to Pending.
        // Note that at this point self.current_height is already incremented to the next height, it
        // is one more than the latest block that was committed.
        if self.bootstrapper.is_caught_up(self.current_height) {
            info!(
                "Bootstrapper sync completed, provider height is now {}, processing backlog...",
                self.current_height
            );
            let backlog = std::mem::take(&mut self.bootstrapper.commit_block_backlog);
            assert!(
                backlog.is_empty()
                    || self.current_height == backlog.first().unwrap().height
                        && backlog
                            .windows(2)
                            .all(|height| height[1].height == height[0].height.unchecked_next()),
                "Backlog must have sequential heights starting sequentially after current height: \
                 {}, backlog: {:?}",
                self.current_height,
                backlog.iter().map(|commit_block| commit_block.height).collect::<Vec<_>>()
            );

            info!(
                "Applying commit-block backlog for heights: {:?}",
                backlog.iter().map(|commit_block| commit_block.height).collect::<Vec<_>>()
            );

            for committed_block in backlog {
                self.apply_commit_block(committed_block.committed_txs, Default::default());
            }

            info!(
                "Bootstrapping done: commit-block backlog was processed, now transitioning to \
                 Pending state at new height: {}.",
                self.current_height
            );

            self.state = ProviderState::Pending;
        }

        Ok(())
    }

    fn check_height_with_error(&mut self, height: BlockNumber) -> L1ProviderResult<()> {
        if height != self.current_height {
            return Err(L1ProviderError::UnexpectedHeight {
                expected_height: self.current_height,
                got: height,
            });
        }
        Ok(())
    }

    /// Checks if the given height appears before the timeline of which the provider is aware of.
    fn is_historical_height(&self, height: BlockNumber) -> bool {
        if let Some(start_height) = self.start_height {
            height < start_height
        } else {
            //  If start_height is not set, the provider is not initialized yet, so there is no
            // historical height.
            false
        }
    }

    // Functions used for debugging or testing.

    pub fn get_l1_provider_snapshot(&self) -> L1ProviderResult<L1ProviderSnapshot> {
        let txs_snapshot = self.tx_manager.snapshot();
        Ok(L1ProviderSnapshot {
            uncommitted_transactions: txs_snapshot.uncommitted,
            uncommitted_staged_transactions: txs_snapshot.uncommitted_staged,
            rejected_transactions: txs_snapshot.rejected,
            rejected_staged_transactions: txs_snapshot.rejected_staged,
            committed_transactions: txs_snapshot.committed,
            cancellation_started_on_l2: txs_snapshot.cancellation_started_on_l2,
            cancelled_on_l2: txs_snapshot.cancelled_on_l2,
            consumed: txs_snapshot.consumed,
            l1_provider_state: self.state.to_string(),
            current_height: self.current_height,
            number_of_txs_in_records: self.tx_manager.records.len(),
        })
    }

    pub fn get_provider_state(&self) -> L1ProviderResult<ProviderState> {
        Ok(self.state)
    }
}

impl PartialEq for L1Provider {
    fn eq(&self, other: &Self) -> bool {
        self.current_height == other.current_height
            && self.tx_manager == other.tx_manager
            && self.state == other.state
    }
}

impl ComponentStarter for L1Provider {}
