use apollo_config::ConfigError;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

use crate::component_execution_config::{
    ActiveComponentExecutionConfig,
    ExpectedComponentConfig,
    ReactiveComponentExecutionConfig,
};

// TODO(Tsabary): consider adding hierarchical structure to the components config based on
// active/reactive components.

pub trait ValidateTxIngestionComponentsDisabled {
    /// Validates that all tx-ingestion components (gateway, http_server, mempool, mempool_p2p) are
    /// disabled, as required for validation-only nodes.
    fn validate_tx_ingestion_components_disabled(&self) -> Result<(), ConfigError>;
}

/// The components configuration.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct ComponentConfig {
    // Reactive component configs.
    #[validate(nested)]
    pub batcher: ReactiveComponentExecutionConfig,
    #[validate(nested)]
    pub class_manager: ReactiveComponentExecutionConfig,
    #[validate(nested)]
    pub committer: ReactiveComponentExecutionConfig,
    #[validate(nested)]
    pub config_manager: ReactiveComponentExecutionConfig,
    #[validate(nested)]
    pub gateway: ReactiveComponentExecutionConfig,
    #[validate(nested)]
    pub l1_events_provider: ReactiveComponentExecutionConfig,
    #[validate(nested)]
    pub l1_gas_price_provider: ReactiveComponentExecutionConfig,
    #[validate(nested)]
    pub mempool: ReactiveComponentExecutionConfig,
    #[validate(nested)]
    pub mempool_p2p: ReactiveComponentExecutionConfig,
    #[validate(nested)]
    pub proof_manager: ReactiveComponentExecutionConfig,
    #[validate(nested)]
    pub sierra_compiler: ReactiveComponentExecutionConfig,
    #[validate(nested)]
    pub signature_manager: ReactiveComponentExecutionConfig,
    #[validate(nested)]
    pub state_sync: ReactiveComponentExecutionConfig,

    // Active component configs.
    #[validate(nested)]
    pub consensus_manager: ActiveComponentExecutionConfig,
    #[validate(nested)]
    pub http_server: ActiveComponentExecutionConfig,
    #[validate(nested)]
    pub l1_events_scraper: ActiveComponentExecutionConfig,
    #[validate(nested)]
    pub l1_gas_price_scraper: ActiveComponentExecutionConfig,
    #[validate(nested)]
    pub monitoring_endpoint: ActiveComponentExecutionConfig,
}

impl ComponentConfig {
    pub fn disabled() -> ComponentConfig {
        ComponentConfig {
            batcher: ReactiveComponentExecutionConfig::disabled(),
            class_manager: ReactiveComponentExecutionConfig::disabled(),
            committer: ReactiveComponentExecutionConfig::disabled(),
            config_manager: ReactiveComponentExecutionConfig::disabled(),
            consensus_manager: ActiveComponentExecutionConfig::disabled(),
            http_server: ActiveComponentExecutionConfig::disabled(),
            gateway: ReactiveComponentExecutionConfig::disabled(),
            l1_events_provider: ReactiveComponentExecutionConfig::disabled(),
            l1_gas_price_provider: ReactiveComponentExecutionConfig::disabled(),
            l1_events_scraper: ActiveComponentExecutionConfig::disabled(),
            l1_gas_price_scraper: ActiveComponentExecutionConfig::disabled(),
            mempool: ReactiveComponentExecutionConfig::disabled(),
            mempool_p2p: ReactiveComponentExecutionConfig::disabled(),
            monitoring_endpoint: ActiveComponentExecutionConfig::disabled(),
            proof_manager: ReactiveComponentExecutionConfig::disabled(),
            sierra_compiler: ReactiveComponentExecutionConfig::disabled(),
            signature_manager: ReactiveComponentExecutionConfig::disabled(),
            state_sync: ReactiveComponentExecutionConfig::disabled(),
        }
    }

    /// Resolves the url of every reactive component.
    pub fn validate_urls(&self) -> Result<(), ValidationError> {
        // Destructure exhaustively (no `..`) so a new component must be classified here.
        let Self {
            batcher,
            class_manager,
            committer,
            config_manager,
            gateway,
            l1_events_provider,
            l1_gas_price_provider,
            mempool,
            mempool_p2p,
            proof_manager,
            sierra_compiler,
            signature_manager,
            state_sync,
            consensus_manager: _,
            http_server: _,
            l1_events_scraper: _,
            l1_gas_price_scraper: _,
            monitoring_endpoint: _,
        } = self;

        let reactive_components = [
            ("batcher", batcher),
            ("class_manager", class_manager),
            ("committer", committer),
            ("config_manager", config_manager),
            ("gateway", gateway),
            ("l1_events_provider", l1_events_provider),
            ("l1_gas_price_provider", l1_gas_price_provider),
            ("mempool", mempool),
            ("mempool_p2p", mempool_p2p),
            ("proof_manager", proof_manager),
            ("sierra_compiler", sierra_compiler),
            ("signature_manager", signature_manager),
            ("state_sync", state_sync),
        ];

        let failed_component_urls: Vec<String> = reactive_components
            .into_iter()
            .filter_map(|(name, component)| {
                component.validate_url().err().map(|error| format!("components.{name}: {error}"))
            })
            .collect();

        if failed_component_urls.is_empty() {
            Ok(())
        } else {
            Err(ValidationError::new("Failed to resolve url")
                .with_message(failed_component_urls.join("; ").into()))
        }
    }

    #[cfg(any(feature = "testing", test))]
    pub fn set_urls_to_localhost(&mut self) {
        self.batcher.set_url_to_localhost();
        self.class_manager.set_url_to_localhost();
        self.committer.set_url_to_localhost();
        self.config_manager.set_url_to_localhost();
        self.gateway.set_url_to_localhost();
        self.l1_events_provider.set_url_to_localhost();
        self.l1_gas_price_provider.set_url_to_localhost();
        self.mempool.set_url_to_localhost();
        self.mempool_p2p.set_url_to_localhost();
        self.proof_manager.set_url_to_localhost();
        self.sierra_compiler.set_url_to_localhost();
        self.signature_manager.set_url_to_localhost();
        self.state_sync.set_url_to_localhost();
    }
}

impl ValidateTxIngestionComponentsDisabled for ComponentConfig {
    fn validate_tx_ingestion_components_disabled(&self) -> Result<(), ConfigError> {
        let checks = [
            ("gateway", self.gateway.is_disabled()),
            ("http_server", self.http_server.is_disabled()),
            ("mempool", self.mempool.is_disabled()),
            ("mempool_p2p", self.mempool_p2p.is_disabled()),
        ];
        for (name, disabled) in checks {
            if !disabled {
                return Err(ConfigError::ComponentConfigMismatch {
                    component_config_mismatch: format!("{name} must be disabled"),
                });
            }
        }
        Ok(())
    }
}

#[cfg(any(feature = "testing", test))]
pub fn set_urls_to_localhost<'a>(
    component_configs: impl IntoIterator<Item = &'a mut ComponentConfig>,
) {
    for config in component_configs {
        config.set_urls_to_localhost();
    }
}
