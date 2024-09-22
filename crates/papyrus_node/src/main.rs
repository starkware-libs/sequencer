use std::env::args;

use papyrus_config::ConfigError;
use papyrus_node::config::NodeConfig;
use papyrus_node::run::{run, PapyrusResources, PapyrusTaskHandles};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = NodeConfig::load_and_process(args().collect());
    if let Err(ConfigError::CommandInput(clap_err)) = config {
        clap_err.exit();
    }
    let config = config?;

    let resources = PapyrusResources::new(&config)?;
    let tasks = PapyrusTaskHandles::default();
    run(config, resources, tasks).await
}
