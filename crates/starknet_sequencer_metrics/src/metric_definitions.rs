use crate::metrics::{MetricCounter, MetricGauge, MetricScope};

/// Macro to define all metric constants for specified scopes and store them in a collection.
/// This generates:
/// - Individual metric constant according to type: `MetricCounter`or `MetricGauge` or
///   `LabeledMetricCounter`.
/// - A const array `ALL_METRICS` containing all $keys of all the metrics constants.
#[macro_export]
macro_rules! define_metrics {
    (
        $(
            $scope:ident => {
                $(
                    $type:ty { $name:ident, $key:expr, $desc:expr $(, init = $init:expr)? $(, labels = $labels:expr)? }
                ),*
                $(,)?
            }
        ),*
        $(,)?
    ) => {
        $(
            $(
                $crate::paste::paste! {
                    pub const $name: $type = <$type>::new(
                        MetricScope::$scope,
                        $key,
                        $desc
                        $(, $init)? // Only expands if `init = ...` is provided
                        $(, $labels)? // Only expands if `labels = ...` is provided
                    );
                }
            )*
        )*

        // TODO(Lev): change macro to output this only for cfg[(test,testing)].
        $(
            $crate::paste::paste! {
                pub const [<$scope:snake:upper _ALL_METRICS>]: &[&'static str] = &[
                    $(
                        $key,
                    )*
                ];
            }
        )*
    };
}

define_metrics!(
    Network => {
        // Gauges
        MetricGauge { CONSENSUS_NUM_CONNECTED_PEERS, "apollo_consensus_num_connected_peers", "The number of connected peers to the consensus p2p component" },
        MetricGauge { STATE_SYNC_P2P_NUM_CONNECTED_PEERS, "apollo_sync_num_connected_peers", "The number of connected peers to the state sync p2p component" },
        MetricGauge { STATE_SYNC_P2P_NUM_ACTIVE_INBOUND_SESSIONS, "apollo_sync_num_active_inbound_sessions", "The number of inbound sessions to the state sync p2p component" },
        MetricGauge { STATE_SYNC_P2P_NUM_ACTIVE_OUTBOUND_SESSIONS, "apollo_sync_num_active_outbound_sessions", "The number of outbound sessions to the state sync p2p component" },
        // Counters
        MetricCounter { CONSENSUS_NUM_SENT_MESSAGES, "apollo_consensus_num_sent_messages", "The number of messages sent by the consensus p2p component", init = 0 },
        MetricCounter { CONSENSUS_NUM_RECEIVED_MESSAGES, "apollo_consensus_num_received_messages", "The number of messages received by the consensus p2p component", init = 0 },
    },
);
