use starknet_sequencer_metrics::define_metrics;
use starknet_sequencer_metrics::metrics::{MetricCounter, MetricGauge};

define_metrics!(
    Consensus => {
        // Gauges
        MetricGauge { CONSENSUS_NUM_CONNECTED_PEERS, "apollo_consensus_num_connected_peers", "The number of connected peers to the consensus p2p component" },
        MetricGauge { CONSENSUS_NUM_BLACKLISTED_PEERS, "apollo_consensus_num_blacklisted_peers", "The number of currently blacklisted peers by the consensus component" },
        // Counters
        MetricCounter { CONSENSUS_NUM_SENT_MESSAGES, "apollo_consensus_num_sent_messages", "The number of messages sent by the consensus p2p component", init = 0 },
        MetricCounter { CONSENSUS_NUM_RECEIVED_MESSAGES, "apollo_consensus_num_received_messages", "The number of messages received by the consensus p2p component", init = 0 },
    },
);
