use libp2p::metrics::Registry;
use metrics::{counter, describe_counter, describe_gauge, gauge};
use prometheus_client::encoding::text::encode;
use prometheus_parse::Value;

/// libp2p uses `prometheus-client` for metrics, which updates the metrics to a `Registry`.
/// We use `metrics-exporter-prometheus` so we need to update these
/// metrics when the registry is updated.
pub fn connect_libp2p_registry_to_metrics_exporter_prometheus(prefix: String, registry: Registry) {
    tokio::spawn(async move {
        // This will update the libp2p metrics in the registry.
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            let mut sink = String::new();
            encode(&mut sink, &registry).unwrap();

            let metrics =
                prometheus_parse::Scrape::parse(sink.lines().map(|s| Ok(s.to_owned()))).unwrap();

            for sample in metrics.samples.iter() {
                let metric_name = format!("{}{}", prefix, sample.metric);

                match &sample.value {
                    Value::Counter(value) => {
                        // Register and update counter metric
                        describe_counter!(metric_name.clone(), "LibP2P counter metric");
                        #[allow(clippy::as_conversions)]
                        counter!(metric_name).absolute(*value as u64);
                    }
                    Value::Gauge(value) => {
                        // Register and update gauge metric
                        describe_gauge!(metric_name.clone(), "LibP2P gauge metric");
                        gauge!(metric_name).set(*value);
                    }
                    Value::Histogram(_) => {
                        // Skip histograms for now as they require more complex handling

                        continue;
                    }
                    Value::Summary(_) => {
                        // Skip summaries for now as they require more complex handling
                        continue;
                    }
                    Value::Untyped(value) => {
                        // Treat untyped metrics as gauges
                        describe_gauge!(metric_name.clone(), "LibP2P untyped metric");
                        gauge!(metric_name).set(*value);
                    }
                }
            }
        }
    });
}
