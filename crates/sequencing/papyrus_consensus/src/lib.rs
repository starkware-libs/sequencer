#![warn(missing_docs)]
// TODO(Matan): Add a description of the crate.
// TODO(Matan): fix #[allow(missing_docs)].
//! A consensus implementation for a [`Starknet`](https://www.starknet.io/) node.

use std::time::Duration;

use futures::channel::{mpsc, oneshot};
use futures::Stream;
use manager::Manager;
use papyrus_common::metrics as papyrus_metrics;
use papyrus_network::network_manager::ReportSender;
use papyrus_protobuf::consensus::{ConsensusMessage, Proposal};
use papyrus_protobuf::converters::ProtobufConversionError;
use starknet_api::block::{BlockHash, BlockNumber};
use tracing::{debug, info, instrument};
use types::{ConsensusBlock, ConsensusContext, ConsensusError, ProposalInit, ValidatorId};

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

// TODO(dvir): add test for this.
#[instrument(skip(context, start_height, network_receiver), level = "info")]
#[allow(missing_docs)]
pub async fn run_consensus<BlockT, ContextT, NetworkReceiverT>(
    mut context: ContextT,
    start_height: BlockNumber,
    validator_id: ValidatorId,
    consensus_delay: Duration,
    mut network_receiver: NetworkReceiverT,
) -> Result<(), ConsensusError>
where
    BlockT: ConsensusBlock,
    ContextT: ConsensusContext<Block = BlockT>,
    NetworkReceiverT:
        Stream<Item = (Result<ConsensusMessage, ProtobufConversionError>, ReportSender)> + Unpin,
    ProposalWrapper:
        Into<(ProposalInit, mpsc::Receiver<BlockT::ProposalChunk>, oneshot::Receiver<BlockHash>)>,
{
    // Add a short delay to allow peers to connect and avoid "InsufficientPeers" error
    tokio::time::sleep(consensus_delay).await;
    let mut current_height = start_height;
    let mut manager = Manager::new();
    loop {
        let decision = manager
            .run_height(&mut context, current_height, validator_id, &mut network_receiver)
            .await?;

        info!(
            "Finished consensus for height: {current_height}. Agreed on block with id: {:x}",
            decision.block.id().0
        );
        debug!("Decision: {:?}", decision);
        metrics::gauge!(papyrus_metrics::PAPYRUS_CONSENSUS_HEIGHT, current_height.0 as f64);
        current_height = current_height.unchecked_next();
    }
}

// `Proposal` is defined in the protobuf crate so we can't implement `Into` for it because of the
// orphan rule. This wrapper enables us to implement `Into` for the inner `Proposal`.
#[allow(missing_docs)]
pub struct ProposalWrapper(Proposal);
