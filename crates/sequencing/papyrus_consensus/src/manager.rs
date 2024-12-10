//! Consensus manager, see Manager struct.

#[cfg(test)]
#[path = "manager_test.rs"]
mod manager_test;

use std::collections::BTreeMap;
use std::time::Duration;

use futures::channel::mpsc;
use futures::stream::FuturesUnordered;
use futures::{Stream, StreamExt};
use papyrus_common::metrics::{PAPYRUS_CONSENSUS_HEIGHT, PAPYRUS_CONSENSUS_SYNC_COUNT};
use papyrus_network::network_manager::BroadcastTopicClientTrait;
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use papyrus_protobuf::consensus::{ConsensusMessage, ProposalInit};
use papyrus_protobuf::converters::ProtobufConversionError;
use starknet_api::block::BlockNumber;
use tracing::{debug, info, instrument};

use crate::config::TimeoutsConfig;
use crate::single_height_consensus::{ShcReturn, SingleHeightConsensus};
use crate::types::{
    BroadcastConsensusMessageChannel,
    ConsensusContext,
    ConsensusError,
    Decision,
    ValidatorId,
};

// TODO(dvir): add test for this.
#[instrument(skip_all, level = "info")]
#[allow(missing_docs)]
#[allow(clippy::too_many_arguments)]
pub async fn run_consensus<ContextT, SyncReceiverT>(
    mut context: ContextT,
    start_active_height: BlockNumber,
    start_observe_height: BlockNumber,
    validator_id: ValidatorId,
    consensus_delay: Duration,
    timeouts: TimeoutsConfig,
    mut broadcast_channels: BroadcastConsensusMessageChannel,
    mut inbound_proposal_receiver: mpsc::Receiver<mpsc::Receiver<ContextT::ProposalPart>>,
    mut sync_receiver: SyncReceiverT,
) -> Result<(), ConsensusError>
where
    ContextT: ConsensusContext,
    SyncReceiverT: Stream<Item = BlockNumber> + Unpin,
{
    info!(
        "Running consensus, start_active_height={}, start_observe_height={}, validator_id={}, \
         consensus_delay={}, timeouts={:?}",
        start_active_height,
        start_observe_height,
        validator_id,
        consensus_delay.as_secs(),
        timeouts
    );

    // Add a short delay to allow peers to connect and avoid "InsufficientPeers" error
    tokio::time::sleep(consensus_delay).await;
    assert!(start_observe_height <= start_active_height);
    let mut current_height = start_observe_height;
    let mut manager = MultiHeightManager::new(validator_id, timeouts);
    #[allow(clippy::as_conversions)] // FIXME: use int metrics so `as f64` may be removed.
    loop {
        metrics::gauge!(PAPYRUS_CONSENSUS_HEIGHT, current_height.0 as f64);

        let is_observer = current_height < start_active_height;
        match manager
            .run_height(
                &mut context,
                current_height,
                is_observer,
                &mut broadcast_channels,
                &mut inbound_proposal_receiver,
                &mut sync_receiver,
            )
            .await?
        {
            RunHeightRes::Decision(decision) => {
                context.decision_reached(decision.block, decision.precommits).await?;
                current_height = current_height.unchecked_next();
            }
            RunHeightRes::Sync(sync_height) => {
                metrics::increment_counter!(PAPYRUS_CONSENSUS_SYNC_COUNT);
                current_height = sync_height.unchecked_next();
            }
        }
    }
}

/// Run height can end either when consensus reaches a decision or when we learn, via sync, of the
/// decision.
// TODO(Matan): Sync may change when Shahak actually implements.
pub enum RunHeightRes {
    /// Decision reached.
    Decision(Decision),
    /// Sync protocol returned a future height.
    Sync(BlockNumber),
}

/// Runs Tendermint repeatedly across different heights. Handles issues which are not explicitly
/// part of the single height consensus algorithm (e.g. messages from future heights).
#[derive(Debug, Default)]
struct MultiHeightManager<ContextT: ConsensusContext> {
    validator_id: ValidatorId,
    cached_messages: BTreeMap<u64, Vec<ConsensusMessage>>,
    cached_proposals: BTreeMap<u64, (ProposalInit, mpsc::Receiver<ContextT::ProposalPart>)>,
    timeouts: TimeoutsConfig,
}

impl<ContextT: ConsensusContext> MultiHeightManager<ContextT> {
    /// Create a new consensus manager.
    pub fn new(validator_id: ValidatorId, timeouts: TimeoutsConfig) -> Self {
        Self {
            validator_id,
            cached_messages: BTreeMap::new(),
            cached_proposals: BTreeMap::new(),
            timeouts,
        }
    }

    /// Run the consensus algorithm for a single height.
    ///
    /// Assumes that `height` is monotonically increasing across calls for the sake of filtering
    /// `cached_messaged`.
    #[instrument(skip(self, context, broadcast_channels, sync_receiver), level = "info")]
    pub async fn run_height<SyncReceiverT>(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        is_observer: bool,
        broadcast_channels: &mut BroadcastConsensusMessageChannel,
        proposal_receiver: &mut mpsc::Receiver<mpsc::Receiver<ContextT::ProposalPart>>,
        sync_receiver: &mut SyncReceiverT,
    ) -> Result<RunHeightRes, ConsensusError>
    where
        SyncReceiverT: Stream<Item = BlockNumber> + Unpin,
    {
        let validators = context.validators(height).await;
        info!("running consensus for height {height:?} with validator set {validators:?}");
        let mut shc = SingleHeightConsensus::new(
            height,
            is_observer,
            self.validator_id,
            validators,
            self.timeouts.clone(),
        );
        let mut shc_events = FuturesUnordered::new();

        match self.start_height(context, height, &mut shc).await? {
            ShcReturn::Decision(decision) => return Ok(RunHeightRes::Decision(decision)),
            ShcReturn::Tasks(tasks) => {
                for task in tasks {
                    shc_events.push(task.run());
                }
            }
        }

        // Loop over incoming proposals, messages, and self generated events.
        loop {
            let shc_return = tokio::select! {
                message = broadcast_channels.broadcasted_messages_receiver.next() => {
                    self.handle_message(
                        context, height, &mut shc, message, broadcast_channels).await?
                },
                Some(mut content_receiver) = proposal_receiver.next() => {
                    // Get the first message to verify the init was sent.
                    // TODO(guyn): add a timeout and panic, since StreamHandler should only send once
                    // the first message (message_id=0) has arrived.
                    let Some(first_part) = content_receiver.next().await else {
                        return Err(ConsensusError::InternalNetworkError(
                            "Proposal receiver closed".to_string(),
                        ));
                    };
                    let proposal_init: ProposalInit = first_part.try_into()?;
                    self.handle_proposal(context, height, &mut shc, proposal_init, content_receiver).await?
                },
                Some(shc_event) = shc_events.next() => {
                    shc.handle_event(context, shc_event).await?
                },
                sync_height = sync_receiver.next() => {
                    let Some(sync_height) = sync_height else {
                        return Err(ConsensusError::SyncError("Sync receiver closed".to_string()))
                    };
                    if sync_height >= height {
                        info!("Sync to height: {}. current_height={}", sync_height, height);
                        return Ok(RunHeightRes::Sync(sync_height));
                    }
                    debug!("Ignoring sync to height: {}. current_height={}", sync_height, height);
                    continue;
                }
            };

            match shc_return {
                ShcReturn::Decision(decision) => return Ok(RunHeightRes::Decision(decision)),
                ShcReturn::Tasks(tasks) => {
                    for task in tasks {
                        shc_events.push(task.run());
                    }
                }
            }
        }
    }

    async fn start_height(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        shc: &mut SingleHeightConsensus,
    ) -> Result<ShcReturn, ConsensusError> {
        let mut tasks = match shc.start(context).await? {
            decision @ ShcReturn::Decision(_) => return Ok(decision),
            ShcReturn::Tasks(tasks) => tasks,
        };

        if let Some((init, content_receiver)) = self.get_current_proposal(height) {
            match shc.handle_proposal(context, init, content_receiver).await? {
                decision @ ShcReturn::Decision(_) => return Ok(decision),
                ShcReturn::Tasks(new_tasks) => tasks.extend(new_tasks),
            }
        };

        for msg in self.get_current_height_messages(height) {
            match shc.handle_message(context, msg).await? {
                decision @ ShcReturn::Decision(_) => return Ok(decision),
                ShcReturn::Tasks(new_tasks) => tasks.extend(new_tasks),
            }
        }

        Ok(ShcReturn::Tasks(tasks))
    }

    // Handle a new proposal receiver from the network.
    async fn handle_proposal(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        shc: &mut SingleHeightConsensus,
        proposal_init: ProposalInit,
        content_receiver: mpsc::Receiver<ContextT::ProposalPart>,
    ) -> Result<ShcReturn, ConsensusError> {
        if proposal_init.height != height {
            debug!("Received a proposal for a different height. {:?}", proposal_init);
            if proposal_init.height > height {
                // Note: this will overwrite an existing content_receiver for this height!
                self.cached_proposals
                    .insert(proposal_init.height.0, (proposal_init, content_receiver));
            }
            return Ok(ShcReturn::Tasks(Vec::new()));
        }
        shc.handle_proposal(context, proposal_init, content_receiver).await
    }

    // Handle a single consensus message.
    async fn handle_message(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        shc: &mut SingleHeightConsensus,
        message: Option<(
            Result<ConsensusMessage, ProtobufConversionError>,
            BroadcastedMessageMetadata,
        )>,
        broadcast_channels: &mut BroadcastConsensusMessageChannel,
    ) -> Result<ShcReturn, ConsensusError> {
        let message = match message {
            None => Err(ConsensusError::InternalNetworkError(
                "NetworkReceiver should never be closed".to_string(),
            )),
            Some((Ok(msg), metadata)) => {
                // TODO(matan): Hold onto report_sender for use in later errors by SHC.
                let _ =
                    broadcast_channels.broadcast_topic_client.continue_propagation(&metadata).await;
                Ok(msg)
            }
            Some((Err(e), metadata)) => {
                // Failed to parse consensus message
                let _ = broadcast_channels.broadcast_topic_client.report_peer(metadata).await;
                Err(e.into())
            }
        }?;

        // TODO(matan): We need to figure out an actual caching strategy under 2 constraints:
        // 1. Malicious - must be capped so a malicious peer can't DoS us.
        // 2. Parallel proposals - we may send/receive a proposal for (H+1, 0).
        // In general I think we will want to only cache (H+1, 0) messages.
        if message.height() != height.0 {
            debug!("Received a message for a different height. {:?}", message);
            if message.height() > height.0 {
                self.cached_messages.entry(message.height()).or_default().push(message);
            }
            return Ok(ShcReturn::Tasks(Vec::new()));
        }

        shc.handle_message(context, message).await
    }

    // Checks if a cached proposal already exists
    // - returns the proposal if it exists and removes it from the cache.
    // - returns None if no proposal exists.
    // - cleans up any proposals from earlier heights.
    fn get_current_proposal(
        &mut self,
        height: BlockNumber,
    ) -> Option<(ProposalInit, mpsc::Receiver<ContextT::ProposalPart>)> {
        loop {
            let entry = self.cached_proposals.first_entry()?;
            match entry.key().cmp(&height.0) {
                std::cmp::Ordering::Greater => return None,
                std::cmp::Ordering::Equal => return Some(entry.remove()),
                std::cmp::Ordering::Less => {
                    entry.remove();
                }
            }
        }
    }

    // Filters the cached messages:
    // - returns all of the current height messages.
    // - drops messages from earlier heights.
    // - retains future messages in the cache.
    fn get_current_height_messages(&mut self, height: BlockNumber) -> Vec<ConsensusMessage> {
        // Depends on `cached_messages` being sorted by height.
        loop {
            let Some(entry) = self.cached_messages.first_entry() else {
                return Vec::new();
            };
            match entry.key().cmp(&height.0) {
                std::cmp::Ordering::Greater => return Vec::new(),
                std::cmp::Ordering::Equal => return entry.remove(),
                std::cmp::Ordering::Less => {
                    entry.remove();
                }
            }
        }
    }
}
