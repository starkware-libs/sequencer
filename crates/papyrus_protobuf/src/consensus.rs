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
    pub chunk_id: u64,
    pub fin: bool,
}

impl<T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>> std::fmt::Display
    for StreamMessage<T>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message: Vec<u8> = self.message.clone().into();
        write!(
            f,
            "StreamMessage {{ message: {:?}, stream_id: {}, chunk_id: {}, fin: {} }}",
            message, self.stream_id, self.chunk_id, self.fin
        )
    }
}

pub struct StreamHandler<
    T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>,
> {
    pub sender: mpsc::Sender<StreamMessage<T>>,
    pub receiver: mpsc::Receiver<StreamMessage<T>>,

    // these dictionaries are keyed on the stream_id
    pub next_chunk_ids: BTreeMap<u64, u64>,

    // there is a separate message buffer for each stream,
    // each message buffer is keyed by the chunk_id of the first message in
    // a contiguous sequence of messages (saved in a Vec)
    pub message_buffers: BTreeMap<u64, BTreeMap<u64, Vec<StreamMessage<T>>>>,
}

impl<T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>>
    StreamHandler<T>
{
    pub fn new(sender: mpsc::Sender<StreamMessage<T>>, receiver: mpsc::Receiver<StreamMessage<T>>) -> Self {
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
        while let Ok(Some(value)) = self.receiver.recv(){
            value
        
    }

    pub async fn listen(&mut self) {
        loop {
            println!("Listening for messages");
            let message = self.receive().await;
            println!("Received message: {}", message.chunk_id);
            let stream_id = message.stream_id;
            let chunk_id = message.chunk_id;
            let next_chunk_id = self.next_chunk_ids.entry(stream_id).or_insert(0);

            // this means we can just send the message without buffering it
            if chunk_id == *next_chunk_id {
                let fin = message.fin;
                self.sender.try_send(message).expect("Send should succeed");
                *next_chunk_id += 1;
                if fin {
                    // remove the buffer if the stream is finished
                    self.message_buffers.remove(&stream_id);
                }
                // try to drain the buffer
                self.drain_buffer(stream_id);
            } else {
                // save the message in the buffer.
                self.store(message);
            }
        }
    }

    // go over each vector in the buffer, push to the end of it if the chunk_id is contiguous
    // if no vector has a contiguous chunk_id, start a new vector
    fn store(&mut self, message: StreamMessage<T>) {
        let stream_id = message.stream_id;
        let chunk_id = message.chunk_id;
        let buffer = self.message_buffers.entry(stream_id).or_insert(BTreeMap::new());
        let keys = buffer.keys().cloned().collect::<Vec<u64>>();
        for id in keys {
            // go over the keys in order from smallest to largest id
            let last_id = buffer[&id].last().unwrap().chunk_id;

            if last_id == chunk_id - 1 {
                // we can just add the message to the end of the vector
                buffer.get_mut(&id).unwrap().push(message);
                return;
            }

            if last_id > chunk_id {
                // no vector with last chunk_id will match, we can just start a new vector
                buffer.insert(chunk_id, vec![message]);
                return;
            }

            if chunk_id >= id || last_id < chunk_id - 1 {
                // this message should already be inside this vector!
                let old_message = buffer[&id].iter().filter(|m| m.chunk_id == chunk_id).next();
                if let Some(old_message) = old_message {
                    panic!(
                        "Two messages with the same chunk_id in buffer! Old message: {}, new \
                         message: {}",
                        old_message, message
                    );
                } else if let None = old_message {
                    panic!("Message with chunk_id {} should be in buffer but is not! ", chunk_id);
                }
            }
        }
    }

    // Tries to drain as many messages as possible from the buffer (in order)
    // DOES NOT guarantee that the buffer will be empty after calling this function
    fn drain_buffer(&mut self, stream_id: u64) {
        if let Some(buffer) = self.message_buffers.get_mut(&stream_id) {
            let chunk_id = self.next_chunk_ids.entry(stream_id).or_insert(0);

            // drain each vec of messages one by one, if they are in order
            // to drain a vector, we must match the first id (the key) with the chunk_id
            // this while loop will keep draining vectors one by one, until chunk_id doesn't match
            while let Some(messages) = buffer.remove(chunk_id) {
                for message in messages {
                    self.sender.try_send(message).expect("Send should succeed");
                    *chunk_id += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_random_message(stream_id: u64, chunk_id: u64, fin: bool) -> StreamMessage<ConsensusMessage> {
        StreamMessage {
            message: ConsensusMessage::Proposal(Proposal::default()),
            stream_id,
            chunk_id,
            fin,
        }
    }

    #[tokio::test]
    async fn test_stream_handler() {
        let (mut tx_input, rx_input) = futures::channel::mpsc::channel::<StreamMessage<ConsensusMessage>>(100);
        let (tx_output, mut rx_output) = futures::channel::mpsc::channel::<StreamMessage<ConsensusMessage>>(100);
        let mut h = StreamHandler::new(tx_output, rx_input);
        println!("Sending message");
        tx_input.try_send(make_random_message(0, 0, false)).expect("Send should succeed");

        println!("Spawning task");
        let _h = tokio::spawn(async move {
            h.listen().await;
        });

        // wait a bit for the task to start
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        let _message = rx_output.try_next().expect("Receive should succeed").expect("Receive should succeed");

        

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
