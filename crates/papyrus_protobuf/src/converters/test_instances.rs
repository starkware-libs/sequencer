use std::fmt::Display;

use papyrus_test_utils::{auto_impl_get_test_instance, get_number_of_variants, GetTestInstance};
use prost::DecodeError;
use rand::Rng;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ContractAddress;
use starknet_api::transaction::Transaction;

use super::ProtobufConversionError;
use crate::consensus::{
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
        pub height: BlockNumber,
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
pub struct TestStreamId(pub u64);

impl From<TestStreamId> for Vec<u8> {
    fn from(value: TestStreamId) -> Self {
        value.0.to_be_bytes().to_vec()
    }
}

impl TryFrom<Vec<u8>> for TestStreamId {
    type Error = ProtobufConversionError;
    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        if bytes.len() != 8 {
            return Err(ProtobufConversionError::DecodeError(DecodeError::new("Invalid length")));
        };
        let mut array = [0; 8];
        array.copy_from_slice(&bytes);
        Ok(TestStreamId(u64::from_be_bytes(array)))
    }
}

impl PartialOrd for TestStreamId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TestStreamId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl Display for TestStreamId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TestStreamId({})", self.0)
    }
}

// The auto_impl_get_test_instance macro does not work for StreamMessage because it has
// a generic type. TODO(guyn): try to make the macro work with generic types.
impl GetTestInstance for StreamMessage<ProposalPart, TestStreamId> {
    fn get_test_instance(rng: &mut rand_chacha::ChaCha8Rng) -> Self {
        let message = if rng.gen_bool(0.5) {
            StreamMessageBody::Content(ProposalPart::Transactions(TransactionBatch {
                transactions: vec![Transaction::get_test_instance(rng)],
            }))
        } else {
            StreamMessageBody::Fin
        };
        Self { message, stream_id: TestStreamId(12), message_id: 47 }
    }
}
