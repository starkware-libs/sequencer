use std::fs;

use crate::mod_utils::{local_deployment_working_directory, run_cmd};
use crate::pr;

pub fn run() -> anyhow::Result<()> {
    let working_dir = local_deployment_working_directory()?;
    let compose_file = working_dir.join("docker-compose.json");
    anyhow::ensure!(compose_file.exists(), "No local deployment found");

    pr!("Stopping local network stress test...");
    run_cmd(
        &format!("docker compose -f {} down -v", compose_file.display()),
        "Make sure Docker Compose is installed.",
        false,
    )?;

    fs::remove_dir_all(&working_dir).ok();

    pr!("Local network stress test stopped successfully.");
    Ok(())
}
