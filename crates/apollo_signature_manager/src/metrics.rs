use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
};
use apollo_metrics::define_infra_metrics;
use apollo_signature_manager_types::SIGNATURE_MANAGER_REQUEST_LABELS;

define_infra_metrics!(signature_manager);
