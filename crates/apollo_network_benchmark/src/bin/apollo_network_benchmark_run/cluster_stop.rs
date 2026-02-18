use crate::mod_utils::{
    cluster_deployment_file_path,
    cluster_deployment_working_directory,
    connect_to_cluster,
    create_namespace,
    delete_namespace,
    get_deployment_namespace,
    read_deployment_file,
    remove_directory,
    run_cmd,
};
use crate::pr;

pub fn run() -> anyhow::Result<()> {
    let file_path = cluster_deployment_file_path();
    if !file_path.exists() {
        return Ok(());
    }

    let deployment_data = read_deployment_file(&file_path)?;

    if let Ok(namespace_name) = get_deployment_namespace(&deployment_data) {
        connect_to_cluster()?;

        let rbac_name = format!("prometheus-cadvisor-{namespace_name}");
        run_cmd(
            &format!(
                "kubectl delete clusterrole,clusterrolebinding {rbac_name} --ignore-not-found"
            ),
            "none",
            true,
        )?;

        // Namespace deletion can hang in "Terminating" state due to stuck finalizers.
        // Re-creating then deleting forces Kubernetes to clean up.
        delete_namespace(namespace_name, true)?;
        create_namespace(namespace_name)?;
        delete_namespace(namespace_name, false)?;
    }

    let cluster_dir = cluster_deployment_working_directory();
    remove_directory(&cluster_dir, false)?;

    pr!("Network stress test stopped successfully.");
    Ok(())
}
