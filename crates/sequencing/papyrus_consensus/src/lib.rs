#![warn(missing_docs)]
// TODO(Matan): Add a description of the crate.
// TODO(Matan): fix #[allow(missing_docs)].
//! A consensus implementation for a [`Starknet`](https://www.starknet.io/) node.

use std::time::Duration;

use futures::channel::{mpsc, oneshot};
use futures::future::Either;
use futures::stream::FuturesUnordered;
use futures::{Stream, StreamExt};
use manager::MultiHeightManager;
use papyrus_common::metrics as papyrus_metrics;
use papyrus_network::network_manager::ReportSender;
use papyrus_protobuf::consensus::{ConsensusMessage, Proposal};
use papyrus_protobuf::converters::ProtobufConversionError;
use starknet_api::block::{BlockHash, BlockNumber};
use tracing::{debug, info, instrument};
use types::{
    ConsensusBlock,
    ConsensusContext,
    ConsensusError,
    Decision,
    ProposalInit,
    ValidatorId,
};

pub mod config;
pub mod manager;
#[allow(missing_docs)]
pub mod papyrus_consensus_context;
#[allow(missing_docs)]
pub mod simulation_network_receiver;
#[allow(missing_docs)]
pub mod single_height_consensus;
#[allow(missing_docs)]
pub mod state_machine;
#[cfg(test)]
pub(crate) mod test_utils;
#[allow(missing_docs)]
pub mod types;

// Return only when a height is reached that is greater than or equal to the current height.
async fn future_height<SyncReceiverT>(
    height: BlockNumber,
    mut sync_receiver: SyncReceiverT,
) -> Option<BlockNumber>
where
    SyncReceiverT: Stream<Item = BlockNumber> + Unpin,
{
    loop {
        match sync_receiver.next().await {
            Some(sync_height) => {
                debug!("Sync to height: {}. current_height={}", sync_height, height);
                if sync_height >= height {
                    return Some(sync_height);
                }
            }
            None => {
                return None;
            }
        }
    }
}

// TODO(dvir): add test for this.
#[instrument(skip(context, start_height, network_receiver, sync_receiver), level = "info")]
#[allow(missing_docs)]
pub async fn run_consensus<BlockT, ContextT, NetworkReceiverT, SyncReceiverT>(
    mut context: ContextT,
    start_height: BlockNumber,
    validator_id: ValidatorId,
    consensus_delay: Duration,
    mut network_receiver: NetworkReceiverT,
    mut sync_receiver: SyncReceiverT,
) -> Result<(), ConsensusError>
where
    BlockT: ConsensusBlock,
    ContextT: ConsensusContext<Block = BlockT>,
    NetworkReceiverT:
        Stream<Item = (Result<ConsensusMessage, ProtobufConversionError>, ReportSender)> + Unpin,
    SyncReceiverT: Stream<Item = BlockNumber> + Unpin,
    ProposalWrapper:
        Into<(ProposalInit, mpsc::Receiver<BlockT::ProposalChunk>, oneshot::Receiver<BlockHash>)>,
{
    // Add a short delay to allow peers to connect and avoid "InsufficientPeers" error
    tokio::time::sleep(consensus_delay).await;
    let mut current_height = start_height;
    let mut manager = MultiHeightManager::new();

    loop {
        // run_height is not cancel safe and so other branches must only be reached if we intend are
        // certain to exit the current height. Hence the use of the `future_height` function.
        let run_height =
            manager.run_height(&mut context, current_height, validator_id, &mut network_receiver);
        tokio::select! {
            decision = run_height => {
                let decision = decision?;
                info!(
                    "Finished consensus for height: {current_height}. Agreed on block with id: \
                     {:x}",
                    decision.block.id().0
                );
                debug!("Decision: {:?}", decision);
                metrics::gauge!(papyrus_metrics::PAPYRUS_CONSENSUS_HEIGHT, current_height.0 as f64);
                current_height = current_height.unchecked_next();
            }
            sync_height = future_height(current_height, &mut sync_receiver) => {
                let Some(sync_height) = sync_height else {
                    return Err(ConsensusError::Other("Sync receiver closed".to_string()));
                };
                current_height = sync_height.unchecked_next();
            }
        };
    }
}

// `Proposal` is defined in the protobuf crate so we can't implement `Into` for it because of the
// orphan rule. This wrapper enables us to implement `Into` for the inner `Proposal`.
#[allow(missing_docs)]
pub struct ProposalWrapper(Proposal);
