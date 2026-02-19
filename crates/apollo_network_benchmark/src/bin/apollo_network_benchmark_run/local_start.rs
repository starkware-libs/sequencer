use std::time::Duration;
use std::{fs, thread};

use anyhow::Context;
use apollo_network_benchmark::peer_key::peer_id_from_node_id;
use clap::Parser;
use serde_json::json;

use crate::args::{get_env_var_pairs, SharedArgs};
use crate::grafana_config::*;
use crate::mod_utils::{
    build_docker_image,
    local_deployment_working_directory,
    make_local_image_tag,
    make_multi_address,
    make_timestamp,
    run_cmd,
    verify_docker_image_exists,
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

fn generate_docker_compose_json(
    args: &LocalStartArgs,
    image_tag: &str,
    metric_port_base: u16,
    p2p_port_base: u16,
) -> anyhow::Result<String> {
    let mut services = serde_json::Map::new();

    for i in 0..args.shared.num_nodes {
        let metric_port = metric_port_base + u16::try_from(i).context("Node index too large")?;
        let p2p_port = p2p_port_base + u16::try_from(i).context("Node index too large")?;

        let bootstrap_nodes: Vec<String> = (0..args.shared.num_nodes)
            .map(|j| {
                let peer_id = peer_id_from_node_id(u64::from(j))?;
                let port = p2p_port_base + u16::try_from(j).context("Node index too large")?;
                Ok(make_multi_address(
                    &format!("/dns4/broadcast-network-stress-test-node-{}", j),
                    port,
                    &peer_id,
                    args.shared.quic,
                ))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let env_pairs = get_env_var_pairs(
            Some(i),
            metric_port,
            p2p_port,
            &bootstrap_nodes,
            &args.shared,
            args.latency,
            args.throughput,
        );

        let env: serde_json::Map<String, serde_json::Value> =
            env_pairs.into_iter().map(|(k, v)| (k, json!(v))).collect();

        services.insert(
            format!("node-{}", i),
            json!({
                "image": image_tag,
                "container_name": format!("broadcast-network-stress-test-node-{}", i),
                "cap_add": ["NET_ADMIN"],
                "mem_limit": &args.memory_limit,
                "environment": env,
            }),
        );
    }

    services.insert("node_exporter".into(), json!({
        "image": "prom/node-exporter:latest",
        "container_name": "node_exporter_network_stress_test",
        "ports": ["9100:9100"],
        "pid": "host",
        "volumes": ["/proc:/host/proc:ro", "/sys:/host/sys:ro", "/:/rootfs:ro"],
        "command": ["--path.procfs=/host/proc", "--path.sysfs=/host/sys", "--path.rootfs=/rootfs"],
    }));

    services.insert(
        "prometheus".into(),
        json!({
            "image": "prom/prometheus",
            "container_name": "prometheus_network_stress_test",
            "ports": ["9090:9090"],
            "volumes": ["./prometheus.yml:/etc/prometheus/prometheus.yml:ro"],
        }),
    );

    services.insert("grafana".into(), json!({
        "image": "grafana/grafana:latest",
        "container_name": "grafana_network_stress_test",
        "ports": ["3000:3000"],
        "environment": {"GF_PATHS_CONFIG": "/etc/grafana/grafana.ini"},
        "volumes": [
            "./grafana.ini:/etc/grafana/grafana.ini:ro",
            "./datasource.yml:/etc/grafana/provisioning/datasources/datasource.yml:ro",
            "./dashboard_config.yml:/etc/grafana/provisioning/dashboards/dashboard_config.yml:ro",
            "./dashboard.json:/etc/grafana/provisioning/dashboards/dashboard.json:ro",
        ],
    }));

    let compose = json!({ "services": services });
    serde_json::to_string_pretty(&compose).context("Failed to serialize docker-compose config")
}

fn write_config_files(
    working_dir: &std::path::Path,
    num_nodes: u32,
    metric_port_base: u16,
) -> anyhow::Result<()> {
    let metric_urls: Vec<String> = (0..num_nodes)
        .map(|i| {
            let port = metric_port_base + u16::try_from(i).expect("num_nodes fits in u16");
            format!("broadcast-network-stress-test-node-{}:{}", i, port)
        })
        .collect();

    let prometheus_config =
        get_prometheus_config(false, &metric_urls, Some("node_exporter_network_stress_test:9100"))?;
    fs::write(working_dir.join("prometheus.yml"), prometheus_config)
        .context("Failed to write prometheus.yml")?;

    let dashboard_json = get_grafana_dashboard_json("5s");
    fs::write(working_dir.join("dashboard.json"), dashboard_json)
        .context("Failed to write dashboard.json")?;

    let datasource_config = get_grafana_datasource_config("http://prometheus:9090");
    fs::write(working_dir.join("datasource.yml"), datasource_config)
        .context("Failed to write datasource.yml")?;

    let dashboard_config = get_grafana_dashboard_provisioning_config();
    fs::write(working_dir.join("dashboard_config.yml"), dashboard_config)
        .context("Failed to write dashboard_config.yml")?;

    let grafana_config = get_grafana_config();
    fs::write(working_dir.join("grafana.ini"), grafana_config)
        .context("Failed to write grafana.ini")?;

    Ok(())
}

fn run_experiment(args: LocalStartArgs) -> anyhow::Result<()> {
    pr!("{:?}", args);

    let timestamp = make_timestamp();
    let metric_port_base: u16 = 2000;
    let p2p_port_base: u16 = 10000;

    let working_dir = local_deployment_working_directory();
    fs::create_dir_all(&working_dir).context("Failed to create working directory")?;

    pr!("Using working directory: {}", working_dir.display());

    pr!("Checking if Docker works...");
    run_cmd(
        "docker info --format '{{.ServerVersion}}'",
        "Make sure you have Docker installed and running.",
        false,
    )?;

    let image_tag = if let Some(ref img) = args.image {
        img.clone()
    } else {
        pr!("Building image");
        let tag = make_local_image_tag(&timestamp);
        build_docker_image(&tag, true)?;
        tag
    };

    pr!("Image tag: {}", image_tag);
    verify_docker_image_exists(&image_tag)?;

    write_config_files(&working_dir, args.shared.num_nodes, metric_port_base)?;

    let compose_json =
        generate_docker_compose_json(&args, &image_tag, metric_port_base, p2p_port_base)?;
    let compose_path = working_dir.join("docker-compose.json");
    fs::write(&compose_path, compose_json).context("Failed to write docker-compose.json")?;

    pr!("Starting services with docker compose...");
    run_cmd(
        &format!("docker compose -f {} up -d", compose_path.display()),
        "Make sure you have Docker Compose installed.",
        false,
    )?;

    pr!("Running nodes in Docker containers (custom network with traffic control)...");
    pr!("Visit http://localhost:9090 to see the metrics.");
    pr!("Visit http://localhost:3000 to see the Grafana dashboard (no login required).");
    pr!("Direct dashboard URL: http://localhost:3000/d/broadcast-network-stress-test/broadcast-network-stress-test");
    pr!("");
    pr!("To stop the local deployment, run: cargo run --release --bin \
         apollo_network_benchmark_run -- local stop");

    thread::sleep(Duration::from_secs(5));

    pr!("Local deployment started successfully!");

    Ok(())
}

pub fn run(args: LocalStartArgs) -> anyhow::Result<()> {
    let working_dir = local_deployment_working_directory();
    let compose_file = working_dir.join("docker-compose.json");

    if compose_file.exists() {
        println!("A local deployment already exists. Do you want to stop it first? (y/N): ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).expect("Failed to read input");

        if input.trim().to_lowercase() == "y" {
            crate::local_stop::run()?;
        } else {
            pr!("Exiting without running the experiment.");
            return Ok(());
        }
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
