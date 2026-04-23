use std::time::Duration;
use std::{fs, thread};

use anyhow::Context;
use clap::Parser;
use serde::Serialize;

use crate::args::{NetworkControls, ResourceLimits, SharedArgs, STRESS_TEST_NAME};
use crate::mod_utils::{
    build_docker_image,
    cluster_deployment_file_path,
    cluster_deployment_working_directory,
    connect_to_cluster,
    create_namespace,
    deploy_json_files_to_cluster,
    get_commit,
    login_to_docker_registry,
    make_cluster_image_tag,
    make_timestamp,
    run_cmd,
    run_with_deployment_guard,
    upload_image_to_registry,
    verify_docker_image_exists,
    write_deployment_file,
    write_json_file_to_working_dir,
};
use crate::pr;
use crate::yaml_maker::*;

#[derive(Parser, Debug, Serialize)]
pub struct ClusterStartArgs {
    #[command(flatten)]
    pub shared: SharedArgs,

    /// Previously built image tag to use instead of re-building the docker image
    #[arg(long)]
    pub image: Option<String>,

    #[command(flatten)]
    pub network_controls: NetworkControls,

    /// Role selector for the dedicated node pool
    #[arg(long, default_value = "apollo-network-benchmark")]
    pub node_pool_role: String,

    /// Node toleration in format key=value (e.g., "workload=apollo-network-benchmark")
    /// If set, adds a NoSchedule toleration to pods
    #[arg(long)]
    pub node_toleration: Option<String>,

    #[command(flatten)]
    pub resource_limits: ResourceLimits,

    /// Use production Docker build instead of development Docker build, defaults to faster
    /// development build if not specified
    #[arg(long, default_value = "false")]
    pub production_docker: bool,
}

fn write_json_files(
    image_tag: &str,
    args: &ClusterStartArgs,
    namespace_name: &str,
) -> anyhow::Result<Vec<String>> {
    let deployment_file_name = format!("{STRESS_TEST_NAME}-deployment.json");
    let headless_service_file_name = format!("{STRESS_TEST_NAME}-headless-service.json");
    let files = vec![
        (
            deployment_file_name.as_str(),
            get_network_stress_test_deployment_json_file(
                image_tag,
                &args.shared,
                &args.resource_limits,
                &args.network_controls,
                &args.node_pool_role,
                args.node_toleration.as_deref(),
            )?,
        ),
        (headless_service_file_name.as_str(), STRESS_TEST_HEADLESS_SERVICE_JSON.to_string()),
        ("prometheus-rbac.json", get_prometheus_rbac_json(namespace_name)?),
        ("prometheus-config.json", get_prometheus_json_file(args.shared.num_nodes)?),
        ("prometheus-statefulset.json", PROMETHEUS_DEPLOYMENT_JSON.to_string()),
        ("prometheus-service.json", PROMETHEUS_SERVICE_JSON.to_string()),
        ("prometheus-headless-service.json", PROMETHEUS_HEADLESS_SERVICE_JSON.to_string()),
        ("grafana-config.json", get_grafana_configmap_json_file()?),
        ("grafana-statefulset.json", GRAFANA_DEPLOYMENT_JSON.to_string()),
        ("grafana-service.json", GRAFANA_SERVICE_JSON.to_string()),
        ("grafana-headless-service.json", GRAFANA_HEADLESS_SERVICE_JSON.to_string()),
    ];

    let file_names: Vec<String> = files.iter().map(|(name, _)| name.to_string()).collect();

    for (file_name, file_content) in files {
        write_json_file_to_working_dir(file_name, &file_content)?;
    }

    Ok(file_names)
}

fn run_experiment(args: ClusterStartArgs) -> anyhow::Result<()> {
    pr!("{:?}", args);

    let timestamp = make_timestamp();
    let mut deployment_data = serde_json::json!({
        "cluster_working_directory": cluster_deployment_working_directory().display().to_string(),
        "timestamp": timestamp,
        "args": serde_json::to_value(&args).context("Failed to serialize args")?
    });

    let working_dir = cluster_deployment_working_directory();
    fs::create_dir_all(&working_dir).context("Failed to create working directory")?;

    let image_tag = if let Some(ref img) = args.image {
        deployment_data["was_image_built"] = serde_json::json!(false);
        img.clone()
    } else {
        pr!("Building image");
        let tag = make_cluster_image_tag(&timestamp);
        build_docker_image(&tag, !args.production_docker)?;
        deployment_data["commit"] = serde_json::json!(get_commit()?);
        deployment_data["was_image_built"] = serde_json::json!(true);
        tag
    };

    pr!("Image tag: {}", image_tag);
    deployment_data["image_tag"] = serde_json::json!(&image_tag);

    verify_docker_image_exists(&image_tag)?;

    connect_to_cluster()?;
    login_to_docker_registry()?;
    upload_image_to_registry(&image_tag)?;

    let namespace_name = format!("{STRESS_TEST_NAME}-{timestamp}");
    let delay_seconds = args.shared.user.timeout;
    deployment_data["delay_seconds"] = serde_json::json!(delay_seconds);
    deployment_data["namespace"] = serde_json::json!(&namespace_name);

    let deployment_file = cluster_deployment_file_path();
    write_deployment_file(&deployment_file, &deployment_data)?;

    create_namespace(&namespace_name)?;

    let file_names = write_json_files(&image_tag, &args, &namespace_name)?;
    deployment_data["json_files"] = serde_json::json!(file_names);

    write_deployment_file(&deployment_file, &deployment_data)?;

    deploy_json_files_to_cluster(&namespace_name, &file_names)?;

    thread::sleep(Duration::from_secs(10));

    run_cmd(
        &format!("kubectl get pods -n {}", namespace_name),
        "Check if pods are running",
        false,
    )?;

    pr!("Prometheus and Grafana deployment complete!");
    pr!("Deployment files saved to: `{}`", cluster_deployment_file_path().display());
    pr!("WARNING: Please don't forget to delete the namespace manually after the experiment is \
         complete !!!");
    pr!("");
    pr!("To set up port forwarding, run: cargo run --release --bin run -- cluster port-forward");

    Ok(())
}

pub fn run(args: ClusterStartArgs) -> anyhow::Result<()> {
    run_with_deployment_guard(
        &cluster_deployment_file_path(),
        || {
            pr!("Stopping last cluster run...");
            crate::cluster_stop::run()
        },
        || run_experiment(args),
    )
}
