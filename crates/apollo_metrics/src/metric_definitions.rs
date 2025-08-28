#[macro_export]
macro_rules! metric_label_filter {
    () => {
        "{cluster=~\"$cluster\", namespace=~\"$namespace\"}"
    };
}

/// Macro to define all metric constants for specified scopes and store them in a collection.
/// This generates:
/// - Individual metric constant according to type:
///     - `MetricCounter`
///     - `MetricGauge`
///     - `MetricHistogram`
///     - `LabeledMetricCounter`
///     - `LabeledMetricGauge`
///     - `LabeledMetricHistogram`
/// - A const array `ALL_METRICS` containing all $keys of all the metrics constants.
#[macro_export]
macro_rules! define_metrics {
    (
        $(
            $scope:ident => { // Metric scope, e.g., Infra, Sequencer, etc.
                $(
                    $type:ident { // Metric type, e.g., MetricCounter, MetricGauge, etc.
                        $name:ident, // Metric name, e.g., MEMPOOL_TRANSACTIONS_COMMITTED
                        $key:expr, // Metric key, e.g., "mempool_txs_committed"
                        $desc:expr // Metric description, e.g., "The number of transactions that were committed to block"
                        $(, init = $init:expr)? // Optional initialization value for counters and gauges
                        $(, labels = $labels:expr)? // Optional labels for labeled metrics
                    }
                ),*
                $(,)?
            }
        ),*
        $(,)?
    ) => {
        $(
            $(
                $crate::define_metrics!(@define_single
                    $scope, $type, $name, $key, $desc $(, init = $init)? $(, labels = $labels)?
                );
            )*
        )*

        $(
            #[cfg(any(feature = "testing", test))]
            $crate::paste::paste! {
                pub const [<$scope:snake:upper _ALL_METRICS>]: &[&'static str] = &[
                    $(
                        $key,
                    )*
                ];
            }
        )*
    };

    (@define_single $scope:ident, $type:ident, $name:ident, $key:expr, $desc:expr
        $(, init = $init:expr)? $(, labels = $labels:expr)?
    ) => {
        $crate::paste::paste! {
            pub const $name: $crate::metrics::$type = $crate::metrics::$type::new(
                $crate::metrics::MetricScope::$scope,
                $key,
                $desc
                $(, $init)?
                $(, $labels)?
            );
        }
    };
}

#[macro_export]
macro_rules! define_infra_metrics {
    ($component:ident) => {
        $crate::paste::paste! {
            $crate::define_metrics!(
                Infra => {
                    LabeledMetricHistogram {
                        [<$component:snake:upper _LABELED_PROCESSING_TIMES_SECS>],
                        stringify!([<$component:snake _labeled_processing_times_secs>]),
                        concat!("Request processing times of the ", stringify!([<$component:snake>]), ", per label (secs)"),
                        labels = [<$component:snake:upper _REQUEST_LABELS>]
                    },
                    LabeledMetricHistogram {
                        [<$component:snake:upper _LABELED_QUEUEING_TIMES_SECS>],
                        stringify!([<$component:snake _labeled_queueing_times_secs>]),
                        concat!("Request queueing times of the ", stringify!([<$component:snake>]), ", per label (secs)"),
                        labels = [<$component:snake:upper _REQUEST_LABELS>]
                    },
                    LabeledMetricHistogram {
                        [<$component:snake:upper _LABELED_LOCAL_RESPONSE_TIMES_SECS>],
                        stringify!([<$component:snake _labeled_local_response_times_secs>]),
                        concat!("Request local response times of the ", stringify!([<$component:snake>]), ", per label (secs)"),
                        labels = [<$component:snake:upper _REQUEST_LABELS>]
                    },
                    LabeledMetricHistogram {
                        [<$component:snake:upper _LABELED_REMOTE_RESPONSE_TIMES_SECS>],
                        stringify!([<$component:snake _labeled_remote_response_times_secs>]),
                        concat!("Request remote response times of the ", stringify!([<$component:snake>]), ", per label (secs)"),
                        labels = [<$component:snake:upper _REQUEST_LABELS>]
                    },
                    LabeledMetricHistogram {
                        [<$component:snake:upper _LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS>],
                        stringify!([<$component:snake _labeled_remote_client_communication_failure_times_secs>]),
                        concat!("Request communication failure times of the ", stringify!([<$component:snake>]), ", per label (secs)"),
                        labels = [<$component:snake:upper _REQUEST_LABELS>]
                    },

                    MetricCounter {
                        [<$component:snake:upper _LOCAL_MSGS_RECEIVED>],
                        stringify!([<$component:snake _local_msgs_received>]),
                        concat!("Counter of messages received by ", stringify!([<$component:snake>]), " local server"),
                        init = 0
                    },
                    MetricCounter {
                        [<$component:snake:upper _LOCAL_MSGS_PROCESSED>],
                        stringify!([<$component:snake _local_msgs_processed>]),
                        concat!("Counter of messages processed by ", stringify!([<$component:snake>]), " local server"),
                        init = 0
                    },
                    MetricCounter {
                        [<$component:snake:upper _REMOTE_MSGS_RECEIVED>],
                        stringify!([<$component:snake _remote_msgs_received>]),
                        concat!("Counter of messages received by ", stringify!([<$component:snake>]), " remote server"),
                        init = 0
                    },
                    MetricCounter {
                        [<$component:snake:upper _REMOTE_VALID_MSGS_RECEIVED>],
                        stringify!([<$component:snake _remote_valid_msgs_received>]),
                        concat!("Counter of valid messages received by ", stringify!([<$component:snake>]), " remote server"),
                        init = 0
                    },
                    MetricCounter {
                        [<$component:snake:upper _REMOTE_MSGS_PROCESSED>],
                        stringify!([<$component:snake _remote_msgs_processed>]),
                        concat!("Counter of messages processed by ", stringify!([<$component:snake>]), " remote server"),
                        init = 0
                    },
                    MetricGauge {
                        [<$component:snake:upper _REMOTE_NUMBER_OF_CONNECTIONS>],
                        stringify!([<$component:snake _remote_number_of_connections>]),
                        concat!("Number of connections to ", stringify!([<$component:snake>]), " remote server")
                    },
                    MetricGauge {
                        [<$component:snake:upper _LOCAL_QUEUE_DEPTH>],
                        stringify!([<$component:snake _local_queue_depth>]),
                        concat!("The depth of the ", stringify!([<$component:snake>]), "'s local message queue")
                    },
                    MetricHistogram {
                        [<$component:snake:upper _REMOTE_CLIENT_SEND_ATTEMPTS>],
                        stringify!([<$component:snake _remote_client_send_attempts>]),
                        concat!("Required number of remote connection attempts made by a ", stringify!([<$component:snake>]), " remote client")
                    },
                },
            );

            pub const [<$component:snake:upper _INFRA_METRICS>]: InfraMetrics = InfraMetrics::new(
                LocalClientMetrics::new(
                    &[<$component:snake:upper _LABELED_LOCAL_RESPONSE_TIMES_SECS>],
                ),
                RemoteClientMetrics::new(
                    &[<$component:snake:upper _REMOTE_CLIENT_SEND_ATTEMPTS>],
                    &[<$component:snake:upper _LABELED_REMOTE_RESPONSE_TIMES_SECS>],
                    &[<$component:snake:upper _LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS>],
                ),
                LocalServerMetrics::new(
                    &[<$component:snake:upper _LOCAL_MSGS_RECEIVED>],
                    &[<$component:snake:upper _LOCAL_MSGS_PROCESSED>],
                    &[<$component:snake:upper _LOCAL_QUEUE_DEPTH>],
                    &[<$component:snake:upper _LABELED_PROCESSING_TIMES_SECS>],
                    &[<$component:snake:upper _LABELED_QUEUEING_TIMES_SECS>],
                ),
                RemoteServerMetrics::new(
                    &[<$component:snake:upper _REMOTE_MSGS_RECEIVED>],
                    &[<$component:snake:upper _REMOTE_VALID_MSGS_RECEIVED>],
                    &[<$component:snake:upper _REMOTE_MSGS_PROCESSED>],
                    &[<$component:snake:upper _REMOTE_NUMBER_OF_CONNECTIONS>],
                ),
            );
        }
    };
}
