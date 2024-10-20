use futures::channel::{mpsc, oneshot};
// #[cfg(any(feature = "testing", test))]
// use papyrus_test_utils::{auto_impl_get_test_instance, get_number_of_variants,
// GetTestInstance}; #[cfg(any(feature = "testing", test))]
// use rand::Rng;
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

/// This message must be sent first when proposing a new block.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposalInit {
    /// The height of the consensus (block number).
    pub height: u64,
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
}

/// The propsal is done when receiving this fin message, which contains the block hash.
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

// #[cfg(any(feature = "testing", test))]
// auto_impl_get_test_instance! {
//     pub enum ConsensusMessage {
//         Proposal(Proposal) = 0,
//         Vote(Vote) = 1,
//     }
//     pub struct Proposal {
//         pub height: u64,
//         pub round: u32,
//         pub proposer: ContractAddress,
//         pub transactions: Vec<Transaction>,
//         pub block_hash: BlockHash,
//         pub valid_round: Option<u32>,
//     }
//     pub struct Vote {
//         pub vote_type: VoteType,
//         pub height: u64,
//         pub round: u32,
//         pub block_hash: Option<BlockHash>,
//         pub voter: ContractAddress,
//     }
//     pub enum VoteType {
//         Prevote = 0,
//         Precommit = 1,
//     }
//     pub struct ProposalInit {
//         pub height: u64,
//         pub round: u32,
//         pub valid_round: Option<u32>,
//         pub proposer: ContractAddress,
//     }
//     pub struct ProposalFin {
//         pub proposal_content_id: BlockHash,
//     }
//     pub struct TransactionBatch {
//         pub transactions: Vec<Transaction>,
//     }
//     pub enum ProposalPart {
//         Init(ProposalInit) = 0,
//         Fin(ProposalFin) = 1,
//         Transactions(TransactionBatch) = 2,
//     }

// }

// // The auto_impl_get_test_instance macro does not work for StreamMessage because it has
// // a generic type. TODO(guyn): try to make the macro work with generic types.
// #[cfg(any(feature = "testing", test))]
// impl GetTestInstance for StreamMessage<ConsensusMessage> {
//     fn get_test_instance(rng: &mut rand_chacha::ChaCha8Rng) -> Self {
//         let message = if rng.gen_bool(0.5) {
//
// StreamMessageBody::Content(ConsensusMessage::Proposal(Proposal::get_test_instance(rng)))
//         } else {
//             StreamMessageBody::Fin
//         };
//         Self { message, stream_id: rng.gen_range(0..100), message_id: rng.gen_range(0..1000) }
//     }
// }

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
