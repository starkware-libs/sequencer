use crate::mod_utils::{
    cluster_deployment_file_path,
    cluster_deployment_working_directory,
    connect_to_cluster,
    create_namespace,
    delete_namespace,
    read_deployment_file,
    remove_directory,
};
use crate::pr;

pub fn run() -> Result<(), String> {
    let file_path = cluster_deployment_file_path();
    let deployment_data = read_deployment_file(&file_path)?;

    if let Some(namespace_name) = deployment_data.get("namespace").and_then(|v| v.as_str()) {
        connect_to_cluster()?;

        // Remove and re-create the namespace to ensure a clean state
        delete_namespace(namespace_name, true)?;
        create_namespace(namespace_name)?;
        delete_namespace(namespace_name, false)?;
    }

    let cluster_dir = cluster_deployment_working_directory();
    remove_directory(&cluster_dir, false)?;

    pr!("Network stress test stopped successfully.");
    Ok(())
}
