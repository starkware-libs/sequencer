use futures::channel::{mpsc, oneshot};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ContractAddress;
use starknet_api::transaction::Transaction;

use crate::converters::ProtobufConversionError;

#[derive(Debug, Default, Hash, Clone, Eq, PartialEq)]
pub struct Proposal {
    pub height: u64,
    pub round: u32,
    pub proposer: ContractAddress,
    pub transactions: Vec<Transaction>,
    pub block_hash: BlockHash,
}

#[derive(Debug, Default, Hash, Clone, Eq, PartialEq)]
pub enum VoteType {
    Prevote,
    #[default]
    Precommit,
}

#[derive(Debug, Default, Hash, Clone, Eq, PartialEq)]
pub struct Vote {
    pub vote_type: VoteType,
    pub height: u64,
    pub round: u32,
    pub block_hash: Option<BlockHash>,
    pub voter: ContractAddress,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum ConsensusMessage {
    Proposal(Proposal),
    Vote(Vote),
}

impl ConsensusMessage {
    pub fn height(&self) -> u64 {
        match self {
            ConsensusMessage::Proposal(proposal) => proposal.height,
            ConsensusMessage::Vote(vote) => vote.height,
        }
    }
}
#[derive(Debug, Default, Clone, Hash, Eq, PartialEq)]
pub struct StreamMessage<T: Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>> {
    pub message: T,
    pub stream_id: u64,
    pub message_id: u64,
    pub fin: bool,
}

impl<T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>> std::fmt::Display
    for StreamMessage<T>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message: Vec<u8> = self.message.clone().into();
        write!(
            f,
            "StreamMessage {{ message: {:?}, stream_id: {}, message_id: {}, fin: {} }}",
            message, self.stream_id, self.message_id, self.fin
        )
    }
}

/// This message must be sent first when proposing a new block.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposalInit {
    /// The height of the consensus (block number).
    pub height: u64,
    /// The current round of the consensus.
    pub round: u32,
    /// The last round that was valid.
    pub valid_round: u32, // TODO(guyn): should this be optional?
    /// Address of the one who proposed the block.
    pub proposer: ContractAddress,
}

/// There is one or more batches of transactions in a proposed block.
#[derive(Debug, Clone, PartialEq)]
pub struct TransactionBatch {
    /// The transactions in the batch.
    pub transactions: Vec<Transaction>,
}

/// The propsal is done when receiving this fin message, which contains the block hash.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposalFin {
    /// The block hash of the proposed block.
    pub block_hash: BlockHash,
}

/// A part of the proposal.
#[derive(Debug, Clone, PartialEq)]
pub enum ProposalPart {
    /// The initialization part of the proposal.
    Init(ProposalInit),
    /// A part of the proposal that contains one or more transactions.
    Transactions(TransactionBatch),
    /// The final part of the proposal, including the block hash.
    Fin(ProposalFin),
}

// TODO(Guy): Remove after implementing broadcast streams.
#[allow(missing_docs)]
pub struct ProposalWrapper(pub Proposal);

impl From<ProposalWrapper>
    for (
        (BlockNumber, u32, ContractAddress),
        mpsc::Receiver<Transaction>,
        oneshot::Receiver<BlockHash>,
    )
{
    fn from(val: ProposalWrapper) -> Self {
        let transactions: Vec<Transaction> = val.0.transactions.into_iter().collect();
        let proposal_init = (BlockNumber(val.0.height), val.0.round, val.0.proposer);
        let (mut content_sender, content_receiver) = mpsc::channel(transactions.len());
        for tx in transactions {
            content_sender.try_send(tx).expect("Send should succeed");
        }
        content_sender.close_channel();

        let (fin_sender, fin_receiver) = oneshot::channel();
        fin_sender.send(val.0.block_hash).expect("Send should succeed");

        (proposal_init, content_receiver, fin_receiver)
    }
}
