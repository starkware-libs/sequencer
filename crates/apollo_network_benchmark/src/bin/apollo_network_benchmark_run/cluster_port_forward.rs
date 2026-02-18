use std::process::{Command, Stdio};
use std::thread;

use crate::mod_utils::{connect_to_cluster, get_deployment_namespace, read_cluster_deployment};
use crate::pr;

fn port_forward(service_name: &str, local_port: u16, remote_port: u16, namespace_name: &str) {
    let _ = Command::new("kubectl")
        .args([
            "port-forward",
            &format!("service/{service_name}"),
            &format!("{local_port}:{remote_port}"),
            "-n",
            namespace_name,
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();
}

pub fn run() -> anyhow::Result<()> {
    let deployment_data = read_cluster_deployment()?;
    let namespace_name = get_deployment_namespace(&deployment_data)?.to_string();

    connect_to_cluster()?;

    pr!("Port forwarding in namespace: {namespace_name}");
    pr!("  → Grafana:    http://localhost:3000");
    pr!("  → Prometheus: http://localhost:9090");
    pr!("\nPress Ctrl+C to stop\n");

    let namespace_clone = namespace_name.clone();

    let grafana_thread = thread::spawn(move || {
        port_forward("grafana-service", 3000, 3000, &namespace_name);
    });

    let prometheus_thread = thread::spawn(move || {
        port_forward("prometheus-service", 9090, 9090, &namespace_clone);
    });

    let _ = grafana_thread.join();
    let _ = prometheus_thread.join();

    Ok(())
}
