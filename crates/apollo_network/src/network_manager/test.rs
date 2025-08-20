use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use std::vec;

use apollo_network_types::test_utils::DUMMY_PEER_ID;
use deadqueue::unlimited::Queue;
use futures::channel::mpsc::{unbounded, UnboundedSender};
use futures::channel::oneshot;
use futures::future::FutureExt;
use futures::stream::Stream;
use futures::{pin_mut, Future, SinkExt, StreamExt};
use lazy_static::lazy_static;
use libp2p::core::transport::PortUse;
use libp2p::core::ConnectedPoint;
use libp2p::gossipsub::{SubscriptionError, TopicHash};
use libp2p::swarm::ConnectionId;
use libp2p::{Multiaddr, PeerId, StreamProtocol};
use tokio::select;
use tokio::sync::Mutex;
use tokio::time::sleep;

use super::swarm_trait::{Event, SwarmTrait};
use super::{BroadcastTopicChannels, GenericNetworkManager};
use crate::gossipsub_impl::{self, Topic};
use crate::misconduct_score::MisconductScore;
use crate::network_manager::{BroadcastTopicClientTrait, ServerQueryManager};
use crate::sqmr::behaviour::SessionIdNotFoundError;
use crate::sqmr::{GenericEvent, InboundSessionId, OutboundSessionId};
use crate::{mixed_behaviour, Bytes};

const TIMEOUT: Duration = Duration::from_secs(1);

lazy_static! {
    static ref VEC1: Vec<u8> = vec![1, 2, 3, 4, 5];
    static ref VEC2: Vec<u8> = vec![6, 7, 8];
    static ref VEC3: Vec<u8> = vec![9, 10];
}

#[derive(Default)]
struct MockSwarm {
    pub pending_events: Queue<Event>,
    pub subscribed_topics: HashSet<TopicHash>,
    broadcasted_messages_senders: Vec<UnboundedSender<(Bytes, TopicHash)>>,
    reported_peer_senders: Vec<UnboundedSender<PeerId>>,
    supported_inbound_protocols_senders: Vec<UnboundedSender<StreamProtocol>>,
    inbound_session_id_to_response_sender: HashMap<InboundSessionId, UnboundedSender<Bytes>>,
    next_outbound_session_id: usize,
    first_polled_event_notifier: Option<oneshot::Sender<()>>,
}

impl Stream for MockSwarm {
    type Item = Event;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut_self = self.get_mut();
        let mut fut = mut_self.pending_events.pop().map(Some).boxed();
        if let Some(sender) = mut_self.first_polled_event_notifier.take() {
            fut = fut
                .then(|res| async {
                    sender.send(()).unwrap();
                    res
                })
                .boxed();
        };
        pin_mut!(fut);
        fut.poll_unpin(cx)
    }
}

impl MockSwarm {
    pub fn get_responses_sent_to_inbound_session(
        &mut self,
        inbound_session_id: InboundSessionId,
    ) -> impl Future<Output = Vec<Bytes>> {
        let (responses_sender, responses_receiver) = unbounded();
        if self
            .inbound_session_id_to_response_sender
            .insert(inbound_session_id, responses_sender)
            .is_some()
        {
            panic!("Called get_responses_sent_to_inbound_session on {inbound_session_id:?} twice");
        }
        responses_receiver.collect()
    }

    pub fn stream_messages_we_broadcasted(&mut self) -> impl Stream<Item = (Bytes, TopicHash)> {
        let (sender, receiver) = unbounded();
        self.broadcasted_messages_senders.push(sender);
        receiver
    }

    pub fn get_reported_peers_stream(&mut self) -> impl Stream<Item = PeerId> {
        let (sender, receiver) = unbounded();
        self.reported_peer_senders.push(sender);
        receiver
    }

    pub fn get_supported_inbound_protocol(&mut self) -> impl Stream<Item = StreamProtocol> {
        let (sender, receiver) = unbounded();
        self.supported_inbound_protocols_senders.push(sender);
        receiver
    }

    fn create_response_events_for_query_each_num_becomes_response(
        &self,
        query: Vec<u8>,
        outbound_session_id: OutboundSessionId,
        peer_id: PeerId,
    ) {
        for response in query {
            self.pending_events.push(Event::Behaviour(mixed_behaviour::Event::ExternalEvent(
                mixed_behaviour::ExternalEvent::Sqmr(GenericEvent::ReceivedResponse {
                    response: vec![response],
                    outbound_session_id,
                    peer_id,
                }),
            )));
        }
    }
}

impl SwarmTrait for MockSwarm {
    fn send_response(
        &mut self,
        response: Vec<u8>,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        let responses_sender =
            self.inbound_session_id_to_response_sender.get(&inbound_session_id).expect(
                "Called send_response without calling get_responses_sent_to_inbound_session first",
            );
        responses_sender.unbounded_send(response).unwrap();
        Ok(())
    }

    fn send_query(&mut self, query: Vec<u8>, _protocol: StreamProtocol) -> OutboundSessionId {
        let outbound_session_id = OutboundSessionId { value: self.next_outbound_session_id };
        self.create_response_events_for_query_each_num_becomes_response(
            query,
            outbound_session_id,
            *DUMMY_PEER_ID,
        );
        self.next_outbound_session_id += 1;
        outbound_session_id
    }

    fn dial(&mut self, _peer: Multiaddr) -> Result<(), libp2p::swarm::DialError> {
        Ok(())
    }

    fn close_inbound_session(
        &mut self,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        let responses_sender =
            self.inbound_session_id_to_response_sender.get(&inbound_session_id).expect(
                "Called close_inbound_session without calling \
                 get_responses_sent_to_inbound_session first",
            );
        responses_sender.close_channel();
        Ok(())
    }

    fn behaviour_mut(&mut self) -> &mut mixed_behaviour::MixedBehaviour {
        unimplemented!()
    }

    fn add_external_address(&mut self, _address: Multiaddr) {}

    fn subscribe_to_topic(&mut self, topic: &Topic) -> Result<(), SubscriptionError> {
        self.subscribed_topics.insert(topic.hash());
        Ok(())
    }

    fn broadcast_message(&mut self, message: Bytes, topic_hash: TopicHash) {
        for sender in &self.broadcasted_messages_senders {
            sender.unbounded_send((message.clone(), topic_hash.clone())).unwrap();
        }
    }

    fn report_peer_as_malicious(&mut self, peer_id: PeerId, _: MisconductScore) {
        for sender in &self.reported_peer_senders {
            sender.unbounded_send(peer_id).unwrap();
        }
    }
    fn add_new_supported_inbound_protocol(&mut self, protocol_name: StreamProtocol) {
        for sender in &self.supported_inbound_protocols_senders {
            sender.unbounded_send(protocol_name.clone()).unwrap();
        }
    }

    fn get_peer_id_from_session_id(
        &self,
        _session_id: crate::sqmr::SessionId,
    ) -> Result<PeerId, SessionIdNotFoundError> {
        Ok(*DUMMY_PEER_ID)
    }

    // TODO(shahak): Add test for continue propagation.
    fn continue_propagation(&mut self, _message_metadata: super::BroadcastedMessageMetadata) {
        unimplemented!()
    }

    fn update_metrics(&self, _: &super::metrics::NetworkMetrics) {}
}

const BUFFER_SIZE: usize = 100;
const SIGNED_BLOCK_HEADER_PROTOCOL: StreamProtocol = StreamProtocol::new("/starknet/headers/1");
const MESSAGE_METADATA_BUFFER_SIZE: usize = 100000;

#[tokio::test]
async fn register_sqmr_protocol_client_and_use_channels() {
    // mock swarm to send and track connection established event
    let mut mock_swarm = MockSwarm::default();
    let peer_id = *DUMMY_PEER_ID;
    mock_swarm.pending_events.push(get_test_connection_established_event(peer_id));
    let (event_notifier, first_event_listner) = oneshot::channel();
    mock_swarm.first_polled_event_notifier = Some(event_notifier);

    // network manager to register subscriber
    let mut network_manager = GenericNetworkManager::generic_new(
        mock_swarm,
        None,
        None,
        MESSAGE_METADATA_BUFFER_SIZE,
        MESSAGE_METADATA_BUFFER_SIZE,
    );

    // register subscriber and send payload
    let mut payload_sender = network_manager.register_sqmr_protocol_client::<Vec<u8>, Vec<u8>>(
        SIGNED_BLOCK_HEADER_PROTOCOL.to_string(),
        BUFFER_SIZE,
    );

    let response_receiver_length = Arc::new(Mutex::new(0));
    let cloned_response_receiver_length = Arc::clone(&response_receiver_length);

    tokio::select! {
        _ = network_manager.run() => panic!("network manager ended"),
        _ = first_event_listner.then(|_| async move {
            let client_response_manager = payload_sender.send_new_query(VEC1.clone()).await.unwrap();
            let response_receiver_collector = client_response_manager.responses_receiver
            .enumerate()
            .take(VEC1.len())
            .map(|(i, result)| {
                let result: Vec<_> = result.unwrap();
                // this simulates how the mock swarm parses the query and sends responses to it
                assert_eq!(result, vec![VEC1[i]]);
                result
            })
            .collect::<Vec<_>>();
            response_receiver_collector.await.len()
        })
            .then(|response_receiver_length| async move {
                *cloned_response_receiver_length.lock().await = response_receiver_length;
            }) => {},
        _ = sleep(Duration::from_secs(5)) => {
            panic!("Test timed out");
        }
    }
    assert_eq!(*response_receiver_length.lock().await, VEC1.len());
}

// TODO(shahak): Add multiple protocols and multiple queries in the test.
#[tokio::test]
async fn process_incoming_query() {
    // Create responses for test.
    let query = VEC1.clone();
    let responses = vec![VEC1.clone(), VEC2.clone(), VEC3.clone()];
    let protocol: StreamProtocol = SIGNED_BLOCK_HEADER_PROTOCOL;

    // Setup mock swarm and tell it to return an event of new inbound query.
    let mut mock_swarm = MockSwarm::default();
    let inbound_session_id = InboundSessionId { value: 0 };
    mock_swarm.pending_events.push(Event::Behaviour(mixed_behaviour::Event::ExternalEvent(
        mixed_behaviour::ExternalEvent::Sqmr(GenericEvent::NewInboundSession {
            query: query.clone(),
            inbound_session_id,
            peer_id: *DUMMY_PEER_ID,
            protocol_name: protocol.clone(),
        }),
    )));

    // Create a future that will return when the session is closed with the responses sent on the
    // swarm.
    let get_responses_fut = mock_swarm.get_responses_sent_to_inbound_session(inbound_session_id);
    let mut get_supported_inbound_protocol_fut = mock_swarm.get_supported_inbound_protocol();

    let mut network_manager = GenericNetworkManager::generic_new(
        mock_swarm,
        None,
        None,
        MESSAGE_METADATA_BUFFER_SIZE,
        MESSAGE_METADATA_BUFFER_SIZE,
    );

    let mut inbound_payload_receiver = network_manager
        .register_sqmr_protocol_server::<Vec<u8>, Vec<u8>>(protocol.to_string(), BUFFER_SIZE);

    let actual_protocol = get_supported_inbound_protocol_fut.next().await.unwrap();
    assert_eq!(protocol, actual_protocol);

    let responses_clone = responses.clone();
    select! {
        _ = async move {
            let ServerQueryManager{query: query_got, report_sender: _report_sender, mut responses_sender} = inbound_payload_receiver.next().await.unwrap();
            assert_eq!(query_got.unwrap(), query);
            for response in responses_clone {
                responses_sender.feed(response).await.unwrap();
            }
            responses_sender.sender.close().await.unwrap();
            assert_eq!(get_responses_fut.await, responses);
        } => {}
        _ = network_manager.run() => {
            panic!("GenericNetworkManager::run finished before the session finished");
        }
        _ = sleep(Duration::from_secs(5)) => {
            panic!("Test timed out");
        }
    }
}

#[tokio::test]
async fn broadcast_message() {
    let topic = Topic::new("TOPIC");
    let message = vec![1u8, 2u8, 3u8];

    let mut mock_swarm = MockSwarm::default();
    let mut messages_we_broadcasted_stream = mock_swarm.stream_messages_we_broadcasted();

    let mut network_manager = GenericNetworkManager::generic_new(
        mock_swarm,
        None,
        None,
        MESSAGE_METADATA_BUFFER_SIZE,
        MESSAGE_METADATA_BUFFER_SIZE,
    );

    let mut broadcast_topic_client = network_manager
        .register_broadcast_topic(topic.clone(), BUFFER_SIZE, BUFFER_SIZE)
        .unwrap()
        .broadcast_topic_client;
    broadcast_topic_client.broadcast_message(message.clone()).await.unwrap();

    tokio::select! {
        _ = network_manager.run() => panic!("network manager ended"),
        result = tokio::time::timeout(
            TIMEOUT, messages_we_broadcasted_stream.next()
        ) => {
            let (actual_message, topic_hash) = result.unwrap().unwrap();
            assert_eq!(message, actual_message);
            assert_eq!(topic.hash(), topic_hash);
        }
    }
}

#[tokio::test]
async fn receive_broadcasted_message_and_report_it() {
    let topic = Topic::new("TOPIC");
    let message = vec![1u8, 2u8, 3u8];
    let originated_peer_id = *DUMMY_PEER_ID;

    let mut mock_swarm = MockSwarm::default();
    mock_swarm.pending_events.push(Event::Behaviour(mixed_behaviour::Event::ExternalEvent(
        mixed_behaviour::ExternalEvent::GossipSub(gossipsub_impl::ExternalEvent::Received {
            originated_peer_id,
            message: message.clone(),
            topic_hash: topic.hash(),
        }),
    )));
    let mut reported_peer_receiver = mock_swarm.get_reported_peers_stream();

    let mut network_manager = GenericNetworkManager::generic_new(
        mock_swarm,
        None,
        None,
        MESSAGE_METADATA_BUFFER_SIZE,
        MESSAGE_METADATA_BUFFER_SIZE,
    );

    let BroadcastTopicChannels {
        mut broadcast_topic_client,
        mut broadcasted_messages_receiver,
        ..
    } = network_manager.register_broadcast_topic::<Bytes>(topic.clone(), BUFFER_SIZE, BUFFER_SIZE).unwrap();

    tokio::select! {
        _ = network_manager.run() => panic!("network manager ended"),
        // We need to do the entire calculation in the future here so that the network will keep
        // running while we call report_callback.
        reported_peer_result = tokio::time::timeout(TIMEOUT, async {
            let result = broadcasted_messages_receiver.next().await;
            let (message_result, broadcasted_message_metadata) = result.unwrap();
            assert_eq!(message, message_result.unwrap());
            broadcast_topic_client.report_peer(broadcasted_message_metadata).await.unwrap();
            reported_peer_receiver.next().await
        }) => {
            assert_eq!(originated_peer_id, reported_peer_result.unwrap().unwrap());
        }
    }
}

fn get_test_connection_established_event(mock_peer_id: PeerId) -> Event {
    Event::ConnectionEstablished {
        peer_id: mock_peer_id,
        connection_id: ConnectionId::new_unchecked(0),
        endpoint: ConnectedPoint::Dialer {
            address: Multiaddr::empty(),
            role_override: libp2p::core::Endpoint::Dialer,
            port_use: PortUse::Reuse,
        },
        num_established: std::num::NonZeroU32::new(1).unwrap(),
        concurrent_dial_errors: None,
        established_in: Duration::from_secs(0),
    }
}
