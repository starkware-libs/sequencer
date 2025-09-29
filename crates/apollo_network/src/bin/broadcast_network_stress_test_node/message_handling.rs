use apollo_network::network_manager::{
    BroadcastTopicClient,
    BroadcastTopicClientTrait,
    BroadcastTopicServer,
    SqmrClientSender,
    SqmrServerReceiver,
};
use futures::StreamExt;
use libp2p::PeerId;
use tracing::{error, info, trace};

use crate::network_channels::TopicType;

/// Message sender abstraction for different protocols
pub enum MessageSender {
    Gossipsub(BroadcastTopicClient<TopicType>),
    Sqmr(SqmrClientSender<TopicType, TopicType>),
    ReveresedSqmr(ReveresedSqmrSender),
}

/// Wrapper for ReveresedSqmr that maintains the last active query
pub struct ReveresedSqmrSender {
    server: SqmrServerReceiver<TopicType, TopicType>,
    active_query: Option<apollo_network::network_manager::ServerQueryManager<TopicType, TopicType>>,
}

impl ReveresedSqmrSender {
    pub fn new(server: SqmrServerReceiver<TopicType, TopicType>) -> Self {
        Self { server, active_query: None }
    }

    async fn collect_new_queries(&mut self) {
        // Non-blocking check for new queries, keeping only the last one
        while let Ok(query) =
            tokio::time::timeout(tokio::time::Duration::from_millis(1), self.server.next()).await
        {
            if let Some(query) = query {
                info!("ReveresedSqmr: Received new query, replacing previous query");
                self.active_query = Some(query);
            } else {
                break;
            }
        }
    }

    async fn broadcast_to_queries(&mut self, message: TopicType) {
        if let Some(query) = &mut self.active_query {
            match query.send_response(message).await {
                Ok(()) => {
                    trace!("ReveresedSqmr: Sent response to active query");
                }
                Err(e) => {
                    // Query failed, remove it
                    error!("ReveresedSqmr: Active query failed, removing it, error: {:?}", e);
                    self.active_query = None;
                }
            }
        }
    }
}

impl MessageSender {
    pub async fn send_message(&mut self, _peers: &[PeerId], message: TopicType) {
        match self {
            MessageSender::Gossipsub(client) => {
                client.broadcast_message(message).await.unwrap();
            }
            MessageSender::Sqmr(client) => {
                // Send query and properly handle the response manager to avoid session warnings
                match client.send_new_query(message).await {
                    Ok(mut response_manager) => {
                        // Consume the response manager to properly close the session
                        // This prevents the "finished with no messages" warning
                        tokio::spawn(async move {
                            while let Some(_response) = response_manager.next().await {
                                // Process any responses if they come, but don't block the sender
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to send SQMR query: {:?}", e);
                    }
                }
            }
            MessageSender::ReveresedSqmr(sender) => {
                // Collect any new queries first
                sender.collect_new_queries().await;
                // Then broadcast the message to all active queries
                sender.broadcast_to_queries(message).await;
            }
        }
    }
}

pub enum MessageReceiver {
    Gossipsub(BroadcastTopicServer<TopicType>),
    Sqmr(SqmrServerReceiver<TopicType, TopicType>),
    ReveresedSqmr(SqmrClientSender<TopicType, TopicType>),
}

impl MessageReceiver {
    pub async fn for_each<F>(self, mut f: F)
    where
        F: FnMut(TopicType) + Copy,
    {
        match self {
            MessageReceiver::Gossipsub(receiver) => {
                receiver
                    .for_each(|message| async move {
                        f(message.0.unwrap());
                    })
                    .await
            }
            MessageReceiver::Sqmr(receiver) => {
                receiver
                    .for_each(|x| async move {
                        f(x.query().as_ref().unwrap().to_vec());
                    })
                    .await
            }
            MessageReceiver::ReveresedSqmr(mut client) => loop {
                match client.send_new_query(vec![]).await {
                    Ok(mut response_manager) => loop {
                        let response_result = response_manager.next().await;
                        match response_result {
                            Some(Ok(response_data)) => {
                                f(response_data);
                            }
                            Some(Err(_)) => {
                                error!("ReveresedSqmr: Failed to parse response");
                                break;
                            }
                            None => {
                                error!("ReveresedSqmr: Response stream ended");
                                break;
                            }
                        }
                    },
                    Err(e) => {
                        error!(
                            "Failed to establish ReveresedSqmr connection, keeping client alive, \
                             error: {:?}",
                            e
                        );
                        // sleep(Duration::from_secs(1)).await;
                    }
                }
            },
        }
    }
}
