use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{fs, thread};

use clap::Parser;

use crate::args::{get_arguments, SharedArgs};
use crate::grafana_config::*;
use crate::mod_utils::{
    build_docker_image,
    check_docker,
    create_docker_network,
    get_peer_id_from_node_id,
    local_deployment_file_path,
    local_deployment_working_directory,
    make_local_image_tag,
    make_multi_address,
    make_timestamp,
    remove_docker_container,
    run_cmd,
    verify_docker_image_exists,
    write_deployment_file,
};
use crate::pr;
use crate::yaml_maker::get_prometheus_config;

#[derive(Parser, Debug)]
pub struct LocalStartArgs {
    #[command(flatten)]
    pub shared: SharedArgs,

    /// Previously built image tag to use instead of re-building
    #[arg(long)]
    pub image: Option<String>,

    /// Min latency to use when gating the network in milliseconds
    #[arg(long)]
    pub latency: Option<u32>,

    /// Max throughput to use when gating the network in KB/s
    #[arg(long)]
    pub throughput: Option<u32>,

    /// Memory limit for each node container (e.g., "512m", "1g")
    #[arg(long, default_value = "3g")]
    pub memory_limit: String,
}

fn write_prometheus_config(tmp_dir: &Path, metric_ports: &[(u32, u16)]) -> Result<PathBuf, String> {
    let metric_urls: Vec<String> = metric_ports
        .iter()
        .map(|(i, port)| format!("broadcast-network-stress-test-node-{}:{}", i, port))
        .collect();

    let config = get_prometheus_config(false, &metric_urls)?;
    let prometheus_config_path = tmp_dir.join("prometheus.yml");

    pr!("Writing Prometheus configuration to {}...", prometheus_config_path.display());
    fs::write(&prometheus_config_path, config)
        .map_err(|e| format!("Failed to write Prometheus config: {}", e))?;

    Ok(prometheus_config_path)
}

fn generate_grafana_dashboard(tmp_dir: &Path) -> Result<PathBuf, String> {
    pr!("Generating Grafana dashboard configuration...");
    let dashboard_json = get_grafana_dashboard_json("5s");
    let dashboard_path = tmp_dir.join("dashboard.json");

    fs::write(&dashboard_path, dashboard_json)
        .map_err(|e| format!("Failed to write dashboard: {}", e))?;

    pr!("Dashboard configuration saved to {}", dashboard_path.display());
    Ok(dashboard_path)
}

fn write_grafana_datasource_config(tmp_dir: &Path) -> Result<PathBuf, String> {
    let datasource_config =
        get_grafana_datasource_config("http://prometheus_network_stress_test:9090");

    let datasource_path = tmp_dir.join("datasource.yml");
    fs::write(&datasource_path, datasource_config)
        .map_err(|e| format!("Failed to write datasource config: {}", e))?;

    Ok(datasource_path)
}

fn write_grafana_dashboard_config(tmp_dir: &Path) -> Result<PathBuf, String> {
    let dashboard_config = get_grafana_dashboard_provisioning_config();
    let config_path = tmp_dir.join("dashboard_config.yml");

    fs::write(&config_path, dashboard_config)
        .map_err(|e| format!("Failed to write dashboard config: {}", e))?;

    Ok(config_path)
}

fn run_grafana(
    tmp_dir: &Path,
    network_name: &str,
    docker_containers: &mut Vec<String>,
) -> Result<(), String> {
    pr!("Running Grafana...");

    let dashboard_path = generate_grafana_dashboard(tmp_dir)?;
    let datasource_path = write_grafana_datasource_config(tmp_dir)?;
    let dashboard_config_path = write_grafana_dashboard_config(tmp_dir)?;

    let grafana_config = get_grafana_config();
    let grafana_config_path = tmp_dir.join("grafana.ini");
    fs::write(&grafana_config_path, grafana_config)
        .map_err(|e| format!("Failed to write grafana config: {}", e))?;

    let preferences_json = get_grafana_preferences_json();
    let preferences_path = tmp_dir.join("preferences.json");
    fs::write(&preferences_path, preferences_json)
        .map_err(|e| format!("Failed to write preferences: {}", e))?;

    remove_docker_container("grafana_network_stress_test", true)?;

    let cmd = format!(
        "docker run -d --name grafana_network_stress_test --network={} -p 3000:3000 -e \
         GF_PATHS_CONFIG=/etc/grafana/grafana.ini -v {}:/etc/grafana/grafana.ini:ro -v \
         {}:/etc/grafana/provisioning/datasources/datasource.yml:ro -v \
         {}:/etc/grafana/provisioning/dashboards/dashboard_config.yml:ro -v \
         {}:/etc/grafana/provisioning/dashboards/dashboard.json:ro grafana/grafana:latest",
        network_name,
        grafana_config_path.display(),
        datasource_path.display(),
        dashboard_config_path.display(),
        dashboard_path.display()
    );

    run_cmd(&cmd, "none", false)?;
    docker_containers.push("grafana_network_stress_test".to_string());

    pr!("Grafana available at http://localhost:3000 (no login required)");
    pr!("Direct dashboard link: http://localhost:3000/d/broadcast-network-stress-test/broadcast-network-stress-test");

    Ok(())
}

fn run_prometheus(
    tmp_dir: &Path,
    network_name: &str,
    metric_ports: &[(u32, u16)],
    docker_containers: &mut Vec<String>,
) -> Result<(), String> {
    pr!("Running Prometheus...");
    let prometheus_config_path = write_prometheus_config(tmp_dir, metric_ports)?;

    remove_docker_container("prometheus_network_stress_test", true)?;

    let cmd = format!(
        "docker run -d --name prometheus_network_stress_test --network={} -p 9090:9090 -v \
         {}:/etc/prometheus/prometheus.yml:ro prom/prometheus",
        network_name,
        prometheus_config_path.display()
    );

    run_cmd(&cmd, "none", false)?;
    docker_containers.push("prometheus_network_stress_test".to_string());

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_network_stress_test_node_container(
    i: u32,
    args: &LocalStartArgs,
    _tmp_dir: &Path,
    network_name: &str,
    image_tag: &str,
    metric_port_base: u16,
    p2p_port_base: u16,
    docker_containers: &mut Vec<String>,
    metric_ports: &mut Vec<(u32, u16)>,
) -> Result<(), String> {
    pr!("Running node {} in Docker container...", i);

    let metric_port = metric_port_base
        + u16::try_from(i).map_err(|e| format!("Node index {} too large: {}", i, e))?;
    let p2p_port = p2p_port_base
        + u16::try_from(i).map_err(|e| format!("Node index {} too large: {}", i, e))?;
    let container_name = format!("broadcast-network-stress-test-node-{}", i);

    remove_docker_container(&container_name, true)?;

    let bootstrap_nodes: Vec<String> = (0..args.shared.num_nodes)
        .map(|j| {
            let peer_id = get_peer_id_from_node_id(j)?;
            let port = p2p_port_base
                + u16::try_from(j).map_err(|e| format!("Node index {} too large: {}", j, e))?;
            Ok(make_multi_address(
                &format!("/dns4/broadcast-network-stress-test-node-{}", j),
                port,
                &peer_id,
                args.shared.quic,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let argument_tuples =
        get_arguments(Some(i), metric_port, p2p_port, &bootstrap_nodes, &args.shared);

    let mut env_args = String::new();
    for (name, value) in argument_tuples {
        let env_name = name[2..].replace("-", "_").to_uppercase();
        env_args.push_str(&format!("-e {}={} ", env_name, value));
    }

    if let Some(latency) = args.latency {
        env_args.push_str(&format!("-e LATENCY={} ", latency));
    }
    if let Some(throughput) = args.throughput {
        env_args.push_str(&format!("-e THROUGHPUT={} ", throughput));
    }

    let cmd = format!(
        "docker run -d --name {} --network={} --cap-add=NET_ADMIN --memory={} {} {}",
        container_name, network_name, args.memory_limit, env_args, image_tag
    );

    run_cmd(&cmd, "none", false)?;

    docker_containers.push(container_name);
    metric_ports.push((i, metric_port));

    Ok(())
}

fn stop_last_local_run() -> Result<(), String> {
    let file_path = local_deployment_file_path();

    if !file_path.exists() {
        return Ok(());
    }

    pr!("Stopping last local run...");

    // Import and use the stop logic
    crate::local_stop::stop_local_deployment(&file_path)?;

    pr!("Last local run stopped successfully.");
    Ok(())
}

fn run_experiment(args: LocalStartArgs) -> Result<(), String> {
    pr!("{:?}", args);

    let timestamp = make_timestamp();
    let network_name = format!("stress-test-net-{}", timestamp);
    let metric_port_base: u16 = 2000;
    let p2p_port_base: u16 = 10000;

    // Create working directory
    let working_dir = local_deployment_working_directory();
    fs::create_dir_all(&working_dir)
        .map_err(|e| format!("Failed to create working directory: {}", e))?;

    pr!("Using working directory: {}", working_dir.display());

    let mut deployment_data = serde_json::json!({
        "timestamp": timestamp,
        "working_dir": working_dir.to_str().unwrap(),
        "network_name": network_name,
        "metric_port_base": metric_port_base,
        "p2p_port_base": p2p_port_base,
        "docker_containers": Vec::<String>::new(),
        "args": {
            "num_nodes": args.shared.num_nodes,
            "verbosity": args.shared.user.verbosity,
            "buffer_size": args.shared.user.buffer_size,
            "message_size_bytes": args.shared.user.message_size_bytes,
            "heartbeat_millis": args.shared.user.heartbeat_millis,
            "mode": &args.shared.user.mode,
            "network_protocol": &args.shared.user.network_protocol,
            "broadcaster": args.shared.user.broadcaster,
            "round_duration_seconds": args.shared.user.round_duration_seconds,
            "quic": args.shared.quic,
            "timeout": args.shared.user.timeout,
            "latency": args.latency,
            "throughput": args.throughput,
            "memory_limit": &args.memory_limit,
        }
    });

    check_docker()?;

    let image_tag = if let Some(ref img) = args.image {
        deployment_data["was_image_built"] = serde_json::json!(false);
        img.clone()
    } else {
        pr!("Building image");
        let tag = make_local_image_tag(&timestamp);
        build_docker_image(&tag, true)?;
        deployment_data["was_image_built"] = serde_json::json!(true);
        tag
    };

    pr!("Image tag: {}", image_tag);
    deployment_data["image_tag"] = serde_json::json!(&image_tag);

    verify_docker_image_exists(&image_tag)?;

    create_docker_network(&network_name)?;

    let mut docker_containers: Vec<String> = vec![];
    let mut metric_ports: Vec<(u32, u16)> = vec![];

    pr!("Running broadcast_network_stress_test_node nodes in Docker containers...");
    for i in 0..args.shared.num_nodes {
        run_network_stress_test_node_container(
            i,
            &args,
            &working_dir,
            &network_name,
            &image_tag,
            metric_port_base,
            p2p_port_base,
            &mut docker_containers,
            &mut metric_ports,
        )?;
    }

    run_prometheus(&working_dir, &network_name, &metric_ports, &mut docker_containers)?;
    run_grafana(&working_dir, &network_name, &mut docker_containers)?;

    deployment_data["docker_containers"] = serde_json::json!(&docker_containers);
    let deployment_file = local_deployment_file_path();
    write_deployment_file(&deployment_file, &deployment_data)?;

    pr!("Running broadcast_network_stress_test_nodes in Docker containers (custom network with \
         traffic control)...");
    pr!("Visit http://localhost:9090 to see the metrics.");
    pr!("Visit http://localhost:3000 to see the Grafana dashboard (no login required).");
    pr!("Direct dashboard URL: http://localhost:3000/d/broadcast-network-stress-test/broadcast-network-stress-test");
    pr!("Deployment files saved to: `{}`", deployment_file.display());
    pr!("");
    pr!("To stop the local deployment, run: cargo run --release --bin \
         apollo_network_benchmark_run -- local stop");

    // Wait a bit for services to stabilize
    thread::sleep(Duration::from_secs(5));

    pr!("Local deployment started successfully!");

    Ok(())
}

pub fn run(args: LocalStartArgs) -> Result<(), String> {
    let file_path = local_deployment_file_path();
    if file_path.exists() {
        println!("Deployment file already exists. Do you want to stop the last run? (y/N): ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).expect("Failed to read input");

        if input.trim().to_lowercase() == "y" {
            stop_last_local_run()?;
        } else {
            pr!("Exiting without running the experiment.");
            return Ok(());
        }
    }

    if file_path.exists() {
        return Err("Deployment file already exists. Please run 'local stop' before running the \
                    experiment."
            .to_string());
    }

    pr!("Starting network stress test experiment...");
    pr!("This will run {} nodes using Docker containers.", args.shared.num_nodes);

    if args.latency.is_some() || args.throughput.is_some() {
        let mut controls = vec![];
        if let Some(latency) = args.latency {
            controls.push(format!("latency: {}ms", latency));
        }
        if let Some(throughput) = args.throughput {
            controls.push(format!("throughput: {}KB/s", throughput));
        }
        pr!("Network controls: {}", controls.join(", "));
    }

    run_experiment(args)
}
