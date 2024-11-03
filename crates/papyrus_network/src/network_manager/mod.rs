mod swarm_trait;

#[cfg(test)]
mod test;
#[cfg(feature = "testing")]
pub mod test_utils;
#[cfg(test)]
pub mod test_utils;

use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures::channel::mpsc::{Receiver, SendError, Sender};
use futures::channel::oneshot;
use futures::future::{ready, BoxFuture, Ready};
use futures::sink::With;
use futures::stream::{FuturesUnordered, Map, Stream};
use futures::{pin_mut, FutureExt, Sink, SinkExt, StreamExt};
use libp2p::gossipsub::{SubscriptionError, TopicHash};
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, StreamProtocol, Swarm};
use metrics::gauge;
use papyrus_common::metrics as papyrus_metrics;
use papyrus_network_types::network_types::{BroadcastedMessageMetadata, OpaquePeerId};
use sqmr::Bytes;
use tracing::{debug, error, info, trace, warn};

use self::swarm_trait::SwarmTrait;
use crate::bin_utils::build_swarm;
use crate::gossipsub_impl::Topic;
use crate::mixed_behaviour::{self, BridgedBehaviour};
use crate::sqmr::behaviour::SessionError;
use crate::sqmr::{self, InboundSessionId, OutboundSessionId, SessionId};
use crate::utils::{is_localhost, StreamHashMap};
use crate::{gossipsub_impl, NetworkConfig};

#[derive(thiserror::Error, Debug)]
pub enum NetworkError {
    #[error(transparent)]
    DialError(#[from] libp2p::swarm::DialError),
}

// TODO: Understand whats the correct thing to do here.
const MESSAGE_METADATA_BUFFER_SIZE: usize = 100000;

pub struct GenericNetworkManager<SwarmT: SwarmTrait> {
    swarm: SwarmT,
    inbound_protocol_to_buffer_size: HashMap<StreamProtocol, usize>,
    sqmr_inbound_response_receivers: StreamHashMap<InboundSessionId, ResponsesReceiver>,
    sqmr_inbound_payload_senders: HashMap<StreamProtocol, SqmrServerSender>,
    sqmr_outbound_payload_receivers: StreamHashMap<StreamProtocol, SqmrClientReceiver>,
    sqmr_outbound_response_senders: HashMap<OutboundSessionId, ResponsesSender>,
    sqmr_outbound_report_receivers_awaiting_assignment: HashMap<OutboundSessionId, ReportReceiver>,
    // Splitting the broadcast receivers from the broadcasted senders in order to poll all
    // receivers simultaneously.
    // Each receiver has a matching sender and vice versa (i.e the maps have the same keys).
    messages_to_broadcast_receivers: StreamHashMap<TopicHash, Receiver<Bytes>>,
    broadcasted_messages_senders: HashMap<TopicHash, Sender<(Bytes, BroadcastedMessageMetadata)>>,
    reported_peer_receivers: FuturesUnordered<BoxFuture<'static, Option<PeerId>>>,
    advertised_multiaddr: Option<Multiaddr>,
    reported_peers_receiver: Receiver<PeerId>,
    reported_peers_sender: Sender<PeerId>,
    continue_propagation_sender: Sender<BroadcastedMessageMetadata>,
    continue_propagation_receiver: Receiver<BroadcastedMessageMetadata>,
    // Fields for metrics
    num_active_inbound_sessions: usize,
    num_active_outbound_sessions: usize,
}

impl<SwarmT: SwarmTrait> GenericNetworkManager<SwarmT> {
    pub async fn run(mut self) -> Result<(), NetworkError> {
        loop {
            tokio::select! {
                Some(event) = self.swarm.next() => self.handle_swarm_event(event),
                Some(res) = self.sqmr_inbound_response_receivers.next() => self.handle_response_for_inbound_query(res),
                Some((protocol, client_payload)) = self.sqmr_outbound_payload_receivers.next() => {
                    self.handle_local_sqmr_payload(protocol, client_payload.expect("An SQMR client channel should not be terminated."))
                }
                Some((topic_hash, message)) = self.messages_to_broadcast_receivers.next() => {
                    self.broadcast_message(message.expect("A broadcast channel should not be terminated."), topic_hash);
                }
                Some(Some(peer_id)) = self.reported_peer_receivers.next() => self.swarm.report_peer_as_malicious(peer_id),
                Some(peer_id) = self.reported_peers_receiver.next() => self.swarm.report_peer_as_malicious(peer_id),
                Some(broadcasted_message_metadata) = self.continue_propagation_receiver.next() => {
                    self.swarm.continue_propagation(broadcasted_message_metadata);
                }
            }
        }
    }

    // TODO(shahak): remove the advertised_multiaddr arg once we manage external addresses
    // in a behaviour.
    pub(crate) fn generic_new(mut swarm: SwarmT, advertised_multiaddr: Option<Multiaddr>) -> Self {
        gauge!(papyrus_metrics::PAPYRUS_NUM_CONNECTED_PEERS, 0f64);
        let reported_peer_receivers = FuturesUnordered::new();
        reported_peer_receivers.push(futures::future::pending().boxed());
        if let Some(address) = advertised_multiaddr.clone() {
            swarm.add_external_address(address);
        }
        let (reported_peers_sender, reported_peers_receiver) =
            futures::channel::mpsc::channel(MESSAGE_METADATA_BUFFER_SIZE);
        let (continue_propagation_sender, continue_propagation_receiver) =
            futures::channel::mpsc::channel(MESSAGE_METADATA_BUFFER_SIZE);
        Self {
            swarm,
            inbound_protocol_to_buffer_size: HashMap::new(),
            sqmr_inbound_response_receivers: StreamHashMap::new(HashMap::new()),
            sqmr_inbound_payload_senders: HashMap::new(),
            sqmr_outbound_payload_receivers: StreamHashMap::new(HashMap::new()),
            sqmr_outbound_response_senders: HashMap::new(),
            sqmr_outbound_report_receivers_awaiting_assignment: HashMap::new(),
            messages_to_broadcast_receivers: StreamHashMap::new(HashMap::new()),
            broadcasted_messages_senders: HashMap::new(),
            reported_peer_receivers,
            advertised_multiaddr,
            reported_peers_receiver,
            reported_peers_sender,
            continue_propagation_sender,
            continue_propagation_receiver,
            num_active_inbound_sessions: 0,
            num_active_outbound_sessions: 0,
        }
    }

    // TODO: Support multiple protocols where they're all different versions of the same protocol
    pub fn register_sqmr_protocol_server<Query, Response>(
        &mut self,
        protocol: String,
        buffer_size: usize,
    ) -> SqmrServerReceiver<Query, Response>
    where
        Bytes: From<Response>,
        Query: TryFrom<Bytes> + Clone,
        <Query as TryFrom<Bytes>>::Error: Clone,
        Response: 'static,
    {
        let protocol = StreamProtocol::try_from_owned(protocol)
            .expect("Could not parse protocol into StreamProtocol.");
        self.swarm.add_new_supported_inbound_protocol(protocol.clone());
        if let Some(_old_buffer_size) =
            self.inbound_protocol_to_buffer_size.insert(protocol.clone(), buffer_size)
        {
            panic!("Protocol '{}' has already been registered as a server.", protocol);
        }
        let (inbound_payload_sender, inbound_payload_receiver) =
            futures::channel::mpsc::channel(buffer_size);
        let insert_result = self
            .sqmr_inbound_payload_senders
            .insert(protocol.clone(), Box::new(inbound_payload_sender));
        if insert_result.is_some() {
            panic!("Protocol '{}' has already been registered as a server.", protocol);
        }

        let inbound_payload_receiver = inbound_payload_receiver
            .map(|payload: SqmrServerPayload| ServerQueryManager::from(payload));
        SqmrServerReceiver { receiver: Box::new(inbound_payload_receiver) }
    }

    /// Register a new subscriber for sending a single query and receiving multiple responses.
    /// Panics if the given protocol is already subscribed.
    // TODO: Support multiple protocols where they're all different versions of the same protocol
    // TODO: Seperate query and response buffer sizes.
    pub fn register_sqmr_protocol_client<Query, Response>(
        &mut self,
        protocol: String,
        buffer_size: usize,
    ) -> SqmrClientSender<Query, Response>
    where
        Bytes: From<Query>,
        Response: TryFrom<Bytes> + 'static + Send,
        <Response as TryFrom<Bytes>>::Error: 'static + Send,
        Query: 'static,
    {
        let protocol = StreamProtocol::try_from_owned(protocol)
            .expect("Could not parse protocol into StreamProtocol.");
        self.swarm.add_new_supported_inbound_protocol(protocol.clone());
        let (payload_sender, payload_receiver) = futures::channel::mpsc::channel(buffer_size);

        let insert_result = self
            .sqmr_outbound_payload_receivers
            .insert(protocol.clone(), Box::new(payload_receiver));
        if insert_result.is_some() {
            panic!("Protocol '{}' has already been registered as a client.", protocol);
        };

        SqmrClientSender::new(Box::new(payload_sender), buffer_size)
    }

    /// Register a new subscriber for broadcasting and receiving broadcasts for a given topic.
    /// Panics if this topic is already subscribed.
    // TODO: consider splitting into register_broadcast_topic_client and
    // register_broadcast_topic_server
    pub fn register_broadcast_topic<T>(
        &mut self,
        topic: Topic,
        buffer_size: usize,
    ) -> Result<BroadcastTopicChannels<T>, SubscriptionError>
    where
        T: TryFrom<Bytes> + 'static,
        Bytes: From<T>,
    {
        self.swarm.subscribe_to_topic(&topic)?;

        let topic_hash = topic.hash();

        let (messages_to_broadcast_sender, messages_to_broadcast_receiver) =
            futures::channel::mpsc::channel(buffer_size);
        let (broadcasted_messages_sender, broadcasted_messages_receiver) =
            futures::channel::mpsc::channel(buffer_size);

        let insert_result = self
            .messages_to_broadcast_receivers
            .insert(topic_hash.clone(), messages_to_broadcast_receiver);
        if insert_result.is_some() {
            panic!("Topic '{}' has already been registered.", topic);
        }

        let insert_result = self
            .broadcasted_messages_senders
            .insert(topic_hash.clone(), broadcasted_messages_sender.clone());
        if insert_result.is_some() {
            panic!("Topic '{}' has already been registered.", topic);
        }

        let broadcasted_messages_fn: BroadcastReceivedMessagesConverterFn<T> =
            |(x, broadcasted_message_metadata)| (T::try_from(x), broadcasted_message_metadata);
        let broadcasted_messages_receiver =
            broadcasted_messages_receiver.map(broadcasted_messages_fn);

        let messages_to_broadcast_fn: fn(T) -> Ready<Result<Bytes, SendError>> =
            |x| ready(Ok(Bytes::from(x)));
        let messages_to_broadcast_sender =
            messages_to_broadcast_sender.with(messages_to_broadcast_fn);

        let reported_messages_fn: fn(
            BroadcastedMessageMetadata,
        ) -> Ready<Result<PeerId, SendError>> = |broadcasted_message_metadata| {
            ready(Ok(broadcasted_message_metadata.originator_id.private_get_peer_id()))
        };
        let reported_messages_sender =
            self.reported_peers_sender.clone().with(reported_messages_fn);

        let continue_propagation_sender = self.continue_propagation_sender.clone();

        Ok(BroadcastTopicChannels {
            broadcasted_messages_receiver,
            broadcast_topic_client: BroadcastTopicClient::new(
                messages_to_broadcast_sender,
                reported_messages_sender,
                continue_propagation_sender,
            ),
        })
    }

    fn handle_swarm_event(&mut self, event: SwarmEvent<mixed_behaviour::Event>) {
        match event {
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                debug!("Connected to peer id: {peer_id:?}");
                gauge!(
                    papyrus_metrics::PAPYRUS_NUM_CONNECTED_PEERS,
                    self.swarm.num_connected_peers() as f64
                );
            }
            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                match cause {
                    Some(connection_error) => {
                        debug!("Connection to {peer_id:?} closed due to {connection_error:?}.")
                    }
                    None => debug!("Connection to {peer_id:?} closed."),
                }
                gauge!(
                    papyrus_metrics::PAPYRUS_NUM_CONNECTED_PEERS,
                    self.swarm.num_connected_peers() as f64
                );
            }
            SwarmEvent::Behaviour(event) => {
                self.handle_behaviour_event(event);
            }
            SwarmEvent::OutgoingConnectionError { connection_id, peer_id, error } => {
                error!(
                    "Outgoing connection error. connection id: {connection_id:?}, requested peer \
                     id: {peer_id:?}, error: {error:?}"
                );
            }
            SwarmEvent::IncomingConnectionError {
                connection_id,
                local_addr,
                send_back_addr,
                error,
            } => {
                // No need to panic here since this is a result of another peer trying to dial to us
                // and failing. Other peers are welcome to retry.
                error!(
                    "Incoming connection error. connection id: {connection_id:?}, local addr: \
                     {local_addr:?}, send back addr: {send_back_addr:?}, error: {error:?}"
                );
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                // TODO(shahak): Find a better way to filter private addresses.
                if !is_localhost(&address) && self.advertised_multiaddr.is_none() {
                    self.swarm.add_external_address(address);
                }
            }
            SwarmEvent::IncomingConnection { .. }
            | SwarmEvent::Dialing { .. }
            | SwarmEvent::NewExternalAddrCandidate { .. }
            | SwarmEvent::NewExternalAddrOfPeer { .. } => {}
            _ => {
                error!("Unexpected event {event:?}");
            }
        }
    }

    fn handle_behaviour_event(&mut self, event: mixed_behaviour::Event) {
        match event {
            mixed_behaviour::Event::ExternalEvent(external_event) => {
                self.handle_behaviour_external_event(external_event);
            }
            mixed_behaviour::Event::ToOtherBehaviourEvent(internal_event) => {
                self.handle_to_other_behaviour_event(internal_event);
            }
        }
    }

    fn handle_behaviour_external_event(&mut self, event: mixed_behaviour::ExternalEvent) {
        match event {
            mixed_behaviour::ExternalEvent::Sqmr(event) => {
                self.handle_sqmr_event(event);
            }
            mixed_behaviour::ExternalEvent::GossipSub(event) => {
                self.handle_gossipsub_behaviour_event(event);
            }
        }
    }

    // TODO(shahak): Move this logic to mixed_behaviour.
    fn handle_to_other_behaviour_event(&mut self, event: mixed_behaviour::ToOtherBehaviourEvent) {
        if let mixed_behaviour::ToOtherBehaviourEvent::NoOp = event {
            return;
        }
        self.swarm.behaviour_mut().identify.on_other_behaviour_event(&event);
        self.swarm.behaviour_mut().kademlia.on_other_behaviour_event(&event);
        if let Some(discovery) = self.swarm.behaviour_mut().discovery.as_mut() {
            discovery.on_other_behaviour_event(&event);
        }
        self.swarm.behaviour_mut().sqmr.on_other_behaviour_event(&event);
        self.swarm.behaviour_mut().peer_manager.on_other_behaviour_event(&event);
        self.swarm.behaviour_mut().gossipsub.on_other_behaviour_event(&event);
    }

    fn handle_sqmr_event(&mut self, event: sqmr::behaviour::ExternalEvent) {
        match event {
            sqmr::behaviour::ExternalEvent::NewInboundSession {
                query,
                inbound_session_id,
                peer_id,
                protocol_name,
            } => self.handle_sqmr_event_new_inbound_session(
                peer_id,
                protocol_name,
                inbound_session_id,
                query,
            ),
            sqmr::behaviour::ExternalEvent::ReceivedResponse {
                outbound_session_id,
                response,
                peer_id,
            } => self.handle_sqmr_event_received_response(outbound_session_id, peer_id, response),
            sqmr::behaviour::ExternalEvent::SessionFailed { session_id, error } => {
                self.handle_sqmr_event_session_failed(session_id, error)
            }
            sqmr::behaviour::ExternalEvent::SessionFinishedSuccessfully { session_id } => {
                self.handle_sqmr_event_session_finished_successfully(session_id)
            }
        }
    }

    fn handle_sqmr_event_new_inbound_session(
        &mut self,
        peer_id: PeerId,
        protocol_name: StreamProtocol,
        inbound_session_id: InboundSessionId,
        query: Vec<u8>,
    ) {
        self.num_active_inbound_sessions += 1;
        gauge!(
            papyrus_metrics::PAPYRUS_NUM_ACTIVE_INBOUND_SESSIONS,
            self.num_active_inbound_sessions as f64
        );
        let (report_sender, report_receiver) = oneshot::channel::<()>();
        self.handle_new_report_receiver(peer_id, report_receiver);
        // TODO: consider returning error instead of panic.
        let Some(query_sender) = self.sqmr_inbound_payload_senders.get_mut(&protocol_name) else {
            return;
        };
        let (responses_sender, responses_receiver) = futures::channel::mpsc::channel(
            *self
                .inbound_protocol_to_buffer_size
                .get(&protocol_name)
                .expect("A protocol is registered in NetworkManager but it has no buffer size."),
        );
        let responses_sender = Box::new(responses_sender);
        self.sqmr_inbound_response_receivers.insert(
            inbound_session_id,
            // Adding a None at the end of the stream so that we will receive a message
            // letting us know the stream has ended.
            Box::new(responses_receiver),
        );

        // TODO(shahak): Close the inbound session if the buffer is full.
        send_now(
            query_sender,
            SqmrServerPayload { query, report_sender, responses_sender },
            format!(
                "Received an inbound query while the buffer is full. Dropping query for session \
                 {inbound_session_id:?}"
            ),
        );
    }

    fn handle_sqmr_event_received_response(
        &mut self,
        outbound_session_id: OutboundSessionId,
        peer_id: PeerId,
        response: Vec<u8>,
    ) {
        trace!(
            "Received response from peer for session id: {outbound_session_id:?}. sending to sync \
             subscriber."
        );
        if let Some(report_receiver) =
            self.sqmr_outbound_report_receivers_awaiting_assignment.remove(&outbound_session_id)
        {
            self.handle_new_report_receiver(peer_id, report_receiver)
        }
        if let Some(response_sender) =
            self.sqmr_outbound_response_senders.get_mut(&outbound_session_id)
        {
            // TODO(shahak): Close the channel if the buffer is full.
            send_now(
                response_sender,
                response,
                format!(
                    "Received response for an outbound query while the buffer is full. Dropping \
                     it. Session: {outbound_session_id:?}"
                ),
            );
        }
    }

    fn handle_sqmr_event_session_failed(&mut self, session_id: SessionId, error: SessionError) {
        error!("Session {session_id:?} failed on {error:?}");
        self.report_session_removed_to_metrics(session_id);
        // TODO: Handle reputation and retry.
        if let SessionId::OutboundSessionId(outbound_session_id) = session_id {
            self.sqmr_outbound_response_senders.remove(&outbound_session_id);
            if let Some(_report_receiver) =
                self.sqmr_outbound_report_receivers_awaiting_assignment.remove(&outbound_session_id)
            {
                debug!(
                    "Outbound session failed before peer assignment. Ignoring incoming reports \
                     for the session."
                );
            }
        }
    }

    fn handle_sqmr_event_session_finished_successfully(&mut self, session_id: SessionId) {
        debug!("Session completed successfully. session_id: {session_id:?}");
        self.report_session_removed_to_metrics(session_id);
        if let SessionId::OutboundSessionId(outbound_session_id) = session_id {
            self.sqmr_outbound_response_senders.remove(&outbound_session_id);
            if let Some(_report_receiver) =
                self.sqmr_outbound_report_receivers_awaiting_assignment.remove(&outbound_session_id)
            {
                error!(
                    "Outbound session finished with no messages in it. Ignoring incoming reports \
                     for the session."
                );
            }
        }
    }

    fn handle_gossipsub_behaviour_event(&mut self, event: gossipsub_impl::ExternalEvent) {
        let gossipsub_impl::ExternalEvent::Received { originated_peer_id, message, topic_hash } =
            event;
        let broadcasted_message_metadata = BroadcastedMessageMetadata {
            originator_id: OpaquePeerId::private_new(originated_peer_id),
        };
        let Some(sender) = self.broadcasted_messages_senders.get_mut(&topic_hash) else {
            error!(
                "Received a message from a topic we're not subscribed to with hash {topic_hash:?}"
            );
            return;
        };
        let send_result = sender.try_send((message, broadcasted_message_metadata));
        if let Err(e) = send_result {
            if e.is_disconnected() {
                panic!("Receiver was dropped. This should never happen.")
            } else if e.is_full() {
                error!(
                    "Receiver buffer is full. Dropping broadcasted message for topic with hash: \
                     {topic_hash:?}."
                );
            }
        }
    }

    fn handle_response_for_inbound_query(&mut self, res: (InboundSessionId, Option<Bytes>)) {
        let (inbound_session_id, maybe_response) = res;
        match maybe_response {
            Some(response) => {
                self.swarm.send_response(response, inbound_session_id).unwrap_or_else(|e| {
                    error!(
                        "Failed to send response to peer. Session id: {inbound_session_id:?} not \
                         found error: {e:?}"
                    );
                });
            }
            // The None is inserted by the network manager after the receiver end terminated so
            // that we'll know here when it terminated.
            None => {
                self.swarm.close_inbound_session(inbound_session_id).unwrap_or_else(|e| {
                    error!(
                        "Failed to close session after sending all response. Session id: \
                         {inbound_session_id:?} not found error: {e:?}"
                    )
                });
            }
        };
    }

    fn handle_local_sqmr_payload(
        &mut self,
        protocol: StreamProtocol,
        client_payload: SqmrClientPayload,
    ) {
        let SqmrClientPayload { query, report_receiver, responses_sender } = client_payload;
        match self.swarm.send_query(query, PeerId::random(), protocol.clone()) {
            Ok(outbound_session_id) => {
                debug!(
                    "Network received new query. waiting for peer assignment. \
                     outbound_session_id: {outbound_session_id:?}"
                );
                self.num_active_outbound_sessions += 1;
                gauge!(
                    papyrus_metrics::PAPYRUS_NUM_ACTIVE_OUTBOUND_SESSIONS,
                    self.num_active_outbound_sessions as f64
                );
                self.sqmr_outbound_response_senders.insert(outbound_session_id, responses_sender);
                self.sqmr_outbound_report_receivers_awaiting_assignment
                    .insert(outbound_session_id, report_receiver);
            }
            Err(e) => {
                info!(
                    "Failed to send query to peer. Peer not connected error: {e:?} Returning \
                     empty response to sync subscriber."
                );
            }
        }
    }

    fn broadcast_message(&mut self, message: Bytes, topic_hash: TopicHash) {
        self.swarm.broadcast_message(message, topic_hash);
    }

    fn report_session_removed_to_metrics(&mut self, session_id: SessionId) {
        match session_id {
            SessionId::InboundSessionId(_) => {
                self.num_active_inbound_sessions -= 1;
                gauge!(
                    papyrus_metrics::PAPYRUS_NUM_ACTIVE_INBOUND_SESSIONS,
                    self.num_active_inbound_sessions as f64
                );
            }
            SessionId::OutboundSessionId(_) => {
                self.num_active_outbound_sessions += 1;
                gauge!(
                    papyrus_metrics::PAPYRUS_NUM_ACTIVE_OUTBOUND_SESSIONS,
                    self.num_active_outbound_sessions as f64
                );
            }
        }
    }
    fn handle_new_report_receiver(&self, peer_id: PeerId, report_receiver: oneshot::Receiver<()>) {
        self.reported_peer_receivers.push(
            report_receiver
                .map(move |result| match result {
                    Ok(_) => Some(peer_id),
                    Err(_) => None,
                })
                .boxed(),
        );
    }
}

fn send_now<Item>(sender: &mut GenericSender<Item>, item: Item, buffer_full_message: String) {
    pin_mut!(sender);
    match sender.as_mut().send(item).now_or_never() {
        Some(Ok(())) => {}
        Some(Err(error)) => {
            error!("Received error while sending message: {:?}", error);
        }
        None => {
            error!(buffer_full_message);
        }
    }
}

pub type NetworkManager = GenericNetworkManager<Swarm<mixed_behaviour::MixedBehaviour>>;

impl NetworkManager {
    pub fn new(config: NetworkConfig, node_version: Option<String>) -> Self {
        let NetworkConfig {
            tcp_port,
            quic_port: _,
            session_timeout,
            idle_connection_timeout,
            bootstrap_peer_multiaddr,
            advertised_multiaddr,
            secret_key,
            chain_id,
            discovery_config,
            peer_manager_config,
        } = config;

        let listen_addresses = vec![
            // TODO: uncomment once quic transpot works.
            // format!("/ip4/0.0.0.0/udp/{quic_port}/quic-v1"),
            format!("/ip4/0.0.0.0/tcp/{tcp_port}"),
        ];

        let swarm = build_swarm(listen_addresses, idle_connection_timeout, secret_key, |key| {
            mixed_behaviour::MixedBehaviour::new(
                key,
                bootstrap_peer_multiaddr.clone(),
                sqmr::Config { session_timeout },
                chain_id,
                node_version,
                discovery_config,
                peer_manager_config,
            )
        });
        let advertised_multiaddr = advertised_multiaddr.map(|address| {
            address
                .with_p2p(*swarm.local_peer_id())
                .expect("advertised_multiaddr has a peer id different than the local peer id")
        });
        Self::generic_new(swarm, advertised_multiaddr)
    }

    pub fn get_local_peer_id(&self) -> String {
        self.swarm.local_peer_id().to_string()
    }
}

pub type ReportSender = oneshot::Sender<()>;
type ReportReceiver = oneshot::Receiver<()>;

type GenericSender<T> = Box<dyn Sink<T, Error = SendError> + Unpin + Send>;
// Box<S> implements Stream only if S: Stream + Unpin
pub type GenericReceiver<T> = Box<dyn Stream<Item = T> + Unpin + Send>;

type ResponsesSender = GenericSender<Bytes>;
type ResponsesReceiver = GenericReceiver<Bytes>;

type ClientResponsesReceiver<Response> =
    GenericReceiver<Result<Response, <Response as TryFrom<Bytes>>::Error>>;

struct ServerResponsesSender<Response> {
    sender: GenericSender<Response>,
}

impl<Response> ServerResponsesSender<Response> {
    async fn feed(&mut self, response: Response) -> Result<(), SendError> {
        self.sender.feed(response).await
    }
}

impl<Response> Drop for ServerResponsesSender<Response> {
    fn drop(&mut self) {
        let _ = self.sender.flush().now_or_never().unwrap_or_else(|| {
            error!("Failed to flush responses sender when dropping channel");
            Ok(())
        });
    }
}

pub struct SqmrClientSender<Query, Response>
where
    Bytes: From<Query>,
    Response: TryFrom<Bytes> + 'static + Send,
    <Response as TryFrom<Bytes>>::Error: 'static + Send,
{
    sender: GenericSender<SqmrClientPayload>,
    buffer_size: usize,
    _query_type: std::marker::PhantomData<Query>,
    _response_type: std::marker::PhantomData<Response>,
}

impl<Query, Response> SqmrClientSender<Query, Response>
where
    Bytes: From<Query>,
    Response: TryFrom<Bytes> + 'static + Send,
    <Response as TryFrom<Bytes>>::Error: 'static + Send,
{
    fn new(sender: GenericSender<SqmrClientPayload>, buffer_size: usize) -> Self {
        Self {
            sender,
            buffer_size,
            _query_type: std::marker::PhantomData,
            _response_type: std::marker::PhantomData,
        }
    }
    pub async fn send_new_query(
        &mut self,
        query: Query,
    ) -> Result<ClientResponsesManager<Response>, SendError> {
        let (report_sender, report_receiver) = oneshot::channel::<()>();
        let (responses_sender, responses_receiver) =
            futures::channel::mpsc::channel(self.buffer_size);
        let responses_receiver = Box::new(responses_receiver);
        let query = Bytes::from(query);
        let responses_sender =
            Box::new(responses_sender.with(|response| ready(Ok(Response::try_from(response)))));
        let payload = SqmrClientPayload { query, report_receiver, responses_sender };
        self.sender.send(payload).await?;
        Ok(ClientResponsesManager { report_sender, responses_receiver })
    }
}

pub struct ClientResponsesManager<Response: TryFrom<Bytes>> {
    report_sender: ReportSender,
    pub(crate) responses_receiver: ClientResponsesReceiver<Response>,
}

impl<Response: TryFrom<Bytes>> ClientResponsesManager<Response> {
    /// Use this function to report peer as malicious
    pub fn report_peer(self) {
        warn!("Reporting peer");
        if let Err(e) = self.report_sender.send(()) {
            error!("Failed to report peer. Error: {e:?}");
        }
    }
}

impl<Response: TryFrom<Bytes>> Stream for ClientResponsesManager<Response> {
    type Item = Result<Response, <Response as TryFrom<Bytes>>::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.responses_receiver.poll_next_unpin(cx)
    }
}

type SqmrClientReceiver = GenericReceiver<SqmrClientPayload>;

pub struct SqmrClientPayload {
    query: Bytes,
    report_receiver: ReportReceiver,
    responses_sender: ResponsesSender,
}

pub struct SqmrServerReceiver<Query, Response>
where
    Query: TryFrom<Bytes>,
{
    receiver: GenericReceiver<ServerQueryManager<Query, Response>>,
}

impl<Query, Response> Stream for SqmrServerReceiver<Query, Response>
where
    Query: TryFrom<Bytes>,
{
    type Item = ServerQueryManager<Query, Response>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.receiver.poll_next_unpin(cx)
    }
}

pub struct ServerQueryManager<Query, Response>
where
    Query: TryFrom<Bytes>,
{
    query: Result<Query, <Query as TryFrom<Bytes>>::Error>,
    report_sender: ReportSender,
    responses_sender: ServerResponsesSender<Response>,
}

impl<Query, Response> ServerQueryManager<Query, Response>
where
    Query: TryFrom<Bytes>,
    Response: Send + 'static,
{
    pub fn query(&self) -> &Result<Query, <Query as TryFrom<Bytes>>::Error> {
        &self.query
    }

    pub fn report_peer(self) {
        debug!("Reporting peer from server to network");
        if let Err(e) = self.report_sender.send(()) {
            error!("Failed to report peer. Error: {e:?}");
        }
    }

    pub async fn send_response(&mut self, response: Response) -> Result<(), SendError> {
        debug!("Sending response from server to network");
        match self.responses_sender.feed(response).await {
            Ok(()) => Ok(()),
            Err(e) => {
                error!("Failed to send response from server to network. Error: {e:?}");
                Err(e)
            }
        }
    }
}

impl<Query, Response> From<SqmrServerPayload> for ServerQueryManager<Query, Response>
where
    Bytes: From<Response>,
    Response: 'static,
    Query: TryFrom<Bytes>,
{
    fn from(payload: SqmrServerPayload) -> Self {
        let SqmrServerPayload { query, report_sender, responses_sender } = payload;
        let query = Query::try_from(query);
        let responses_sender =
            Box::new(responses_sender.with(|response| ready(Ok(Bytes::from(response)))));
        let responses_sender = ServerResponsesSender { sender: responses_sender };

        Self { query, report_sender, responses_sender }
    }
}

type SqmrServerSender = GenericSender<SqmrServerPayload>;

struct SqmrServerPayload {
    query: Bytes,
    report_sender: ReportSender,
    responses_sender: ResponsesSender,
}

#[async_trait]
pub trait BroadcastTopicClientTrait<T> {
    async fn broadcast_message(&mut self, message: T) -> Result<(), SendError>;
    async fn report_peer(
        &mut self,
        broadcasted_message_metadata: BroadcastedMessageMetadata,
    ) -> Result<(), SendError>;
    async fn continue_propagation(
        &mut self,
        broadcasted_message_metadata: &BroadcastedMessageMetadata,
    ) -> Result<(), SendError>;
}

#[derive(Clone)]
pub struct BroadcastTopicClient<T: TryFrom<Bytes>> {
    messages_to_broadcast_sender: BroadcastTopicSender<T, Bytes>,
    reported_messages_sender: BroadcastTopicSender<BroadcastedMessageMetadata, PeerId>,
    continue_propagation_sender: Sender<BroadcastedMessageMetadata>,
}

impl<T: TryFrom<Bytes>> BroadcastTopicClient<T> {
    // TODO(matan): Remove once consensus_manager no longer needs to build fake channels.
    pub fn new(
        messages_to_broadcast_sender: BroadcastTopicSender<T, Bytes>,
        reported_messages_sender: BroadcastTopicSender<BroadcastedMessageMetadata, PeerId>,
        continue_propagation_sender: Sender<BroadcastedMessageMetadata>,
    ) -> Self {
        BroadcastTopicClient {
            messages_to_broadcast_sender,
            reported_messages_sender,
            continue_propagation_sender,
        }
    }
}

#[async_trait]
impl<T: TryFrom<Bytes> + Send> BroadcastTopicClientTrait<T> for BroadcastTopicClient<T> {
    async fn broadcast_message(&mut self, message: T) -> Result<(), SendError> {
        self.messages_to_broadcast_sender.send(message).await
    }

    async fn report_peer(
        &mut self,
        broadcasted_message_metadata: BroadcastedMessageMetadata,
    ) -> Result<(), SendError> {
        self.reported_messages_sender.send(broadcasted_message_metadata).await
    }

    async fn continue_propagation(
        &mut self,
        broadcasted_message_metadata: &BroadcastedMessageMetadata,
    ) -> Result<(), SendError> {
        self.continue_propagation_sender.send(broadcasted_message_metadata.clone()).await
    }
}

pub type BroadcastTopicSender<T, Message> = With<
    Sender<Message>,
    Message,
    T,
    Ready<Result<Message, SendError>>,
    fn(T) -> Ready<Result<Message, SendError>>,
>;

pub type BroadcastTopicServer<T> =
    Map<Receiver<(Bytes, BroadcastedMessageMetadata)>, BroadcastReceivedMessagesConverterFn<T>>;

type ReceivedBroadcastedMessage<Message> =
    (Result<Message, <Message as TryFrom<Bytes>>::Error>, BroadcastedMessageMetadata);

type BroadcastReceivedMessagesConverterFn<T> =
    fn((Bytes, BroadcastedMessageMetadata)) -> ReceivedBroadcastedMessage<T>;

pub struct BroadcastTopicChannels<T: TryFrom<Bytes>> {
    pub broadcasted_messages_receiver: BroadcastTopicServer<T>,
    pub broadcast_topic_client: BroadcastTopicClient<T>,
}
