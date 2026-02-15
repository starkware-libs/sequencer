//! Types for interfacing between consensus and the node.
use std::fmt::Debug;
use std::time::Duration;

use apollo_batcher_types::communication::BatcherClientError;
use apollo_network::network_manager::{
    BroadcastTopicChannels,
    BroadcastTopicClient,
    GenericReceiver,
};
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_protobuf::consensus::{BuildParam, ProposalInit, SignedProposalPart, Vote};
pub use apollo_protobuf::consensus::{ProposalCommitment, Round};
use apollo_protobuf::converters::ProtobufConversionError;
use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;

/// Used to identify the node by consensus.
/// 1. This ID is derived from the id registered with Starknet's L2 staking contract.
/// 2. We must be able to derive the public key associated with this ID for the sake of validating
///    signatures.
// TODO(matan): Determine the actual type of NodeId.
pub type ValidatorId = ContractAddress;

/// Interface for consensus to call out to the node.
///
/// Function calls should be assumed to not be cancel safe.
#[async_trait]
pub trait ConsensusContext {
    /// The parts of the proposal that are streamed in.
    /// Must contain at least the SignedProposalPart and ProposalFin.
    type SignedProposalPart: TryFrom<Vec<u8>, Error = ProtobufConversionError>
        + Into<Vec<u8>>
        + TryInto<SignedProposalPart>
        + From<SignedProposalPart>
        + Clone
        + Send
        + Debug;

    // TODO(matan): The oneshot for receiving the build block could be generalized to just be some
    // future which returns a block.

    /// This function is called by consensus to request a block from the node. It expects that this
    /// call will return immediately and that consensus can then stream in the block's content in
    /// parallel to the block being built.
    ///
    /// Params:
    /// - `build_param`: The `BuildParam` that is broadcast to the network.
    /// - `timeout`: The maximum time to wait for the block to be built.
    ///
    /// Returns:
    /// - A receiver for the block id once ConsensusContext has finished streaming out the content
    ///   and building it. If the block fails to be built, the Sender will be dropped by
    ///   ConsensusContext.
    async fn build_proposal(
        &mut self,
        build_param: BuildParam,
        timeout: Duration,
    ) -> Result<oneshot::Receiver<ProposalCommitment>, ConsensusError>;

    /// This function is called by consensus to validate a block. It expects that this call will
    /// return immediately and that context can then stream in the block's content in parallel to
    /// consensus continuing to handle other tasks.
    ///
    /// Params:
    /// - `height`: The height of the block to be built. Specifically this indicates the initial
    ///   state of the block.
    /// - `round`: The round of the block to be built.
    /// - `timeout`: The maximum time to wait for the block to be built.
    /// - `content`: A receiver for the stream of the block's content.
    ///
    /// Returns:
    /// - A receiver for the block id. If a valid block cannot be built the Sender will be dropped
    ///   by ConsensusContext.
    async fn validate_proposal(
        &mut self,
        init: ProposalInit,
        timeout: Duration,
        content: mpsc::Receiver<Self::SignedProposalPart>,
    ) -> oneshot::Receiver<ProposalCommitment>;

    /// This function is called by consensus to retrieve the content of a previously built or
    /// validated proposal. It broadcasts the proposal to the network.
    ///
    /// Params:
    /// - `id`: The `ProposalCommitment` associated with the block's content.
    /// - `build_param`: The consensus metadata for reproposing.
    async fn repropose(&mut self, id: ProposalCommitment, build_param: BuildParam);

    async fn broadcast(&mut self, message: Vote) -> Result<(), ConsensusError>;

    /// Update the context that a decision has been reached for a given height.
    /// - `commitment` identifies the decision.
    async fn decision_reached(
        &mut self,
        height: BlockNumber,
        commitment: ProposalCommitment,
    ) -> Result<(), ConsensusError>;

    /// Attempt to learn of a decision from the sync protocol.
    /// Returns true if a decision was learned so consensus can proceed.
    async fn try_sync(&mut self, height: BlockNumber) -> bool;

    /// Update the context with the current height and round.
    /// Must be called at the beginning of each height.
    async fn set_height_and_round(
        &mut self,
        height: BlockNumber,
        round: Round,
    ) -> Result<(), ConsensusError>;
}

#[derive(PartialEq, Debug)]
pub struct Decision {
    pub precommits: Vec<Vote>,
    pub block: ProposalCommitment,
}

pub struct BroadcastVoteChannel {
    pub broadcasted_messages_receiver:
        GenericReceiver<(Result<Vote, ProtobufConversionError>, BroadcastedMessageMetadata)>,
    pub broadcast_topic_client: BroadcastTopicClient<Vote>,
}

impl From<BroadcastTopicChannels<Vote>> for BroadcastVoteChannel {
    fn from(broadcast_topic_channels: BroadcastTopicChannels<Vote>) -> Self {
        BroadcastVoteChannel {
            broadcasted_messages_receiver: Box::new(
                broadcast_topic_channels.broadcasted_messages_receiver,
            ),
            broadcast_topic_client: broadcast_topic_channels.broadcast_topic_client,
        }
    }
}

#[derive(thiserror::Error, PartialEq, Debug)]
pub enum ConsensusError {
    #[error(transparent)]
    BatcherError(#[from] BatcherClientError),
    #[error("Committee error: {0}")]
    CommitteeError(String),
    // Indicates an error in communication between consensus and the node's networking component.
    // As opposed to an error between this node and peer nodes.
    #[error("{0}")]
    InternalNetworkError(String),
}
