use core::net::Ipv4Addr;
use std::time::Duration;

use futures::channel::mpsc::{Receiver, SendError, Sender};
use futures::channel::oneshot;
use futures::future::{ready, Ready};
use futures::sink::With;
use futures::stream::Map;
use futures::{SinkExt, StreamExt};
use libp2p::gossipsub::SubscriptionError;
use libp2p::identity::Keypair;
use libp2p::PeerId;

use super::{
    BroadcastReceivedMessagesConverterFn,
    BroadcastTopicChannels,
    BroadcastTopicClient,
    BroadcastedMessageMetadata,
    GenericReceiver,
    NetworkError,
    NetworkManager,
    ReportReceiver,
    ServerQueryManager,
    ServerResponsesSender,
    SqmrClientPayload,
    SqmrClientSender,
    SqmrServerReceiver,
    Topic,
};
use crate::utils::make_multiaddr;
use crate::{Bytes, NetworkConfig};

pub fn mock_register_sqmr_protocol_client<Query, Response>(
    buffer_size: usize,
    // TODO(eitan): wrap second type with a struct to make it more readable
) -> (SqmrClientSender<Query, Response>, GenericReceiver<MockClientResponsesManager<Query, Response>>)
where
    Query: Send + 'static + TryFrom<Bytes>,
    Response: TryFrom<Bytes> + Send + 'static,
    <Response as TryFrom<Bytes>>::Error: Send + 'static,
    Bytes: From<Query> + From<Response>,
{
    let (sender, receiver) = futures::channel::mpsc::channel(buffer_size);
    let sender = Box::new(sender);
    let receiver = receiver.map(|payload: SqmrClientPayload| {
        MockClientResponsesManager::<Query, Response>::from(payload)
    });
    let receiver = Box::new(receiver);
    (SqmrClientSender::new(sender, buffer_size), receiver)
}

pub fn mock_register_sqmr_protocol_server<Query, Response>(
    buffer_size: usize,
    // TODO(eitan): wrap second type with a struct and make the function
    // create_test_server_query_manager a method of that struct to remove the need to make the
    // channel
) -> (SqmrServerReceiver<Query, Response>, Sender<ServerQueryManager<Query, Response>>)
where
    Query: TryFrom<Bytes> + Send + 'static,
    Response: Send + 'static,
    <Query as TryFrom<Bytes>>::Error: Send + 'static,
{
    let (sender, receiver) = futures::channel::mpsc::channel(buffer_size);
    let receiver = Box::new(receiver);
    (SqmrServerReceiver { receiver }, sender)
}

pub fn create_test_server_query_manager<Query, Response>(
    query: Query,
    // TODO(Shahak): wrap the second and third types with a struct to make them more readable
) -> (ServerQueryManager<Query, Response>, ReportReceiver, GenericReceiver<Response>)
where
    Query: TryFrom<Bytes>,
    Response: Send + 'static,
{
    let (report_sender, report_receiver) = oneshot::channel::<()>();
    let (responses_sender, responses_receiver) = futures::channel::mpsc::channel::<Response>(1);
    let responses_sender = ServerResponsesSender { sender: Box::new(responses_sender) };
    let responses_receiver = Box::new(responses_receiver);
    (
        ServerQueryManager { query: Ok(query), report_sender, responses_sender },
        report_receiver,
        responses_receiver,
    )
}

const CHANNEL_BUFFER_SIZE: usize = 1000;

/// Mock register subscriber for a given topic. BroadcastNetworkMock is used to send and catch
/// messages broadcasted by and to the subscriber respectively.
pub fn mock_register_broadcast_topic<T>() -> Result<TestSubscriberChannels<T>, SubscriptionError>
where
    T: TryFrom<Bytes> + 'static,
    Bytes: From<T>,
{
    let (messages_to_broadcast_sender, mock_messages_to_broadcast_receiver) =
        futures::channel::mpsc::channel(CHANNEL_BUFFER_SIZE);
    let (mock_broadcasted_messages_sender, broadcasted_messages_receiver) =
        futures::channel::mpsc::channel(CHANNEL_BUFFER_SIZE);

    let messages_to_broadcast_fn: fn(T) -> Ready<Result<Bytes, SendError>> =
        |x| ready(Ok(Bytes::from(x)));
    let messages_to_broadcast_sender = messages_to_broadcast_sender.with(messages_to_broadcast_fn);

    let broadcasted_messages_fn: BroadcastReceivedMessagesConverterFn<T> =
        |(x, report_sender)| (T::try_from(x), report_sender);
    let broadcasted_messages_receiver = broadcasted_messages_receiver.map(broadcasted_messages_fn);

    let (reported_messages_sender, mock_reported_messages_receiver) =
        futures::channel::mpsc::channel(CHANNEL_BUFFER_SIZE);
    let reported_messages_fn: fn(BroadcastedMessageMetadata) -> Ready<Result<PeerId, SendError>> =
        |broadcasted_message_metadata| {
            ready(Ok(broadcasted_message_metadata.originator_id.private_get_peer_id()))
        };
    let reported_messages_sender = reported_messages_sender.with(reported_messages_fn);

    let (continue_propagation_sender, mock_continue_propagation_receiver) =
        futures::channel::mpsc::channel(CHANNEL_BUFFER_SIZE);

    let subscriber_channels = BroadcastTopicChannels {
        broadcasted_messages_receiver,
        broadcast_topic_client: BroadcastTopicClient::new(
            messages_to_broadcast_sender,
            reported_messages_sender,
            continue_propagation_sender,
        ),
    };

    let mock_broadcasted_messages_fn: MockBroadcastedMessagesFn<T> =
        |(x, report_call_back)| ready(Ok((Bytes::from(x), report_call_back)));
    let mock_broadcasted_messages_sender =
        mock_broadcasted_messages_sender.with(mock_broadcasted_messages_fn);

    let mock_messages_to_broadcast_fn: fn(Bytes) -> T = |x| match T::try_from(x) {
        Ok(result) => result,
        Err(_) => {
            panic!("Failed to convert Bytes that we received from conversion to bytes");
        }
    };
    let mock_messages_to_broadcast_receiver =
        mock_messages_to_broadcast_receiver.map(mock_messages_to_broadcast_fn);

    let mock_network = BroadcastNetworkMock {
        broadcasted_messages_sender: mock_broadcasted_messages_sender,
        messages_to_broadcast_receiver: mock_messages_to_broadcast_receiver,
        reported_messages_receiver: mock_reported_messages_receiver,
        continue_propagation_receiver: mock_continue_propagation_receiver,
    };

    Ok(TestSubscriberChannels { subscriber_channels, mock_network })
}

fn create_peer_private_key(peer_index: usize) -> [u8; 32] {
    let peer_index: u64 = peer_index.try_into().expect("Failed converting usize to u64");
    let array = peer_index.to_le_bytes();
    assert_eq!(array.len(), 8);

    let mut private_key = [0u8; 32];
    for i in 0..private_key.len() {
        private_key[i] = array[i % array.len()]
    }
    private_key
}

pub fn create_connected_network_configs(ports: Vec<u16>) -> Vec<NetworkConfig> {
    let number_of_nodes = ports.len();
    let private_keys: Vec<_> = (0..number_of_nodes).map(create_peer_private_key).collect();
    let public_keys: Vec<_> = private_keys
        .iter()
        .cloned()
        .map(Keypair::ed25519_from_bytes)
        .map(|key_pair_result| key_pair_result.unwrap().public())
        .collect();
    let nodes_addresses: Vec<_> = public_keys
        .iter()
        .zip(ports.iter())
        .map(|(public_key, port)| {
            make_multiaddr(Ipv4Addr::LOCALHOST, *port, Some(PeerId::from_public_key(public_key)))
        })
        .collect();
    ports
        .into_iter()
        .zip(private_keys)
        .map(|(port, private_key)| NetworkConfig {
            port,
            bootstrap_peer_multiaddr: Some(nodes_addresses.clone()),
            secret_key: Some(private_key.to_vec()),
            ..Default::default()
        })
        .collect()
}

pub fn network_config_into_broadcast_channels<T>(
    network_config: NetworkConfig,
    topic: Topic,
) -> BroadcastTopicChannels<T>
where
    T: TryFrom<Bytes> + 'static,
    Bytes: From<T>,
{
    const BUFFER_SIZE: usize = 1000;

    let mut network_manager = NetworkManager::new(network_config, None, None);
    let broadcast_channels =
        network_manager.register_broadcast_topic(topic.clone(), BUFFER_SIZE, BUFFER_SIZE).unwrap();

    tokio::task::spawn(async move {
        let result = network_manager.run().await;
        match result {
            Ok(()) => panic!("Network manager terminated."),
            // The user of this function can drop the broadcast channels if they want to. In that
            // case we should just terminate NetworkManager's run quietly.
            Err(NetworkError::BroadcastChannelsDropped { topic_hash })
                if topic_hash == topic.into() => {}
            Err(err) => panic!("Network manager failed on {err:?}"),
        }
    });

    broadcast_channels
}

pub struct MockClientResponsesManager<Query: TryFrom<Bytes>, Response: TryFrom<Bytes>> {
    query: Result<Query, <Query as TryFrom<Bytes>>::Error>,
    report_receiver: ReportReceiver,
    responses_sender: ServerResponsesSender<Response>,
}

impl<Query: TryFrom<Bytes>, Response: TryFrom<Bytes>> MockClientResponsesManager<Query, Response> {
    pub fn query(&self) -> &Result<Query, <Query as TryFrom<Bytes>>::Error> {
        &self.query
    }

    pub async fn assert_reported(self, timeout: Duration) {
        tokio::time::timeout(timeout, self.report_receiver).await.unwrap().unwrap();
    }

    pub async fn send_response(&mut self, response: Response) -> Result<(), SendError> {
        self.responses_sender.sender.send(response).await
    }
}

impl<Query, Response> From<SqmrClientPayload> for MockClientResponsesManager<Query, Response>
where
    Query: TryFrom<Bytes>,
    Response: TryFrom<Bytes> + Send + 'static,
    <Response as TryFrom<Bytes>>::Error: Send + 'static,
    Bytes: From<Response>,
{
    fn from(payload: SqmrClientPayload) -> Self {
        let SqmrClientPayload { query, report_receiver, responses_sender } = payload;
        let query = Query::try_from(query);
        let responses_sender =
            Box::new(responses_sender.with(|response: Response| ready(Ok(Bytes::from(response)))));
        Self {
            query,
            report_receiver,
            responses_sender: ServerResponsesSender { sender: responses_sender },
        }
    }
}

pub type MockBroadcastedMessagesSender<T> = With<
    Sender<(Bytes, BroadcastedMessageMetadata)>,
    (Bytes, BroadcastedMessageMetadata),
    (T, BroadcastedMessageMetadata),
    Ready<Result<(Bytes, BroadcastedMessageMetadata), SendError>>,
    MockBroadcastedMessagesFn<T>,
>;

pub(crate) type MockBroadcastedMessagesFn<T> =
    fn(
        (T, BroadcastedMessageMetadata),
    ) -> Ready<Result<(Bytes, BroadcastedMessageMetadata), SendError>>;

pub type MockMessagesToBroadcastReceiver<T> = Map<Receiver<Bytes>, fn(Bytes) -> T>;

/// Mock network for testing broadcast topics. It allows to send and catch messages broadcasted to
/// and by a subscriber (respectively). The naming convension is to mimick BroadcastTopicChannels
/// and replace sender and receiver.
pub struct BroadcastNetworkMock<T: TryFrom<Bytes>> {
    pub broadcasted_messages_sender: MockBroadcastedMessagesSender<T>,
    pub messages_to_broadcast_receiver: MockMessagesToBroadcastReceiver<T>,
    pub reported_messages_receiver: Receiver<PeerId>,
    pub continue_propagation_receiver: Receiver<BroadcastedMessageMetadata>,
}

pub struct TestSubscriberChannels<T: TryFrom<Bytes>> {
    pub subscriber_channels: BroadcastTopicChannels<T>,
    pub mock_network: BroadcastNetworkMock<T>,
}
