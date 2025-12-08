use apollo_metrics::define_metrics;

define_metrics!(
    Infra => {
        MetricGauge { RECEIVE_MESSAGE_BYTES, "receive_message_bytes", "Size of the stress test received message in bytes" },
        MetricCounter { RECEIVE_MESSAGE_COUNT, "receive_message_count", "Number of stress test messages received via broadcast", init = 0 },
        MetricCounter { RECEIVE_MESSAGE_BYTES_SUM, "receive_message_bytes_sum", "Sum of the stress test messages received via broadcast", init = 0 },
        MetricHistogram { RECEIVE_MESSAGE_DELAY_SECONDS, "receive_message_delay_seconds", "Message delay in seconds" },
        MetricHistogram { RECEIVE_MESSAGE_NEGATIVE_DELAY_SECONDS, "receive_message_negative_delay_seconds", "Negative message delay in seconds" },
    },
);
