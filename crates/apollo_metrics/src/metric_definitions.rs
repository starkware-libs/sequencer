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

    // Special case: MetricHistogram
    (@define_single $scope:ident, MetricHistogram, $name:ident, $key:expr, $desc:expr
        $(, init = $init:expr)? $(, labels = $labels:expr)?
    ) => {
        $crate::paste::paste! {
            pub const $name: $crate::metrics::MetricHistogram = $crate::metrics::MetricHistogram::new(
                $crate::metrics::MetricScope::$scope,
                $key,
                concat!($key, $crate::metric_label_filter!()),
                concat!($key, "_sum", $crate::metric_label_filter!()),
                concat!($key, "_count", $crate::metric_label_filter!()),
                $desc
                $(, $init)?
                $(, $labels)?
            );
        }
    };

    // TODO(Tsabary): add a similar support for LabeledMetricHistogram.

    // Fallback: all others (MetricCounter, MetricGauge, etc.)
    (@define_single $scope:ident, $type:ident, $name:ident, $key:expr, $desc:expr
        $(, init = $init:expr)? $(, labels = $labels:expr)?
    ) => {
        $crate::paste::paste! {
            pub const $name: $crate::metrics::$type = $crate::metrics::$type::new(
                $crate::metrics::MetricScope::$scope,
                $key,
                concat!($key, $crate::metric_label_filter!()),
                $desc
                $(, $init)?
                $(, $labels)?
            );
        }
    };
}
