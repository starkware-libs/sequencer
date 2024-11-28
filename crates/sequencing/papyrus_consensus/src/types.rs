use std::fmt::Debug;
use std::time::Duration;

use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use papyrus_network::network_manager::{
    BroadcastTopicChannels,
    BroadcastTopicClient,
    GenericReceiver,
};
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use papyrus_protobuf::consensus::{ConsensusMessage, ProposalInit, Vote};
use papyrus_protobuf::converters::ProtobufConversionError;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ContractAddress;

/// Used to identify the node by consensus.
/// 1. This ID is derived from the id registered with Starknet's L2 staking contract.
/// 2. We must be able to derive the public key associated with this ID for the sake of validating
///    signatures.
// TODO(matan): Determine the actual type of NodeId.
pub type ValidatorId = ContractAddress;
pub type Round = u32;
pub type ProposalContentId = BlockHash;

/// Interface for consensus to call out to the node.
#[async_trait]
pub trait ConsensusContext {
    /// The chunks of content returned when iterating the proposal.
    // In practice I expect this to match the type sent to the network
    // (papyrus_protobuf::ConsensusMessage), and not to be specific to just the block's content.
    type ProposalChunk;

    // TODO(matan): The oneshot for receiving the build block could be generalized to just be some
    // future which returns a block.

    /// This function is called by consensus to request a block from the node. It expects that this
    /// call will return immediately and that consensus can then stream in the block's content in
    /// parallel to the block being built.
    ///
    /// Params:
    /// - `init`: The `ProposalInit` that is broadcast to the network.
    /// - `timeout`: The maximum time to wait for the block to be built.
    ///
    /// Returns:
    /// - A receiver for the block id once ConsensusContext has finished streaming out the content
    ///   and building it. If the block fails to be built, the Sender will be dropped by
    ///   ConsensusContext.
    async fn build_proposal(
        &mut self,
        init: ProposalInit,
        timeout: Duration,
    ) -> oneshot::Receiver<ProposalContentId>;

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
        height: BlockNumber,
        round: Round,
        proposer: ValidatorId,
        timeout: Duration,
        content: mpsc::Receiver<Self::ProposalChunk>,
    ) -> oneshot::Receiver<ProposalContentId>;

    /// This function is called by consensus to retrieve the content of a previously built or
    /// validated proposal. It broadcasts the proposal to the network.
    ///
    /// Params:
    /// - `id`: The `ProposalContentId` associated with the block's content.
    /// - `init`: The `ProposalInit` that is broadcast to the network.
    async fn repropose(&mut self, id: ProposalContentId, init: ProposalInit);

    /// Get the set of validators for a given height. These are the nodes that can propose and vote
    /// on blocks.
    // TODO(matan): We expect this to change in the future to BTreeMap. Why?
    // 1. Map - The nodes will have associated information (e.g. voting weight).
    // 2. BTreeMap - We want a stable ordering of the nodes for deterministic leader selection.
    async fn validators(&self, height: BlockNumber) -> Vec<ValidatorId>;

    /// Calculates the ID of the Proposer based on the inputs.
    fn proposer(&self, height: BlockNumber, round: Round) -> ValidatorId;

    async fn broadcast(&mut self, message: ConsensusMessage) -> Result<(), ConsensusError>;

    /// Update the context that a decision has been reached for a given height.
    /// - `block` identifies the decision.
    /// - `precommits` - All precommits must be for the same `(block, height, round)` and form a
    ///   quorum (>2/3 of the voting power) for this height.
    async fn decision_reached(
        &mut self,
        block: ProposalContentId,
        precommits: Vec<Vote>,
    ) -> Result<(), ConsensusError>;

    /// Update the context with the current height and round.
    /// Must be called at the beginning of each height.
    async fn set_height_and_round(
        &mut self,
        height: BlockNumber,
        round: Round,
        proposer: ValidatorId,
    );
}

#[derive(PartialEq)]
pub struct Decision {
    pub precommits: Vec<Vote>,
    pub block: ProposalContentId,
}

impl Debug for Decision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decision")
            .field("block", &self.block)
            .field("precommits", &self.precommits)
            .finish()
    }
}

pub struct BroadcastConsensusMessageChannel {
    pub broadcasted_messages_receiver: GenericReceiver<(
        Result<ConsensusMessage, ProtobufConversionError>,
        BroadcastedMessageMetadata,
    )>,
    pub broadcast_topic_client: BroadcastTopicClient<ConsensusMessage>,
}

impl From<BroadcastTopicChannels<ConsensusMessage>> for BroadcastConsensusMessageChannel {
    fn from(broadcast_topic_channels: BroadcastTopicChannels<ConsensusMessage>) -> Self {
        BroadcastConsensusMessageChannel {
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
    Canceled(#[from] oneshot::Canceled),
    #[error(transparent)]
    ProtobufConversionError(#[from] ProtobufConversionError),
    /// This should never occur, since events are internally generated.
    #[error("Invalid event: {0}")]
    InvalidEvent(String),
    #[error("Invalid proposal sent by peer {0:?} at height {1}: {2}")]
    InvalidProposal(ValidatorId, BlockNumber, String),
    #[error(transparent)]
    SendError(#[from] mpsc::SendError),
    #[error("Conflicting messages for block {0}. Old: {1:?}, New: {2:?}")]
    Equivocation(BlockNumber, ConsensusMessage, ConsensusMessage),
    // Indicates an error in communication between consensus and the node's networking component.
    // As opposed to an error between this node and peer nodes.
    #[error("{0}")]
    InternalNetworkError(String),
    #[error("{0}")]
    SyncError(String),
}
