use std::collections::BTreeMap;

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

pub struct StreamHandler<T: Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>> {
    pub sender: mpsc::Sender<StreamMessage<T>>,
    pub receiver: mpsc::Receiver<StreamMessage<T>>,

    // these dictionaries are keyed on the stream_id
    pub next_chunk_ids: BTreeMap<u64, u32>,

    // there is a separate message buffer for each stream,
    // each message buffer is keyed by the chunk_id of the first message in
    // a contiguous sequence of messages (saved in a Vec)
    pub message_buffers: BTreeMap<u64, BTreeMap<u32, Vec<StreamMessage<T>>>>,
}

// impl<T: Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>>
impl<T: Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>> StreamHandler<T> {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel(100);
        StreamHandler {
            sender,
            receiver,
            next_chunk_ids: BTreeMap::new(),
            message_buffers: BTreeMap::new(),
        }
    }

    pub async fn send(&mut self, message: StreamMessage<T>) {
        self.sender.try_send(message).expect("Send should succeed");
    }

    pub async fn receive(&mut self) -> StreamMessage<T> {
        self.receiver
            .try_next()
            .await
            .expect("Receive should succeed")
            .expect("Receive should succeed")
    }

    pub async fn listen(&mut self) {
        loop {
            let message = self.receive().await;
            let stream_id = message.stream_id;
            let chunk_id = message.chunk_id;
            let next_chunk_id = self.next_chunk_ids.entry(stream_id).or_insert(0);

            // this means we can just send the message without buffering it
            if chunk_id == *next_chunk_id {
                self.send(message).await;
                *next_chunk_id += 1;
                if message.fin {
                    // remove the buffer if the stream is finished
                    self.message_buffers.remove(&stream_id);
                }
            } else {
                // save the message in the buffer.
                self.store(message);
            }
        }
    }

    fn store(&mut self, message: StreamMessage<T>) {
        let stream_id = message.stream_id;
        let chunk_id = message.chunk_id;
        let mut buffer = self.message_buffers.entry(stream_id).or_insert(BTreeMap::new());
        for id in buffer.keys() {
            if id > chunk_id {
                buffer.insert(chunk_id, vec![message]);
                return;
            }
        }
        // self.message_buffers
        //     .entry(stream_id)
        //     .or_insert(BTreeMap::new())
        //     .entry(chunk_id)
        //     .or_insert(Vec::new())
        //     .push(message);
    }

    // Tries to drain as many messages as possible from the buffer (in order)
    // DOES NOT guarantee that the buffer will be empty after calling this function
    fn drain_buffer(&mut self, stream_id: u64) {
        if let Some(mut buffer) = self.message_buffers.get(&stream_id) {
            let mut chunk_id = self.next_chunk_ids.entry(stream_id).or_insert(0);

            // try to drain each vec of message one by one, if they are in order
            while let Some(messages) = buffer.get(chunk_id) {
                for message in messages {
                    self.send(message);
                    *chunk_id += 1;
                }
                buffer.remove(chunk_id);
            }
        }
    }
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
