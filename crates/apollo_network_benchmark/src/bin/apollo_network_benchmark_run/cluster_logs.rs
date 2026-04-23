use crate::args::STRESS_TEST_NAME;
use crate::mod_utils::{
    get_deployment_namespace,
    get_deployment_num_nodes,
    read_cluster_deployment,
    run_cmd,
};

pub fn run() -> anyhow::Result<()> {
    let deployment_data = read_cluster_deployment()?;
    let num_nodes = get_deployment_num_nodes(&deployment_data)?;
    let namespace_name = get_deployment_namespace(&deployment_data)?;

    run_cmd(&format!("kubectl get pods -n {namespace_name}"), "Check if pods are running", false)?;

    for node_index in 0..num_nodes {
        run_cmd(
            &format!(
                "timeout 5 kubectl logs -n {namespace_name} -l \
                 app={STRESS_TEST_NAME},batch.kubernetes.io/job-completion-index={node_index} > \
                 /tmp/{STRESS_TEST_NAME}-{node_index}.logs.txt",
            ),
            &format!("Check logs for node {node_index}"),
            true,
        )?;
    }

    run_cmd(&format!("kubectl get pods -n {namespace_name}"), "Check if pods are running", false)?;

    println!("Cluster logs have been saved to /tmp/{STRESS_TEST_NAME}-*.logs.txt");
    Ok(())
}
