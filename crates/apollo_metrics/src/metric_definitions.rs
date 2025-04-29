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
                        $crate::metrics::MetricScope::$scope,
                        $key,
                        concat!($key, "{cluster=~\"$cluster\", namespace=~\"$namespace\"}"),
                        $desc
                        $(, $init)? // Only expands if `init = ...` is provided
                        $(, $labels)? // Only expands if `labels = ...` is provided
                    );
                }
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
}
