use starknet_sequencer_metrics::define_metrics;
use starknet_sequencer_metrics::metrics::{MetricCounter, MetricGauge};

define_metrics!(
    Consensus => {
        // topic agnostic metrics
        MetricGauge { CONSENSUS_NUM_CONNECTED_PEERS, "apollo_consensus_num_connected_peers", "The number of connected peers to the consensus p2p component" },
        MetricGauge { CONSENSUS_NUM_BLACKLISTED_PEERS, "apollo_consensus_num_blacklisted_peers", "The number of currently blacklisted peers by the consensus component" },

        // Votes topic metrics
        MetricCounter { CONSENSUS_VOTES_NUM_SENT_MESSAGES, "apollo_consensus_votes_num_sent_messages", "The number of messages sent by the consensus p2p component over the Votes topic", init = 0 },
        MetricCounter { CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES, "apollo_consensus_votes_num_received_messages", "The number of messages received by the consensus p2p component over the Votes topic", init = 0 },

        // Proposals topic metrics
        MetricCounter { CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES, "apollo_consensus_proposals_num_sent_messages", "The number of messages sent by the consensus p2p component over the Proposals topic", init = 0 },
        MetricCounter { CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES, "apollo_consensus_proposals_num_received_messages", "The number of messages received by the consensus p2p component over the Proposals topic", init = 0 },

    },
);
