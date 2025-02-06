//! Types for interfacing between consensus and the node.

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::time::Duration;

use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_network::network_manager::{
    BroadcastTopicChannels,
    BroadcastTopicClient,
    GenericReceiver,
};
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use papyrus_protobuf::consensus::{ProposalFin, ProposalInit, Vote};
use papyrus_protobuf::converters::ProtobufConversionError;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ChainId, ContractAddress};
use validator::Validate;

/// Used to identify the node by consensus.
/// 1. This ID is derived from the id registered with Starknet's L2 staking contract.
/// 2. We must be able to derive the public key associated with this ID for the sake of validating
///    signatures.
// TODO(matan): Determine the actual type of NodeId.
pub type ValidatorId = ContractAddress;
pub type Round = u32;
pub type ProposalCommitment = BlockHash;

// TODO(guyn): move this to another file.
/// Configuration for the Context struct.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct ContextConfig {
    /// The buffer size for streaming outbound proposals.
    pub batcher_build_buffer: usize,
    /// The number of validators.
    pub num_validators: u64,
    /// The chain id of the Starknet chain.
    pub chain_id: ChainId,
    /// Maximum allowed deviation (seconds) of a proposed block's timestamp from the current time.
    pub block_timestamp_window: u64,
    /// The data availability mode, true: Blob, false: Calldata.
    pub l1_da_mode: bool,
}

impl SerializeConfig for ContextConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "batcher_build_buffer",
                &self.batcher_build_buffer,
                "The buffer size for the batcher when building proposals.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "num_validators",
                &self.num_validators,
                "The number of validators.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "chain_id",
                &self.chain_id,
                "The chain id of the Starknet chain.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "block_timestamp_window",
                &self.block_timestamp_window,
                "Maximum allowed deviation (seconds) of a proposed block's timestamp from the \
                 current time.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "l1_da_mode",
                &self.l1_da_mode,
                "The data availability mode, true: Blob, false: Calldata.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            batcher_build_buffer: 100,
            num_validators: 1,
            chain_id: ChainId::Mainnet,
            block_timestamp_window: 1,
            l1_da_mode: true,
        }
    }
}

/// Interface for consensus to call out to the node.
///
/// Function calls should be assumed to not be cancel safe.
#[async_trait]
pub trait ConsensusContext {
    /// The parts of the proposal that are streamed in.
    /// Must contain at least the ProposalInit and ProposalFin.
    type ProposalPart: TryFrom<Vec<u8>, Error = ProtobufConversionError>
        + Into<Vec<u8>>
        + TryInto<ProposalInit, Error = ProtobufConversionError>
        + From<ProposalInit>
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
    ) -> oneshot::Receiver<ProposalCommitment>;

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
        content: mpsc::Receiver<Self::ProposalPart>,
    ) -> oneshot::Receiver<(ProposalCommitment, ProposalFin)>;

    /// This function is called by consensus to retrieve the content of a previously built or
    /// validated proposal. It broadcasts the proposal to the network.
    ///
    /// Params:
    /// - `id`: The `ProposalCommitment` associated with the block's content.
    /// - `init`: The `ProposalInit` that is broadcast to the network.
    async fn repropose(&mut self, id: ProposalCommitment, init: ProposalInit);

    /// Get the set of validators for a given height. These are the nodes that can propose and vote
    /// on blocks.
    // TODO(matan): We expect this to change in the future to BTreeMap. Why?
    // 1. Map - The nodes will have associated information (e.g. voting weight).
    // 2. BTreeMap - We want a stable ordering of the nodes for deterministic leader selection.
    async fn validators(&self, height: BlockNumber) -> Vec<ValidatorId>;

    /// Calculates the ID of the Proposer based on the inputs.
    fn proposer(&self, height: BlockNumber, round: Round) -> ValidatorId;

    async fn broadcast(&mut self, message: Vote) -> Result<(), ConsensusError>;

    /// Update the context that a decision has been reached for a given height.
    /// - `block` identifies the decision.
    /// - `precommits` - All precommits must be for the same `(block, height, round)` and form a
    ///   quorum (>2/3 of the voting power) for this height.
    async fn decision_reached(
        &mut self,
        block: ProposalCommitment,
        precommits: Vec<Vote>,
    ) -> Result<(), ConsensusError>;

    /// Attempt to learn of a decision from the sync protocol.
    /// Returns true if a decision was learned so consensus can proceed.
    async fn try_sync(&mut self, height: BlockNumber) -> bool;

    /// Update the context with the current height and round.
    /// Must be called at the beginning of each height.
    async fn set_height_and_round(&mut self, height: BlockNumber, round: Round);
}

#[derive(PartialEq)]
pub struct Decision {
    pub precommits: Vec<Vote>,
    pub block: ProposalCommitment,
}

impl Debug for Decision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decision")
            .field("block", &self.block)
            .field("precommits", &self.precommits)
            .finish()
    }
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
    Canceled(#[from] oneshot::Canceled),
    #[error(transparent)]
    ProtobufConversionError(#[from] ProtobufConversionError),
    #[error(transparent)]
    SendError(#[from] mpsc::SendError),
    // Indicates an error in communication between consensus and the node's networking component.
    // As opposed to an error between this node and peer nodes.
    #[error("{0}")]
    InternalNetworkError(String),
    #[error("{0}")]
    SyncError(String),
    // For example the state machine and SHC are out of sync.
    #[error("{0}")]
    InternalInconsistency(String),
    #[error("{0}")]
    Other(String),
}
