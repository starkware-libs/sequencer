use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
};
use apollo_metrics::define_infra_metrics;
use apollo_proof_manager_types::PROOF_MANAGER_REQUEST_LABELS;

define_infra_metrics!(proof_manager);
