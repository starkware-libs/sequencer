use std::collections::BTreeSet;
use std::future::pending;
use std::path::PathBuf;

use apollo_config::presentation::get_config_presentation;
use apollo_config::validators::validate_path_exists;
use apollo_config::{CONFIG_FILE_ARG, CONFIG_FILE_SHORT_ARG_NAME};
use apollo_config_manager_config::config::ConfigManagerConfig;
use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_infra::component_definitions::{default_component_start_fn, ComponentStarter};
use apollo_infra::component_server::WrapperServer;
use apollo_node_config::config_utils::load_and_validate_config;
use apollo_node_config::node_config::NodeDynamicConfig;
use async_trait::async_trait;
use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration as TokioDuration, Interval};

const FS_EVENT_CHANNEL_CAPACITY: usize = 16;
use tracing::{error, info};

#[cfg(test)]
#[path = "config_manager_runner_tests.rs"]
pub mod config_manager_runner_tests;

pub struct ConfigManagerRunner {
    config_manager_config: ConfigManagerConfig,
    config_manager_client: SharedConfigManagerClient,
    latest_node_dynamic_config: NodeDynamicConfig,
    cli_args: Vec<String>,
}

#[async_trait]
impl ComponentStarter for ConfigManagerRunner {
    async fn start(&mut self) {
        default_component_start_fn::<Self>().await;

        info!("ConfigManagerRunner: starting filesystem watcher");

        if self.config_manager_config.enable_config_updates {
            let update_interval = interval(TokioDuration::from_secs_f64(
                self.config_manager_config.config_update_interval_secs,
            ));

            if let Err(e) = self.run_watcher_loop(update_interval).await {
                error!("ConfigManagerRunner: watcher terminated with error: {e}");
            }
        } else {
            // Avoid returning, as this fn is expected to run perpetually.
            pending::<()>().await;
        }
    }
}

impl ConfigManagerRunner {
    pub fn new(
        config_manager_config: ConfigManagerConfig,
        config_manager_client: SharedConfigManagerClient,
        initial_node_dynamic_config: NodeDynamicConfig,
        cli_args: Vec<String>,
    ) -> Self {
        Self {
            config_manager_config,
            config_manager_client,
            latest_node_dynamic_config: initial_node_dynamic_config,
            cli_args,
        }
    }

    /// Monitors config files for changes via file system events and periodic polling.
    async fn run_watcher_loop(&mut self, mut update_interval: Interval) {
        // Channel to receive events in async context.
        let (tx, mut rx) = mpsc::channel(FS_EVENT_CHANNEL_CAPACITY);

        // Build watcher that sends into the channel.
        let mut watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.blocking_send(res);
            },
            NotifyConfig::default(),
        )
        .expect("Failed to create file watcher");

        let config_file_paths = Self::extract_config_file_paths(&self.cli_args);
        for path in config_file_paths {
            watcher.watch(&path, RecursiveMode::NonRecursive)?;
        }

        loop {
            tokio::select! {
                // File system event
                Some(event) = rx.recv() => {
                    match event {
                        Ok(ev) => match ev.kind {
                            EventKind::Modify(_)
                            | EventKind::Create(_)
                            | EventKind::Remove(_)
                            | EventKind::Other => {
                                info!("ConfigManagerRunner: file change detected, updating config");
                                if let Err(e) = self.update_config().await {
                                    error!("ConfigManagerRunner: failed to update config: {e}");
                                }
                            }
                            _ => {}
                        },
                        Err(e) => error!("ConfigManagerRunner: watcher error: {e}"),
                    }
                }
                // Periodic tick
                _ = update_interval.tick() => {
                    info!("ConfigManagerRunner: periodic check triggered, updating config");
                    if let Err(e) = self.update_config().await {
                        error!("ConfigManagerRunner: failed to update config: {e}");
                    }
                }
            }
        }
    }

    // TODO(Nadin): Define a proper result type instead of Box<dyn std::error::Error + Send + Sync>
    pub(crate) async fn update_config(
        &mut self,
    ) -> Result<NodeDynamicConfig, Box<dyn std::error::Error + Send + Sync>> {
        let config = load_and_validate_config(self.cli_args.clone(), false)?;
        let node_dynamic_config = NodeDynamicConfig::from(&config);

        // Compare the previous and the newly read node dynamic config.
        if self.latest_node_dynamic_config == node_dynamic_config {
            // No change, so no action is needed.
            Ok(node_dynamic_config)
        } else {
            // Log the diff between the latest and the new node dynamic config.
            self.log_config_diff(&self.latest_node_dynamic_config, &node_dynamic_config);
            // Update the latest node dynamic config.
            self.latest_node_dynamic_config = node_dynamic_config.clone();
            match self
                .config_manager_client
                .set_node_dynamic_config(node_dynamic_config.clone())
                .await
            {
                Ok(()) => {
                    info!("Successfully updated dynamic config");
                    Ok(node_dynamic_config)
                }
                Err(e) => {
                    error!("Failed to update dynamic config: {:?}", e);
                    Err(format!("Failed to update dynamic config: {:?}", e).into())
                }
            }
        }
    }

    fn log_config_diff(&self, old_config: &NodeDynamicConfig, new_config: &NodeDynamicConfig) {
        let old_config = get_config_presentation(old_config, false).unwrap();
        let new_config = get_config_presentation(new_config, false).unwrap();
        let all_keys: BTreeSet<_> = old_config
            .as_object()
            .unwrap()
            .keys()
            .chain(new_config.as_object().unwrap().keys())
            .collect();

        for key in all_keys {
            let old_value = old_config.as_object().unwrap().get(key).unwrap_or(&Value::Null);
            let new_value = new_config.as_object().unwrap().get(key).unwrap_or(&Value::Null);

            if old_value != new_value {
                info!("ConfigManagerRunner: {key} changed from {old_value} to {new_value}");
            }
        }
    }

    /// Extracts config file paths from CLI arguments.
    /// Expects the format: --config_file path1 --config_file path2 ...
    fn extract_config_file_paths(args: &[String]) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        let mut i = 0;

        while i < args.len() {
            if args[i] == CONFIG_FILE_ARG || args[i] == format!("-{}", CONFIG_FILE_SHORT_ARG_NAME) {
                // Next arg is the path
                i += 1;
                if i < args.len() {
                    validate_path_exists(&args[i]).expect("Config file path does not exist");
                    paths.push(PathBuf::from(&args[i]));
                }
            }
            i += 1;
        }

        paths
    }
}

pub type ConfigManagerRunnerServer = WrapperServer<ConfigManagerRunner>;
