use std::time::Duration;
use std::{fs, thread};

use clap::Parser;
use serde::Serialize;

use crate::args::SharedArgs;
use crate::mod_utils::{
    build_docker_image,
    cluster_deployment_file_path,
    cluster_deployment_working_directory,
    connect_to_cluster,
    create_namespace,
    delete_namespace,
    deploy_json_files_to_cluster,
    get_commit,
    login_to_docker_registry,
    make_cluster_image_tag,
    make_timestamp,
    remove_directory,
    run_cmd,
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

    /// Min latency to use when gating the network in milliseconds
    #[arg(long)]
    pub latency: Option<u32>,

    /// Max throughput to use when gating the network in KB/s
    #[arg(long)]
    pub throughput: Option<u32>,

    /// Role selector for the dedicated node pool
    #[arg(long, default_value = "apollo-network-benchmark")]
    pub node_pool_role: String,

    /// CPU requests for each network stress test pod
    #[arg(long, default_value = "7500m")]
    pub cpu_requests: String,

    /// Memory requests for each network stress test pod
    #[arg(long, default_value = "10Gi")]
    pub memory_requests: String,

    /// CPU limit for each network stress test pod
    #[arg(long, default_value = "7500m")]
    pub cpu_limits: String,

    /// Memory limit for each network stress test pod
    #[arg(long, default_value = "10Gi")]
    pub memory_limits: String,

    /// Use production Docker build instead of development Docker build, defaults to faster
    /// development build if not specified
    #[arg(long, default_value = "false")]
    pub production_docker: bool,
}

fn write_json_files(image_tag: &str, args: &ClusterStartArgs) -> Result<Vec<String>, String> {
    let files = vec![
        (
            "broadcast-network-stress-test-deployment.json",
            get_network_stress_test_deployment_json_file(
                image_tag,
                &args.shared,
                &args.cpu_requests,
                &args.memory_requests,
                &args.cpu_limits,
                &args.memory_limits,
                &args.node_pool_role,
                args.latency,
                args.throughput,
            )?,
        ),
        (
            "broadcast-network-stress-test-headless-service.json",
            get_network_stress_test_headless_service_json_file()?,
        ),
        ("prometheus-config.json", get_prometheus_json_file(args.shared.num_nodes)?),
        ("prometheus-statefulset.json", get_prometheus_deployment_json_file()?),
        ("prometheus-service.json", get_prometheus_service_json_file()?),
        ("prometheus-headless-service.json", get_prometheus_headless_service_json_file()?),
        ("grafana-config.json", get_grafana_configmap_json_file()?),
        ("grafana-statefulset.json", get_grafana_deployment_json_file()?),
        ("grafana-service.json", get_grafana_service_json_file()?),
        ("grafana-headless-service.json", get_grafana_headless_service_json_file()?),
    ];

    let file_names: Vec<String> = files.iter().map(|(name, _)| name.to_string()).collect();

    for (file_name, file_content) in files {
        write_json_file_to_working_dir(file_name, &file_content)?;
    }

    Ok(file_names)
}

fn run_experiment(args: ClusterStartArgs) -> Result<(), String> {
    pr!("{:?}", args);

    let timestamp = make_timestamp();
    let mut deployment_data = serde_json::json!({
        "cluster_working_directory": cluster_deployment_working_directory().to_str().unwrap(),
        "timestamp": timestamp,
        "args": serde_json::to_value(&args).map_err(|e| format!("Failed to serialize args: {}", e))?
    });

    // Create working directory
    let working_dir = cluster_deployment_working_directory();
    fs::create_dir_all(&working_dir)
        .map_err(|e| format!("Failed to create working directory: {}", e))?;

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

    let namespace_name = format!("broadcast-network-stress-test-{}", timestamp);
    let delay_seconds = args.shared.user.timeout;
    deployment_data["delay_seconds"] = serde_json::json!(delay_seconds);
    deployment_data["namespace"] = serde_json::json!(&namespace_name);

    let deployment_file = cluster_deployment_file_path();
    write_deployment_file(&deployment_file, &deployment_data)?;

    create_namespace(&namespace_name)?;

    let file_names = write_json_files(&image_tag, &args)?;
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

fn stop_last_cluster_run() -> Result<(), String> {
    let file_path = cluster_deployment_file_path();

    if !file_path.exists() {
        return Ok(());
    }

    let deployment_data = crate::mod_utils::read_deployment_file(&file_path)?;

    if let Some(namespace_name) = deployment_data.get("namespace").and_then(|n| n.as_str()) {
        connect_to_cluster()?;

        delete_namespace(namespace_name, true)?;
        create_namespace(namespace_name)?;
        delete_namespace(namespace_name, false)?;
    }

    let cluster_dir = cluster_deployment_working_directory();
    remove_directory(&cluster_dir, false)?;

    pr!("Network stress test stopped successfully.");
    Ok(())
}

pub fn run(args: ClusterStartArgs) -> Result<(), String> {
    let file_path = cluster_deployment_file_path();
    if file_path.exists() {
        println!("Deployment file already exists. Do you want to stop the last run? (y/N): ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).expect("Failed to read input");

        if input.trim().to_lowercase() == "y" {
            pr!("Stopping last cluster run...");
            stop_last_cluster_run()?;
        } else {
            pr!("Exiting without running the experiment.");
            return Ok(());
        }
    }

    if file_path.exists() {
        return Err("Deployment file already exists. Please run 'cluster stop' before running \
                    the experiment."
            .to_string());
    }

    run_experiment(args)
}
