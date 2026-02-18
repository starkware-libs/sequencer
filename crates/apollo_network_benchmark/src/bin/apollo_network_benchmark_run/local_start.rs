use std::fs;

use anyhow::Context;
use apollo_network_benchmark::peer_key::peer_id_from_node_id;
use clap::Parser;
use serde_json::json;

use crate::args::{
    get_env_var_pairs,
    port_with_offset,
    NetworkControls,
    SharedArgs,
    METRIC_PORT_BASE,
    P2P_PORT_BASE,
    STRESS_TEST_NAME,
};
use crate::grafana_config::*;
use crate::mod_utils::{
    build_docker_image,
    local_deployment_working_directory,
    make_local_image_tag,
    make_multi_address,
    make_timestamp,
    run_cmd,
    run_with_deployment_guard,
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

    #[command(flatten)]
    pub network_controls: NetworkControls,

    /// Memory limit for each node container (e.g., "512m", "1g")
    #[arg(long, default_value = "3g")]
    pub memory_limit: String,
}

fn generate_docker_compose_json(args: &LocalStartArgs, image_tag: &str) -> anyhow::Result<String> {
    let mut services = serde_json::Map::new();

    let bootstrap_nodes: Vec<String> = (0..args.shared.num_nodes)
        .map(|j| {
            let peer_id = peer_id_from_node_id(u64::from(j))?;
            let port = port_with_offset(P2P_PORT_BASE, j)?;
            Ok(make_multi_address(
                &format!("/dns4/{STRESS_TEST_NAME}-node-{j}"),
                port,
                &peer_id,
                args.shared.quic,
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    for i in 0..args.shared.num_nodes {
        let metric_port = port_with_offset(METRIC_PORT_BASE, i)?;
        let p2p_port = port_with_offset(P2P_PORT_BASE, i)?;

        let env_pairs = get_env_var_pairs(
            Some(i),
            metric_port,
            p2p_port,
            &bootstrap_nodes,
            &args.shared,
            &args.network_controls,
        )?;

        let env: serde_json::Map<String, serde_json::Value> =
            env_pairs.into_iter().map(|(k, v)| (k, json!(v))).collect();

        services.insert(
            format!("node-{}", i),
            json!({
                "image": image_tag,
                "container_name": format!("{STRESS_TEST_NAME}-node-{i}"),
                "cap_add": ["NET_ADMIN"],
                "mem_limit": &args.memory_limit,
                "environment": env,
            }),
        );
    }

    services.insert(
        "cadvisor".into(),
        json!({
            "image": "gcr.io/cadvisor/cadvisor:latest",
            "container_name": "cadvisor_network_stress_test",
            "ports": ["8080:8080"],
            "privileged": true,
            "volumes": [
                "/:/rootfs:ro",
                "/var/run:/var/run:ro",
                "/sys:/sys:ro",
                "/var/lib/docker/:/var/lib/docker:ro",
                "/dev/disk/:/dev/disk:ro"
            ],
        }),
    );

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

fn write_config_files(working_dir: &std::path::Path, num_nodes: u32) -> anyhow::Result<()> {
    let metric_urls: Vec<String> = (0..num_nodes)
        .map(|i| {
            let port = port_with_offset(METRIC_PORT_BASE, i)?;
            Ok(format!("{STRESS_TEST_NAME}-node-{i}:{port}"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let prometheus_config =
        get_prometheus_config(false, &metric_urls, Some("cadvisor_network_stress_test:8080"))?;
    fs::write(working_dir.join("prometheus.yml"), prometheus_config)
        .context("Failed to write prometheus.yml")?;

    let dashboard_json = get_grafana_dashboard_json(true);
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

    write_config_files(&working_dir, args.shared.num_nodes)?;

    let compose_json = generate_docker_compose_json(&args, &image_tag)?;
    let compose_path = working_dir.join("docker-compose.json");
    fs::write(&compose_path, compose_json).context("Failed to write docker-compose.json")?;

    pr!("Starting services with docker compose...");
    run_cmd(
        &format!("docker compose -f {} up -d --wait", compose_path.display()),
        "Make sure you have Docker Compose installed.",
        false,
    )?;

    pr!("Local deployment started successfully!");
    pr!("Visit http://localhost:9090 to see the metrics.");
    pr!("Visit http://localhost:3000 to see the Grafana dashboard (no login required).");
    pr!("Direct dashboard URL: http://localhost:3000/d/{STRESS_TEST_NAME}/{STRESS_TEST_NAME}");
    pr!("");
    pr!("To stop the local deployment, run: cargo run --release --bin \
         apollo_network_benchmark_run -- local stop");

    Ok(())
}

pub fn run(args: LocalStartArgs) -> anyhow::Result<()> {
    let working_dir = local_deployment_working_directory();
    let compose_file = working_dir.join("docker-compose.json");

    run_with_deployment_guard(
        &compose_file,
        || {
            pr!("Stopping last local run...");
            crate::local_stop::run()
        },
        || {
            pr!("Starting network stress test experiment...");
            pr!("This will run {} nodes using Docker containers.", args.shared.num_nodes);

            let nc = &args.network_controls;
            if nc.latency.is_some() || nc.throughput.is_some() {
                let mut controls = vec![];
                if let Some(latency) = nc.latency {
                    controls.push(format!("latency: {}ms", latency));
                }
                if let Some(throughput) = nc.throughput {
                    controls.push(format!("throughput: {}KB/s", throughput));
                }
                pr!("Network controls: {}", controls.join(", "));
            }

            run_experiment(args)
        },
    )
}
