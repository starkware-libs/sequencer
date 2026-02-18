use std::process::{Command, Stdio};
use std::thread;

use anyhow::Context;

use crate::mod_utils::{cluster_deployment_file_path, connect_to_cluster, read_deployment_file};
use crate::pr;

const PROMETHEUS_SERVICE_NAME: &str = "prometheus-service";

fn port_forward(service_name: &str, local_port: u16, remote_port: u16, namespace: &str) {
    let _ = Command::new("kubectl")
        .args([
            "port-forward",
            &format!("service/{}", service_name),
            &format!("{}:{}", local_port, remote_port),
            "-n",
            namespace,
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();
}

pub fn run() -> anyhow::Result<()> {
    let deployment_data = read_deployment_file(&cluster_deployment_file_path())?;

    let namespace = deployment_data
        .get("namespace")
        .and_then(|n| n.as_str())
        .context("No namespace found in deployment file")?;

    connect_to_cluster()?;

    pr!("Port forwarding in namespace: {}", namespace);
    pr!("  → Grafana:    http://localhost:3000");
    pr!("  → Prometheus: http://localhost:9090");
    pr!("\nPress Ctrl+C to stop\n");

    let namespace_grafana = namespace.to_string();
    let namespace_prometheus = namespace.to_string();

    let grafana_thread = thread::spawn(move || {
        port_forward("grafana-service", 3000, 3000, &namespace_grafana);
    });

    let prometheus_thread = thread::spawn(move || {
        port_forward(PROMETHEUS_SERVICE_NAME, 9090, 9090, &namespace_prometheus);
    });

    let _ = grafana_thread.join();
    let _ = prometheus_thread.join();

    Ok(())
}
