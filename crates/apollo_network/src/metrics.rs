use std::collections::HashMap;

use apollo_metrics::generate_permutation_labels;
use apollo_metrics::metrics::{LabeledMetricCounter, MetricCounter, MetricGauge, MetricHistogram};
use libp2p::gossipsub::{PublishError, TopicHash};
use strum::{IntoStaticStr, VariantNames};
use strum_macros::EnumVariantNames;

// Labels used for broadcast drop metrics
pub const LABEL_NAME_BROADCAST_DROP_REASON: &str = "drop_reason";

#[derive(IntoStaticStr, EnumVariantNames)]
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
    pub sent_broadcast_message_metrics: MessageMetrics,
    pub dropped_broadcast_message_metrics: LabeledMessageMetrics,
    pub received_broadcast_message_metrics: MessageMetrics,
}

impl BroadcastNetworkMetrics {
    pub fn register(&self) {
        self.sent_broadcast_message_metrics.register();
        self.dropped_broadcast_message_metrics.register();
        self.received_broadcast_message_metrics.register();
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

#[derive(IntoStaticStr, EnumVariantNames)]
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

pub struct LatencyMetrics {
    pub ping_latency_seconds: MetricHistogram,
}

impl LatencyMetrics {
    pub fn register(&self) {
        self.ping_latency_seconds.register();
    }

    pub fn update_ping_latency(&self, latency_seconds: f64) {
        self.ping_latency_seconds.record(latency_seconds);
    }
}

pub struct MessageMetrics {
    pub num_messages: MetricCounter,
    pub message_size_mb: Option<MetricHistogram>,
}

impl MessageMetrics {
    pub fn register(&self) {
        self.num_messages.register();
        if let Some(message_size_bytes) = &self.message_size_mb {
            message_size_bytes.register();
        }
    }

    pub fn record_message(&self, message_size: usize) {
        self.num_messages.increment(1);
        if let Some(message_size_mb) = &self.message_size_mb {
            message_size_mb.record(convert_bytes_to_mb(message_size));
        }
    }
}

fn convert_bytes_to_mb(bytes: usize) -> f64 {
    let bytes = u32::try_from(bytes).unwrap_or_else(|_| {
        panic!(
            "Message size {bytes} is bigger than what is allowed,
            should not have gotten into the system."
        )
    });
    f64::from(bytes) / 1_048_576.0
}

pub struct LabeledMessageMetrics {
    pub num_messages: LabeledMetricCounter,
    pub message_size_mb: Option<MetricHistogram>,
}

impl LabeledMessageMetrics {
    pub fn register(&self) {
        self.num_messages.register();
        if let Some(message_size_mb) = &self.message_size_mb {
            message_size_mb.register();
        }
    }

    fn increment_dropped_msgs(&self, reason: BroadcastPublishDropReason) {
        self.num_messages.increment(1, &[(LABEL_NAME_BROADCAST_DROP_REASON, reason.into())]);
    }

    pub fn record_message(&self, err: &PublishError, message_size: usize) {
        match err {
            PublishError::Duplicate => {
                self.increment_dropped_msgs(BroadcastPublishDropReason::Duplicate);
            }
            PublishError::SigningError(_) => {
                self.increment_dropped_msgs(BroadcastPublishDropReason::SigningError);
            }
            PublishError::NoPeersSubscribedToTopic => {
                self.increment_dropped_msgs(BroadcastPublishDropReason::NoPeersSubscribedToTopic);
            }
            PublishError::MessageTooLarge => {
                self.increment_dropped_msgs(BroadcastPublishDropReason::MessageTooLarge);
            }
            PublishError::TransformFailed(_) => {
                self.increment_dropped_msgs(BroadcastPublishDropReason::TransformFailed);
            }
            PublishError::AllQueuesFull(_) => {
                self.increment_dropped_msgs(BroadcastPublishDropReason::AllQueuesFull);
            }
        }
        if let Some(message_size_mb) = &self.message_size_mb {
            message_size_mb.record(convert_bytes_to_mb(message_size));
        }
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
    pub latency_metrics: Option<LatencyMetrics>,
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
        if let Some(latency_metrics) = self.latency_metrics.as_ref() {
            latency_metrics.register();
        }
    }
}
