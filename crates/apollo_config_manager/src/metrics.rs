use apollo_metrics::define_metrics;

define_metrics!(
    ConfigManager => {
        MetricCounter {
            CONFIG_MANAGER_UPDATE_ERRORS,
            "config_manager_update_errors",
            "Number of config manager update errors (load/validate)",
            init = 0
        },
    },
);

pub(crate) fn register_metrics() {
    CONFIG_MANAGER_UPDATE_ERRORS.register();
}
