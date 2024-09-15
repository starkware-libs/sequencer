use papyrus_test_utils::{
    auto_impl_get_test_instance,
    get_number_of_variants,
    get_rng,
    GetTestInstance,
};
use rand::Rng;
use starknet_api::block::BlockHash;
use starknet_api::core::ContractAddress;
use starknet_api::transaction::Transaction;

use crate::consensus::{ConsensusMessage, Proposal, StreamMessage, Vote, VoteType};

auto_impl_get_test_instance! {
    pub enum ConsensusMessage {
        Proposal(Proposal) = 0,
        Vote(Vote) = 1,
    }
}

auto_impl_get_test_instance! {
    pub struct Proposal {
        pub height: u64,
        pub round: u32,
        pub proposer: ContractAddress,
        pub transactions: Vec<Transaction>,
        pub block_hash: BlockHash,
    }
}

auto_impl_get_test_instance! {
    pub struct Vote {
        pub vote_type: VoteType,
        pub height: u64,
        pub round: u32,
        pub block_hash: Option<BlockHash>,
        pub voter: ContractAddress,
    }
}

auto_impl_get_test_instance! {
    pub enum VoteType {
        Prevote = 0,
        Precommit = 1,
    }
}

impl GetTestInstance for StreamMessage<ConsensusMessage> {
    fn get_test_instance(rng: &mut rand_chacha::ChaCha8Rng) -> Self {
        Self {
            message: ConsensusMessage::Proposal(Proposal::default()),
            stream_id: rng.gen_range(0..100),
            chunk_id: rng.gen_range(0..1000),
            fin: rng.gen_bool(0.5),
        }
    }
}

#[test]
fn convert_stream_message_to_vec_u8_and_back() {
    let mut rng = get_rng();

    // test that we can convert a StreamMessage with a ConsensusMessage message to bytes and back
    let stream_message: StreamMessage<ConsensusMessage> =
        StreamMessage::get_test_instance(&mut rng);

    let bytes_data: Vec<u8> = stream_message.clone().into();
    let res_data = StreamMessage::try_from(bytes_data).unwrap();
    assert_eq!(stream_message, res_data);
}

#[test]
fn convert_consensus_message_to_vec_u8_and_back() {
    let mut rng = get_rng();

    // test that we can convert a ConsensusMessage  to bytes and back
    let message = ConsensusMessage::get_test_instance(&mut rng);

    let bytes_data: Vec<u8> = message.clone().into();
    let res_data = ConsensusMessage::try_from(bytes_data).unwrap();
    assert_eq!(message, res_data);
}

#[test]
fn convert_vote_to_vec_u8_and_back() {
    let mut rng = get_rng();

    let vote = Vote::get_test_instance(&mut rng);

    let bytes_data: Vec<u8> = vote.clone().into();
    let res_data = Vote::try_from(bytes_data).unwrap();
    assert_eq!(vote, res_data);
}

#[test]
fn convert_proposal_to_vec_u8_and_back() {
    let mut rng = get_rng();

    let proposal = Proposal::get_test_instance(&mut rng);

    let bytes_data: Vec<u8> = proposal.clone().into();
    let res_data = Proposal::try_from(bytes_data).unwrap();
    assert_eq!(proposal, res_data);
}
