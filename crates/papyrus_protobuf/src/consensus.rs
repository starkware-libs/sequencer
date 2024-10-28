use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ContractAddress;
use starknet_api::transaction::{Transaction, TransactionHash};

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
    Proposal(Proposal), // To be deprecated
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

/// This message must be sent first when proposing a new block.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct ProposalInit {
    /// The height of the consensus (block number).
    pub height: BlockNumber,
    /// The current round of the consensus.
    pub round: u32,
    /// The last round that was valid.
    pub valid_round: Option<u32>,
    /// Address of the one who proposed the block.
    pub proposer: ContractAddress,
}

/// There is one or more batches of transactions in a proposed block.
#[derive(Debug, Clone, PartialEq)]
pub struct TransactionBatch {
    /// The transactions in the batch.
    pub transactions: Vec<Transaction>,
    // TODO(guyn): remove this once we know how to get hashes as part of the compilation.
    /// The transaction's hashes.
    pub tx_hashes: Vec<TransactionHash>,
}

/// The proposal is done when receiving this fin message, which contains the block hash.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposalFin {
    /// The block hash of the proposed block.
    /// TODO(guyn): Consider changing the content ID
    pub proposal_content_id: BlockHash,
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

impl TryInto<ProposalInit> for ProposalPart {
    type Error = ProtobufConversionError;

    fn try_into(self: ProposalPart) -> Result<ProposalInit, Self::Error> {
        match self {
            ProposalPart::Init(init) => Ok(init),
            _ => Err(ProtobufConversionError::WrongEnumVariant {
                type_description: "ProposalPart",
                value_as_str: format!("{:?}", self),
                expected: "Init",
                got: "Transactions or Fin",
            }),
        }
    }
}

impl From<ProposalInit> for ProposalPart {
    fn from(value: ProposalInit) -> Self {
        ProposalPart::Init(value)
    }
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
