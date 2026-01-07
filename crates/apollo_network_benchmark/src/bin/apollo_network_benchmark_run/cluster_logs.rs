use crate::mod_utils::{cluster_deployment_file_path, read_deployment_file, run_cmd};

pub fn run() -> Result<(), String> {
    let deployment_data = read_deployment_file(&cluster_deployment_file_path())?;

    let num_nodes = deployment_data
        .get("args")
        .and_then(|a| a.get("shared"))
        .and_then(|s| s.get("num_nodes"))
        .and_then(|n| n.as_u64())
        .ok_or("Failed to get num_nodes from deployment file")?;

    let namespace_name = deployment_data
        .get("namespace")
        .and_then(|n| n.as_str())
        .ok_or("Failed to get namespace from deployment file")?;

    run_cmd(
        &format!("kubectl get pods -n {}", namespace_name),
        "Check if pods are running",
        false,
    )?;

    for i in 0..num_nodes {
        run_cmd(
            &format!(
                "timeout 5 kubectl logs -n {} -l \
                 app=broadcast-network-stress-test,batch.kubernetes.io/job-completion-index={} > \
                 /tmp/broadcast-network-stress-test-{}.logs.txt",
                namespace_name, i, i
            ),
            &format!("Check logs for node {}", i),
            true,
        )?;
    }

    run_cmd(
        &format!("kubectl get pods -n {}", namespace_name),
        "Check if pods are running",
        false,
    )?;

    println!("Cluster logs have been saved to /tmp/broadcast-network-stress-test-*.logs.txt");
    Ok(())
}
