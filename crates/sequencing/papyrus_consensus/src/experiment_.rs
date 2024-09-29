use std::future::Ready;

use futures::channel::mpsc::{SendError, Sender};
use futures::sink::With;
use futures::{Sink, SinkExt, Stream, StreamExt};
use papyrus_network::network_manager::{
    BroadcastTopicChannels,
    BroadcastTopicSender,
    BroadcastedMessageManager,
    GenericReceiver,
};
use papyrus_protobuf::converters::ProtobufConversionError;

type ReceivedBroadcastedMessage<Message> =
    (Result<Message, <Message as TryFrom<Vec<u8>>>::Error>, BroadcastedMessageManager);

pub struct Experiment<T: From<Vec<u8>>> {
    // channels: BroadcastTopicChannels<ConsensusMessage>,
    messages_to_broadcast_sender: BroadcastTopicSender<T>,
    // messages_to_broadcast_sender: With<Sender<Vec<u8>>, Vec<u8>, T, Ready<Result<Vec<u8>,
    // SendError>>, fn(T) -> Ready<Result<Vec<u8>, SendError>>>,
    broadcasted_messages_receiver: GenericReceiver<ReceivedBroadcastedMessage<T>>,
    // broadcasted_messages_receiver: Box<dyn Stream<Item = (Result<(Result<T,
    // ProtobufConversionError>, BroadcastedMessageManager), ProtobufConversionError>,
    // BroadcastedMessageManager)> + Send + Unpin>,
    reported_messages_sender:
        Box<dyn Sink<BroadcastedMessageManager, Error = SendError> + Send + Unpin>,
    continue_propagation_sender:
        Box<dyn Sink<BroadcastedMessageManager, Error = SendError> + Send + Unpin>,
}

impl<T: From<Vec<u8>>> Experiment<T> {
    pub fn new(channels: BroadcastTopicChannels<T>) -> Self {
        let BroadcastTopicChannels {
            messages_to_broadcast_sender,
            broadcasted_messages_receiver,
            reported_messages_sender,
            continue_propagation_sender,
        } = channels;
        Self {
            messages_to_broadcast_sender,
            broadcasted_messages_receiver,
            reported_messages_sender,
            continue_propagation_sender,
        }
    }

    pub async fn run(&mut self) {
        println!("Experiment::run");
        println!("messages_to_broadcast_sender: {:?}", self.messages_to_broadcast_sender);
    }
}

#[cfg(test)]
mod tests {
    use papyrus_protobuf::consensus::ConsensusMessage;

    use super::*;

    #[tokio::test]
    async fn test_experiment() {
        let (messages_to_broadcast_sender, messages_to_broadcast_receiver) =
            futures::channel::mpsc::channel(100);
        let (broadcasted_messages_sender, broadcasted_messages_receiver) =
            futures::channel::mpsc::channel(100);
        let (reported_messages_sender, reported_messages_receiver) =
            futures::channel::mpsc::channel(100);
        let (continue_propagation_sender, continue_propagation_receiver) =
            futures::channel::mpsc::channel(100);

        let channels = BroadcastTopicChannels {
            messages_to_broadcast_sender,
            broadcasted_messages_receiver,
            reported_messages_sender,
            continue_propagation_sender,
        };

        let mut experiment = Experiment::new(channels);
        experiment.run().await;
    }
}
