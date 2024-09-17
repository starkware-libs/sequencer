//! Stream handler, see StreamManager struct.
use std::collections::BTreeMap;

use futures::channel::mpsc;
use papyrus_protobuf::consensus::StreamMessage;
use papyrus_protobuf::converters::ProtobufConversionError;

#[cfg(test)]
#[path = "stream_handler_test.rs"]
mod stream_handler_test;

pub struct StreamHandlerConfig {
    pub timeout_seconds: Option<u64>,
    pub timeout_millis: Option<u64>,
    pub max_buffer_size: Option<u64>,
    pub max_num_streams: Option<u64>,
}

impl Default for StreamHandlerConfig {
    fn default() -> Self {
        StreamHandlerConfig {
            timeout_seconds: None,
            timeout_millis: None,
            max_buffer_size: None,
            max_num_streams: None,
        }
    }
}

pub struct StreamHandler<
    T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>,
> {
    pub config: StreamHandlerConfig,

    pub sender: mpsc::Sender<StreamMessage<T>>,
    pub receiver: mpsc::Receiver<StreamMessage<T>>,

    // these dictionaries are keyed on the stream_id
    pub next_chunk_ids: BTreeMap<u64, u64>,
    pub fin_chunk_id: BTreeMap<u64, u64>,
    pub max_chunk_id: BTreeMap<u64, u64>,
    pub num_buffered: BTreeMap<u64, u64>,

    // there is a separate message buffer for each stream,
    // each message buffer is keyed by the chunk_id of the first message in
    // a contiguous sequence of messages (saved in a Vec)
    pub message_buffers: BTreeMap<u64, BTreeMap<u64, Vec<StreamMessage<T>>>>,
}

impl<T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>>
    StreamHandler<T>
{
    pub fn new(
        config: StreamHandlerConfig,
        sender: mpsc::Sender<StreamMessage<T>>,
        receiver: mpsc::Receiver<StreamMessage<T>>,
    ) -> Self {
        StreamHandler {
            config,
            sender,
            receiver,
            next_chunk_ids: BTreeMap::new(),
            fin_chunk_id: BTreeMap::new(),
            max_chunk_id: BTreeMap::new(),
            num_buffered: BTreeMap::new(),
            message_buffers: BTreeMap::new(),
        }
    }

    pub async fn listen(&mut self) {
        let t0 = std::time::Instant::now();
        loop {
            log::debug!("Listening for messages for {} milliseconds", t0.elapsed().as_millis());

            if let Some(timeout) = self.config.timeout_seconds {
                if t0.elapsed().as_secs() > timeout {
                    break;
                }
            }

            if let Some(timeout) = self.config.timeout_millis {
                if t0.elapsed().as_millis() > timeout.into() {
                    break;
                }
            }

            if let Ok(message) = self.receiver.try_next() {
                if let None = message {
                    // message is none in case the channel was closed!
                    break;
                }

                let message = message.unwrap(); // code above handles case where message is None

                log::debug!(
                    "Received: stream_id= {}, chunk_id= {}, fin= {}",
                    message.stream_id,
                    message.chunk_id,
                    message.fin
                );
                let stream_id = message.stream_id;
                let chunk_id = message.chunk_id;
                let next_chunk_id = self.next_chunk_ids.entry(stream_id).or_insert(0);

                self.num_buffered
                    .entry(stream_id)
                    .and_modify(|num_buffered| {
                        *num_buffered += 1;
                    })
                    .or_insert(1); // first message

                self.max_chunk_id
                    .entry(stream_id)
                    .and_modify(|max_chunk_id| {
                        if chunk_id > *max_chunk_id {
                            *max_chunk_id = chunk_id;
                        }
                    })
                    .or_insert(chunk_id);

                // check if this there are too many streams
                if let Some(max_streams) = self.config.max_num_streams {
                    let num_streams = self.num_buffered.len() as u64;
                    if num_streams > max_streams {
                        panic!("Max number of streams reached! {}", max_streams);
                    }
                }

                if message.fin {
                    // there is guaranteed to be a maximum chunk_id for this stream, as we have
                    // received at least one message
                    let max_chunk_id = self.max_chunk_id.get(&stream_id).unwrap();
                    if *max_chunk_id > chunk_id {
                        panic!(
                            "Received fin message with chunk_id {} that is smaller than the \
                             max_chunk_id {}",
                            chunk_id, max_chunk_id
                        );
                    }
                    self.fin_chunk_id.insert(stream_id, chunk_id);
                }

                // check that chunk_id is not bigger than the fin_chunk_id
                if let Some(fin_chunk_id) = self.fin_chunk_id.get(&stream_id) {
                    if chunk_id > *fin_chunk_id {
                        panic!(
                            "Received message with chunk_id {} that is bigger than the \
                             fin_chunk_id {}",
                            chunk_id, fin_chunk_id
                        );
                    }
                }

                // this means we can just send the message without buffering it
                if chunk_id == *next_chunk_id {
                    self.sender.try_send(message).expect("Send should succeed");
                    *next_chunk_id += 1;
                    // try to drain the buffer
                    self.drain_buffer(stream_id);
                } else if chunk_id > *next_chunk_id {
                    // save the message in the buffer.
                    self.store(message);
                } else {
                    panic!(
                        "Received message with chunk_id {} that is smaller than next_chunk_id {}",
                        chunk_id, next_chunk_id
                    );
                }
            } else {
                // Err comes when the channel is open but no message was received
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        } // end of loop
        log::debug!("Done listening for messages");
    }

    // go over each vector in the buffer, push to the end of it if the chunk_id is contiguous
    // if no vector has a contiguous chunk_id, start a new vector
    fn store(&mut self, message: StreamMessage<T>) {
        let stream_id = message.stream_id;
        let chunk_id = message.chunk_id;
        let num_buf = self.num_buffered.get(&stream_id).unwrap();
        if let Some(max_buf_size) = self.config.max_buffer_size {
            if *num_buf > max_buf_size {
                panic!("Buffer is full! stream_id= {} with {} messages!", stream_id, num_buf);
            }
        }
        let buffer = self.message_buffers.entry(stream_id).or_insert(BTreeMap::new());
        let keys = buffer.keys().cloned().collect::<Vec<u64>>();
        for id in keys {
            // go over the keys in order from smallest to largest id
            let last_id = buffer[&id].last().unwrap().chunk_id;

            // we can just add the message to the end of the vector
            if last_id == chunk_id - 1 {
                buffer.get_mut(&id).unwrap().push(message);
                return;
            }

            // no vector with last chunk_id will match, skip the rest of the loop
            if last_id > chunk_id {
                break;
            }

            // this message should already be inside this vector!
            if chunk_id >= id || last_id < chunk_id - 1 {
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
        buffer.insert(chunk_id, vec![message]);
    }

    // Tries to drain as many messages as possible from the buffer (in order)
    // DOES NOT guarantee that the buffer will be empty after calling this function
    fn drain_buffer(&mut self, stream_id: u64) {
        if let Some(buffer) = self.message_buffers.get_mut(&stream_id) {
            let chunk_id = self.next_chunk_ids.entry(stream_id).or_insert(0);
            let num_buf = self.num_buffered.get_mut(&stream_id).unwrap();

            // drain each vec of messages one by one, if they are in order
            // to drain a vector, we must match the first id (the key) with the chunk_id
            // this while loop will keep draining vectors one by one, until chunk_id doesn't match
            while let Some(messages) = buffer.remove(chunk_id) {
                for message in messages {
                    self.sender.try_send(message).expect("Send should succeed");
                    *chunk_id += 1;
                    *num_buf -= 1;
                }
            }

            if let Some(fin_chunk_id) = self.fin_chunk_id.get(&stream_id) {
                log::debug!("buffer.is_empty()= {}, fin= {}", buffer.is_empty(), fin_chunk_id);
            } else {
                log::debug!("buffer.is_empty()= {}, fin= None", buffer.is_empty());
            }

            if buffer.is_empty() && self.fin_chunk_id.get(&stream_id).is_some() {
                self.message_buffers.remove(&stream_id);
                self.next_chunk_ids.remove(&stream_id);
                self.fin_chunk_id.remove(&stream_id);
                self.max_chunk_id.remove(&stream_id);
                self.num_buffered.remove(&stream_id);
            }
        }
    }
}
