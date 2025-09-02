use apollo_config_manager_types::communication::CONFIG_MANAGER_REQUEST_LABELS;
use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
};
use apollo_metrics::define_infra_metrics;

define_infra_metrics!(config_manager);
