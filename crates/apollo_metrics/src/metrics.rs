use std::fmt::Debug;
use std::sync::OnceLock;

use crate::metric_definitions::METRIC_LABEL_FILTER;

#[cfg(test)]
#[path = "metrics_test.rs"]
mod metrics_tests;

mod counters;
mod gauges;
mod histograms;

// re exports
pub use crate::metrics::counters::{LabeledMetricCounter, MetricCounter};
pub use crate::metrics::gauges::{LabeledMetricGauge, MetricGauge};
pub use crate::metrics::histograms::{HistogramValue, LabeledMetricHistogram, MetricHistogram};

/// Global variable set by the main config to enable collecting profiling metrics.
pub static COLLECT_SEQUENCER_PROFILING_METRICS: OnceLock<bool> = OnceLock::new();

/// Relevant components for which metrics can be defined.
#[derive(Clone, Copy, Debug)]
pub enum MetricScope {
    Batcher,
    Blockifier,
    ClassManager,
    Consensus,
    ConsensusManager,
    ConsensusOrchestrator,
    Gateway,
    HttpServer,
    Infra,
    L1GasPrice,
    L1Provider,
    Mempool,
    MempoolP2p,
    CompileToCasm,
    StateSync,
    Storage,
    Tokio,
}

// Inner struct used to define a metric of any type.
#[derive(Clone, Debug)]
struct Metric {
    scope: MetricScope,
    name: &'static str,
    description: &'static str,
}

impl Metric {
    pub const fn new(scope: MetricScope, name: &'static str, description: &'static str) -> Self {
        Self { scope, name, description }
    }
}

// Access common metric details.
pub trait MetricDetails {
    fn get_name(&self) -> &'static str;
    fn get_scope(&self) -> MetricScope;
    fn get_description(&self) -> &'static str;
}

// Access specific metric PromQL filtering query, differing between the various metric types.
pub trait MetricQueryName: MetricDetails {
    fn get_name_with_filter(&self) -> String;
}

// An enum to distinguish between the various metric types, used to set the appropriate query.
#[derive(Copy, Clone)]
pub(crate) enum MetricFilterKind {
    CounterOrGauge,
    Histogram,
}

pub(crate) trait HasMetricFilterKind: MetricDetails {
    const FILTER_KIND: MetricFilterKind;
}

impl<T: HasMetricFilterKind> MetricQueryName for T {
    fn get_name_with_filter(&self) -> String {
        match T::FILTER_KIND {
            MetricFilterKind::CounterOrGauge => {
                format!("{}{METRIC_LABEL_FILTER}", self.get_name())
            }
            MetricFilterKind::Histogram => {
                format!("{}_bucket{METRIC_LABEL_FILTER}", self.get_name())
            }
        }
    }
}

impl MetricDetails for Metric {
    fn get_name(&self) -> &'static str {
        self.name
    }

    fn get_scope(&self) -> MetricScope {
        self.scope
    }

    fn get_description(&self) -> &'static str {
        self.description
    }
}

trait HasMetricDetails {
    type InnerMetricDetails: MetricDetails;
    fn get_metric_description(&self) -> &Self::InnerMetricDetails;
}

impl<T> MetricDetails for T
where
    T: HasMetricDetails,
{
    fn get_name(&self) -> &'static str {
        self.get_metric_description().get_name()
    }

    fn get_scope(&self) -> MetricScope {
        self.get_metric_description().get_scope()
    }

    fn get_description(&self) -> &'static str {
        self.get_metric_description().get_description()
    }
}

/// An object which can be lossy converted into a `f64` representation.
pub trait LossyIntoF64 {
    fn into_f64(self) -> f64;
}

impl LossyIntoF64 for f64 {
    fn into_f64(self) -> f64 {
        self
    }
}

macro_rules! into_f64 {
    ($($ty:ty),*) => {
        $(
            impl LossyIntoF64 for $ty {
                #[allow(clippy::as_conversions)]
                fn into_f64(self) -> f64 {
                    self as f64
                }
            }
        )*
    };
}
into_f64!(u64, usize, i64, u128);
