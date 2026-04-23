use std::time::Duration;

use apollo_network::network_manager::{
    BroadcastTopicChannels,
    BroadcastTopicClient,
    BroadcastTopicClientTrait,
    BroadcastTopicServer,
    NetworkManager,
    ServerQueryManager,
    SqmrClientSender,
    SqmrServerReceiver,
};
use apollo_network_benchmark::node_args::NetworkProtocol;
use futures::StreamExt;
use libp2p::gossipsub::{Sha256Topic, Topic};
use libp2p::PeerId;
use tracing::{debug, trace, warn};

lazy_static::lazy_static! {
    pub static ref TOPIC: Sha256Topic = Topic::new("stress_test_topic".to_string());
}

// Pre-1.0 — this protocol is internal benchmarking infrastructure, not a public API.
const SQMR_PROTOCOL_NAME: &str = "/stress-test/0.1.0";

/// Backoff between retries when the ReversedSqmr receiver fails to open a query.
const REVERSED_SQMR_RETRY_BACKOFF: Duration = Duration::from_millis(100);

/// Registers protocol channels on an existing network manager.
/// Returns a sender and receiver for the configured protocol.
pub fn register_protocol_channels(
    network_manager: &mut NetworkManager,
    buffer_size: usize,
    protocol: &NetworkProtocol,
) -> (MessageSender, MessageReceiver) {
    match protocol {
        NetworkProtocol::Gossipsub => {
            let channels = network_manager
                .register_broadcast_topic::<Vec<u8>>(TOPIC.clone(), buffer_size)
                .expect(
                    "topic is registered once at node startup; registration only fails on \
                     duplicate names",
                );
            let BroadcastTopicChannels { broadcasted_messages_receiver, broadcast_topic_client } =
                channels;

            (
                MessageSender::Gossipsub(broadcast_topic_client),
                MessageReceiver::Gossipsub(broadcasted_messages_receiver),
            )
        }
        NetworkProtocol::Sqmr => {
            let sqmr_client = network_manager.register_sqmr_protocol_client::<Vec<u8>, Vec<u8>>(
                SQMR_PROTOCOL_NAME.to_string(),
                buffer_size,
            );
            let sqmr_server = network_manager.register_sqmr_protocol_server::<Vec<u8>, Vec<u8>>(
                SQMR_PROTOCOL_NAME.to_string(),
                buffer_size,
            );

            (MessageSender::Sqmr(sqmr_client), MessageReceiver::Sqmr(sqmr_server))
        }
        NetworkProtocol::ReversedSqmr => {
            let sqmr_client = network_manager.register_sqmr_protocol_client::<Vec<u8>, Vec<u8>>(
                SQMR_PROTOCOL_NAME.to_string(),
                buffer_size,
            );
            let sqmr_server = network_manager.register_sqmr_protocol_server::<Vec<u8>, Vec<u8>>(
                SQMR_PROTOCOL_NAME.to_string(),
                buffer_size,
            );

            (
                MessageSender::ReversedSqmr(ReversedSqmrSender::new(sqmr_server)),
                MessageReceiver::ReversedSqmr(sqmr_client),
            )
        }
    }
}

/// Message sender abstraction for different protocols.
pub enum MessageSender {
    Gossipsub(BroadcastTopicClient<Vec<u8>>),
    Sqmr(SqmrClientSender<Vec<u8>, Vec<u8>>),
    ReversedSqmr(ReversedSqmrSender),
}

/// Wrapper for the Reversed-SQMR sender role. Receivers initiate a query; this side
/// keeps the most recent incoming query and pushes broadcasts to it as responses.
pub struct ReversedSqmrSender {
    server: SqmrServerReceiver<Vec<u8>, Vec<u8>>,
    active_query: Option<ServerQueryManager<Vec<u8>, Vec<u8>>>,
}

impl ReversedSqmrSender {
    fn new(server: SqmrServerReceiver<Vec<u8>, Vec<u8>>) -> Self {
        Self { server, active_query: None }
    }

    async fn collect_new_queries(&mut self) {
        // Drain any pending queries with a short blocking timeout per iteration so that
        // the broadcaster always operates on the most recent query.
        while let Ok(query) =
            tokio::time::timeout(Duration::from_millis(1), self.server.next()).await
        {
            if let Some(query) = query {
                debug!("ReversedSqmr: Received new query, replacing previous query");
                self.active_query = Some(query);
            } else {
                break;
            }
        }
    }

    async fn broadcast_to_queries(&mut self, message: Vec<u8>) {
        if let Some(query) = &mut self.active_query {
            match query.send_response(message).await {
                Ok(()) => {
                    trace!("ReversedSqmr: Sent response to active query");
                }
                Err(send_error) => {
                    // Subscribers come and go under stress; this is expected churn.
                    warn!(
                        "ReversedSqmr: Active query failed, removing it, error: {:?}",
                        send_error
                    );
                    self.active_query = None;
                }
            }
        }
    }
}

impl MessageSender {
    pub async fn send_message(&mut self, message: Vec<u8>) {
        match self {
            MessageSender::Gossipsub(client) => {
                if let Err(broadcast_error) = client.broadcast_message(message).await {
                    // Individual broadcast failures under stress are expected and recoverable.
                    warn!("Gossipsub broadcast failed: {:?}", broadcast_error);
                }
            }
            MessageSender::Sqmr(client) => match client.send_new_query(message).await {
                Ok(mut response_manager) => {
                    // Detached on purpose: this benchmark binary discards SQMR responses
                    // and is torn down by `race_and_kill_tasks` at test end.
                    tokio::spawn(async move {
                        while let Some(_response) = response_manager.next().await {}
                    });
                }
                Err(query_error) => {
                    warn!("Failed to send SQMR query: {:?}", query_error);
                }
            },
            MessageSender::ReversedSqmr(sender) => {
                sender.collect_new_queries().await;
                sender.broadcast_to_queries(message).await;
            }
        }
    }
}

pub enum MessageReceiver {
    Gossipsub(BroadcastTopicServer<Vec<u8>>),
    Sqmr(SqmrServerReceiver<Vec<u8>, Vec<u8>>),
    ReversedSqmr(SqmrClientSender<Vec<u8>, Vec<u8>>),
}

impl MessageReceiver {
    pub async fn for_each<F>(self, mut f: F)
    where
        F: FnMut(Vec<u8>, Option<PeerId>) + Copy,
    {
        match self {
            MessageReceiver::Gossipsub(receiver) => {
                receiver
                    .for_each(|message| async move {
                        let (payload_result, meta) = message;
                        let peer_id = meta.originator_id.private_get_peer_id();
                        // Vec<u8> round-trip is `TryFrom<Vec<u8>>` with Error = Infallible,
                        // so this can never fail at runtime.
                        let payload = payload_result.expect("Vec<u8> round-trip is Infallible");
                        f(payload, Some(peer_id));
                    })
                    .await
            }
            MessageReceiver::Sqmr(receiver) => {
                receiver
                    .for_each(|server_query| async move {
                        match server_query.query() {
                            Ok(payload) => f(payload.to_vec(), None),
                            Err(parse_error) => {
                                warn!("SQMR: failed to parse query: {:?}", parse_error);
                            }
                        }
                    })
                    .await
            }
            // Loops until aborted by `race_and_kill_tasks` at test teardown; there is no
            // internal shutdown signal.
            MessageReceiver::ReversedSqmr(mut client) => loop {
                match client.send_new_query(vec![]).await {
                    Ok(mut response_manager) => loop {
                        match response_manager.next().await {
                            Some(Ok(response_data)) => {
                                f(response_data, None);
                            }
                            Some(Err(parse_error)) => {
                                warn!("ReversedSqmr: failed to parse response: {:?}", parse_error);
                                break;
                            }
                            None => {
                                debug!("ReversedSqmr: response stream ended; reconnecting");
                                break;
                            }
                        }
                    },
                    Err(query_error) => {
                        // Back off so we don't spin when the peer is gone or the protocol
                        // isn't negotiated yet. Expected during connection setup.
                        debug!(
                            "Failed to establish ReversedSqmr connection, retrying after {:?}: \
                             {:?}",
                            REVERSED_SQMR_RETRY_BACKOFF, query_error
                        );
                        tokio::time::sleep(REVERSED_SQMR_RETRY_BACKOFF).await;
                    }
                }
            },
        }
    }
}
