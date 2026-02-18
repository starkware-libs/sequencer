use apollo_config_manager_types::communication::CONFIG_MANAGER_REQUEST_LABELS;
use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
};
use apollo_metrics::{define_infra_metrics, define_metrics};

define_infra_metrics!(config_manager);

define_metrics!(
    ConfigManager => {
        MetricCounter {
            CONFIG_MANAGER_UPDATE_ERRORS,
            "config_manager_update_errors",
            "Number of config manager update errors (load/validate or set dynamic config)",
            init = 0
        },
    },
);

pub(crate) fn register_metrics() {
    CONFIG_MANAGER_UPDATE_ERRORS.register();
}
