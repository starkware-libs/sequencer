use std::fs;
use std::path::{Path, PathBuf};

use crate::mod_utils::{
    local_deployment_file_path,
    read_deployment_file,
    remove_docker_container,
    remove_docker_network,
};
use crate::pr;

pub fn stop_local_deployment(file_path: &Path) -> Result<(), String> {
    let deployment_data = read_deployment_file(file_path)?;

    // Stop all Docker containers
    if let Some(containers) = deployment_data.get("docker_containers").and_then(|c| c.as_array()) {
        for container in containers {
            if let Some(container_name) = container.as_str() {
                pr!("Stopping container {}...", container_name);
                remove_docker_container(container_name, true)?;
            }
        }
    }

    // Remove Docker network
    if let Some(network_name) = deployment_data.get("network_name").and_then(|n| n.as_str()) {
        remove_docker_network(network_name, true)?;
    }

    // Clean up temporary directory
    if let Some(tmp_dir) = deployment_data.get("tmp_dir").and_then(|t| t.as_str()) {
        pr!("Removing temporary directory {}...", tmp_dir);
        let tmp_path = PathBuf::from(tmp_dir);
        if tmp_path.exists() {
            fs::remove_dir_all(&tmp_path)
                .map_err(|e| format!("Failed to remove temporary directory: {}", e))?;
        }
    }

    // Remove deployment file
    pr!("Removing deployment file...");
    fs::remove_file(file_path).map_err(|e| format!("Failed to remove deployment file: {}", e))?;

    Ok(())
}

pub fn run() -> Result<(), String> {
    let file_path = local_deployment_file_path();
    let _deployment_data = read_deployment_file(&file_path)?;

    pr!("Stopping local network stress test...");

    stop_local_deployment(&file_path)?;

    pr!("Local network stress test stopped successfully.");
    Ok(())
}
