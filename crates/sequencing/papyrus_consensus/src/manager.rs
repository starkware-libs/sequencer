//! Consensus manager, see Manager struct.

#[cfg(test)]
#[path = "manager_test.rs"]
mod manager_test;

use std::collections::BTreeMap;

use futures::channel::{mpsc, oneshot};
use futures::{Stream, StreamExt};
use papyrus_network::network_manager::ReportSender;
use papyrus_protobuf::consensus::ConsensusMessage;
use papyrus_protobuf::converters::ProtobufConversionError;
use starknet_api::block::{BlockHash, BlockNumber};
use tracing::{debug, instrument};

use crate::single_height_consensus::SingleHeightConsensus;
use crate::types::{
    ConsensusBlock,
    ConsensusContext,
    ConsensusError,
    Decision,
    ProposalInit,
    ValidatorId,
};
use crate::ProposalWrapper;

/// Runs Tendermint repeatedly across different heights. Handles issues which are not explicitly
/// part of the single height consensus algorithm (e.g. messages from future heights).
#[derive(Debug, Default)]
pub struct MultiHeightManager {
    cached_messages: BTreeMap<u64, Vec<ConsensusMessage>>,
}

impl MultiHeightManager {
    /// Create a new consensus manager.
    pub fn new() -> Self {
        Self { cached_messages: BTreeMap::new() }
    }

    /// Run the consensus algorithm for a single height.
    ///
    /// Assumes that `height` is monotonically increasing across calls for the sake of filtering
    /// `cached_messaged`.
    #[instrument(skip(self, context, validator_id, network_receiver), level = "info")]
    pub async fn run_height<BlockT, ContextT, NetworkReceiverT>(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        validator_id: ValidatorId,
        network_receiver: &mut NetworkReceiverT,
    ) -> Result<Decision<BlockT>, ConsensusError>
    where
        BlockT: ConsensusBlock,
        ContextT: ConsensusContext<Block = BlockT>,
        NetworkReceiverT: Stream<Item = (Result<ConsensusMessage, ProtobufConversionError>, ReportSender)>
            + Unpin,
        ProposalWrapper: Into<(
            ProposalInit,
            mpsc::Receiver<BlockT::ProposalChunk>,
            oneshot::Receiver<BlockHash>,
        )>,
    {
        let validators = context.validators(height).await;
        let mut shc = SingleHeightConsensus::new(height, validator_id, validators);

        if let Some(decision) = shc.start(context).await? {
            return Ok(decision);
        }

        let mut current_height_messages = self.get_current_height_messages(height);
        loop {
            let message = next_message(&mut current_height_messages, network_receiver).await?;
            // TODO(matan): We need to figure out an actual cacheing strategy under 2 constraints:
            // 1. Malicious - must be capped so a malicious peer can't DoS us.
            // 2. Parallel proposals - we may send/receive a proposal for (H+1, 0).
            // In general I think we will want to only cache (H+1, 0) messages.
            if message.height() != height.0 {
                debug!("Received a message for a different height. {:?}", message);
                if message.height() > height.0 {
                    self.cached_messages.entry(message.height()).or_default().push(message);
                }
                continue;
            }

            let maybe_decision = match message {
                ConsensusMessage::Proposal(proposal) => {
                    // Special case due to fake streaming.
                    let (proposal_init, content_receiver, fin_receiver) =
                        ProposalWrapper(proposal).into();
                    shc.handle_proposal(context, proposal_init, content_receiver, fin_receiver)
                        .await?
                }
                _ => shc.handle_message(context, message).await?,
            };

            if let Some(decision) = maybe_decision {
                return Ok(decision);
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

async fn next_message<NetworkReceiverT>(
    cached_messages: &mut Vec<ConsensusMessage>,
    network_receiver: &mut NetworkReceiverT,
) -> Result<ConsensusMessage, ConsensusError>
where
    NetworkReceiverT:
        Stream<Item = (Result<ConsensusMessage, ProtobufConversionError>, ReportSender)> + Unpin,
{
    if let Some(msg) = cached_messages.pop() {
        return Ok(msg);
    }

    let (msg, report_sender) = network_receiver.next().await.ok_or_else(|| {
        ConsensusError::InternalNetworkError(format!("NetworkReceiver should never be closed"))
    })?;
    match msg {
        // TODO(matan): Return report_sender for use in later errors by SHC.
        Ok(msg) => Ok(msg),
        Err(e) => {
            // Failed to parse consensus message
            report_sender
                .send(())
                .or(Err(ConsensusError::InternalNetworkError(format!("Failed to send report"))))?;
            Err(e.into())
        }
    }
}
