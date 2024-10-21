#[cfg(test)]
use {
    crate::consensus::{
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
    },
    papyrus_test_utils::{auto_impl_get_test_instance, get_number_of_variants, GetTestInstance},
    rand::Rng,
    starknet_api::block::BlockHash,
    starknet_api::core::ContractAddress,
    starknet_api::transaction::Transaction,
};

#[cfg(test)]
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

// The auto_impl_get_test_instance macro does not work for StreamMessage because it has
// a generic type. TODO(guyn): try to make the macro work with generic types.
#[cfg(test)]
impl GetTestInstance for StreamMessage<ConsensusMessage> {
    fn get_test_instance(rng: &mut rand_chacha::ChaCha8Rng) -> Self {
        let message = if rng.gen_bool(0.5) {
            StreamMessageBody::Content(ConsensusMessage::Proposal(Proposal::get_test_instance(rng)))
        } else {
            StreamMessageBody::Fin
        };
        Self { message, stream_id: 12, message_id: 47 }
    }
}
