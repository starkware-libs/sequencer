//! Consensus manager, see Manager struct.

#[cfg(test)]
#[path = "manager_test.rs"]
mod manager_test;

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
pub struct MultiHeightManager {
    cached_messages: Vec<ConsensusMessage>,
}

impl MultiHeightManager {
    /// Create a new consensus manager.
    pub fn new() -> Self {
        Self { cached_messages: Vec::new() }
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

        let mut current_height_messages = Vec::new();
        for msg in std::mem::take(&mut self.cached_messages) {
            match height.0.cmp(&msg.height()) {
                std::cmp::Ordering::Less => self.cached_messages.push(msg),
                std::cmp::Ordering::Equal => current_height_messages.push(msg),
                std::cmp::Ordering::Greater => {}
            }
        }

        loop {
            let message = if let Some(msg) = current_height_messages.pop() {
                msg
            } else {
                // TODO(matan): Handle parsing failures and utilize ReportCallback.
                network_receiver
                    .next()
                    .await
                    .expect("Network receiver closed unexpectedly")
                    .0
                    .expect("Failed to parse consensus message")
            };

            if message.height() != height.0 {
                debug!("Received a message for a different height. {:?}", message);
                if message.height() > height.0 {
                    self.cached_messages.push(message);
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
}
