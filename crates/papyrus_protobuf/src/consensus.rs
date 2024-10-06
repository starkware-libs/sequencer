use futures::channel::{mpsc, oneshot};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ContractAddress;
use starknet_api::executable_transaction::Transaction as ExecutableTransaction;
use starknet_api::transaction::Transaction;

use crate::converters::ProtobufConversionError;

#[derive(Debug, Default, Hash, Clone, Eq, PartialEq)]
pub struct Proposal {
    pub height: u64,
    pub round: u32,
    pub proposer: ContractAddress,
    pub transactions: Vec<Transaction>,
    pub block_hash: BlockHash,
    pub valid_round: Option<u32>,
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

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum StreamMessageBody<T> {
    Content(T),
    Fin,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct StreamMessage<T: Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>> {
    pub message: StreamMessageBody<T>,
    pub stream_id: u64,
    pub message_id: u64,
}

impl<T> std::fmt::Display for StreamMessage<T>
where
    T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO(guyn): add option to display when message is Fin and doesn't have content (PR #1048)
        if let StreamMessageBody::Content(message) = &self.message {
            let message: Vec<u8> = message.clone().into();
            write!(
                f,
                "StreamMessage {{ stream_id: {}, message_id: {}, message_length: {}}}",
                self.stream_id,
                self.message_id,
                message.len(),
            )
        } else {
            write!(
                f,
                "StreamMessage {{ stream_id: {}, message_id: {}, message is fin }}",
                self.stream_id, self.message_id,
            )
        }
    }
}

// TODO(Guy): Remove after implementing broadcast streams.
#[allow(missing_docs)]
pub struct ProposalWrapper(pub Proposal);

impl From<ProposalWrapper>
    for (
        (BlockNumber, u32, ContractAddress, Option<u32>),
        mpsc::Receiver<Transaction>,
        oneshot::Receiver<BlockHash>,
    )
{
    fn from(val: ProposalWrapper) -> Self {
        let transactions: Vec<Transaction> = val.0.transactions.into_iter().collect();
        let proposal_init =
            (BlockNumber(val.0.height), val.0.round, val.0.proposer, val.0.valid_round);
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

impl From<ProposalWrapper>
    for (
        (BlockNumber, u32, ContractAddress, Option<u32>),
        mpsc::Receiver<Vec<ExecutableTransaction>>,
        oneshot::Receiver<BlockHash>,
    )
{
    fn from(val: ProposalWrapper) -> Self {
        let proposal_init =
            (BlockNumber(val.0.height), val.0.round, val.0.proposer, val.0.valid_round);

        let (_, content_receiver) = mpsc::channel(0);
        // This should only be used for Milestone 1, and then removed once streaming is supported.
        println!("Cannot build ExecutableTransaction from Transaction.");

        let (fin_sender, fin_receiver) = oneshot::channel();
        fin_sender.send(val.0.block_hash).expect("Send should succeed");

        (proposal_init, content_receiver, fin_receiver)
    }
}
