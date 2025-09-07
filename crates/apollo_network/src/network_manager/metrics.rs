use std::collections::HashMap;

use apollo_metrics::generate_permutation_labels;
use apollo_metrics::metrics::{LabeledMetricCounter, MetricCounter, MetricGauge};
use libp2p::gossipsub::{PublishError, TopicHash};
use strum::{EnumIter, IntoStaticStr, VariantNames};
use strum_macros::EnumVariantNames;

// Labels used for broadcast drop metrics
pub const LABEL_NAME_BROADCAST_DROP_REASON: &str = "drop_reason";

#[derive(IntoStaticStr, EnumIter, EnumVariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum BroadcastPublishDropReason {
    Duplicate,
    SigningError,
    NoPeersSubscribedToTopic,
    MessageTooLarge,
    TransformFailed,
    AllQueuesFull,
}

generate_permutation_labels! {
    NETWORK_BROADCAST_DROP_LABELS,
    (LABEL_NAME_BROADCAST_DROP_REASON, BroadcastPublishDropReason),
}

pub struct BroadcastNetworkMetrics {
    pub num_sent_broadcast_messages: MetricCounter,
    pub num_dropped_broadcast_messages: LabeledMetricCounter,
    pub num_received_broadcast_messages: MetricCounter,
}

impl BroadcastNetworkMetrics {
    pub fn register(&self) {
        self.num_sent_broadcast_messages.register();
        self.num_dropped_broadcast_messages.register();
        self.num_received_broadcast_messages.register();
    }

    fn inc_dropped_msgs(&self, reason: BroadcastPublishDropReason) {
        self.num_dropped_broadcast_messages
            .increment(1, &[(LABEL_NAME_BROADCAST_DROP_REASON, reason.into())]);
    }

    pub fn increment_publish_error(&self, err: &PublishError) {
        match err {
            PublishError::Duplicate => {
                self.inc_dropped_msgs(BroadcastPublishDropReason::Duplicate);
            }
            PublishError::SigningError(_) => {
                self.inc_dropped_msgs(BroadcastPublishDropReason::SigningError);
            }
            PublishError::NoPeersSubscribedToTopic => {
                self.inc_dropped_msgs(BroadcastPublishDropReason::NoPeersSubscribedToTopic);
            }
            PublishError::MessageTooLarge => {
                self.inc_dropped_msgs(BroadcastPublishDropReason::MessageTooLarge);
            }
            PublishError::TransformFailed(_) => {
                self.inc_dropped_msgs(BroadcastPublishDropReason::TransformFailed);
            }
            PublishError::AllQueuesFull(_) => {
                self.inc_dropped_msgs(BroadcastPublishDropReason::AllQueuesFull);
            }
        }
    }
}

pub struct SqmrNetworkMetrics {
    pub num_active_inbound_sessions: MetricGauge,
    pub num_active_outbound_sessions: MetricGauge,
}

impl SqmrNetworkMetrics {
    pub fn register(&self) {
        self.num_active_inbound_sessions.register();
        self.num_active_inbound_sessions.set(0f64);
        self.num_active_outbound_sessions.register();
        self.num_active_outbound_sessions.set(0f64);
    }
}

pub const LABEL_NAME_EVENT_TYPE: &str = "event_type";

#[derive(IntoStaticStr, EnumIter, EnumVariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum EventType {
    ConnectionsEstablished,
    ConnectionsClosed,
    DialFailure,
    ListenFailure,
    ListenError,
    AddressChange,
    NewListeners,
    NewListenAddrs,
    ExpiredListenAddrs,
    ListenerClosed,
    NewExternalAddrCandidate,
    ExternalAddrConfirmed,
    ExternalAddrExpired,
    NewExternalAddrOfPeer,
    InboundConnectionsHandled,
    OutboundConnectionsHandled,
    ConnectionHandlerEvents,
}

generate_permutation_labels! {
    EVENT_TYPE_LABELS,
    (LABEL_NAME_EVENT_TYPE, EventType),
}

pub struct EventMetrics {
    pub event_counter: LabeledMetricCounter,
}

impl EventMetrics {
    pub fn register(&self) {
        self.event_counter.register();
    }

    pub fn increment_event(&self, event_type: EventType) {
        self.event_counter.increment(1, &[(LABEL_NAME_EVENT_TYPE, event_type.into())]);
    }
}

// TODO(alonl, shahak): Consider making these fields private and receive Topics instead of
// TopicHashes in the constructor
pub struct NetworkMetrics {
    pub num_connected_peers: MetricGauge,
    pub num_blacklisted_peers: MetricGauge,
    pub broadcast_metrics_by_topic: Option<HashMap<TopicHash, BroadcastNetworkMetrics>>,
    pub sqmr_metrics: Option<SqmrNetworkMetrics>,
    pub event_metrics: Option<EventMetrics>,
}

impl NetworkMetrics {
    pub fn register(&self) {
        self.num_connected_peers.register();
        self.num_connected_peers.set(0f64);
        self.num_blacklisted_peers.register();
        self.num_blacklisted_peers.set(0f64);
        if let Some(broadcast_metrics_by_topic) = self.broadcast_metrics_by_topic.as_ref() {
            for broadcast_metrics in broadcast_metrics_by_topic.values() {
                broadcast_metrics.register();
            }
        }
        if let Some(sqmr_metrics) = self.sqmr_metrics.as_ref() {
            sqmr_metrics.register();
        }
        if let Some(event_metrics) = self.event_metrics.as_ref() {
            event_metrics.register();
        }
    }
}
