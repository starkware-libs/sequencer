use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
};
use apollo_metrics::define_infra_metrics;
use apollo_proof_manager_types::PROOF_MANAGER_REQUEST_LABELS;

// TODO(Einat): Add the proof manager metrics and panels.
define_infra_metrics!(proof_manager);
