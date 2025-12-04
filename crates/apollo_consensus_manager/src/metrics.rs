use apollo_metrics::define_metrics;
use apollo_network::metrics::{EVENT_TYPE_LABELS, NETWORK_BROADCAST_DROP_LABELS};

define_metrics!(
    ConsensusManager => {
        // topic agnostic metrics
        MetricGauge { CONSENSUS_NUM_CONNECTED_PEERS, "apollo_consensus_num_connected_peers", "The number of connected peers to the consensus p2p component" },
        MetricGauge { CONSENSUS_NUM_BLACKLISTED_PEERS, "apollo_consensus_num_blacklisted_peers", "The number of currently blacklisted peers by the consensus component" },
        MetricHistogram { CONSENSUS_PING_LATENCY, "apollo_consensus_ping_latency_seconds", "The ping latency in seconds for the consensus p2p component" },

        // Votes topic metrics
        MetricCounter { CONSENSUS_VOTES_NUM_SENT_MESSAGES, "apollo_consensus_votes_num_sent_messages", "The number of messages sent by the consensus p2p component over the Votes topic", init = 0 },
        MetricHistogram { CONSENSUS_VOTES_SENT_MESSAGE_SIZE_BYTES, "apollo_consensus_votes_sent_message_size_bytes", "The size in bytes of messages sent by the consensus p2p component over the Votes topic" },
        MetricCounter { CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES, "apollo_consensus_votes_num_received_messages", "The number of messages received by the consensus p2p component over the Votes topic", init = 0 },
        MetricHistogram { CONSENSUS_VOTES_RECEIVED_MESSAGE_SIZE_BYTES, "apollo_consensus_votes_received_message_size_bytes", "The size in bytes of messages received by the consensus p2p component over the Votes topic" },
        LabeledMetricCounter { CONSENSUS_VOTES_NUM_DROPPED_MESSAGES, "apollo_consensus_votes_num_dropped_messages", "The number of messages dropped by the consensus p2p component over the Votes topic", init = 0, labels = NETWORK_BROADCAST_DROP_LABELS },
        MetricHistogram { CONSENSUS_VOTES_DROPPED_MESSAGE_SIZE_BYTES, "apollo_consensus_votes_dropped_message_size_bytes", "The size in bytes of messages dropped by the consensus p2p component over the Votes topic" },

        // Proposals topic metrics
        MetricCounter { CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES, "apollo_consensus_proposals_num_sent_messages", "The number of messages sent by the consensus p2p component over the Proposals topic", init = 0 },
        MetricHistogram { CONSENSUS_PROPOSALS_SENT_MESSAGE_SIZE_BYTES, "apollo_consensus_proposals_sent_message_size_bytes", "The size in bytes of messages sent by the consensus p2p component over the Proposals topic" },
        MetricCounter { CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES, "apollo_consensus_proposals_num_received_messages", "The number of messages received by the consensus p2p component over the Proposals topic", init = 0 },
        MetricHistogram { CONSENSUS_PROPOSALS_RECEIVED_MESSAGE_SIZE_BYTES, "apollo_consensus_proposals_received_message_size_bytes", "The size in bytes of messages received by the consensus p2p component over the Proposals topic" },
        LabeledMetricCounter { CONSENSUS_PROPOSALS_NUM_DROPPED_MESSAGES, "apollo_consensus_proposals_num_dropped_messages", "The number of messages dropped by the consensus p2p component over the Proposals topic", init = 0, labels = NETWORK_BROADCAST_DROP_LABELS },
        MetricHistogram { CONSENSUS_PROPOSALS_DROPPED_MESSAGE_SIZE_BYTES, "apollo_consensus_proposals_dropped_message_size_bytes", "The size in bytes of messages dropped by the consensus p2p component over the Proposals topic" },

        // Network events
        LabeledMetricCounter { CONSENSUS_NETWORK_EVENTS, "apollo_consensus_network_events", "Network events counter by event type for consensus", init = 0, labels = EVENT_TYPE_LABELS },

        MetricGauge { CONSENSUS_REVERTED_BATCHER_UP_TO_AND_INCLUDING, "apollo_consensus_reverted_batcher_up_to_and_including", "The block number up to which the batcher has reverted"},
    },
);

pub(crate) fn register_metrics() {
    CONSENSUS_NUM_CONNECTED_PEERS.register();
    CONSENSUS_NUM_BLACKLISTED_PEERS.register();
    CONSENSUS_PING_LATENCY.register();
    CONSENSUS_VOTES_NUM_SENT_MESSAGES.register();
    CONSENSUS_VOTES_SENT_MESSAGE_SIZE_BYTES.register();
    CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES.register();
    CONSENSUS_VOTES_RECEIVED_MESSAGE_SIZE_BYTES.register();
    CONSENSUS_VOTES_NUM_DROPPED_MESSAGES.register();
    CONSENSUS_VOTES_DROPPED_MESSAGE_SIZE_BYTES.register();
    CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES.register();
    CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES.register();
    CONSENSUS_PROPOSALS_NUM_DROPPED_MESSAGES.register();
    CONSENSUS_PROPOSALS_SENT_MESSAGE_SIZE_BYTES.register();
    CONSENSUS_PROPOSALS_RECEIVED_MESSAGE_SIZE_BYTES.register();
    CONSENSUS_PROPOSALS_DROPPED_MESSAGE_SIZE_BYTES.register();
    CONSENSUS_NETWORK_EVENTS.register();
    CONSENSUS_REVERTED_BATCHER_UP_TO_AND_INCLUDING.register();
}
