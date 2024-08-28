use prost::bytes::Bytes;
use starknet_api::block::BlockHash;
use starknet_api::core::ContractAddress;
use starknet_api::transaction::Transaction;

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

pub struct StreamMessage {
    pub message: Bytes,
    pub stream_id: u64,
    pub chunk_id: u64,
    pub done: bool, 
}