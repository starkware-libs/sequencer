use std::fmt::Display;

use papyrus_test_utils::{auto_impl_get_test_instance, get_number_of_variants, GetTestInstance};
use rand::Rng;
use starknet_api::block::BlockHash;
use starknet_api::core::ContractAddress;
use starknet_api::transaction::Transaction;

use crate::consensus::{
    ConsensusMessage,
    Proposal,
    ProposalFin,
    ProposalInit,
    ProposalPart,
    StreamMessage,
    StreamMessageBody,
    TransactionBatch,
    Vote,
    VoteType,
};

auto_impl_get_test_instance! {
    pub enum ConsensusMessage {
        Proposal(Proposal) = 0,
        Vote(Vote) = 1,
    }
    pub struct Proposal {
        pub height: u64,
        pub round: u32,
        pub proposer: ContractAddress,
        pub transactions: Vec<Transaction>,
        pub block_hash: BlockHash,
        pub valid_round: Option<u32>,
    }
    pub struct Vote {
        pub vote_type: VoteType,
        pub height: u64,
        pub round: u32,
        pub block_hash: Option<BlockHash>,
        pub voter: ContractAddress,
    }
    pub enum VoteType {
        Prevote = 0,
        Precommit = 1,
    }
    pub struct ProposalInit {
        pub height: u64,
        pub round: u32,
        pub valid_round: Option<u32>,
        pub proposer: ContractAddress,
    }
    pub struct ProposalFin {
        pub proposal_content_id: BlockHash,
    }
    pub struct TransactionBatch {
        pub transactions: Vec<Transaction>,
    }
    pub enum ProposalPart {
        Init(ProposalInit) = 0,
        Fin(ProposalFin) = 1,
        Transactions(TransactionBatch) = 2,
    }

}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StreamId(pub u64);

impl Into<Vec<u8>> for StreamId {
    fn into(self) -> Vec<u8> {
        self.0.to_be_bytes().to_vec()
    }
}

impl From<Vec<u8>> for StreamId {
    fn from(bytes: Vec<u8>) -> Self {
        let mut array = [0; 8];
        array.copy_from_slice(&bytes);
        StreamId(u64::from_be_bytes(array))
    }
}

impl Display for StreamId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StreamId({})", self.0)
    }
}

// The auto_impl_get_test_instance macro does not work for StreamMessage because it has
// a generic type. TODO(guyn): try to make the macro work with generic types.
impl GetTestInstance for StreamMessage<ConsensusMessage, StreamId> {
    fn get_test_instance(rng: &mut rand_chacha::ChaCha8Rng) -> Self {
        let message = if rng.gen_bool(0.5) {
            StreamMessageBody::Content(ConsensusMessage::Proposal(Proposal::get_test_instance(rng)))
        } else {
            StreamMessageBody::Fin
        };
        Self { message, stream_id: StreamId(12), message_id: 47 }
    }
}
