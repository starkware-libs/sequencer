use apollo_committer_types::communication::COMMITTER_REQUEST_LABELS;
use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
};
use apollo_metrics::define_infra_metrics;

define_infra_metrics!(committer);
