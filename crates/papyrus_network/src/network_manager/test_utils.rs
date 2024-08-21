use futures::channel::mpsc::{Receiver, SendError, Sender};
use futures::channel::oneshot;
use futures::future::{ready, Ready};
use futures::sink::With;
use futures::stream::Map;
use futures::{SinkExt, Stream, StreamExt};
use libp2p::gossipsub::SubscriptionError;

use super::{
    GenericReceiver,
    ReportReceiver,
    ReportSender,
    ServerQueryManager,
    ServerResponsesSender,
    SqmrClientPayload,
    SqmrClientSender,
    SqmrServerReceiver,
};
use crate::network_manager::{BroadcastReceivedMessagesConverterFn, BroadcastSubscriberChannels};
use crate::sqmr::Bytes;

pub fn create_test_sqmr_client_channel<Query, Response>(
    buffer_size: usize,
) -> (SqmrClientSender, TestReceiver<SqmrClientPayloadForTest<Query, Response>>)
where
    Query: Send + 'static + TryFrom<Bytes>,
    Response: TryFrom<Bytes> + Send + 'static,
    <Response as TryFrom<Bytes>>::Error: Send + 'static,
    Bytes: From<Query> + From<Response>,
{
    let (sender, receiver) = futures::channel::mpsc::channel(buffer_size);
    let sender = Box::new(sender);
    let receiver = receiver.map(|payload: SqmrClientPayload| {
        SqmrClientPayloadForTest::<Query, Response>::from(payload)
    });
    let receiver = Box::new(receiver);
    (SqmrClientSender { sender, buffer_size }, receiver)
}

pub fn create_test_sqmr_server_channel<Query, Response>(
    buffer_size: usize,
) -> (Sender<ServerQueryManager<Query, Response>>, SqmrServerReceiver<Query, Response>)
where
    Query: TryFrom<Bytes> + Send + 'static,
    Response: Send + 'static,
    <Query as TryFrom<Bytes>>::Error: Send + 'static,
{
    let (sender, receiver) = futures::channel::mpsc::channel(buffer_size);
    let receiver = Box::new(receiver);
    (sender, SqmrServerReceiver { receiver })
}

pub fn create_test_server_query_manager<Query, Response>(
    query: Query,
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

pub fn mock_register_broadcast_subscriber<T>()
-> Result<TestSubscriberChannels<T>, SubscriptionError>
where
    T: TryFrom<Bytes>,
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

    let subscriber_channels =
        BroadcastSubscriberChannels { messages_to_broadcast_sender, broadcasted_messages_receiver };

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
    };

    Ok(TestSubscriberChannels { subscriber_channels, mock_network })
}

pub type TestReceiver<T> = Box<dyn Stream<Item = T> + Unpin + Send>;

pub struct SqmrClientPayloadForTest<Query: TryFrom<Bytes>, Response: TryFrom<Bytes>> {
    pub query: Result<Query, <Query as TryFrom<Bytes>>::Error>,
    pub report_receiver: ReportReceiver,
    responses_sender: ServerResponsesSender<Response>,
}

impl<Query: TryFrom<Bytes>, Response: TryFrom<Bytes>> SqmrClientPayloadForTest<Query, Response> {
    pub async fn send(&mut self, response: Response) -> Result<(), SendError> {
        self.responses_sender.sender.send(response).await
    }
}

impl<Query, Response> From<SqmrClientPayload> for SqmrClientPayloadForTest<Query, Response>
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
    Sender<(Bytes, ReportSender)>,
    (Bytes, ReportSender),
    (T, ReportSender),
    Ready<Result<(Bytes, ReportSender), SendError>>,
    MockBroadcastedMessagesFn<T>,
>;

pub(crate) type MockBroadcastedMessagesFn<T> =
    fn((T, ReportSender)) -> Ready<Result<(Bytes, ReportSender), SendError>>;

pub type MockMessagesToBroadcastReceiver<T> = Map<Receiver<Bytes>, fn(Bytes) -> T>;

pub struct BroadcastNetworkMock<T: TryFrom<Bytes>> {
    pub broadcasted_messages_sender: MockBroadcastedMessagesSender<T>,
    pub messages_to_broadcast_receiver: MockMessagesToBroadcastReceiver<T>,
}

pub struct TestSubscriberChannels<T: TryFrom<Bytes>> {
    pub subscriber_channels: BroadcastSubscriberChannels<T>,
    pub mock_network: BroadcastNetworkMock<T>,
}
