use starknet_sequencer_metrics::metrics::{LabeledMetricCounter, MetricCounter, MetricGauge};
use starknet_sequencer_metrics::{define_metrics, generate_permutation_labels};

define_metrics!(
    Consensus => {
        // Gauges
        MetricGauge { CONSENSUS_NUM_CONNECTED_PEERS, "apollo_consensus_num_connected_peers", "The number of connected peers to the consensus p2p component" },
        // Counters
        MetricCounter { CONSENSUS_NUM_SENT_MESSAGES, "apollo_consensus_num_sent_messages", "The number of messages sent by the consensus p2p component", init = 0 },
        MetricCounter { CONSENSUS_NUM_RECEIVED_MESSAGES, "apollo_consensus_num_received_messages", "The number of messages received by the consensus p2p component", init = 0 },

        // Labeled Counters (require changing topic to an enum)
        LabeledMetricCounter { CONSENSUS_NUM_SENT_MESSAGES, "apollo_consensus_num_sent_messages", "The number of messages sent by the consensus p2p component", init = 0, labels = BROADCAST_TOPIC_LABELS },
        LabeledMetricCounter { CONSENSUS_NUM_RECEIVED_MESSAGES, "apollo_consensus_num_received_messages", "The number of messages received by the consensus p2p component", init = 0, labels = BROADCAST_TOPIC_LABELS },
    },
);

pub const LABEL_NAME_BROADCAST_TOPIC: &str = "broadcast_topic";

pub(crate) enum BroadcastTopic {
    MempoolP2p,
    Consensus,
}

generate_permutation_labels! {
    BROADCAST_TOPIC_LABELS,
    (LABEL_NAME_BROADCAST_TOPIC, BroadcastTopic),
}
