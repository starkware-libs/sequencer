use apollo_metrics::define_metrics;

define_metrics!(
    ConsensusManager => {
        // topic agnostic metrics
        MetricGauge { CONSENSUS_NUM_CONNECTED_PEERS, "apollo_consensus_num_connected_peers", "The number of connected peers to the consensus p2p component" },
        MetricGauge { CONSENSUS_NUM_BLACKLISTED_PEERS, "apollo_consensus_num_blacklisted_peers", "The number of currently blacklisted peers by the consensus component" },

        // Votes topic metrics
        MetricCounter { CONSENSUS_VOTES_NUM_SENT_MESSAGES, "apollo_consensus_votes_num_sent_messages", "The number of messages sent by the consensus p2p component over the Votes topic", init = 0 },
        MetricCounter { CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES, "apollo_consensus_votes_num_received_messages", "The number of messages received by the consensus p2p component over the Votes topic", init = 0 },
        MetricCounter { CONSENSUS_VOTES_NUM_DROPPED_MESSAGES, "apollo_consensus_votes_num_dropped_messages", "The number of messages dropped by the consensus p2p component over the Votes topic", init = 0 },

        // Proposals topic metrics
        MetricCounter { CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES, "apollo_consensus_proposals_num_sent_messages", "The number of messages sent by the consensus p2p component over the Proposals topic", init = 0 },
        MetricCounter { CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES, "apollo_consensus_proposals_num_received_messages", "The number of messages received by the consensus p2p component over the Proposals topic", init = 0 },
        MetricCounter { CONSENSUS_PROPOSALS_NUM_DROPPED_MESSAGES, "apollo_consensus_proposals_num_dropped_messages", "The number of messages dropped by the consensus p2p component over the Proposals topic", init = 0 },

        // Event metrics
        MetricCounter { CONSENSUS_CONNECTIONS_ESTABLISHED, "apollo_consensus_connections_established", "The number of connections established by the consensus p2p component", init = 0 },
        MetricCounter { CONSENSUS_CONNECTIONS_CLOSED, "apollo_consensus_connections_closed", "The number of connections closed by the consensus p2p component", init = 0 },
        MetricCounter { CONSENSUS_DIAL_FAILURE, "apollo_consensus_dial_failure", "The number of dial failures by the consensus p2p component", init = 0 },
        MetricCounter { CONSENSUS_LISTEN_FAILURE, "apollo_consensus_listen_failure", "The number of listen failures by the consensus p2p component", init = 0 },
        MetricCounter { CONSENSUS_LISTEN_ERROR, "apollo_consensus_listen_error", "The number of listen errors by the consensus p2p component", init = 0 },
        MetricCounter { CONSENSUS_ADDRESS_CHANGE, "apollo_consensus_address_change", "The number of address changes by the consensus p2p component", init = 0 },
        MetricCounter { CONSENSUS_NEW_LISTENERS, "apollo_consensus_new_listeners", "The number of new listeners by the consensus p2p component", init = 0 },
        MetricCounter { CONSENSUS_NEW_LISTEN_ADDRS, "apollo_consensus_new_listen_addrs", "The number of new listen addresses by the consensus p2p component", init = 0 },
        MetricCounter { CONSENSUS_EXPIRED_LISTEN_ADDRS, "apollo_consensus_expired_listen_addrs", "The number of expired listen addresses by the consensus p2p component", init = 0 },
        MetricCounter { CONSENSUS_LISTENER_CLOSED, "apollo_consensus_listener_closed", "The number of listeners closed by the consensus p2p component", init = 0 },
        MetricCounter { CONSENSUS_NEW_EXTERNAL_ADDR_CANDIDATE, "apollo_consensus_new_external_addr_candidate", "The number of new external address candidates by the consensus p2p component", init = 0 },
        MetricCounter { CONSENSUS_EXTERNAL_ADDR_CONFIRMED, "apollo_consensus_external_addr_confirmed", "The number of external addresses confirmed by the consensus p2p component", init = 0 },
        MetricCounter { CONSENSUS_EXTERNAL_ADDR_EXPIRED, "apollo_consensus_external_addr_expired", "The number of external addresses expired by the consensus p2p component", init = 0 },
        MetricCounter { CONSENSUS_NEW_EXTERNAL_ADDR_OF_PEER, "apollo_consensus_new_external_addr_of_peer", "The number of new external addresses of peers by the consensus p2p component", init = 0 },
        MetricCounter { CONSENSUS_INBOUND_CONNECTIONS_HANDLED, "apollo_consensus_inbound_connections_handled", "The number of inbound connections handled by the consensus p2p component", init = 0 },
        MetricCounter { CONSENSUS_OUTBOUND_CONNECTIONS_HANDLED, "apollo_consensus_outbound_connections_handled", "The number of outbound connections handled by the consensus p2p component", init = 0 },
        MetricCounter { CONSENSUS_CONNECTION_HANDLER_EVENTS, "apollo_consensus_connection_handler_events", "The number of connection handler events by the consensus p2p component", init = 0 },

    },
);
